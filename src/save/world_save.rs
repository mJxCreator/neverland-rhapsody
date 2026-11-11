//! 世界存档：按区块记录玩家对程序化地形的改动、区块掉落物、区块生物、玩家背包与生存状态，
//! 并兼容 v0.8（version 1）至 v0.15（version 5）旧档。
//!
//! v0.15（version 6）新增 `entities`（随区块保存的生物状态）与 `populated_chunks`
//! （已自然生成过的水平区块列），旧档缺省为空，不会触发重复生成。
//! v0.19（version 7）新增 `equipment`（四槽护甲），旧档缺省为空装备。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::entity::EntityRecord;
use crate::equipment::SavedEquipment;
use crate::item::{DroppedItemRecord, SavedInventory};
use crate::player::SavedSurvival;
use crate::world::{Block, ChunkPos, World, CHUNK_SIZE, DEFAULT_SEED, WORLD_HEIGHT};

/// 默认存档位置，相对进程工作目录。
pub const SAVE_PATH: &str = "saves/world.json";
/// 当前存档格式版本：区块数据 + 掉落物 + 生物 + 生成记录 + 玩家状态 + 装备。
const SAVE_VERSION: u32 = 7;
/// v0.8 使用的旧版格式版本：世界坐标平铺记录。
const LEGACY_SAVE_VERSION: u32 = 1;

/// 一次方块改动：世界坐标与改动后的方块。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockEdit {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: Block,
}

/// 区块内的一条改动：局部坐标与方块类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LocalEdit {
    x: i32,
    y: i32,
    z: i32,
    block: Block,
}

/// 一个区块的全部改动、掉落物与生物。
#[derive(Serialize, Deserialize)]
struct ChunkEdits {
    pos: [i32; 3],
    edits: Vec<LocalEdit>,
    /// v0.12 起随区块保存的掉落物；旧档缺省为空。
    #[serde(default)]
    drops: Vec<DroppedItemRecord>,
    /// v0.15 起随区块保存的生物实体；旧档缺省为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    entities: Vec<EntityRecord>,
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    version: u32,
    /// 存档对应的世界种子；旧版存档缺少该字段时按默认种子处理。
    #[serde(default = "default_seed")]
    seed: u32,
    /// v0.8 旧版字段：世界坐标改动。保留字段以便迁移和旧存档识别。
    #[serde(default)]
    edits: Vec<BlockEdit>,
    /// v0.9 起按区块组织的改动。
    #[serde(default)]
    chunk_edits: Vec<ChunkEdits>,
    /// v0.12 起的玩家背包与快捷栏选择；旧档缺省为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    player: Option<SavedInventory>,
    /// v0.14 起的生存状态；旧档缺省时按满血存活处理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    survival: Option<SavedSurvival>,
    /// v0.15 起记录已自然生成过的水平区块列（y 恒为 0）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    populated_chunks: Vec<[i32; 3]>,
    /// v0.19 起的四个护甲槽；旧档缺省为空装备。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    equipment: Option<SavedEquipment>,
}

fn default_seed() -> u32 {
    DEFAULT_SEED
}

/// 玩家改动的内存记录，与磁盘上的存档文件保持同步。
pub struct WorldSave {
    path: PathBuf,
    seed: u32,
    /// 以区块坐标索引，区块内以局部坐标去重，只保留最后一次改动。
    edits: HashMap<ChunkPos, HashMap<(i32, i32, i32), Block>>,
    /// 各区块尚未恢复的掉落物；区块卸载时写入，重新加载时恢复。
    drops: HashMap<ChunkPos, Vec<DroppedItemRecord>>,
    /// 各区块尚未恢复的生物；区块卸载时写入，重新加载时恢复。
    entities: HashMap<ChunkPos, Vec<EntityRecord>>,
    /// 已自然生成过生物的水平区块列，防止重新加载后重复生成。
    populated_chunks: HashSet<ChunkPos>,
    /// 玩家背包与快捷栏选择；旧档尚未提供时为 `None`。
    player: Option<SavedInventory>,
    /// 玩家生存状态；旧档尚未提供时为 `None`。
    survival: Option<SavedSurvival>,
    /// 玩家装备；旧档尚未提供时为 `None`。
    equipment: Option<SavedEquipment>,
    /// 早期写盘失败时保留内存数据，下次写入或卸载前重试。
    unsaved: bool,
    /// 主存档读取失败、本次数据来自备份时为 `true`。
    /// 此时首次写盘不能把损坏的主存档滚动成 `.bak`，否则会覆盖唯一可用的备份。
    primary_suspect: bool,
}

impl WorldSave {
    /// 默认存档路径。
    pub fn default_path() -> PathBuf {
        PathBuf::from(SAVE_PATH)
    }

    /// 读取存档；主文件不可用时回退到备份，均不可用时按空存档启动。
    ///
    /// 旧版存档在内存中迁移为区块记录，不立即改写文件，原始存档在首次写盘时滚动为 `.bak`。
    pub fn load(path: impl Into<PathBuf>, seed: u32) -> Self {
        let path = path.into();
        let mut save = Self {
            path,
            seed,
            edits: HashMap::new(),
            drops: HashMap::new(),
            entities: HashMap::new(),
            populated_chunks: HashSet::new(),
            player: None,
            survival: None,
            equipment: None,
            unsaved: false,
            primary_suspect: false,
        };

        for (index, candidate) in [save.path.clone(), save.backup_path()]
            .into_iter()
            .enumerate()
        {
            let is_primary = index == 0;
            let text = match fs::read_to_string(&candidate) {
                Ok(text) => text,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => {
                    warn!(error = %err, path = %candidate.display(), "读取存档失败，尝试下一个来源");
                    if is_primary {
                        save.primary_suspect = true;
                    }
                    continue;
                }
            };

            match save.apply_text(&text) {
                Ok(()) => {
                    info!(
                        path = %candidate.display(),
                        edits = save.total_edits(),
                        "世界存档已读取"
                    );
                    return save;
                }
                Err(err) => {
                    let source = if is_primary {
                        "主存档"
                    } else {
                        "备份存档"
                    };
                    warn!(
                        path = %candidate.display(),
                        error = %format!("{err:#}"),
                        "{source}不可用，尝试下一个来源"
                    );
                    if is_primary {
                        save.primary_suspect = true;
                    }
                }
            }
        }
        save
    }

    /// 解析存档文本、校验版本与种子，并把改动载入内存。
    fn apply_text(&mut self, text: &str) -> Result<()> {
        let data: SaveFile = serde_json::from_str(text).context("解析存档失败")?;
        if !(LEGACY_SAVE_VERSION..=SAVE_VERSION).contains(&data.version) {
            bail!(
                "存档版本 {} 与当前支持的版本 1..={} 不一致",
                data.version,
                SAVE_VERSION
            );
        }
        if data.seed != self.seed {
            bail!("存档种子 {} 与世界种子 {} 不一致", data.seed, self.seed);
        }

        let mut edits: HashMap<ChunkPos, HashMap<(i32, i32, i32), Block>> = HashMap::new();
        let mut drops: HashMap<ChunkPos, Vec<DroppedItemRecord>> = HashMap::new();
        let mut entities: HashMap<ChunkPos, Vec<EntityRecord>> = HashMap::new();
        let mut populated_chunks: HashSet<ChunkPos> = HashSet::new();
        if data.version == LEGACY_SAVE_VERSION {
            for edit in data.edits {
                Self::insert_world_edit(&mut edits, edit);
            }
        } else {
            for chunk in data.chunk_edits {
                let pos = ChunkPos::new(chunk.pos[0], chunk.pos[1], chunk.pos[2]);
                if !(0..WORLD_HEIGHT / CHUNK_SIZE).contains(&pos.y) {
                    warn!(pos = ?pos, "忽略垂直层级越界的存档区块");
                    continue;
                }
                for edit in chunk.edits {
                    if !(0..CHUNK_SIZE).contains(&edit.x)
                        || !(0..CHUNK_SIZE).contains(&edit.y)
                        || !(0..CHUNK_SIZE).contains(&edit.z)
                    {
                        warn!(pos = ?pos, "忽略区块内越界的存档改动");
                        continue;
                    }
                    edits
                        .entry(pos)
                        .or_default()
                        .insert((edit.x, edit.y, edit.z), edit.block);
                }

                let valid_drops = chunk
                    .drops
                    .into_iter()
                    .filter(|drop| {
                        Self::drop_belongs_to(drop, pos)
                            && drop.x.is_finite()
                            && drop.y.is_finite()
                            && drop.z.is_finite()
                    })
                    .collect::<Vec<_>>();
                if !valid_drops.is_empty() {
                    drops.entry(pos).or_default().extend(valid_drops);
                }

                let valid_entities = chunk
                    .entities
                    .into_iter()
                    .filter(|entity| entity.is_valid() && entity.chunk() == pos)
                    .collect::<Vec<_>>();
                if !valid_entities.is_empty() {
                    entities.entry(pos).or_default().extend(valid_entities);
                }
            }
        }

        for column in data.populated_chunks {
            populated_chunks.insert(ChunkPos::new(column[0], 0, column[2]));
        }

        self.edits = edits;
        self.drops = drops;
        self.entities = entities;
        self.populated_chunks = populated_chunks;
        self.player = data.player;
        self.survival = data.survival.map(SavedSurvival::sanitized);
        self.equipment = data.equipment;
        Ok(())
    }

    /// 掉落物记录的坐标是否归属指定区块，防止跨区块数据错位。
    fn drop_belongs_to(drop: &DroppedItemRecord, pos: ChunkPos) -> bool {
        let drop_pos = ChunkPos::from_world(
            drop.x.floor() as i32,
            drop.y.floor() as i32,
            drop.z.floor() as i32,
        );
        drop_pos == pos
    }

    /// 把一条世界坐标改动迁移到区块记录；垂直越界的改动直接丢弃。
    fn insert_world_edit(
        edits: &mut HashMap<ChunkPos, HashMap<(i32, i32, i32), Block>>,
        edit: BlockEdit,
    ) {
        if !(0..WORLD_HEIGHT).contains(&edit.y) {
            warn!(x = edit.x, y = edit.y, z = edit.z, "忽略越界的存档改动");
            return;
        }
        let pos = ChunkPos::from_world(edit.x, edit.y, edit.z);
        let (local_x, local_y, local_z) = pos.local_of(edit.x, edit.y, edit.z);
        edits
            .entry(pos)
            .or_default()
            .insert((local_x, local_y, local_z), edit.block);
    }

    /// 把指定区块的历史改动盖回世界，返回受影响的区块坐标。
    ///
    /// 区块必须已经由地形生成并插入世界，否则不会产生任何修改。
    pub fn apply_chunk(&self, pos: ChunkPos, world: &mut World) -> Vec<ChunkPos> {
        let Some(edits) = self.edits.get(&pos) else {
            return Vec::new();
        };

        let mut keys: Vec<(i32, i32, i32)> = edits.keys().copied().collect();
        keys.sort_unstable();

        let mut affected = Vec::new();
        for (local_x, local_y, local_z) in keys {
            let block = edits[&(local_x, local_y, local_z)];
            affected.extend(world.set_block(
                pos.x * CHUNK_SIZE + local_x,
                pos.y * CHUNK_SIZE + local_y,
                pos.z * CHUNK_SIZE + local_z,
                block,
            ));
        }
        affected.sort_by_key(|chunk| (chunk.x, chunk.y, chunk.z));
        affected.dedup();
        affected
    }

    /// 记录一次方块改动：同一格只保留最后一次，并立即写盘。
    pub fn record(&mut self, edit: BlockEdit) -> Result<()> {
        if !(0..WORLD_HEIGHT).contains(&edit.y) {
            bail!("改动超出垂直世界范围: y = {}", edit.y);
        }
        let pos = ChunkPos::from_world(edit.x, edit.y, edit.z);
        let (local_x, local_y, local_z) = pos.local_of(edit.x, edit.y, edit.z);
        self.edits
            .entry(pos)
            .or_default()
            .insert((local_x, local_y, local_z), edit.block);
        self.unsaved = true;
        self.persist()
    }

    /// 若存在尚未落盘的改动则重试写入；区块卸载与会话结束前调用。
    pub fn ensure_persisted(&mut self) -> Result<()> {
        if self.unsaved {
            self.persist()?;
        }
        Ok(())
    }

    /// 读取指定区块尚未恢复的掉落物记录。
    pub fn chunk_drops(&self, pos: ChunkPos) -> &[DroppedItemRecord] {
        self.drops.get(&pos).map_or(&[], Vec::as_slice)
    }

    /// 记录指定区块当前的掉落物快照；空列表表示该区块没有掉落物。
    ///
    /// 只更新内存并标记待写盘，由上层在批量处理完成后调用 `ensure_persisted`。
    pub fn record_chunk_drops(&mut self, pos: ChunkPos, drops: Vec<DroppedItemRecord>) {
        if drops.is_empty() {
            self.drops.remove(&pos);
        } else {
            self.drops.insert(pos, drops);
        }
        self.unsaved = true;
    }

    /// 读取指定区块尚未恢复的生物记录。
    pub fn chunk_entities(&self, pos: ChunkPos) -> &[EntityRecord] {
        self.entities.get(&pos).map_or(&[], Vec::as_slice)
    }

    /// 记录指定区块当前的生物快照；空列表表示该区块没有生物。
    ///
    /// 只更新内存并标记待写盘，由上层在批量处理后调用 `ensure_persisted`。
    pub fn record_chunk_entities(&mut self, pos: ChunkPos, entities: Vec<EntityRecord>) {
        if entities.is_empty() {
            self.entities.remove(&pos);
        } else {
            self.entities.insert(pos, entities);
        }
        self.unsaved = true;
    }

    /// 该水平区块列是否已有自然生成记录。
    pub fn is_chunk_populated(&self, column: ChunkPos) -> bool {
        self.populated_chunks
            .contains(&ChunkPos::new(column.x, 0, column.z))
    }

    /// 标记水平区块列已自然生成；只更新内存并标记待写盘。
    pub fn record_populated_chunk(&mut self, column: ChunkPos) {
        if self
            .populated_chunks
            .insert(ChunkPos::new(column.x, 0, column.z))
        {
            self.unsaved = true;
        }
    }

    /// 已记录的自然生成区块列数量。
    #[allow(dead_code)]
    pub fn populated_chunk_count(&self) -> usize {
        self.populated_chunks.len()
    }

    /// 已记录的生物总数。
    #[allow(dead_code)]
    pub fn total_entities(&self) -> usize {
        self.entities.values().map(Vec::len).sum()
    }

    /// 玩家背包存档；旧档尚未提供时为 `None`。
    pub fn player(&self) -> Option<&SavedInventory> {
        self.player.as_ref()
    }

    /// 玩家生存状态存档；旧档尚未提供时为 `None`。
    pub fn survival(&self) -> Option<SavedSurvival> {
        self.survival
    }

    /// 玩家装备存档；旧档尚未提供时为 `None`。
    pub fn equipment(&self) -> Option<&SavedEquipment> {
        self.equipment.as_ref()
    }

    /// 同时更新背包、生存状态与装备并只写盘一次。
    pub fn set_player_state(
        &mut self,
        player: SavedInventory,
        survival: SavedSurvival,
        equipment: SavedEquipment,
    ) -> Result<()> {
        self.player = Some(player);
        self.survival = Some(survival.sanitized());
        self.equipment = Some(equipment);
        self.unsaved = true;
        self.persist()
    }

    /// 已记录的掉落物总数。
    #[allow(dead_code)]
    pub fn total_drops(&self) -> usize {
        self.drops.values().map(Vec::len).sum()
    }

    /// 已记录的改动总数。
    pub fn total_edits(&self) -> usize {
        self.edits.values().map(HashMap::len).sum()
    }

    /// 把当前改动写入存档文件。
    fn persist(&mut self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("创建存档目录失败: {}", parent.display()))?;
            }
        }

        // 固定输出顺序，便于人工查看与版本比对。
        let mut chunk_positions: Vec<ChunkPos> = self
            .edits
            .keys()
            .chain(self.drops.keys())
            .chain(self.entities.keys())
            .copied()
            .collect();
        chunk_positions.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        chunk_positions.dedup();

        let mut chunk_edits = Vec::with_capacity(chunk_positions.len());
        let mut world_edits = Vec::with_capacity(self.total_edits());
        for pos in chunk_positions {
            let mut local_edits = Vec::new();
            if let Some(blocks) = self.edits.get(&pos) {
                let mut keys: Vec<(i32, i32, i32)> = blocks.keys().copied().collect();
                keys.sort_unstable();

                local_edits.reserve(keys.len());
                for (local_x, local_y, local_z) in keys {
                    let block = blocks[&(local_x, local_y, local_z)];
                    local_edits.push(LocalEdit {
                        x: local_x,
                        y: local_y,
                        z: local_z,
                        block,
                    });
                    world_edits.push(BlockEdit {
                        x: pos.x * CHUNK_SIZE + local_x,
                        y: pos.y * CHUNK_SIZE + local_y,
                        z: pos.z * CHUNK_SIZE + local_z,
                        block,
                    });
                }
            }

            let drops = self.drops.get(&pos).cloned().unwrap_or_default();
            let entities = self.entities.get(&pos).cloned().unwrap_or_default();
            if local_edits.is_empty() && drops.is_empty() && entities.is_empty() {
                continue;
            }
            chunk_edits.push(ChunkEdits {
                pos: [pos.x, pos.y, pos.z],
                edits: local_edits,
                drops,
                entities,
            });
        }
        world_edits.sort_by_key(|edit| (edit.x, edit.y, edit.z));

        let mut populated_chunks: Vec<[i32; 3]> = self
            .populated_chunks
            .iter()
            .map(|pos| [pos.x, 0, pos.z])
            .collect();
        populated_chunks.sort_by_key(|pos| (pos[0], pos[2]));

        let data = SaveFile {
            version: SAVE_VERSION,
            seed: self.seed,
            edits: world_edits,
            chunk_edits,
            player: self.player.clone(),
            survival: self.survival,
            equipment: self.equipment.clone(),
            populated_chunks,
        };
        let json = serde_json::to_string_pretty(&data).context("序列化存档失败")?;

        // 先写临时文件再改名，避免写到一半退出留下残缺 JSON；旧文件滚动为备份。
        let temp = self.temp_path();
        fs::write(&temp, &json).with_context(|| format!("写入临时存档失败: {}", temp.display()))?;

        // 主存档损坏、当前数据来自备份时，跳过滚动，保留唯一可用的备份。
        if self.path.exists() && !self.primary_suspect {
            let backup = self.backup_path();
            if let Err(err) = fs::rename(&self.path, &backup) {
                warn!(
                    error = %err,
                    path = %backup.display(),
                    "备份旧存档失败，继续写入主存档"
                );
            }
        }

        if let Err(err) = fs::rename(&temp, &self.path) {
            warn!(
                error = %err,
                path = %self.path.display(),
                "临时存档改名失败，改为直接写入"
            );
            let _ = fs::remove_file(&temp);
            fs::write(&self.path, &json)
                .with_context(|| format!("写入存档失败: {}", self.path.display()))?;
        }
        self.unsaved = false;
        self.primary_suspect = false;
        Ok(())
    }

    /// 主存档文件。
    fn backup_path(&self) -> PathBuf {
        suffixed(&self.path, ".bak")
    }

    /// 写入中的临时文件。
    fn temp_path(&self) -> PathBuf {
        suffixed(&self.path, ".tmp")
    }
}

/// 在文件名后追加后缀，例如 `world.json.tmp`。
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::inventory::SavedSlot;

    /// 为并行测试生成互不冲突的临时文件路径。
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nr-save-{}-{name}.json", std::process::id()))
    }

    /// 清理主存档、备份与临时文件。
    fn remove_all(path: &Path) {
        let _ = fs::remove_file(path);
        for suffix in [".bak", ".tmp"] {
            let _ = fs::remove_file(suffixed(path, suffix));
        }
    }

    /// 读取存档文件中按区块记录的有效改动数量。
    fn saved_edit_count(path: &Path) -> usize {
        let text = fs::read_to_string(path).expect("读取存档失败");
        let data: SaveFile = serde_json::from_str(&text).expect("解析存档失败");
        data.chunk_edits.iter().map(|chunk| chunk.edits.len()).sum()
    }

    /// 创建只包含指定区块的世界。
    fn world_with(positions: &[ChunkPos]) -> World {
        let mut world = World::new(DEFAULT_SEED);
        for &pos in positions {
            let chunk = world.generate_chunk(pos);
            assert!(world.insert_chunk(chunk));
        }
        world
    }

    #[test]
    fn edits_round_trip_through_the_save_file() {
        let path = temp_path("roundtrip");
        remove_all(&path);

        let world = world_with(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let height = world.height_at(10, 10);
        let edit = BlockEdit {
            x: 10,
            y: height,
            z: 10,
            block: Block::Stone,
        };
        WorldSave::load(&path, DEFAULT_SEED)
            .record(edit)
            .expect("写盘应成功");
        assert!(path.exists());

        let mut restored = world_with(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let loaded = WorldSave::load(&path, DEFAULT_SEED);
        loaded.apply_chunk(ChunkPos::new(0, 0, 0), &mut restored);
        assert_eq!(restored.block_at(10, height, 10), Block::Stone);
        assert_eq!(loaded.total_edits(), 1);

        remove_all(&path);
    }

    #[test]
    fn repeated_edits_on_one_cell_keep_only_the_last_block() {
        let path = temp_path("dedupe");
        remove_all(&path);

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        let first = BlockEdit {
            x: 1,
            y: 2,
            z: 3,
            block: Block::Stone,
        };
        save.record(first).expect("写盘应成功");
        let last = BlockEdit {
            block: Block::Air,
            ..first
        };
        save.record(last).expect("写盘应成功");

        assert_eq!(save.total_edits(), 1);
        assert!(!save.unsaved);
        let text = fs::read_to_string(&path).expect("读取临时存档失败");
        assert!(text.contains("\"air\""));

        remove_all(&path);
    }

    #[test]
    fn legacy_v08_edits_migrate_to_chunk_records_without_rewriting_first() {
        let path = temp_path("legacy");
        remove_all(&path);
        // v0.8 存档：version 1，世界坐标平铺，包含负坐标。
        let legacy = r#"{
            "version": 1,
            "seed": 24301,
            "edits": [
                {"x": -17, "y": 5, "z": 20, "block": "stone"},
                {"x": 17, "y": 5, "z": 20, "block": "stone"}
            ]
        }"#;
        fs::write(&path, legacy).expect("写入旧存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(save.total_edits(), 2);
        assert_eq!(
            fs::read_to_string(&path).expect("读取旧存档失败"),
            legacy,
            "加载旧存档时不应改写原始文件"
        );

        let mut world = world_with(&[ChunkPos::new(-2, 0, 1), ChunkPos::new(1, 0, 1)]);
        save.apply_chunk(ChunkPos::new(-2, 0, 1), &mut world);
        save.apply_chunk(ChunkPos::new(1, 0, 1), &mut world);
        assert_eq!(world.block_at(-17, 5, 20), Block::Stone);
        assert_eq!(world.block_at(17, 5, 20), Block::Stone);

        remove_all(&path);
    }

    #[test]
    fn migration_keeps_the_original_as_backup_after_the_first_write() {
        let path = temp_path("migration");
        remove_all(&path);
        let legacy = r#"{"version":1,"seed":24301,"edits":[{"x":5,"y":5,"z":5,"block":"stone"}]}"#;
        fs::write(&path, legacy).expect("写入旧存档失败");

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.record(BlockEdit {
            x: 6,
            y: 6,
            z: 6,
            block: Block::Dirt,
        })
        .expect("写盘应成功");

        let text = fs::read_to_string(&path).expect("读取新存档失败");
        assert!(text.contains("\"version\": 7"));
        assert!(text.contains("chunk_edits"));
        assert_eq!(
            fs::read_to_string(suffixed(&path, ".bak")).expect("读取备份失败"),
            legacy
        );

        remove_all(&path);
    }

    #[test]
    fn negative_coordinate_edits_survive_restart() {
        let path = temp_path("negative");
        remove_all(&path);

        let pos = ChunkPos::new(-2, 1, -3);
        let edit = BlockEdit {
            x: -17,
            y: 20,
            z: -33,
            block: Block::Dirt,
        };
        WorldSave::load(&path, DEFAULT_SEED)
            .record(edit)
            .expect("写盘应成功");

        let mut restored = world_with(&[pos]);
        let loaded = WorldSave::load(&path, DEFAULT_SEED);
        loaded.apply_chunk(pos, &mut restored);
        assert_eq!(restored.block_at(-17, 20, -33), Block::Dirt);

        remove_all(&path);
    }

    #[test]
    fn a_chunk_can_be_unloaded_and_restored_from_the_save() {
        let path = temp_path("unload");
        remove_all(&path);

        let pos = ChunkPos::new(1, 0, 1);
        let edit = BlockEdit {
            x: 17,
            y: 3,
            z: 17,
            block: Block::Stone,
        };
        WorldSave::load(&path, DEFAULT_SEED)
            .record(edit)
            .expect("写盘应成功");

        // 模拟运行时：加载区块、应用改动、卸载、再重新加载。
        let mut world = world_with(&[pos]);
        let save = WorldSave::load(&path, DEFAULT_SEED);
        save.apply_chunk(pos, &mut world);
        assert_eq!(world.block_at(17, 3, 17), Block::Stone);

        world.remove_chunk(pos);
        assert_eq!(world.block_at(17, 3, 17), Block::Air);

        let chunk = world.generate_chunk(pos);
        world.insert_chunk(chunk);
        save.apply_chunk(pos, &mut world);
        assert_eq!(world.block_at(17, 3, 17), Block::Stone);

        remove_all(&path);
    }

    #[test]
    fn broken_save_file_is_ignored() {
        let path = temp_path("broken");
        remove_all(&path);
        fs::write(&path, "not json").expect("写入临时存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(save.total_edits(), 0);

        remove_all(&path);
    }

    #[test]
    fn corrupted_primary_falls_back_to_backup() {
        let path = temp_path("backup");
        remove_all(&path);

        let first = BlockEdit {
            x: 5,
            y: 6,
            z: 5,
            block: Block::Stone,
        };
        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.record(first).expect("写盘应成功");
        save.record(BlockEdit {
            x: 6,
            y: 6,
            z: 6,
            block: Block::Dirt,
        })
        .expect("写盘应成功");
        assert!(suffixed(&path, ".bak").exists());

        fs::write(&path, "not json").expect("破坏主存档失败");

        let loaded = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(loaded.total_edits(), 1);

        remove_all(&path);
    }

    #[test]
    fn fallback_backup_survives_the_first_write_after_corruption() {
        let path = temp_path("preserve");
        remove_all(&path);

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.record(BlockEdit {
            x: 1,
            y: 1,
            z: 1,
            block: Block::Stone,
        })
        .expect("写盘应成功");
        save.record(BlockEdit {
            x: 2,
            y: 1,
            z: 2,
            block: Block::Dirt,
        })
        .expect("写盘应成功");
        assert_eq!(saved_edit_count(&suffixed(&path, ".bak")), 1);

        fs::write(&path, "not json").expect("破坏主存档失败");

        // 主存档损坏后从备份恢复；此时写盘不能把损坏的主存档滚动成 .bak。
        let mut recovered = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(recovered.total_edits(), 1);
        recovered
            .record(BlockEdit {
                x: 3,
                y: 1,
                z: 3,
                block: Block::Stone,
            })
            .expect("写盘应成功");
        assert_eq!(
            saved_edit_count(&suffixed(&path, ".bak")),
            1,
            "唯一可用的备份被损坏的主存档覆盖"
        );
        assert_eq!(saved_edit_count(&path), 2);

        // 主存档恢复有效后，下一次写盘重新滚动备份。
        recovered
            .record(BlockEdit {
                x: 4,
                y: 1,
                z: 4,
                block: Block::Stone,
            })
            .expect("写盘应成功");
        assert_eq!(saved_edit_count(&suffixed(&path, ".bak")), 2);
        assert_eq!(saved_edit_count(&path), 3);

        remove_all(&path);
    }

    #[test]
    fn edits_from_another_seed_are_ignored() {
        let path = temp_path("seed");
        remove_all(&path);

        WorldSave::load(&path, DEFAULT_SEED)
            .record(BlockEdit {
                x: 10,
                y: 10,
                z: 10,
                block: Block::Stone,
            })
            .expect("写盘应成功");

        let loaded = WorldSave::load(&path, DEFAULT_SEED + 1);
        assert_eq!(loaded.total_edits(), 0);

        remove_all(&path);
    }

    #[test]
    fn edits_outside_the_vertical_range_are_rejected() {
        let path = temp_path("vertical");
        remove_all(&path);
        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        let result = save.record(BlockEdit {
            x: 0,
            y: -1,
            z: 0,
            block: Block::Stone,
        });
        assert!(result.is_err());
        assert_eq!(save.total_edits(), 0);
        remove_all(&path);
    }

    #[test]
    fn a_v011_save_loads_with_an_empty_inventory_and_no_drops() {
        let path = temp_path("v011");
        remove_all(&path);
        let legacy = r#"{
            "version": 2,
            "seed": 24301,
            "edits": [],
            "chunk_edits": [
                {"pos": [0, 0, 0], "edits": [{"x": 2, "y": 3, "z": 4, "block": "stone"}]}
            ]
        }"#;
        fs::write(&path, legacy).expect("写入 v2 存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(save.total_edits(), 1);
        assert_eq!(save.total_drops(), 0);
        assert!(save.player().is_none(), "旧档必须按空背包启动");

        remove_all(&path);
    }

    #[test]
    fn player_inventory_round_trips_through_the_save() {
        let path = temp_path("player");
        remove_all(&path);
        let inventory = SavedInventory {
            slots: vec![
                Some(SavedSlot {
                    item: 1,
                    count: 5,
                    durability: None,
                }),
                None,
                Some(SavedSlot {
                    item: 3,
                    count: 2,
                    durability: None,
                }),
            ],
            selected_hotbar: 4,
        };

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.set_player_state(
            inventory.clone(),
            SavedSurvival {
                max_health: 20,
                current_health: 20,
            },
            SavedEquipment::default(),
        )
        .expect("保存玩家背包应成功");

        let loaded = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(loaded.player(), Some(&inventory));
        remove_all(&path);
    }

    #[test]
    fn chunk_drops_round_trip_with_the_chunk_data() {
        let path = temp_path("drops");
        remove_all(&path);
        let pos = ChunkPos::new(0, 0, 0);
        let records = vec![DroppedItemRecord {
            x: 2.5,
            y: 5.0,
            z: 3.5,
            item: 1,
            count: 2,
        }];

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.record_chunk_drops(pos, records.clone());
        save.ensure_persisted().expect("写盘应成功");
        assert_eq!(save.total_drops(), 1);

        let loaded = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(loaded.chunk_drops(pos), records.as_slice());
        assert_eq!(loaded.chunk_drops(ChunkPos::new(1, 0, 0)), &[]);

        remove_all(&path);
    }

    #[test]
    fn drops_outside_their_chunk_are_ignored() {
        let path = temp_path("drop-chunk");
        remove_all(&path);
        let text = r#"{
            "version": 3,
            "seed": 24301,
            "chunk_edits": [
                {"pos": [0, 0, 0], "edits": [], "drops": [
                    {"x": 20.5, "y": 5.0, "z": 2.5, "item": 1, "count": 1}
                ]}
            ]
        }"#;
        fs::write(&path, text).expect("写入存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(save.total_drops(), 0, "错位的掉落物记录必须被忽略");
        remove_all(&path);
    }

    #[test]
    fn a_v012_save_restores_tools_and_upgrades_to_current_version() {
        let path = temp_path("v012");
        remove_all(&path);
        let legacy = r#"{
            "version": 3,
            "seed": 24301,
            "player": {
                "slots": [{"item": 5, "count": 1}, {"item": 1, "count": 4}],
                "selected_hotbar": 0
            }
        }"#;
        fs::write(&path, legacy).expect("写入 v3 存档失败");

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        let player = save.player().expect("v3 存档必须保留玩家背包");
        assert_eq!(
            player.slots[0].and_then(|slot| slot.durability),
            None,
            "v3 槽位本身不含耐久字段"
        );

        let (inventory, selected) = crate::item::Inventory::from_saved(player);
        assert_eq!(selected, 0);
        assert_eq!(
            inventory.slot(0).and_then(|stack| stack.durability()),
            Some(132),
            "石镐按满耐久恢复"
        );
        assert_eq!(
            inventory.slot(1).and_then(|stack| stack.durability()),
            None,
            "普通物品没有耐久"
        );

        // 首次写盘把格式升级到当前版本，且保留玩家数据。
        save.record(BlockEdit {
            x: 2,
            y: 5,
            z: 2,
            block: Block::Stone,
        })
        .expect("写盘应成功");
        let text = fs::read_to_string(&path).expect("读取新存档失败");
        let data: SaveFile = serde_json::from_str(&text).expect("解析新存档失败");
        assert_eq!(data.version, 7);
        assert!(data.survival.is_none(), "旧档升级时不应伪造生存数据");

        remove_all(&path);
    }

    #[test]
    fn survival_round_trips_with_the_player_state() {
        let path = temp_path("survival");
        remove_all(&path);

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        let inventory = crate::item::Inventory::new().to_saved(0);
        save.set_player_state(
            inventory,
            SavedSurvival {
                max_health: 20,
                current_health: 12,
            },
            SavedEquipment::default(),
        )
        .expect("写盘应成功");

        let reloaded = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(
            reloaded.survival(),
            Some(SavedSurvival {
                max_health: 20,
                current_health: 12,
            })
        );
        remove_all(&path);
    }

    #[test]
    fn equipment_round_trips_with_durability() {
        use crate::equipment::Equipment;
        use crate::item::inventory::SavedSlot;
        use crate::item::{ItemId, ItemStack};

        let path = temp_path("equipment");
        remove_all(&path);

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        let inventory = crate::item::Inventory::new().to_saved(0);
        let equipment = SavedEquipment {
            slots: vec![
                Some(SavedSlot {
                    item: ItemId::IRON_HELMET.raw(),
                    count: 1,
                    durability: Some(120),
                }),
                None,
                Some(SavedSlot {
                    item: ItemId::DIAMOND_LEGGINGS.raw(),
                    count: 1,
                    durability: None,
                }),
                Some(SavedSlot {
                    item: ItemId::LEATHER_BOOTS.raw(),
                    count: 1,
                    durability: Some(0),
                }),
            ],
        };
        save.set_player_state(
            inventory,
            SavedSurvival {
                max_health: 20,
                current_health: 20,
            },
            equipment,
        )
        .expect("写盘应成功");

        let reloaded = WorldSave::load(&path, DEFAULT_SEED);
        let saved = reloaded.equipment().expect("应保存装备");
        let restored = Equipment::from_saved(saved);
        assert_eq!(
            restored.get(crate::equipment::EquipmentSlot::Head),
            ItemStack::new(ItemId::IRON_HELMET, 1).map(|stack| stack.with_durability(120))
        );
        assert_eq!(
            restored.get(crate::equipment::EquipmentSlot::Chest),
            None,
            "空槽保持为空"
        );
        assert_eq!(
            restored.get(crate::equipment::EquipmentSlot::Feet),
            ItemStack::new(ItemId::LEATHER_BOOTS, 1).map(|stack| stack.with_durability(0)),
            "耐久归零的护甲同样保留"
        );

        remove_all(&path);
    }

    #[test]
    fn a_save_without_survival_stays_backwards_compatible() {
        let path = temp_path("no-survival");
        remove_all(&path);
        let text = r#"{
            "version": 4,
            "seed": 24301,
            "player": {"slots": [], "selected_hotbar": 0}
        }"#;
        fs::write(&path, text).expect("写入旧档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert!(save.survival().is_none(), "旧档缺省生存状态");
        assert!(save.equipment().is_none(), "旧档缺省装备");
        assert!(save.player().is_some(), "旧档仍应保留背包");
        remove_all(&path);
    }

    #[test]
    fn zero_health_in_a_save_is_recovered_to_full_health() {
        let path = temp_path("zero-health");
        remove_all(&path);
        let text = r#"{
            "version": 5,
            "seed": 24301,
            "player": {"slots": [], "selected_hotbar": 0},
            "survival": {"max_health": 20, "current_health": 0}
        }"#;
        fs::write(&path, text).expect("写入存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(
            save.survival(),
            Some(SavedSurvival {
                max_health: 20,
                current_health: 20,
            }),
            "崩溃时的死亡中间态必须恢复满血"
        );
        remove_all(&path);
    }

    #[test]
    fn invalid_survival_values_are_sanitized() {
        let path = temp_path("bad-survival");
        remove_all(&path);
        let text = r#"{
            "version": 5,
            "seed": 24301,
            "player": {"slots": [], "selected_hotbar": 0},
            "survival": {"max_health": 0, "current_health": 30}
        }"#;
        fs::write(&path, text).expect("写入存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(
            save.survival(),
            Some(SavedSurvival {
                max_health: 20,
                current_health: 20,
            }),
            "非法最大值回落默认，当前值钳制到最大值"
        );
        remove_all(&path);
    }

    fn sample_entity(id: u64) -> EntityRecord {
        EntityRecord {
            id,
            kind: crate::entity::EntityTypeId::PIG.raw(),
            x: 2.5,
            y: 10.0,
            z: 3.5,
            vx: 0.4,
            vy: 0.0,
            vz: -0.2,
            health: 8,
            state: 1,
            state_timer: 1.5,
            anchor_x: 2.5,
            anchor_z: 3.5,
        }
    }

    #[test]
    fn entity_records_round_trip_with_the_chunk_data() {
        let path = temp_path("entities");
        remove_all(&path);
        let pos = ChunkPos::new(0, 0, 0);
        let records = vec![sample_entity(5), sample_entity(11)];

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.record_chunk_entities(pos, records.clone());
        save.ensure_persisted().expect("写盘应成功");
        assert_eq!(save.total_entities(), 2);

        let loaded = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(loaded.chunk_entities(pos), records.as_slice());
        assert_eq!(loaded.chunk_entities(ChunkPos::new(1, 0, 0)), &[]);

        remove_all(&path);
    }

    #[test]
    fn populated_chunks_persist_and_normalize_vertical_index() {
        let path = temp_path("populated");
        remove_all(&path);

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.record_populated_chunk(ChunkPos::new(1, 2, -3));
        assert!(save.is_chunk_populated(ChunkPos::new(1, 0, -3)));
        save.ensure_persisted().expect("写盘应成功");

        let loaded = WorldSave::load(&path, DEFAULT_SEED);
        assert!(loaded.is_chunk_populated(ChunkPos::new(1, 0, -3)));
        assert!(!loaded.is_chunk_populated(ChunkPos::new(2, 0, -3)));
        assert_eq!(loaded.populated_chunk_count(), 1);

        remove_all(&path);
    }

    #[test]
    fn a_v014_save_loads_with_no_entities_or_populated_chunks() {
        let path = temp_path("v014-entities");
        remove_all(&path);
        let legacy = r#"{
            "version": 5,
            "seed": 24301,
            "chunk_edits": [
                {"pos": [0, 0, 0], "edits": [{"x": 2, "y": 3, "z": 4, "block": "stone"}], "drops": []}
            ]
        }"#;
        fs::write(&path, legacy).expect("写入 v5 存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(save.total_edits(), 1);
        assert_eq!(save.total_entities(), 0, "旧档不应含生物");
        assert_eq!(save.populated_chunk_count(), 0, "旧档不应含生成记录");
        assert_eq!(save.chunk_entities(ChunkPos::new(0, 0, 0)), &[]);

        remove_all(&path);
    }

    #[test]
    fn invalid_entity_records_are_ignored_on_load() {
        let path = temp_path("bad-entities");
        remove_all(&path);
        let text = r#"{
            "version": 6,
            "seed": 24301,
            "chunk_edits": [
                {"pos": [0, 0, 0], "edits": [], "drops": [], "entities": [
                    {"id": 1, "type": 1, "x": 20.5, "y": 10.0, "z": 3.5, "health": 8},
                    {"id": 2, "type": 99, "x": 2.5, "y": 10.0, "z": 3.5, "health": 8},
                    {"id": 3, "type": 1, "x": 2.5, "y": 10.0, "z": 3.5, "health": 8, "state": 4},
                    {"id": 4, "type": 1, "x": 2.5, "y": 10.0, "z": 3.5, "health": 0}
                ]}
            ]
        }"#;
        fs::write(&path, text).expect("写入存档失败");

        let save = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(
            save.total_entities(),
            0,
            "错位、未知类型、死亡与零血实体都应被忽略"
        );
        remove_all(&path);
    }

    #[test]
    fn a_v015_save_preserves_entities_after_reload() {
        let path = temp_path("v015-reload");
        remove_all(&path);

        let mut save = WorldSave::load(&path, DEFAULT_SEED);
        save.record_chunk_entities(ChunkPos::new(0, 0, 0), vec![sample_entity(9)]);
        save.record_populated_chunk(ChunkPos::new(0, 0, 0));
        save.set_player_state(
            crate::item::Inventory::new().to_saved(0),
            SavedSurvival {
                max_health: 20,
                current_health: 20,
            },
            SavedEquipment::default(),
        )
        .expect("写盘应成功");

        let reloaded = WorldSave::load(&path, DEFAULT_SEED);
        assert_eq!(reloaded.total_entities(), 1);
        assert!(reloaded.is_chunk_populated(ChunkPos::new(0, 0, 0)));
        assert_eq!(reloaded.chunk_entities(ChunkPos::new(0, 0, 0))[0].id, 9);

        remove_all(&path);
    }
}

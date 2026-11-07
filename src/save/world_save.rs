//! 世界存档：按区块记录玩家对程序化地形的改动，兼容 v0.8 的世界坐标旧档。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::world::{Block, ChunkPos, World, CHUNK_SIZE, DEFAULT_SEED, WORLD_HEIGHT};

/// 默认存档位置，相对进程工作目录。
pub const SAVE_PATH: &str = "saves/world.json";
/// 当前存档格式版本：按区块组织改动数据。
const SAVE_VERSION: u32 = 2;
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

/// 一个区块的全部改动。
#[derive(Serialize, Deserialize)]
struct ChunkEdits {
    pos: [i32; 3],
    edits: Vec<LocalEdit>,
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
    /// 早期写盘失败时保留内存数据，下次写入或卸载前重试。
    unsaved: bool,
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
            unsaved: false,
        };

        for (index, candidate) in [save.path.clone(), save.backup_path()]
            .into_iter()
            .enumerate()
        {
            let text = match fs::read_to_string(&candidate) {
                Ok(text) => text,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => {
                    warn!(error = %err, path = %candidate.display(), "读取存档失败，尝试下一个来源");
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
                    let source = if index == 0 {
                        "主存档"
                    } else {
                        "备份存档"
                    };
                    warn!(
                        path = %candidate.display(),
                        error = %format!("{err:#}"),
                        "{source}不可用，尝试下一个来源"
                    );
                }
            }
        }
        save
    }

    /// 解析存档文本、校验版本与种子，并把改动载入内存。
    fn apply_text(&mut self, text: &str) -> Result<()> {
        let data: SaveFile = serde_json::from_str(text).context("解析存档失败")?;
        if data.version != SAVE_VERSION && data.version != LEGACY_SAVE_VERSION {
            bail!(
                "存档版本 {} 与当前支持的版本 {} / {} 不一致",
                data.version,
                SAVE_VERSION,
                LEGACY_SAVE_VERSION
            );
        }
        if data.seed != self.seed {
            bail!("存档种子 {} 与世界种子 {} 不一致", data.seed, self.seed);
        }

        let mut edits: HashMap<ChunkPos, HashMap<(i32, i32, i32), Block>> = HashMap::new();
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
            }
        }

        self.edits = edits;
        Ok(())
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
        let mut chunk_positions: Vec<ChunkPos> = self.edits.keys().copied().collect();
        chunk_positions.sort_by_key(|pos| (pos.x, pos.y, pos.z));

        let mut chunk_edits = Vec::with_capacity(chunk_positions.len());
        let mut world_edits = Vec::with_capacity(self.total_edits());
        for pos in chunk_positions {
            let Some(blocks) = self.edits.get(&pos) else {
                continue;
            };
            let mut keys: Vec<(i32, i32, i32)> = blocks.keys().copied().collect();
            keys.sort_unstable();

            let mut local_edits = Vec::with_capacity(keys.len());
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
            chunk_edits.push(ChunkEdits {
                pos: [pos.x, pos.y, pos.z],
                edits: local_edits,
            });
        }
        world_edits.sort_by_key(|edit| (edit.x, edit.y, edit.z));

        let data = SaveFile {
            version: SAVE_VERSION,
            seed: self.seed,
            edits: world_edits,
            chunk_edits,
        };
        let json = serde_json::to_string_pretty(&data).context("序列化存档失败")?;

        // 先写临时文件再改名，避免写到一半退出留下残缺 JSON；旧文件滚动为备份。
        let temp = self.temp_path();
        fs::write(&temp, &json).with_context(|| format!("写入临时存档失败: {}", temp.display()))?;

        if self.path.exists() {
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
        assert!(text.contains("\"version\": 2"));
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
}

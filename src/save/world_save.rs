//! 世界存档：以 JSON 记录相对程序化地形的方块改动，启动时重新盖回世界。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::world::{Block, World, DEFAULT_SEED};

/// 默认存档位置，相对进程工作目录。
pub const SAVE_PATH: &str = "saves/world.json";
/// 当前存档格式版本；版本不一致时忽略旧文件，避免读取错误的数据。
const SAVE_VERSION: u32 = 1;

/// 一次方块改动：世界坐标与改动后的方块。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockEdit {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: Block,
}

/// 玩家改动的内存记录，并与存档文件保持同步。
pub struct WorldSave {
    path: PathBuf,
    seed: u32,
    /// 以坐标为键，同一格只保留最后一次改动。
    edits: HashMap<(i32, i32, i32), BlockEdit>,
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    version: u32,
    /// 存档对应的世界种子；旧版存档缺少该字段时按默认种子处理。
    #[serde(default = "default_seed")]
    seed: u32,
    edits: Vec<BlockEdit>,
}

fn default_seed() -> u32 {
    DEFAULT_SEED
}

impl WorldSave {
    /// 默认存档路径。
    pub fn default_path() -> PathBuf {
        PathBuf::from(SAVE_PATH)
    }

    /// 读取存档并应用到 `world`；主文件不可用时回退到备份，均不可用时按新世界启动。
    pub fn load(path: impl Into<PathBuf>, world: &mut World) -> Self {
        let path = path.into();
        let mut save = Self {
            path,
            seed: world.seed(),
            edits: HashMap::new(),
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

            match save.apply_text(&text, world) {
                Ok(edits) => {
                    info!(path = %candidate.display(), edits = edits.len(), "世界存档已读取");
                    save.edits = edits;
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

    /// 解析存档文本、校验版本与种子，并把改动盖回世界。
    fn apply_text(
        &self,
        text: &str,
        world: &mut World,
    ) -> Result<HashMap<(i32, i32, i32), BlockEdit>> {
        let data: SaveFile = serde_json::from_str(text).context("解析存档失败")?;
        if data.version != SAVE_VERSION {
            bail!(
                "存档版本 {} 与当前版本 {} 不一致",
                data.version,
                SAVE_VERSION
            );
        }
        if data.seed != self.seed {
            bail!("存档种子 {} 与世界种子 {} 不一致", data.seed, self.seed);
        }

        let mut edits = HashMap::with_capacity(data.edits.len());
        for edit in data.edits {
            if !world.contains(edit.x, edit.y, edit.z) {
                warn!(x = edit.x, y = edit.y, z = edit.z, "忽略越界的存档改动");
                continue;
            }
            world.set_block(edit.x, edit.y, edit.z, edit.block);
            edits.insert((edit.x, edit.y, edit.z), edit);
        }
        Ok(edits)
    }

    /// 记录一次方块改动：同一坐标只保留最后一次，并立即写盘。
    pub fn record(&mut self, edit: BlockEdit) -> Result<()> {
        self.edits.insert((edit.x, edit.y, edit.z), edit);
        self.persist()
    }

    /// 把当前改动写入存档文件。
    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("创建存档目录失败: {}", parent.display()))?;
            }
        }

        // 固定输出顺序，便于人工查看与版本比对。
        let mut edits: Vec<BlockEdit> = self.edits.values().copied().collect();
        edits.sort_by_key(|edit| (edit.x, edit.y, edit.z));

        let data = SaveFile {
            version: SAVE_VERSION,
            seed: self.seed,
            edits,
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

    #[test]
    fn edits_round_trip_through_the_save_file() {
        let path = temp_path("roundtrip");
        remove_all(&path);

        let mut world = World::default();
        let height = world.height_at(10, 10);
        let edit = BlockEdit {
            x: 10,
            y: height,
            z: 10,
            block: Block::Stone,
        };
        WorldSave::load(&path, &mut world)
            .record(edit)
            .expect("写盘应成功");
        assert!(path.exists());

        let mut restored = World::default();
        let loaded = WorldSave::load(&path, &mut restored);
        assert_eq!(restored.block_at(10, height, 10), Block::Stone);
        assert_eq!(loaded.edits.get(&(10, height, 10)), Some(&edit));

        remove_all(&path);
    }

    #[test]
    fn repeated_edits_on_one_cell_keep_only_the_last_block() {
        let path = temp_path("dedupe");
        remove_all(&path);

        let mut save = WorldSave::load(&path, &mut World::default());
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

        assert_eq!(save.edits.len(), 1);
        assert_eq!(save.edits.get(&(1, 2, 3)), Some(&last));
        let text = fs::read_to_string(&path).expect("读取临时存档失败");
        assert!(text.contains("\"air\""));

        remove_all(&path);
    }

    #[test]
    fn broken_save_file_is_ignored() {
        let path = temp_path("broken");
        remove_all(&path);
        fs::write(&path, "not json").expect("写入临时存档失败");

        let mut world = World::default();
        let save = WorldSave::load(&path, &mut world);

        assert!(save.edits.is_empty());
        // 世界保持程序化生成的原样。
        let height = world.height_at(10, 10);
        assert_eq!(world.block_at(10, height - 1, 10), Block::Grass);

        remove_all(&path);
    }

    #[test]
    fn corrupted_primary_falls_back_to_backup() {
        let path = temp_path("backup");
        remove_all(&path);

        let mut world = World::default();
        let height = world.height_at(5, 5);
        let first = BlockEdit {
            x: 5,
            y: height,
            z: 5,
            block: Block::Stone,
        };
        let mut save = WorldSave::load(&path, &mut world);
        save.record(first).expect("写盘应成功");
        // 第二次写盘会把第一次的存档滚动成备份。
        save.record(BlockEdit {
            x: 6,
            y: height,
            z: 6,
            block: Block::Dirt,
        })
        .expect("写盘应成功");
        assert!(suffixed(&path, ".bak").exists());

        fs::write(&path, "not json").expect("破坏主存档失败");

        let mut restored = World::default();
        let loaded = WorldSave::load(&path, &mut restored);
        assert_eq!(loaded.edits.get(&(5, height, 5)), Some(&first));
        assert_eq!(restored.block_at(5, height, 5), Block::Stone);

        remove_all(&path);
    }

    #[test]
    fn edits_from_another_seed_are_ignored() {
        let path = temp_path("seed");
        remove_all(&path);

        let mut writer = World::new(DEFAULT_SEED);
        let height = writer.height_at(10, 10);
        WorldSave::load(&path, &mut writer)
            .record(BlockEdit {
                x: 10,
                y: height,
                z: 10,
                block: Block::Stone,
            })
            .expect("写盘应成功");

        let mut other = World::new(DEFAULT_SEED + 1);
        let loaded = WorldSave::load(&path, &mut other);
        assert!(loaded.edits.is_empty());

        remove_all(&path);
    }
}

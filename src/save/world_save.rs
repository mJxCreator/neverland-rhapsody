//! 世界存档：以 JSON 记录相对程序化地形的方块改动，启动时重新盖回世界。

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::world::{Block, World};

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
    edits: Vec<BlockEdit>,
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    version: u32,
    edits: Vec<BlockEdit>,
}

impl WorldSave {
    /// 默认存档路径。
    pub fn default_path() -> PathBuf {
        PathBuf::from(SAVE_PATH)
    }

    /// 读取存档并应用到 `world`；文件缺失、损坏或版本不符时按新世界启动。
    pub fn load(path: impl Into<PathBuf>, world: &mut World) -> Self {
        let path = path.into();
        let mut save = Self {
            path,
            edits: Vec::new(),
        };

        let text = match fs::read_to_string(&save.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return save,
            Err(err) => {
                warn!(error = %err, path = %save.path.display(), "读取存档失败，按新世界启动");
                return save;
            }
        };

        match serde_json::from_str::<SaveFile>(&text) {
            Ok(data) if data.version == SAVE_VERSION => {
                for edit in &data.edits {
                    world.set_block(edit.x, edit.y, edit.z, edit.block);
                }
                info!(edits = data.edits.len(), "世界存档已读取");
                save.edits = data.edits;
            }
            Ok(data) => {
                warn!(
                    version = data.version,
                    expected = SAVE_VERSION,
                    "存档版本不匹配，按新世界启动"
                );
            }
            Err(err) => {
                warn!(error = %err, "存档解析失败，按新世界启动");
            }
        }
        save
    }

    /// 记录一次方块改动：同一坐标只保留最后一次，并立即写盘。
    pub fn record(&mut self, edit: BlockEdit) -> Result<()> {
        self.edits
            .retain(|existing| (existing.x, existing.y, existing.z) != (edit.x, edit.y, edit.z));
        self.edits.push(edit);
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

        let data = SaveFile {
            version: SAVE_VERSION,
            edits: self.edits.clone(),
        };
        let json = serde_json::to_string_pretty(&data).context("序列化存档失败")?;
        fs::write(&self.path, json)
            .with_context(|| format!("写入存档失败: {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    /// 为并行测试生成互不冲突的临时文件路径。
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nr-save-{}-{name}.json", std::process::id()))
    }

    #[test]
    fn edits_round_trip_through_the_save_file() {
        let path = temp_path("roundtrip");
        let _ = fs::remove_file(&path);

        let mut world = World::default();
        let height = world.height_at(10, 10);
        let edit = BlockEdit {
            x: 10,
            y: height,
            z: 10,
            block: Block::Stone,
        };
        world.set_block(edit.x, edit.y, edit.z, edit.block);
        WorldSave::load(&path, &mut world)
            .record(edit)
            .expect("写盘应成功");
        assert!(path.exists());

        let mut restored = World::default();
        let loaded = WorldSave::load(&path, &mut restored);
        assert_eq!(restored.block_at(10, height, 10), Block::Stone);
        assert_eq!(loaded.edits.as_slice(), &[edit]);

        fs::remove_file(&path).expect("清理临时存档失败");
    }

    #[test]
    fn repeated_edits_on_one_cell_keep_only_the_last_block() {
        let path = temp_path("dedupe");
        let _ = fs::remove_file(&path);

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

        assert_eq!(save.edits.as_slice(), &[last]);
        let text = fs::read_to_string(&path).expect("读取临时存档失败");
        assert!(text.contains("\"air\""));

        fs::remove_file(&path).expect("清理临时存档失败");
    }

    #[test]
    fn broken_save_file_is_ignored() {
        let path = temp_path("broken");
        fs::write(&path, "not json").expect("写入临时存档失败");

        let mut world = World::default();
        let save = WorldSave::load(&path, &mut world);

        assert!(save.edits.is_empty());
        // 世界保持程序化生成的原样。
        let height = world.height_at(10, 10);
        assert_eq!(world.block_at(10, height - 1, 10), Block::Grass);

        fs::remove_file(&path).expect("清理临时存档失败");
    }
}

//! 角色存档：把玩家自身的状态从世界存档中独立出来。
//!
//! 一个账号可以有多个角色，每个角色都能进入任意世界。角色快照（背包、生命、
//! 装备、饥饿与体力）与某个世界无关，退出世界时冻结，重新进入任意世界时原样恢复；
//! 世界侧只保存地形改动、掉落物、生物、昼夜时间与营火。两者分开存放，对应
//! `history-docs/规则/玩家状态与时间封印设计理念.md` 中的“角色快照 + 世界存档”。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::equipment::SavedEquipment;
use crate::item::SavedInventory;
use crate::player::SavedSurvival;
use crate::survival::SavedVitals;

/// 角色存档格式版本。
pub const CHARACTER_SAVE_VERSION: u32 = 1;

fn default_version() -> u32 {
    CHARACTER_SAVE_VERSION
}

/// 单个角色的完整快照；缺失字段由游戏层回退到新角色默认值。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharacterSave {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    /// 背包与快捷栏选择；新角色尚未进入过世界时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<SavedInventory>,
    /// 生命值与死亡状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub survival: Option<SavedSurvival>,
    /// 四槽护甲。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment: Option<SavedEquipment>,
    /// 饥饿、体力与床复活点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vitals: Option<SavedVitals>,
}

impl CharacterSave {
    /// 新建一个尚未进入世界的角色文件。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: CHARACTER_SAVE_VERSION,
            name: name.into(),
            inventory: None,
            survival: None,
            equipment: None,
            vitals: None,
        }
    }

    /// 读取角色存档；文件不存在或解析失败时返回 `None`，由调用方跳过该角色。
    pub fn load(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                warn!(path = %path.display(), error = %error, "读取角色存档失败");
                return None;
            }
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(mut save) => {
                if save.version > CHARACTER_SAVE_VERSION {
                    warn!(
                        path = %path.display(),
                        version = save.version,
                        "角色存档版本高于当前支持版本，按当前格式读取"
                    );
                }
                if save.name.trim().is_empty() {
                    save.name = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("CHARACTER")
                        .to_owned();
                }
                Some(save)
            }
            Err(error) => {
                warn!(path = %path.display(), error = %error, "角色存档解析失败");
                None
            }
        }
    }

    /// 原子写入角色存档：先写临时文件，再替换主文件。
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("创建角色目录失败: {}", parent.display()))?;
            }
        }
        let json = serde_json::to_string_pretty(self).context("序列化角色存档失败")?;
        let temp = suffixed(path, ".tmp");
        fs::write(&temp, &json)
            .with_context(|| format!("写入角色临时存档失败: {}", temp.display()))?;
        if let Err(error) = fs::rename(&temp, path) {
            warn!(
                error = %error,
                path = %path.display(),
                "角色存档改名失败，改为直接写入"
            );
            let _ = fs::remove_file(&temp);
            fs::write(path, &json)
                .with_context(|| format!("写入角色存档失败: {}", path.display()))?;
        }
        info!(path = %path.display(), "角色存档已写入");
        Ok(())
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("系统时间有效")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "neverland_character_{tag}_{}_{unique}.json",
            std::process::id()
        ))
    }

    #[test]
    fn a_character_round_trips_through_disk() {
        let path = temp_path("round_trip");
        let mut save = CharacterSave::new("CHARACTER 1");
        save.inventory = Some(SavedInventory::default());
        save.survival = Some(SavedSurvival::default());
        save.equipment = Some(SavedEquipment::default());
        save.vitals = Some(SavedVitals::default());
        save.save(&path).expect("写入角色存档");

        let loaded = CharacterSave::load(&path).expect("读取角色存档");
        assert_eq!(loaded.name, "CHARACTER 1");
        assert_eq!(loaded.version, CHARACTER_SAVE_VERSION);
        assert!(loaded.inventory.is_some());
        assert!(loaded.vitals.is_some());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_missing_or_broken_character_file_is_ignored() {
        let path = temp_path("broken");
        assert!(CharacterSave::load(&path).is_none());
        fs::write(&path, "{ not json").expect("写入损坏文件");
        assert!(CharacterSave::load(&path).is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn an_empty_name_falls_back_to_the_file_stem() {
        let path = temp_path("unnamed");
        fs::write(&path, r#"{"version":1,"name":""}"#).expect("写入无名角色");
        let loaded = CharacterSave::load(&path).expect("读取角色存档");
        assert!(!loaded.name.is_empty());
        assert_ne!(loaded.name, "");
        let _ = fs::remove_file(&path);
    }
}

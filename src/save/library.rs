//! 存档库：管理 `saves/characters/` 与 `saves/worlds/` 下的多角色、多世界文件。
//!
//! 账号本身不对应文件，角色与世界都是独立文件：
//! - 角色：`saves/characters/character_<n>.json`
//! - 世界：`saves/worlds/world_<n>/world.json` + `meta.json`（名称与种子）
//!
//! 任意角色都能进入任意世界；角色快照与世界存档互不覆盖。旧版单档
//! `saves/world.json` 在首次启动时迁移为 `world_1` + `character_1`。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::character_save::CharacterSave;
use super::world_save::WorldSave;
use crate::world::DEFAULT_SEED;

/// 存档根目录。
pub const SAVE_ROOT: &str = "saves";
/// 角色存档目录。
pub const CHARACTER_DIR: &str = "characters";
/// 世界存档目录。
pub const WORLD_DIR: &str = "worlds";
/// 旧版单档文件（相对存档根）。
pub const LEGACY_WORLD_FILE: &str = "world.json";
/// 角色/世界名称允许的最大字符数（字体与按钮宽度约束）。
pub const MAX_NAME_CHARS: usize = 20;

/// 角色或世界的列表条目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveEntry {
    /// 稳定 ID，也是文件名/目录名。
    pub id: String,
    /// 展示名称。
    pub name: String,
    /// 世界种子；角色条目为 `None`。
    pub seed: Option<u32>,
}

/// 世界目录下的元数据。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldMeta {
    pub name: String,
    pub seed: u32,
}

/// 存档库；只负责文件布局与元数据，不保存运行中的世界状态。
#[derive(Clone, Debug)]
pub struct SaveLibrary {
    root: PathBuf,
}

impl Default for SaveLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveLibrary {
    /// 使用默认存档根 `saves/`。
    pub fn new() -> Self {
        Self::with_root(SAVE_ROOT)
    }

    /// 指定存档根；测试可指向临时目录。
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn characters_dir(&self) -> PathBuf {
        self.root.join(CHARACTER_DIR)
    }

    fn worlds_dir(&self) -> PathBuf {
        self.root.join(WORLD_DIR)
    }

    /// 角色存档文件路径。
    pub fn character_path(&self, id: &str) -> PathBuf {
        self.characters_dir().join(format!("{id}.json"))
    }

    /// 世界目录路径。
    pub fn world_dir(&self, id: &str) -> PathBuf {
        self.worlds_dir().join(id)
    }

    /// 世界存档文件路径。
    pub fn world_path(&self, id: &str) -> PathBuf {
        self.world_dir(id).join("world.json")
    }

    /// 世界元数据路径。
    pub fn world_meta_path(&self, id: &str) -> PathBuf {
        self.world_dir(id).join("meta.json")
    }

    /// 把旧版单档迁移为 `world_1` + `character_1`；已有角色或世界时不做任何事。
    pub fn migrate_legacy(&self) -> Result<()> {
        let legacy = self.root.join(LEGACY_WORLD_FILE);
        if !legacy.exists() {
            return Ok(());
        }
        if !self.characters().is_empty() || !self.worlds().is_empty() {
            return Ok(());
        }

        let seed = WorldSave::peek_seed(&legacy).unwrap_or(DEFAULT_SEED);
        let save = WorldSave::load(&legacy, seed);
        let world_id = "world_1";
        let world_dir = self.world_dir(world_id);
        fs::create_dir_all(&world_dir)
            .with_context(|| format!("创建世界目录失败: {}", world_dir.display()))?;

        let target = world_dir.join("world.json");
        move_file(&legacy, &target)?;
        let legacy_backup = self.root.join(format!("{LEGACY_WORLD_FILE}.bak"));
        if legacy_backup.exists() {
            let target_backup = world_dir.join("world.json.bak");
            if let Err(error) = move_file(&legacy_backup, &target_backup) {
                warn!(error = %error, "旧存档备份迁移失败，保留原文件");
            }
        }

        write_world_meta(
            &self.world_meta_path(world_id),
            &WorldMeta {
                name: format!("{} 1", crate::text::WORLD_PREFIX),
                seed,
            },
        )?;

        let character = CharacterSave {
            version: super::character_save::CHARACTER_SAVE_VERSION,
            name: format!("{} 1", crate::text::CHARACTER_PREFIX),
            inventory: save.player().cloned(),
            survival: save.survival(),
            equipment: save.equipment().cloned(),
            vitals: save.vitals(),
        };
        character.save(self.character_path("character_1"))?;
        info!(
            world = %target.display(),
            character = %self.character_path("character_1").display(),
            seed,
            "旧版单档已迁移为多角色/多世界存档"
        );
        Ok(())
    }

    /// 全部角色，按编号排序。
    pub fn characters(&self) -> Vec<SaveEntry> {
        let mut entries = Vec::new();
        let Ok(read_dir) = fs::read_dir(self.characters_dir()) else {
            return entries;
        };
        for item in read_dir.flatten() {
            let path = item.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some(save) = CharacterSave::load(&path) else {
                continue;
            };
            let name = if save.name.trim().is_empty() {
                display_name(crate::text::CHARACTER_PREFIX, id)
            } else {
                save.name
            };
            entries.push(SaveEntry {
                id: id.to_owned(),
                name,
                seed: None,
            });
        }
        sort_entries(&mut entries);
        entries
    }

    /// 全部世界，按编号排序；缺少元数据的世界退回文件名并跳过无种子的目录。
    pub fn worlds(&self) -> Vec<SaveEntry> {
        let mut entries = Vec::new();
        let Ok(read_dir) = fs::read_dir(self.worlds_dir()) else {
            return entries;
        };
        for item in read_dir.flatten() {
            let dir = item.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(id) = dir.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let meta = read_world_meta(&self.world_meta_path(id));
            let seed = meta
                .as_ref()
                .map(|meta| meta.seed)
                .or_else(|| WorldSave::peek_seed(self.world_path(id)));
            let Some(seed) = seed else {
                warn!(world = id, "世界缺少种子，跳过该条目");
                continue;
            };
            let name = meta
                .map(|meta| meta.name)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| display_name(crate::text::WORLD_PREFIX, id));
            entries.push(SaveEntry {
                id: id.to_owned(),
                name,
                seed: Some(seed),
            });
        }
        sort_entries(&mut entries);
        entries
    }

    /// 新建角色，名称为下一个可用编号；返回新条目。
    pub fn create_character(&self) -> Result<SaveEntry> {
        let id = unique_id(&self.characters_dir(), "character", ".json");
        let index = numeric_suffix(&id).unwrap_or(1);
        let name = format!("{} {index}", crate::text::CHARACTER_PREFIX);
        CharacterSave::new(name.clone()).save(self.character_path(&id))?;
        info!(id, name, "已新建角色");
        Ok(SaveEntry {
            id,
            name,
            seed: None,
        })
    }

    /// 重命名角色；名称允许 Unicode 字母、数字与空格。
    pub fn rename_character(&self, id: &str, name: &str) -> Result<SaveEntry> {
        let name = normalize_name(name)
            .with_context(|| format!("角色名称不合法: {name}（仅支持中英文、数字与空格）"))?;
        let path = self.character_path(id);
        let mut save = CharacterSave::load(&path)
            .with_context(|| format!("角色不存在或无法读取: {id}"))?;
        save.name = name.clone();
        save.save(&path)?;
        info!(id, name, "角色已重命名");
        Ok(SaveEntry {
            id: id.to_owned(),
            name,
            seed: None,
        })
    }

    /// 删除角色文件；不存在时视为成功。
    pub fn delete_character(&self, id: &str) -> Result<()> {
        let path = self.character_path(id);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("删除角色存档失败: {}", path.display()))?;
            info!(id, "角色已删除");
        }
        Ok(())
    }

    /// 新建世界：随机种子、空世界存档与元数据；返回新条目。
    pub fn create_world(&self) -> Result<SaveEntry> {
        let id = unique_id(&self.worlds_dir(), "world", "");
        let index = numeric_suffix(&id).unwrap_or(1);
        let name = format!("{} {index}", crate::text::WORLD_PREFIX);
        let seed = random_seed();
        let dir = self.world_dir(&id);
        fs::create_dir_all(&dir)
            .with_context(|| format!("创建世界目录失败: {}", dir.display()))?;
        write_world_meta(&self.world_meta_path(&id), &WorldMeta { name: name.clone(), seed })?;
        // 立即写出空世界存档，使新建世界在列表与删除操作中始终可用。
        let mut save = WorldSave::empty(seed);
        save.set_path(self.world_path(&id));
        save.write_now()?;
        info!(id, name, seed, "已新建世界");
        Ok(SaveEntry {
            id,
            name,
            seed: Some(seed),
        })
    }

    /// 删除整个世界目录；不存在时视为成功。
    pub fn delete_world(&self, id: &str) -> Result<()> {
        let dir = self.world_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .with_context(|| format!("删除世界目录失败: {}", dir.display()))?;
            info!(id, "世界已删除");
        }
        Ok(())
    }

    /// 读取角色快照；文件缺失或损坏时返回 `None`。
    pub fn character(&self, id: &str) -> Option<CharacterSave> {
        CharacterSave::load(self.character_path(id))
    }

    /// 写入角色快照。
    pub fn save_character(&self, id: &str, save: &CharacterSave) -> Result<()> {
        save.save(self.character_path(id))
    }

    /// 读取世界种子；缺失时返回 `None`。
    pub fn world_seed(&self, id: &str) -> Option<u32> {
        read_world_meta(&self.world_meta_path(id))
            .map(|meta| meta.seed)
            .or_else(|| WorldSave::peek_seed(self.world_path(id)))
    }
}

/// 名称规范化：保留 Unicode 字母/数字/空格与常见连接符，压缩多余空格并限制长度。
pub fn normalize_name(input: &str) -> Option<String> {
    let mut filtered = String::new();
    for character in input.chars() {
        if character.is_control() {
            continue;
        }
        let character = if character.is_ascii() {
            character.to_ascii_uppercase()
        } else {
            character
        };
        if character.is_alphanumeric() || matches!(character, ' ' | '-' | '.' | '_') {
            filtered.push(character);
        }
    }
    let name = filtered.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        None
    } else {
        Some(name)
    }
}

fn read_world_meta(path: &Path) -> Option<WorldMeta> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_world_meta(path: &Path, meta: &WorldMeta) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建世界目录失败: {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(meta).context("序列化世界元数据失败")?;
    fs::write(path, json).with_context(|| format!("写入世界元数据失败: {}", path.display()))?;
    Ok(())
}

fn move_file(source: &Path, target: &Path) -> Result<()> {
    if let Err(error) = fs::rename(source, target) {
        warn!(
            error = %error,
            source = %source.display(),
            target = %target.display(),
            "移动存档失败，改为复制后删除"
        );
        fs::copy(source, target).with_context(|| {
            format!(
                "复制存档失败: {} -> {}",
                source.display(),
                target.display()
            )
        })?;
        fs::remove_file(source)
            .with_context(|| format!("删除旧存档失败: {}", source.display()))?;
    }
    Ok(())
}

fn sort_entries(entries: &mut [SaveEntry]) {
    entries.sort_by(|a, b| {
        numeric_suffix(&a.id)
            .cmp(&numeric_suffix(&b.id))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn numeric_suffix(id: &str) -> Option<u32> {
    id.rsplit('_').next().and_then(|suffix| suffix.parse().ok())
}

fn display_name(prefix: &str, id: &str) -> String {
    match numeric_suffix(id) {
        Some(index) => format!("{prefix} {index}"),
        None => format!("{prefix} {id}"),
    }
}

/// 在目录中寻找未占用的 `<prefix>_<n>`；`suffix` 用于区分文件与目录。
fn unique_id(dir: &Path, prefix: &str, suffix: &str) -> String {
    for index in 1..=u32::MAX {
        let candidate = format!("{prefix}_{index}");
        if !dir.join(format!("{candidate}{suffix}")).exists() {
            return candidate;
        }
    }
    format!("{prefix}_1")
}

fn random_seed() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x5EED);
    let mixed = nanos ^ (nanos >> 32) ^ (u64::from(std::process::id()) << 17);
    let seed = (mixed as u32) ^ ((mixed >> 32) as u32) ^ 0x9E37_79B9;
    if seed == 0 {
        1
    } else {
        seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("系统时间有效")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "neverland_library_{tag}_{}_{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn characters_and_worlds_are_created_listed_and_deleted() {
        let root = temp_root("crud");
        let library = SaveLibrary::with_root(&root);

        let character = library.create_character().expect("新建角色");
        assert_eq!(character.id, "character_1");
        assert_eq!(character.name, "角色 1");
        assert!(library.character_path("character_1").exists());

        let second = library.create_character().expect("新建第二个角色");
        assert_eq!(second.id, "character_2");
        assert_eq!(library.characters().len(), 2);

        let renamed = library
            .rename_character("character_1", "alice")
            .expect("重命名");
        assert_eq!(renamed.name, "ALICE");
        assert_eq!(library.character("character_1").expect("角色存在").name, "ALICE");

        let world = library.create_world().expect("新建世界");
        assert_eq!(world.id, "world_1");
        assert!(world.seed.unwrap_or(0) != 0);
        assert!(library.world_path("world_1").exists());
        assert_eq!(library.worlds().len(), 1);
        assert_eq!(library.world_seed("world_1"), world.seed);

        library.delete_world("world_1").expect("删除世界");
        assert!(library.worlds().is_empty());
        library.delete_character("character_1").expect("删除角色");
        assert_eq!(library.characters().len(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn names_are_normalized_and_rejected_when_empty() {
        assert_eq!(normalize_name("  ab  c "), Some("AB C".to_owned()));
        // Unicode 字母与数字保留；控制字符被过滤。
        assert_eq!(normalize_name("角色1"), Some("角色1".to_owned()));
        assert_eq!(normalize_name("新角色\u{0007}"), Some("新角色".to_owned()));
        assert_eq!(normalize_name("角色"), Some("角色".to_owned()));
        assert_eq!(normalize_name(""), None);
        assert_eq!(normalize_name(&"A".repeat(MAX_NAME_CHARS + 1)), None);
    }

    #[test]
    fn a_legacy_single_save_becomes_one_character_and_one_world() {
        let root = temp_root("migrate");
        fs::create_dir_all(&root).expect("创建临时根");
        let legacy = root.join(LEGACY_WORLD_FILE);
        fs::write(
            &legacy,
            r#"{"version":10,"seed":24301,"chunk_edits":[],"player":{"slots":[],"selected_hotbar":0},"survival":{"max_health":100,"current_health":42},"vitals":{"hunger":{"hunger":10.0,"max":100.0},"stamina":{"value":10.0},"respawn":null}}"#,
        )
        .expect("写入旧档");

        let library = SaveLibrary::with_root(&root);
        library.migrate_legacy().expect("迁移旧档");

        assert!(!legacy.exists(), "旧档应已移入世界目录");
        assert!(library.world_path("world_1").exists());
        let worlds = library.worlds();
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].name, "世界 1");
        assert_eq!(worlds[0].seed, Some(24301));

        let characters = library.characters();
        assert_eq!(characters.len(), 1);
        let character = library.character("character_1").expect("迁移后的角色");
        assert_eq!(character.survival.map(|state| state.current_health), Some(42));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn migration_is_skipped_when_library_already_has_entries() {
        let root = temp_root("migrate_skip");
        let library = SaveLibrary::with_root(&root);
        library.create_character().expect("新建角色");
        let legacy = root.join(LEGACY_WORLD_FILE);
        fs::write(&legacy, r#"{"version":10,"seed":1}"#).expect("写入旧档");
        library.migrate_legacy().expect("迁移应被跳过");
        assert!(legacy.exists(), "已有角色时不迁移旧档");
        let _ = fs::remove_dir_all(&root);
    }
}

//! 玩家可见文案与显示名集中表。
//!
//! 所有会显示在屏幕上的文字（菜单、HUD、各界面、物品/方块/生物显示名）都从这里
//! 读取，改文案只需修改本文件。开发者日志、错误上下文与测试断言不纳入本表。

use crate::entity::EntityTypeId;
use crate::equipment::EquipmentSlot;
use crate::item::item::ItemType;
use crate::item::{ItemId, RecipeCategory};
use crate::player::DamageSource;
use crate::player_visual::CameraMode;
use crate::survival::sleep::SleepError;
use crate::survival::world_time::TimePhase;
use crate::world::Block;

// ---------------------------------------------------------------------------
// 通用
// ---------------------------------------------------------------------------

pub const BACK: &str = "返回";
pub const CONFIRM: &str = "确认";
pub const CANCEL: &str = "取消";
pub const EDIT: &str = "编辑";
pub const DELETE: &str = "删除";
pub const EMPTY_HAND: &str = "空手";
pub const BROKEN: &str = "已损坏";
pub const CHARACTER_PREFIX: &str = "角色";
pub const WORLD_PREFIX: &str = "世界";

// ---------------------------------------------------------------------------
// 菜单
// ---------------------------------------------------------------------------

/// 标题图缺失时的文字回退。
pub const GAME_TITLE: &str = "NEVERLAND RHAPSODY";
/// 主菜单入口，顺序与 `main_buttons` 一致。
pub const MAIN_BUTTONS: [&str; 7] = [
    "单人游戏",
    "多人游戏",
    "成就",
    "设置",
    "WIKI",
    "关于",
    "退出",
];
/// 暂停菜单入口，顺序与 `pause_buttons` 一致。
pub const PAUSE_BUTTONS: [&str; 4] = ["设置", "角色成就", "世界成就", "保存并退出"];
pub const CHARACTER_SELECT_TITLE: &str = "选择角色";
pub const EMPTY_CHARACTERS: &str = "暂无角色 - 请新建";
pub const NEW_CHARACTER: &str = "新建角色";
pub const WORLD_SELECT_TITLE: &str = "选择世界";
pub const EMPTY_WORLDS: &str = "暂无世界 - 请新建";
pub const NEW_WORLD: &str = "新建世界";
pub const PAUSED_TITLE: &str = "已暂停";
pub const RESUME_HINT: &str = "按 ESC 继续游戏";
pub const RENAME_TITLE: &str = "重命名角色";
pub const RENAME_HINT: &str = "支持中文输入 · ENTER 确认";

pub const MULTIPLAYER_UNAVAILABLE: &str = "多人游戏暂未开放";
pub const ACHIEVEMENTS_UNAVAILABLE: &str = "成就暂未开放";
pub const SETTINGS_UNAVAILABLE: &str = "设置暂未开放";
pub const WIKI_UNAVAILABLE: &str = "WIKI 暂未开放";
pub const ABOUT_UNAVAILABLE: &str = "关于暂未开放";
pub const CHARACTER_ACHIEVEMENTS_UNAVAILABLE: &str = "角色成就暂未开放";
pub const WORLD_ACHIEVEMENTS_UNAVAILABLE: &str = "世界成就暂未开放";

// ---------------------------------------------------------------------------
// 界面标签
// ---------------------------------------------------------------------------

pub const INVENTORY_TITLE: &str = "背包";
pub const INVENTORY_HINT: &str = "E / ESC 关闭";
pub const ARMOR_LABEL: &str = "护甲";
pub const TOOL_LABEL: &str = "工具";
pub const WEAPON_LABEL: &str = "武器";

pub const CRAFTING_TITLE: &str = "合成";
pub const CRAFTING_HINT: &str = "C / ESC 关闭";
pub const CRAFTING_ALL: &str = "全部";
pub const CRAFTING_EMPTY: &str = "选择一个配方";
pub const CRAFTABLE: &str = "可合成";
pub const REQUIRED_MATERIALS: &str = "所需材料";
pub const CRAFT_BUTTON: &str = "合成";

pub const COOKING_TITLE: &str = "营火";
pub const COOKING_ACTIVE: &str = "烹饪中";
pub const COOKING_INSERT: &str = "点击输入槽放入生肉";
pub const COOKING_CLOSE_HINT: &str = "按 E 或 ESC 关闭";

pub const DEATH_TITLE: &str = "你死了";
pub const RESPAWN: &str = "重生";
pub const MAIN_MENU: &str = "主菜单";

/// 昼夜文字的默认阶段。
pub const DAY: &str = "白天";

// ---------------------------------------------------------------------------
// 应用层提示
// ---------------------------------------------------------------------------

pub const NOTICE_CHARACTER_NOT_FOUND: &str = "未找到角色";
pub const NOTICE_WORLD_DATA_MISSING: &str = "世界数据缺失";
pub const NOTICE_CHARACTER_DATA_MISSING: &str = "角色数据缺失";
pub const NOTICE_CHARACTER_LIMIT: &str = "角色数量已达上限";
pub const NOTICE_CREATE_CHARACTER_FAILED: &str = "新建角色失败";
pub const NOTICE_CHARACTER_CREATED: &str = "角色已创建";
pub const NOTICE_DELETE_CHARACTER_FAILED: &str = "删除角色失败";
pub const NOTICE_CHARACTER_DELETED: &str = "角色已删除";
pub const NOTICE_CHARACTER_RENAMED: &str = "角色已重命名";
pub const NOTICE_INVALID_NAME: &str = "名称不合法";
pub const NOTICE_WORLD_LIMIT: &str = "世界数量已达上限";
pub const NOTICE_CREATE_WORLD_FAILED: &str = "新建世界失败";
pub const NOTICE_WORLD_CREATED: &str = "世界已创建";
pub const NOTICE_DELETE_WORLD_FAILED: &str = "删除世界失败";
pub const NOTICE_WORLD_DELETED: &str = "世界已删除";
pub const NOTICE_SELECT_CHARACTER_FIRST: &str = "请先选择角色";

pub const MESSAGE_NOT_HUNGRY: &str = "还不饿";
pub const MESSAGE_SLEPT_UNTIL_MORNING: &str = "睡到了早晨";
pub const MESSAGE_NEED_SOLID_GROUND: &str = "需要坚实地面";

// ---------------------------------------------------------------------------
// 格式化
// ---------------------------------------------------------------------------

/// 世界选择页头部的角色说明。
pub fn character_caption(name: &str) -> String {
    format!("角色: {name}")
}

/// 死亡界面的死因行。
pub fn death_cause(cause: &str) -> String {
    format!("死因：{cause}")
}

/// 右上角昼夜文字。
pub fn time_label(phase: &str) -> String {
    format!("时间 {phase}")
}

/// 营火界面的背包生肉数量。
pub fn cooking_raw_meat(count: u32) -> String {
    format!("背包中生肉：{count}")
}

/// 合成成功提示。
pub fn crafted(count: u32, item: &str) -> String {
    format!("合成了 {count} 个{item}")
}

/// 拾取提示。
pub fn pickup(count: u32, item: &str) -> String {
    format!("+{count} {item}")
}

/// 物品损坏提示。
pub fn item_broken(item: &str) -> String {
    format!("{item} 已损坏")
}

/// 物品标题：数量大于 1 时附加 `X<n>`。
pub fn item_title(name: &str, count: u32) -> String {
    if count > 1 {
        format!("{name} X{count}")
    } else {
        name.to_owned()
    }
}

/// 物品提示中的堆叠行。
pub fn stack_line(count: u32, max: u32) -> String {
    format!("堆叠 {count}/{max}")
}

/// 合成材料行。
pub fn ingredient_line(name: &str, owned: u32, required: u32) -> String {
    format!("{name} {owned}/{required}")
}

/// 工具耐久摘要。
pub fn tool_line(name: &str, current: u32, max: u32) -> String {
    format!("{name} 耐久 {current}/{max}")
}

/// 武器伤害与耐久摘要。
pub fn weapon_line(name: &str, damage: u32, current: u32, max: u32) -> String {
    format!("{name} {damage} 伤害 耐久 {current}/{max}")
}

/// 已损坏武器的摘要。
pub fn weapon_broken_line(name: &str, damage: u32) -> String {
    format!("{name} {damage} 伤害 已损坏")
}

/// 护甲防御与耐久摘要。
pub fn armor_line(name: &str, defense: u32, current: u32, max: u32) -> String {
    format!("{name} {defense} 防御 耐久 {current}/{max}")
}

// ---------------------------------------------------------------------------
// 名称映射
// ---------------------------------------------------------------------------

/// 物品的中文显示名；未知物品回退到内部英文名。
pub fn item_name(item: ItemId) -> &'static str {
    match item {
        ItemId::STONE => "石头",
        ItemId::DIRT => "泥土",
        ItemId::GRASS => "草方块",
        ItemId::WOODEN_PICKAXE => "木镐",
        ItemId::STONE_PICKAXE => "石镐",
        ItemId::IRON_PICKAXE => "铁镐",
        ItemId::WOODEN_AXE => "木斧",
        ItemId::STONE_AXE => "石斧",
        ItemId::IRON_AXE => "铁斧",
        ItemId::WOODEN_SHOVEL => "木铲",
        ItemId::STONE_SHOVEL => "石铲",
        ItemId::IRON_SHOVEL => "铁铲",
        ItemId::WOOD => "木头",
        ItemId::STICK => "木棍",
        ItemId::IRON_INGOT => "铁锭",
        ItemId::RAW_PORKCHOP => "生猪排",
        ItemId::RAW_BEEF => "生牛肉",
        ItemId::LEATHER => "皮革",
        ItemId::DIAMOND => "钻石",
        ItemId::WOODEN_SWORD => "木剑",
        ItemId::STONE_SWORD => "石剑",
        ItemId::IRON_SWORD => "铁剑",
        ItemId::DIAMOND_SWORD => "钻石剑",
        ItemId::LEATHER_HELMET => "皮革头盔",
        ItemId::LEATHER_CHESTPLATE => "皮革胸甲",
        ItemId::LEATHER_LEGGINGS => "皮革护腿",
        ItemId::LEATHER_BOOTS => "皮革靴子",
        ItemId::IRON_HELMET => "铁头盔",
        ItemId::IRON_CHESTPLATE => "铁胸甲",
        ItemId::IRON_LEGGINGS => "铁护腿",
        ItemId::IRON_BOOTS => "铁靴子",
        ItemId::DIAMOND_HELMET => "钻石头盔",
        ItemId::DIAMOND_CHESTPLATE => "钻石胸甲",
        ItemId::DIAMOND_LEGGINGS => "钻石护腿",
        ItemId::DIAMOND_BOOTS => "钻石靴子",
        ItemId::ROTTEN_FLESH => "腐肉",
        ItemId::COOKED_PORKCHOP => "熟猪排",
        ItemId::COOKED_BEEF => "熟牛排",
        ItemId::APPLE => "苹果",
        ItemId::BREAD => "面包",
        ItemId::CAMPFIRE => "营火",
        ItemId::BED => "床",
        ItemId::SAND => "沙子",
        ItemId::SANDSTONE => "砂岩",
        ItemId::CACTUS => "仙人掌",
        ItemId::COAL => "煤炭",
        ItemId::GOLD_INGOT => "金锭",
        _ => crate::item::item_registry::name(item),
    }
}

/// 方块的中文显示名；当前 UI 尚未展示方块名，保留统一入口。
#[allow(dead_code)]
pub const fn block_name(block: Block) -> &'static str {
    match block {
        Block::Air => "空气",
        Block::Stone => "石头",
        Block::Dirt => "泥土",
        Block::Grass => "草方块",
        Block::Wood => "木头",
        Block::Campfire => "营火",
        Block::Bed => "床",
        Block::Sand => "沙子",
        Block::Sandstone => "砂岩",
        Block::CoalOre => "煤矿石",
        Block::IronOre => "铁矿石",
        Block::GoldOre => "金矿石",
        Block::DiamondOre => "钻石矿石",
        Block::Water => "水",
        Block::Leaves => "树叶",
        Block::TallGrass => "高草",
        Block::FlowerRed => "红花",
        Block::FlowerYellow => "黄花",
        Block::Cactus => "仙人掌",
        Block::Bedrock => "基岩",
    }
}

/// 生物的中文显示名。
pub const fn creature_name(kind: EntityTypeId) -> &'static str {
    match kind {
        EntityTypeId::PIG => "猪",
        EntityTypeId::COW => "牛",
        EntityTypeId::ZOMBIE => "僵尸",
        _ => "生物",
    }
}

/// 物品类型标签。
pub const fn item_type_label(kind: ItemType) -> &'static str {
    match kind {
        ItemType::Block => "方块",
        ItemType::Tool => "工具",
        ItemType::Material => "材料",
        ItemType::Consumable => "食物",
        ItemType::Weapon => "武器",
        ItemType::Armor => "护甲",
    }
}

// ---------------------------------------------------------------------------
// 枚举标签
// ---------------------------------------------------------------------------

/// 合成分类标签。
pub const fn recipe_category(category: RecipeCategory) -> &'static str {
    match category {
        RecipeCategory::Materials => "材料",
        RecipeCategory::Tools => "工具",
        RecipeCategory::Utility => "功能",
        RecipeCategory::Weapons => "武器",
        RecipeCategory::Armor => "护甲",
        RecipeCategory::Consumables => "食物",
        RecipeCategory::Building => "建筑",
    }
}

/// 伤害来源标签。
pub const fn damage_source(source: DamageSource) -> &'static str {
    match source {
        DamageSource::Fall => "坠落",
        DamageSource::Environmental => "环境",
        DamageSource::Entity => "生物",
        DamageSource::Custom => "未知",
    }
}

/// 昼夜阶段标签。
pub const fn time_phase(phase: TimePhase) -> &'static str {
    match phase {
        TimePhase::Day => "白天",
        TimePhase::Dusk => "黄昏",
        TimePhase::Night => "夜晚",
        TimePhase::Dawn => "黎明",
    }
}

/// 睡眠失败提示。
pub const fn sleep_error(error: SleepError) -> &'static str {
    match error {
        SleepError::Dead => "现在无法入睡",
        SleepError::NotNight => "只能在夜晚入睡",
        SleepError::HostileNearby => "附近有怪物",
    }
}

/// 装备槽位标签。
pub const fn equipment_slot(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Head => "头",
        EquipmentSlot::Chest => "胸",
        EquipmentSlot::Legs => "腿",
        EquipmentSlot::Feet => "脚",
    }
}

/// 视角模式提示。
pub const fn camera_mode(mode: CameraMode) -> &'static str {
    match mode {
        CameraMode::FirstPerson => "第一人称",
        CameraMode::ThirdPerson => "第三人称",
    }
}

/// 合成失败提示。
pub const fn craft_error(error: crate::item::crafting::CraftError) -> &'static str {
    match error {
        crate::item::crafting::CraftError::UnknownRecipe => "配方不可用",
        crate::item::crafting::CraftError::InsufficientMaterials => "材料不足",
        crate::item::crafting::CraftError::NoSpace => "背包已满",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_has_a_name() {
        for raw in 1..=47 {
            let item = ItemId::from_raw(raw).expect("已注册物品 ID");
            let name = item_name(item);
            assert!(!name.is_empty());
            assert_ne!(name, "?", "物品 {item:?} 缺少中文名");
        }
    }

    #[test]
    fn every_block_has_a_name() {
        for block in Block::ALL {
            assert!(!block_name(block).is_empty());
        }
    }

    #[test]
    fn every_creature_has_a_name() {
        for kind in [EntityTypeId::PIG, EntityTypeId::COW, EntityTypeId::ZOMBIE] {
            assert!(!creature_name(kind).is_empty());
        }
    }

    #[test]
    fn enum_labels_are_localized() {
        assert_eq!(time_phase(TimePhase::Night), "夜晚");
        assert_eq!(damage_source(DamageSource::Fall), "坠落");
        assert_eq!(equipment_slot(EquipmentSlot::Head), "头");
        assert_eq!(camera_mode(CameraMode::FirstPerson), "第一人称");
        assert!(!sleep_error(SleepError::NotNight).is_ascii());
        assert_eq!(recipe_category(RecipeCategory::Tools), "工具");
    }
}

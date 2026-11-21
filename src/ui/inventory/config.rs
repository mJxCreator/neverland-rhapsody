//! 背包界面的可调参数：所有尺寸、间距、字号集中在此。
//!
//! 修改布局时只改本文件；槽位绘制与点击语义在 `mod.rs`，无需跟随改动。

/// 单个槽位的基准边长（像素）。
pub const BASE_SLOT: f32 = 44.0;
/// 槽位之间的间距（像素）。
pub const BASE_GAP: f32 = 6.0;
/// 面板内边距（像素）。
pub const BASE_PADDING: f32 = 12.0;
/// 标题区高度（像素）。
pub const BASE_TITLE: f32 = 24.0;
/// 主背包与快捷栏之间的间距（像素）。
pub const BASE_HOTBAR_GAP: f32 = 14.0;
/// 槽位区与底部详情区之间的间距（像素）。
pub const BASE_INFO_GAP: f32 = 10.0;
/// 底部详情区高度（像素）。
pub const BASE_INFO_HEIGHT: f32 = 38.0;
/// 主背包与装备列之间的间距（像素）。
pub const BASE_EQUIPMENT_GAP: f32 = 20.0;
/// 主背包列数。
pub const COLUMNS: usize = 9;
/// 主背包行数。
pub const MAIN_ROWS: usize = 3;
/// 装备槽位在 UI 中的起始索引。
pub const EQUIPMENT_SLOT_BASE: usize = crate::item::TOTAL_SLOTS;
/// 背包界面可点击槽位总数（背包/快捷栏 + 装备）。
pub const UI_SLOTS: usize = crate::item::TOTAL_SLOTS + crate::equipment::EQUIPMENT_SLOT_COUNT;
/// 标题字号（像素格）。
pub const TITLE_PIXEL: f32 = 2.4;
/// 详情区字号（像素格）。
pub const INFO_PIXEL: f32 = 1.6;
/// 拖动图标相对鼠标的偏移（像素）。
pub const HELD_OFFSET: f32 = 8.0;

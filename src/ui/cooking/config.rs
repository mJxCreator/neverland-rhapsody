//! 营火烹饪界面的可调参数：面板、槽位、进度条与单次转移数量。

/// 每点击一次输入槽最多转移的物品数量。
pub const TRANSFER_LIMIT: u32 = 16;
/// 面板宽度（像素）。
pub const PANEL_WIDTH: f32 = 560.0;
/// 面板高度（像素）。
pub const PANEL_HEIGHT: f32 = 320.0;
/// 输入/输出槽边长（像素）。
pub const SLOT_SIZE: f32 = 56.0;
/// 槽位距面板左右边缘的距离（像素）。
pub const SLOT_SIDE_MARGIN: f32 = 84.0;
/// 槽位距面板顶部的距离（像素）。
pub const SLOT_TOP: f32 = 150.0;
/// 进度条距面板左右边缘的距离（像素）。
pub const PROGRESS_SIDE_INSET: f32 = 180.0;
/// 进度条距面板顶部的距离（像素）。
pub const PROGRESS_TOP: f32 = 172.0;
/// 进度条高度（像素）。
pub const PROGRESS_HEIGHT: f32 = 16.0;

//! 合成界面的可调参数：面板尺寸、分栏、列表行高、字号与提示时长。

/// 面板基准宽度（像素）。
pub const BASE_PANEL_WIDTH: f32 = 900.0;
/// 面板基准高度（像素）。
pub const BASE_PANEL_HEIGHT: f32 = 620.0;
/// 面板内边距（像素）。
pub const PADDING: f32 = 18.0;
/// 标题区高度（像素）。
pub const HEADER_HEIGHT: f32 = 54.0;
/// 左右分栏间距（像素）。
pub const GUTTER: f32 = 20.0;
/// 左侧分类/列表栏宽度占比。
pub const LEFT_RATIO: f32 = 0.46;
/// 分类标签高度（像素）。
pub const TAB_HEIGHT: f32 = 30.0;
/// 分类标签间距（像素）。
pub const TAB_GAP: f32 = 6.0;
/// 配方行间距（像素）。
pub const LIST_GAP: f32 = 6.0;
/// 列表距顶部的间距（像素）。
pub const LIST_TOP_GAP: f32 = 10.0;
/// 配方行高度（像素）。
pub const ROW_HEIGHT: f32 = 44.0;
/// 配方行最小裁剪高度（像素）。
pub const MIN_ROW_HEIGHT: f32 = 10.0;
/// 合成按钮宽度（像素）。
pub const CRAFT_WIDTH: f32 = 180.0;
/// 合成按钮高度（像素）。
pub const CRAFT_HEIGHT: f32 = 44.0;
/// 详情图标边长（像素）。
pub const DETAIL_ICON_SIZE: f32 = 52.0;
/// 材料行高度（像素）。
pub const INGREDIENT_ROW: f32 = 24.0;
/// 材料行间距（像素）。
pub const INGREDIENT_GAP: f32 = 4.0;
/// 标题字号（像素格）。
pub const TITLE_PIXEL: f32 = 2.0;
/// 正文字号（像素格）。
pub const TEXT_PIXEL: f32 = 1.0;
/// 配方行字号（像素格）。
pub const ROW_TEXT_PIXEL: f32 = 1.0;
/// 分类标签字号（像素格）。
pub const TAB_PIXEL: f32 = 1.0;
/// 成功/失败提示的显示时长（秒）。
pub const MESSAGE_DURATION: f32 = 2.5;
/// 提示最后阶段的淡出时长（秒）。
pub const MESSAGE_FADE: f32 = 0.4;

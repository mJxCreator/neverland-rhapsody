//! 菜单界面的可调参数：背景、面板与按钮尺寸。
//!
//! 所有像素值都以 1280×720 为基准设计；布局函数按视口高度做保守收敛，
//! 修改这里的数值即可统一调整主菜单、角色/世界选择与暂停菜单。

/// 菜单全屏背景（主菜单不绘制世界，使用纯色底）。
pub const MENU_BACKGROUND: [f32; 4] = [0.035, 0.05, 0.075, 1.0];
/// 菜单文字主色。
pub const MENU_TEXT: [f32; 4] = [0.93, 0.96, 1.0, 1.0];
/// 菜单次要文字。
pub const MENU_TEXT_MUTED: [f32; 4] = [0.58, 0.64, 0.74, 1.0];
/// 提示文字颜色。
pub const MENU_NOTICE: [f32; 4] = [1.0, 0.82, 0.42, 1.0];
/// 列表行底色。
pub const ROW_BACKGROUND: [f32; 4] = [0.10, 0.12, 0.16, 0.96];
/// 列表行悬停底色。
pub const ROW_HOVER: [f32; 4] = [0.18, 0.23, 0.31, 0.98];
/// 列表行边框。
pub const ROW_BORDER: [f32; 4] = [0.34, 0.40, 0.50, 0.95];

/// 主菜单按钮宽度（像素）。
pub const MAIN_BUTTON_WIDTH: f32 = 300.0;
/// 主菜单按钮高度（像素）。
pub const MAIN_BUTTON_HEIGHT: f32 = 34.0;
/// 主菜单按钮间距（像素）。
pub const MAIN_BUTTON_GAP: f32 = 7.0;
/// 主菜单底部留白（像素）。
pub const MAIN_BOTTOM_MARGIN: f32 = 26.0;
/// 标题图最大宽度（像素）。
pub const TITLE_MAX_WIDTH: f32 = 560.0;
/// 标题区域与按钮区之间的间距（像素）。
pub const TITLE_GAP: f32 = 22.0;

/// 列表面板宽度上限（像素）。
pub const PANEL_WIDTH: f32 = 660.0;
/// 列表面板高度（像素）。
pub const PANEL_HEIGHT: f32 = 396.0;
/// 面板内边距（像素）。
pub const PANEL_PADDING: f32 = 28.0;
/// 面板标题区高度（像素）。
pub const PANEL_HEADER_HEIGHT: f32 = 66.0;
/// 列表行高度（像素）。
pub const ROW_HEIGHT: f32 = 42.0;
/// 列表行间距（像素）。
pub const ROW_GAP: f32 = 10.0;
/// 行内操作按钮宽度（像素）。
pub const ROW_BUTTON_WIDTH: f32 = 104.0;
/// 行内按钮间距（像素）。
pub const ROW_BUTTON_GAP: f32 = 8.0;
/// 底部按钮宽度（像素）。
pub const FOOTER_BUTTON_WIDTH: f32 = 176.0;
/// 按钮文字字号（像素格）。
pub const BUTTON_TEXT_PIXEL: f32 = 2.0;
/// 面板标题字号（像素格）。
pub const HEADER_TEXT_PIXEL: f32 = 3.0;
/// 重命名输入框字号（像素格）。
pub const INPUT_TEXT_PIXEL: f32 = 2.0;
/// 角色名称最大字符数（与存档层规范一致）。
pub const RENAME_MAX_CHARS: usize = 20;
/// 列表最多显示的行数（与需求文档的角色/世界槽位数一致）。
pub const MAX_ROWS: usize = 3;

//! UI 主题：集中管理颜色、间距与动画时长，避免各界面各自硬编码视觉参数。
//!
//! 颜色统一使用 RGBA 浮点数组，直接写入 UI 顶点颜色通道；
//! 整体采用深色半透明面板、细边框与低饱和高亮，保持像素风的克制表达。

/// 全屏界面后的轻度遮罩（背包）。
pub const OVERLAY_LIGHT: [f32; 4] = [0.02, 0.02, 0.04, 0.55];
/// 全屏界面后的重度遮罩（合成、死亡）。
pub const OVERLAY_DARK: [f32; 4] = [0.01, 0.01, 0.02, 0.78];

/// 面板主体背景。
pub const PANEL_BACKGROUND: [f32; 4] = [0.07, 0.08, 0.10, 0.96];
/// 面板细边框。
pub const PANEL_BORDER: [f32; 4] = [0.32, 0.36, 0.42, 0.92];
/// 面板下方轻微投影。
pub const PANEL_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.34];

/// 槽位背景。
pub const SLOT_BACKGROUND: [f32; 4] = [0.12, 0.13, 0.15, 0.92];
/// 槽位常态边框。
pub const SLOT_BORDER: [f32; 4] = [0.30, 0.33, 0.38, 0.85];
/// 槽位悬停边框与填充。
pub const SLOT_HOVER: [f32; 4] = [0.40, 0.62, 0.86, 0.95];
/// 槽位选中边框（快捷栏选中与背包当前快捷栏格）。
pub const SLOT_SELECTED: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
/// 槽位禁用（损坏工具）覆盖色。
pub const SLOT_DISABLED: [f32; 4] = [0.20, 0.20, 0.22, 0.55];
/// 空槽位内部色块。
pub const SLOT_EMPTY: [f32; 4] = [0.09, 0.09, 0.11, 0.72];

/// 主文本。
pub const TEXT: [f32; 4] = [0.93, 0.95, 1.0, 1.0];
/// 次要文本。
pub const TEXT_MUTED: [f32; 4] = [0.62, 0.66, 0.74, 1.0];
/// 标题文本。
pub const TEXT_TITLE: [f32; 4] = [1.0, 0.94, 0.72, 1.0];
/// 文本阴影。
pub const TEXT_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
/// 数量文本。
pub const COUNT_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// 快捷栏当前物品名称。
pub const TOOL_LABEL: [f32; 4] = [0.92, 0.94, 1.0, 1.0];
/// 损坏状态文本与图标。
pub const TOOL_BROKEN: [f32; 4] = [1.0, 0.42, 0.36, 1.0];
/// 拾取提示文本。
pub const TOAST_TEXT: [f32; 4] = [1.0, 1.0, 0.85, 1.0];

/// 快捷栏/背包底板。
pub const HOTBAR_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// 快捷栏选中光晕基础颜色（透明度按动画调制）。
pub const SLOT_GLOW: [f32; 4] = [1.0, 0.94, 0.72, 1.0];

/// 准星。
pub const CROSSHAIR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
/// 准星底衬。
pub const CROSSHAIR_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.55];

/// 挖掘进度条背景。
pub const PROGRESS_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
/// 挖掘进度条填充。
pub const PROGRESS_FILL: [f32; 4] = [0.42, 0.85, 0.35, 0.95];

/// 耐久条背景。
pub const DURABILITY_BAR_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.72];
/// 工具损坏后的耐久条背景。
pub const DURABILITY_BAR_BROKEN: [f32; 4] = [0.55, 0.12, 0.12, 0.85];
/// 耐久充足。
pub const DURABILITY_HIGH: [f32; 4] = [0.38, 0.86, 0.42, 0.95];
/// 耐久中等。
pub const DURABILITY_MID: [f32; 4] = [0.95, 0.80, 0.28, 0.95];
/// 耐久低。
pub const DURABILITY_LOW: [f32; 4] = [0.95, 0.32, 0.26, 0.95];

/// 满心。
pub const HEART_FULL: [f32; 4] = [0.86, 0.16, 0.16, 0.95];
/// 空心。
pub const HEART_EMPTY: [f32; 4] = [0.28, 0.09, 0.09, 0.85];
/// 受伤边缘反馈。
pub const DAMAGE_FLASH: [f32; 4] = [0.72, 0.03, 0.03, 0.46];
/// 受击方向指示。
pub const DAMAGE_DIRECTION: [f32; 4] = [0.95, 0.12, 0.10, 0.85];
/// 敌对生物血条背景。
pub const TARGET_BAR_BACKGROUND: [f32; 4] = [0.05, 0.05, 0.06, 0.86];
/// 敌对生物血条填充。
pub const TARGET_BAR_FILL: [f32; 4] = [0.80, 0.16, 0.16, 0.95];

/// 提示框背景。
pub const TOOLTIP_BACKGROUND: [f32; 4] = [0.04, 0.05, 0.07, 0.97];
/// 提示框边框。
pub const TOOLTIP_BORDER: [f32; 4] = [0.38, 0.42, 0.50, 0.95];

/// 通用按钮背景。
pub const BUTTON_BACKGROUND: [f32; 4] = [0.16, 0.19, 0.24, 0.95];
/// 通用按钮悬停背景。
pub const BUTTON_HOVER: [f32; 4] = [0.24, 0.30, 0.38, 0.98];
/// 通用按钮边框。
pub const BUTTON_BORDER: [f32; 4] = [0.45, 0.52, 0.62, 0.95];
/// 禁用按钮背景。
pub const BUTTON_DISABLED: [f32; 4] = [0.12, 0.13, 0.15, 0.80];

/// 合成列表行背景。
pub const CRAFT_ROW_BACKGROUND: [f32; 4] = [0.10, 0.11, 0.13, 0.92];
/// 合成列表行悬停背景。
pub const CRAFT_ROW_HOVER: [f32; 4] = [0.17, 0.20, 0.25, 0.96];
/// 合成列表选中行背景。
pub const CRAFT_ROW_ACTIVE: [f32; 4] = [0.20, 0.24, 0.30, 0.98];
/// 合成列表选中边框。
pub const CRAFT_ROW_SELECTED: [f32; 4] = [0.95, 0.80, 0.35, 0.95];
/// 合成分类激活状态。
pub const CRAFT_CATEGORY_ACTIVE: [f32; 4] = [0.95, 0.80, 0.35, 1.0];
/// 材料充足。
pub const CRAFT_MATERIAL_OK: [f32; 4] = [0.52, 0.88, 0.56, 1.0];
/// 材料不足。
pub const CRAFT_MATERIAL_MISSING: [f32; 4] = [1.0, 0.46, 0.40, 1.0];
/// 合成成功提示。
pub const CRAFT_SUCCESS: [f32; 4] = [0.52, 0.92, 0.58, 1.0];
/// 合成失败提示。
pub const CRAFT_FAILURE: [f32; 4] = [1.0, 0.46, 0.40, 1.0];

/// 死亡标题。
pub const DEATH_TITLE: [f32; 4] = [0.92, 0.24, 0.24, 1.0];
/// 死亡界面遮罩。
pub const DEATH_OVERLAY: [f32; 4] = [0.12, 0.01, 0.01, 0.82];

/// 全屏面板与屏幕边缘的最小安全边距（像素）。
pub const SCREEN_MARGIN: f32 = 20.0;
/// 统一的快速反馈动画时长（秒）。
pub const FEEDBACK_DURATION: f32 = 0.22;

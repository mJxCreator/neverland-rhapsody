//! 生命/体力/饥饿状态条的可调参数。

/// 状态条参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateBarConfig {
    /// 状态条底边距快捷栏顶部的距离（像素）。
    pub bottom_offset: f32,
    /// 原图像素到屏幕像素的缩放倍率。
    pub scale: f32,
    /// 背景原图宽度。
    pub background_width: f32,
    /// 背景原图高度。
    pub background_height: f32,
    /// 单条填充原图宽度。
    pub fill_width: f32,
    /// 单条填充原图高度。
    pub fill_height: f32,
    /// 填充相对背景的内边距（原图像素）。
    pub padding: f32,
    /// 缺失背景贴图时的回退底色。
    pub background: [f32; 4],
    /// 生命填充回退色。
    pub heart: [f32; 4],
    /// 体力填充回退色。
    pub stamina: [f32; 4],
    /// 饥饿填充回退色。
    pub hunger: [f32; 4],
}

impl Default for StateBarConfig {
    fn default() -> Self {
        Self {
            bottom_offset: 28.0,
            scale: 3.0,
            background_width: 106.0,
            background_height: 19.0,
            fill_width: 102.0,
            fill_height: 5.0,
            padding: 2.0,
            background: [0.08, 0.09, 0.11, 0.85],
            heart: [0.86, 0.20, 0.24, 0.95],
            stamina: [0.38, 0.76, 0.96, 0.95],
            hunger: [0.80, 0.42, 0.16, 0.95],
        }
    }
}

impl StateBarConfig {
    /// 面板总高度（屏幕像素）。
    pub fn panel_height(&self) -> f32 {
        self.background_height * self.scale
    }

    /// 面板总宽度（屏幕像素）。
    pub fn panel_width(&self) -> f32 {
        self.background_width * self.scale
    }
}

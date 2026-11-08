//! UI 布局基础：像素矩形、视口校验与面板定位。
//!
//! UI 坐标以屏幕左上角为原点、向下为 y 正方向；最终由绘制层换算为裁剪空间。
//! 所有界面都通过本模块计算居中面板与内容区域，避免重复手写尺寸运算。

use super::theme::SCREEN_MARGIN;

/// 像素坐标的矩形；宽高为负时视为无效。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    /// 新建矩形。
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 右边界。
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// 下边界。
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// 判断点是否落在矩形内；用于鼠标命中测试。
    pub fn contains(self, x: f32, y: f32) -> bool {
        self.width > 0.0
            && self.height > 0.0
            && x >= self.x
            && x <= self.x + self.width
            && y >= self.y
            && y <= self.y + self.height
    }

    /// 四边等量内缩。
    pub fn inset(self, amount: f32) -> Self {
        self.inset_xy(amount, amount)
    }

    /// 水平与垂直分别内缩。
    pub fn inset_xy(self, horizontal: f32, vertical: f32) -> Self {
        Self {
            x: self.x + horizontal,
            y: self.y + vertical,
            width: self.width - horizontal * 2.0,
            height: self.height - vertical * 2.0,
        }
    }
}

/// 视口是否可用于布局与绘制。
pub fn valid_viewport(width: f32, height: f32) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

/// 居中面板：在给定最大尺寸与安全边距约束下取可用矩形。
pub fn centered_panel(viewport: (f32, f32), max_width: f32, max_height: f32) -> UiRect {
    let (width, height) = viewport;
    if !valid_viewport(width, height) {
        return UiRect::default();
    }
    let panel_width = max_width.min((width - SCREEN_MARGIN * 2.0).max(0.0));
    let panel_height = max_height.min((height - SCREEN_MARGIN * 2.0).max(0.0));
    UiRect::new(
        (width - panel_width) * 0.5,
        (height - panel_height) * 0.5,
        panel_width,
        panel_height,
    )
}

/// 把矩形限制在视口内；用于跟随鼠标的提示框。
pub fn clamp_to_viewport(rect: UiRect, viewport: (f32, f32), margin: f32) -> UiRect {
    let max_x = (viewport.0 - rect.width - margin).max(margin);
    let max_y = (viewport.1 - rect.height - margin).max(margin);
    UiRect {
        x: rect.x.clamp(margin, max_x),
        y: rect.y.clamp(margin, max_y),
        ..rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_panel_fits_small_viewports() {
        let panel = centered_panel((320.0, 200.0), 880.0, 600.0);
        assert!(panel.width <= 320.0 - SCREEN_MARGIN * 2.0 + 0.01);
        assert!(panel.height <= 200.0 - SCREEN_MARGIN * 2.0 + 0.01);
        assert!((panel.x + panel.width * 0.5 - 160.0).abs() < 0.01);
    }

    #[test]
    fn invalid_viewport_yields_empty_panel() {
        assert_eq!(
            centered_panel((0.0, 720.0), 880.0, 600.0),
            UiRect::default()
        );
        assert_eq!(
            centered_panel((f32::NAN, 720.0), 880.0, 600.0),
            UiRect::default()
        );
    }

    #[test]
    fn hit_testing_respects_bounds() {
        let rect = UiRect::new(10.0, 20.0, 30.0, 40.0);
        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(40.0, 60.0));
        assert!(!rect.contains(9.0, 20.0));
        assert!(!UiRect::new(10.0, 20.0, 0.0, 40.0).contains(10.0, 20.0));
    }

    #[test]
    fn clamping_keeps_tooltip_inside_viewport() {
        let rect = UiRect::new(1260.0, 700.0, 120.0, 60.0);
        let clamped = clamp_to_viewport(rect, (1280.0, 720.0), 4.0);
        assert!(clamped.right() <= 1280.0 - 4.0 + 0.01);
        assert!(clamped.bottom() <= 720.0 - 4.0 + 0.01);
    }
}

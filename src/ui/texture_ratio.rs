//! UI 贴图长宽比契约：集中规定控件贴图的绘制比例，避免单方向拉伸把图案压扁或拉长。
//!
//! 凡是登记了比例的控件（不论整图拉伸还是九宫格），绘制时都先把目标矩形收敛到
//! 规定比例（等比 contain、居中留白），再按矩形采样；因此布局尺寸变化不会改变贴图比例。
//! 九宫格控件若按“目标像素尺寸”出图，切边与中心可以做到 1:1 像素映射，完全不变形。
//! 只有比例随内容/窗口变化的控件（提示框、进度填充、全屏遮罩等）返回 `None`，
//! 表示随目标矩形自由铺满，不参与比例约束。
//!
//! 修改本文件即可调整代码中的贴图比例；人工可读的规范与代码位置见 `src/ui/README.md`。

use super::layout::UiRect;

/// 槽位类控件（快捷栏/背包/合成/烹饪）的目标比例：正方形。
pub const SLOT_RATIO: (u32, u32) = (1, 1);
/// 准星的目标比例：正方形。
pub const CROSSHAIR_RATIO: (u32, u32) = (1, 1);
/// 心形生命值三态的目标比例：正方形。
pub const HEART_RATIO: (u32, u32) = (1, 1);
/// 快捷栏底板的目标比例：与 `modules::hotbar_geometry` 的固定几何一致（184×24，即 23:3）。
pub const HOTBAR_BG_RATIO: (u32, u32) = super::modules::hotbar_geometry::BACKGROUND_RATIO;
/// 挖掘/进食进度条底板的目标比例：240×12。
pub const PROGRESS_BG_RATIO: (u32, u32) = (20, 1);

/// 查询控件贴图的目标长宽比（宽 : 高）。
///
/// 返回 `None` 表示该控件必须随目标矩形自由铺满（内容或窗口尺寸会改变比例），不做约束。
pub fn expected_ratio(name: &str) -> Option<(u32, u32)> {
    match name {
        "hotbar_slot"
        | "hotbar_slot_selected"
        | "inventory_slot"
        | "inventory_slot_hover"
        | "inventory_slot_selected" => Some(SLOT_RATIO),
        "crosshair" => Some(CROSSHAIR_RATIO),
        "heart_full" | "heart_half" | "heart_empty" => Some(HEART_RATIO),
        "hotbar_bg" => Some(HOTBAR_BG_RATIO),
        "progress_bg" => Some(PROGRESS_BG_RATIO),
        // 内容或窗口尺寸会改变比例：提示框、合成行、进度填充、全屏遮罩、共用面板等。
        _ => None,
    }
}

/// 把矩形收敛到指定比例：等比缩放到能完整放入原矩形（contain），并居中。
///
/// 比例无效或矩形非法时原样返回；比例已一致时返回原矩形，避免浮点误差引入抖动。
pub fn fit_rect(rect: UiRect, ratio: (u32, u32)) -> UiRect {
    let (ratio_width, ratio_height) = (ratio.0 as f32, ratio.1 as f32);
    if ratio_width <= 0.0 || ratio_height <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return rect;
    }

    let target = ratio_width / ratio_height;
    let current = rect.width / rect.height;
    if (current - target).abs() <= target * 1e-4 {
        return rect;
    }

    let (width, height) = if current > target {
        (rect.height * target, rect.height)
    } else {
        (rect.width, rect.width / target)
    };
    UiRect::new(
        rect.x + (rect.width - width) * 0.5,
        rect.y + (rect.height - height) * 0.5,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_ratio_letterboxes_a_wide_rect() {
        let fitted = fit_rect(UiRect::new(0.0, 0.0, 100.0, 50.0), (1, 1));
        assert_eq!(fitted, UiRect::new(25.0, 0.0, 50.0, 50.0));
    }

    #[test]
    fn square_ratio_pillarboxes_a_tall_rect() {
        let fitted = fit_rect(UiRect::new(10.0, 20.0, 40.0, 80.0), (1, 1));
        assert_eq!(fitted, UiRect::new(10.0, 40.0, 40.0, 40.0));
    }

    #[test]
    fn matching_ratio_is_returned_untouched() {
        let rect = UiRect::new(3.0, 5.0, 48.0, 48.0);
        assert_eq!(fit_rect(rect, (1, 1)), rect);
    }

    #[test]
    fn invalid_inputs_are_passed_through() {
        let rect = UiRect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(fit_rect(rect, (0, 1)), rect);
        let empty = UiRect::new(0.0, 0.0, 0.0, 10.0);
        assert_eq!(fit_rect(empty, (1, 1)), empty);
    }

    #[test]
    fn fixed_size_controls_declare_a_ratio_and_dynamic_ones_do_not() {
        assert_eq!(expected_ratio("hotbar_slot"), Some((1, 1)));
        assert_eq!(expected_ratio("heart_half"), Some((1, 1)));
        assert_eq!(expected_ratio("crosshair"), Some((1, 1)));
        assert_eq!(expected_ratio("hotbar_bg"), Some((23, 3)));
        assert_eq!(expected_ratio("progress_bg"), Some((20, 1)));
        assert_eq!(expected_ratio("death_overlay"), None);
        assert_eq!(expected_ratio("progress_fill"), None);
        assert_eq!(expected_ratio("tooltip_bg"), None);
        assert_eq!(expected_ratio("inventory_panel"), None);
    }
}

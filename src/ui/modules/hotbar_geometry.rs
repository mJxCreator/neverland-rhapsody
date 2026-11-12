//! 快捷栏三张贴图的固定几何契约：槽位 `16n`、选中态 `18n`、底板 `184n × 24n`，
//! 槽位间距与底板外侧留白均为 `4n`，选中态与槽位同心。
//!
//! 三者的比例与位置关系只在本模块定义：改 `hud.rs` 的槽位边长（即 n）时，
//! 其余尺寸按这里的单位关系自动推导，界面代码不再重复写间距/留白常数。
//! 人工可读的贴图规范见 `src/ui/README.md`。

use crate::ui::layout::UiRect;

/// 槽位贴图的宽度单位：16n。
pub const SLOT_UNITS: f32 = 16.0;
/// 选中槽位贴图的边长单位：18n，与槽位同心。
pub const SELECTED_UNITS: f32 = 18.0;
/// 槽位之间的间距单位：4n。
pub const GAP_UNITS: f32 = 4.0;
/// 底板相对槽位区的外侧留白单位：4n。
pub const MARGIN_UNITS: f32 = 4.0;
/// 底板贴图比例：9 格时为 184n : 24n = 23 : 3。
pub const BACKGROUND_RATIO: (u32, u32) = (23, 3);

/// 由槽位边长推导出的快捷栏像素几何。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotbarGeometry {
    /// 槽位边长（16n）。
    pub slot: f32,
    /// 槽位间距（4n）。
    pub gap: f32,
    /// 底板外侧留白（4n）。
    pub margin: f32,
    /// 选中槽位贴图边长（18n）。
    pub selected: f32,
}

impl HotbarGeometry {
    /// 由槽位边长（16n）推导全部尺寸；非法尺寸按 0 处理。
    pub fn from_slot_size(slot: f32) -> Self {
        let n = if slot.is_finite() && slot > 0.0 {
            slot / SLOT_UNITS
        } else {
            0.0
        };
        Self {
            slot,
            gap: n * GAP_UNITS,
            margin: n * MARGIN_UNITS,
            selected: n * SELECTED_UNITS,
        }
    }

    /// 9 格槽位连成的一行总宽度（不含底板外侧留白）。
    pub fn slots_width(&self, slots: usize) -> f32 {
        slots as f32 * self.slot + slots.saturating_sub(1) as f32 * self.gap
    }

    /// 覆盖整行槽位、四周各留 `4n` 的底板矩形。
    pub fn background_rect(&self, first_slot: UiRect, slots: usize) -> UiRect {
        UiRect::new(
            first_slot.x - self.margin,
            first_slot.y - self.margin,
            self.slots_width(slots) + self.margin * 2.0,
            self.slot + self.margin * 2.0,
        )
    }

    /// 选中槽位贴图矩形：以槽位中心为锚点放大到 `18n`。
    pub fn selected_rect(&self, slot_rect: UiRect) -> UiRect {
        let width = slot_rect.width * self.selected_scale();
        let height = slot_rect.height * self.selected_scale();
        UiRect::new(
            slot_rect.x - (width - slot_rect.width) * 0.5,
            slot_rect.y - (height - slot_rect.height) * 0.5,
            width,
            height,
        )
    }

    fn selected_scale(&self) -> f32 {
        if self.slot > 0.0 {
            self.selected / self.slot
        } else {
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> HotbarGeometry {
        HotbarGeometry::from_slot_size(48.0)
    }

    #[test]
    fn slot_size_derives_gap_margin_and_selected_size() {
        let geometry = geometry();
        assert_eq!(geometry.slot, 48.0);
        assert_eq!(geometry.gap, 12.0);
        assert_eq!(geometry.margin, 12.0);
        assert_eq!(geometry.selected, 54.0);
    }

    #[test]
    fn nine_slots_fill_a_184n_by_24n_background() {
        let geometry = geometry();
        let first = UiRect::new(100.0, 200.0, geometry.slot, geometry.slot);
        let background = geometry.background_rect(first, 9);
        assert_eq!(background, UiRect::new(88.0, 188.0, 552.0, 72.0));
        assert_eq!(552.0 / 72.0, 23.0 / 3.0);
        let (ratio_width, ratio_height) = BACKGROUND_RATIO;
        assert_eq!(
            background.width / background.height,
            ratio_width as f32 / ratio_height as f32
        );
    }

    #[test]
    fn selected_slot_stays_concentric_with_the_slot() {
        let geometry = geometry();
        let slot = UiRect::new(100.0, 200.0, geometry.slot, geometry.slot);
        let selected = geometry.selected_rect(slot);
        assert_eq!(selected, UiRect::new(97.0, 197.0, 54.0, 54.0));
        assert_eq!(
            selected.x + selected.width * 0.5,
            slot.x + slot.width * 0.5
        );
        assert_eq!(
            selected.y + selected.height * 0.5,
            slot.y + slot.height * 0.5
        );
    }

    #[test]
    fn invalid_slot_size_collapses_to_zero() {
        let geometry = HotbarGeometry::from_slot_size(f32::NAN);
        assert_eq!(geometry.gap, 0.0);
        assert_eq!(geometry.margin, 0.0);
        assert_eq!(geometry.selected, 0.0);
    }
}

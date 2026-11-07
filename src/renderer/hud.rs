//! HUD：屏幕空间准星、挖掘进度条与方块快捷栏。
//!
//! 全部内容由纯色四边形组成，直接写入动态顶点缓冲；不引入第三方 UI 框架。

use bytemuck::{Pod, Zeroable};

use crate::interaction::{DEFAULT_SLOTS, HOTBAR_SLOTS};
use crate::world::{Block, Face};

/// 一个 HUD 顶点：裁剪空间坐标与带透明度的颜色。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct HudVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

/// 准星单臂长度（像素）。
const CROSSHAIR_ARM: f32 = 9.0;
/// 准星线宽（像素）。
const CROSSHAIR_THICKNESS: f32 = 2.0;
/// 进度条宽度（像素）。
const PROGRESS_WIDTH: f32 = 240.0;
/// 进度条高度（像素）。
const PROGRESS_HEIGHT: f32 = 12.0;
/// 进度条与屏幕中心的垂直距离（像素）。
const PROGRESS_OFFSET: f32 = 34.0;
/// 快捷栏槽位边长（像素）。
const SLOT_SIZE: f32 = 48.0;
/// 槽位间距（像素）。
const SLOT_GAP: f32 = 8.0;
/// 快捷栏距屏幕底部的距离（像素）。
const BOTTOM_MARGIN: f32 = 18.0;
/// 槽位外框宽度（像素）。
const SLOT_BORDER: f32 = 3.0;
/// 槽位内部方块色块的留白（像素）。
const SLOT_PADDING: f32 = 7.0;

const CROSSHAIR_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const CROSSHAIR_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const PROGRESS_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const PROGRESS_FILL: [f32; 4] = [0.42, 0.85, 0.35, 0.95];
const HOTBAR_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const SLOT_BORDER_SELECTED: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
const SLOT_BORDER_IDLE: [f32; 4] = [0.55, 0.55, 0.55, 0.55];
const SLOT_GLOW: [f32; 4] = [1.0, 1.0, 1.0, 0.22];

/// 每帧由上层写入的 HUD 数据。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudModel {
    pub selected_slot: usize,
    pub slots: [Block; HOTBAR_SLOTS],
    /// 当前挖掘进度；`None` 表示不显示进度条。
    pub mining_progress: Option<f32>,
}

impl Default for HudModel {
    fn default() -> Self {
        Self {
            selected_slot: 0,
            slots: DEFAULT_SLOTS,
            mining_progress: None,
        }
    }
}

impl HudModel {
    /// 更新 HUD 数据；选中下标会被限制在当前槽位范围内。
    pub fn set(
        &mut self,
        selected_slot: usize,
        slots: [Block; HOTBAR_SLOTS],
        mining_progress: Option<f32>,
    ) {
        self.selected_slot = selected_slot.min(HOTBAR_SLOTS - 1);
        self.slots = slots;
        self.mining_progress = mining_progress.filter(|progress| progress.is_finite());
    }

    /// 生成全部 HUD 四边形顶点，写入 `out`。
    pub fn build(&self, out: &mut Vec<HudVertex>, width: f32, height: f32) {
        out.clear();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return;
        }

        self.push_hotbar(out, width, height);
        self.push_progress(out, width, height);
        push_crosshair(out, width, height);
    }

    fn push_hotbar(&self, out: &mut Vec<HudVertex>, width: f32, height: f32) {
        let viewport = (width, height);
        let total = HOTBAR_SLOTS as f32 * SLOT_SIZE + (HOTBAR_SLOTS - 1) as f32 * SLOT_GAP;
        let start_x = (width - total) * 0.5;
        let y = height - BOTTOM_MARGIN - SLOT_SIZE;

        for (index, block) in self.slots.iter().enumerate() {
            let x = start_x + index as f32 * (SLOT_SIZE + SLOT_GAP);
            let selected = index == self.selected_slot;

            if selected {
                push_quad(
                    out,
                    viewport,
                    HudRect {
                        x: x - 2.0,
                        y: y - 2.0,
                        width: SLOT_SIZE + 4.0,
                        height: SLOT_SIZE + 4.0,
                    },
                    SLOT_GLOW,
                );
            }

            push_quad(
                out,
                viewport,
                HudRect {
                    x,
                    y,
                    width: SLOT_SIZE,
                    height: SLOT_SIZE,
                },
                if selected {
                    SLOT_BORDER_SELECTED
                } else {
                    SLOT_BORDER_IDLE
                },
            );
            push_quad(
                out,
                viewport,
                HudRect {
                    x: x + SLOT_BORDER,
                    y: y + SLOT_BORDER,
                    width: SLOT_SIZE - SLOT_BORDER * 2.0,
                    height: SLOT_SIZE - SLOT_BORDER * 2.0,
                },
                HOTBAR_BACKGROUND,
            );

            let color = block.face_color(Face::PosY);
            push_quad(
                out,
                viewport,
                HudRect {
                    x: x + SLOT_PADDING,
                    y: y + SLOT_PADDING,
                    width: SLOT_SIZE - SLOT_PADDING * 2.0,
                    height: SLOT_SIZE - SLOT_PADDING * 2.0,
                },
                [color.x, color.y, color.z, 0.95],
            );
        }
    }

    fn push_progress(&self, out: &mut Vec<HudVertex>, width: f32, height: f32) {
        let Some(progress) = self.mining_progress else {
            return;
        };
        let viewport = (width, height);
        let progress = progress.clamp(0.0, 1.0);
        let x = (width - PROGRESS_WIDTH) * 0.5;
        let y = height * 0.5 + PROGRESS_OFFSET;

        push_quad(
            out,
            viewport,
            HudRect {
                x,
                y,
                width: PROGRESS_WIDTH,
                height: PROGRESS_HEIGHT,
            },
            PROGRESS_BACKGROUND,
        );
        push_quad(
            out,
            viewport,
            HudRect {
                x: x + 2.0,
                y: y + 2.0,
                width: (PROGRESS_WIDTH - 4.0) * progress,
                height: PROGRESS_HEIGHT - 4.0,
            },
            PROGRESS_FILL,
        );
    }
}

fn push_crosshair(out: &mut Vec<HudVertex>, width: f32, height: f32) {
    let center_x = width * 0.5;
    let center_y = height * 0.5;

    // 先画深色底衬，再画亮色线条，保证在草地与石头上都清晰可见。
    push_crosshair_layer(
        out,
        width,
        height,
        center_x,
        center_y,
        1.0,
        CROSSHAIR_SHADOW,
    );
    push_crosshair_layer(out, width, height, center_x, center_y, 0.0, CROSSHAIR_COLOR);
}

fn push_crosshair_layer(
    out: &mut Vec<HudVertex>,
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
    grow: f32,
    color: [f32; 4],
) {
    let arm = CROSSHAIR_ARM + grow;
    let thickness = CROSSHAIR_THICKNESS + grow * 2.0;
    let viewport = (width, height);
    push_quad(
        out,
        viewport,
        HudRect {
            x: center_x - arm,
            y: center_y - thickness * 0.5,
            width: arm * 2.0,
            height: thickness,
        },
        color,
    );
    push_quad(
        out,
        viewport,
        HudRect {
            x: center_x - thickness * 0.5,
            y: center_y - arm,
            width: thickness,
            height: arm * 2.0,
        },
        color,
    );
}

/// 像素坐标的矩形，用于生成 HUD 四边形。
#[derive(Clone, Copy, Debug)]
struct HudRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// 把像素坐标的矩形转为两个裁剪空间三角形。
fn push_quad(out: &mut Vec<HudVertex>, viewport: (f32, f32), rect: HudRect, color: [f32; 4]) {
    let (width, height) = viewport;
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    let x0 = rect.x / width * 2.0 - 1.0;
    let x1 = (rect.x + rect.width) / width * 2.0 - 1.0;
    let y0 = 1.0 - rect.y / height * 2.0;
    let y1 = 1.0 - (rect.y + rect.height) / height * 2.0;

    for (px, py) in [(x0, y0), (x0, y1), (x1, y1), (x0, y0), (x1, y1), (x1, y0)] {
        out.push(HudVertex {
            position: [px, py],
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_geometry_stays_inside_clip_space() {
        let mut vertices = Vec::new();
        let model = HudModel {
            mining_progress: Some(0.5),
            ..HudModel::default()
        };
        model.build(&mut vertices, 1280.0, 720.0);

        assert!(!vertices.is_empty());
        assert_eq!(vertices.len() % 6, 0, "HUD 顶点必须成三角形对");
        for vertex in &vertices {
            assert!((-1.0..=1.0).contains(&vertex.position[0]));
            assert!((-1.0..=1.0).contains(&vertex.position[1]));
        }
    }

    #[test]
    fn progress_bar_is_hidden_without_mining() {
        let mut with_progress = Vec::new();
        let mut without_progress = Vec::new();

        HudModel {
            mining_progress: Some(0.3),
            ..HudModel::default()
        }
        .build(&mut with_progress, 1280.0, 720.0);
        HudModel::default().build(&mut without_progress, 1280.0, 720.0);

        assert!(with_progress.len() > without_progress.len());
    }

    #[test]
    fn selected_slot_is_normalized() {
        let mut model = HudModel::default();
        model.set(100, DEFAULT_SLOTS, Some(2.0));
        assert_eq!(model.selected_slot, HOTBAR_SLOTS - 1);
        assert_eq!(model.mining_progress, Some(2.0));
        model.set(0, DEFAULT_SLOTS, Some(f32::NAN));
        assert_eq!(model.mining_progress, None);
    }

    #[test]
    fn invalid_viewport_produces_no_geometry() {
        let mut vertices = Vec::new();
        HudModel::default().build(&mut vertices, 0.0, 720.0);
        assert!(vertices.is_empty());
        HudModel::default().build(&mut vertices, f32::NAN, 720.0);
        assert!(vertices.is_empty());
    }
}

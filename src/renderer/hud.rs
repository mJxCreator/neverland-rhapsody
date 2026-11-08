//! HUD：屏幕空间准星、挖掘进度条、9 格快捷栏、选中工具名称/耐久与提示。
//!
//! 全部内容由四边形与内联位图字体组成，直接写入动态顶点缓冲；不引入第三方 UI 框架。
//! 纯色四边形使用 UV 哨兵，物品图标则按图集 UV 采样 PNG。
//! 背包打开时隐藏战斗与快捷栏 HUD，使界面焦点完全交给背包面板。

use bytemuck::{Pod, Zeroable};

use super::font;
use crate::item::{item_name, ItemId, ItemStack, HOTBAR_SLOTS};
use crate::texture::types::UVRect;
use crate::texture::ItemTextureMap;

/// HUD 顶点使用的 UV 哨兵值：负值表示该顶点不采样纹理，只使用颜色。
pub(crate) const SOLID_UV: [f32; 2] = [-1.0, -1.0];

/// 一个 HUD 顶点：裁剪空间坐标、图集 UV 与带透明度的颜色。
///
/// `uv` 为 [`SOLID_UV`] 时按纯色绘制；否则按物品图集中的子区域采样。
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct HudVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
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
/// 槽位内部色块留白（像素）。
const SLOT_PADDING: f32 = 7.0;
/// 拾取提示字号（像素格）。
const TOAST_PIXEL: f32 = 2.0;
/// 提示显示时长（秒）。
const TOAST_DURATION: f32 = 1.2;
/// 提示淡出时长（秒）。
const TOAST_FADE: f32 = 0.3;
/// 选中工具名称与快捷栏的距离（像素）。
const TOOL_LABEL_OFFSET: f32 = 50.0;
/// 提示文字与快捷栏的距离（像素）。
const TOAST_OFFSET: f32 = 70.0;
/// 生命值心形图标与快捷栏的距离（像素）。
const HEALTH_OFFSET: f32 = 28.0;
/// 单颗心形的像素边长。
const HEART_PIXEL: f32 = 3.0;
/// 心形之间的水平间距（像素）。
const HEART_GAP: f32 = 2.0;
/// 伤害闪屏持续时间（秒）。
const DAMAGE_FLASH_DURATION: f32 = 0.35;
/// 伤害闪屏边缘厚度（像素）。
const DAMAGE_FLASH_THICKNESS: f32 = 26.0;
/// 槽位耐久条高度（像素）。
const DURABILITY_BAR_HEIGHT: f32 = 4.0;
/// 耐久低于该比例时显示警示色。
const DURABILITY_LOW_THRESHOLD: f32 = 0.25;
/// 耐久低于该比例时显示中间过渡色。
const DURABILITY_MID_THRESHOLD: f32 = 0.5;

const CROSSHAIR_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const CROSSHAIR_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const PROGRESS_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const PROGRESS_FILL: [f32; 4] = [0.42, 0.85, 0.35, 0.95];
const HOTBAR_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const SLOT_BORDER_SELECTED: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
const SLOT_BORDER_IDLE: [f32; 4] = [0.55, 0.55, 0.55, 0.55];
const SLOT_GLOW: [f32; 4] = [1.0, 1.0, 1.0, 0.22];
const SLOT_EMPTY: [f32; 4] = [0.12, 0.12, 0.12, 0.55];
const COUNT_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const TEXT_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
const TOAST_TEXT: [f32; 4] = [1.0, 1.0, 0.85, 1.0];
const TOOL_LABEL_COLOR: [f32; 4] = [0.92, 0.94, 1.0, 1.0];
const TOOL_BROKEN_COLOR: [f32; 4] = [1.0, 0.42, 0.36, 1.0];
const DURABILITY_BAR_BACKGROUND: [f32; 4] = [0.0, 0.0, 0.0, 0.72];
const DURABILITY_BAR_BROKEN: [f32; 4] = [0.55, 0.12, 0.12, 0.85];
const DURABILITY_HIGH: [f32; 4] = [0.38, 0.86, 0.42, 0.95];
const DURABILITY_MID: [f32; 4] = [0.95, 0.80, 0.28, 0.95];
const DURABILITY_LOW: [f32; 4] = [0.95, 0.32, 0.26, 0.95];
const HEART_FULL: [f32; 4] = [0.86, 0.16, 0.16, 0.95];
const HEART_EMPTY: [f32; 4] = [0.28, 0.09, 0.09, 0.85];
const DAMAGE_FLASH: [f32; 4] = [0.72, 0.03, 0.03, 0.46];

/// 7×7 像素心形图案，1 表示实心格。
const HEART_PATTERN: [u8; 7] = [
    0b0110110, 0b1111111, 0b1111111, 0b1110111, 0b0110110, 0b0011100, 0b0001000,
];

/// 提示类型；决定绘制时的文案与颜色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToastKind {
    Pickup,
    ToolBroken,
}

/// 一条提示：物品、数量、类型与剩余显示时间。
#[derive(Clone, Copy, Debug, PartialEq)]
struct Toast {
    item: ItemId,
    count: u32,
    kind: ToastKind,
    remaining: f32,
}

/// 每帧由上层写入的 HUD 数据。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudModel {
    pub selected_slot: usize,
    pub slots: [Option<ItemStack>; HOTBAR_SLOTS],
    /// 当前挖掘进度；`None` 表示不显示进度条。
    pub mining_progress: Option<f32>,
    /// 背包/合成等全屏界面是否打开；打开时不绘制战斗与快捷栏 HUD。
    pub inventory_open: bool,
    /// 当前生命值（1 点 = 半颗心）。
    pub health: u32,
    /// 最大生命值（1 点 = 半颗心）。
    pub max_health: u32,
    damage_flash_remaining: f32,
    toast: Option<Toast>,
}

impl Default for HudModel {
    fn default() -> Self {
        Self {
            selected_slot: 0,
            slots: [None; HOTBAR_SLOTS],
            mining_progress: None,
            inventory_open: false,
            health: 20,
            max_health: 20,
            damage_flash_remaining: 0.0,
            toast: None,
        }
    }
}

impl HudModel {
    /// 更新 HUD 数据；选中下标会被限制在当前槽位范围内。
    pub fn set(
        &mut self,
        selected_slot: usize,
        slots: [Option<ItemStack>; HOTBAR_SLOTS],
        mining_progress: Option<f32>,
        inventory_open: bool,
    ) {
        self.selected_slot = selected_slot.min(HOTBAR_SLOTS - 1);
        self.slots = slots;
        self.mining_progress = mining_progress.filter(|progress| progress.is_finite());
        self.inventory_open = inventory_open;
    }

    /// 写入生命值；最大值为 0 时回落到默认 20 点。
    pub fn set_health(&mut self, health: u32, max_health: u32) {
        self.max_health = if max_health == 0 { 20 } else { max_health };
        self.health = health.min(self.max_health);
    }

    /// 受到伤害时触发短暂红色边缘反馈。
    pub fn show_damage(&mut self) {
        self.damage_flash_remaining = DAMAGE_FLASH_DURATION;
    }

    /// 显示一条拾取提示；重复拾取会刷新计时。
    pub fn show_pickup(&mut self, item: ItemId, count: u32) {
        if count == 0 || !item.is_valid() {
            return;
        }
        self.toast = Some(Toast {
            item,
            count,
            kind: ToastKind::Pickup,
            remaining: TOAST_DURATION,
        });
    }

    /// 显示工具损坏提示；这是耐久归零后的即时反馈。
    pub fn show_tool_broken(&mut self, item: ItemId) {
        if !item.is_valid() {
            return;
        }
        self.toast = Some(Toast {
            item,
            count: 0,
            kind: ToastKind::ToolBroken,
            remaining: TOAST_DURATION,
        });
    }

    /// 推进提示与伤害闪屏计时。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        self.damage_flash_remaining = (self.damage_flash_remaining - delta_secs).max(0.0);
        let Some(toast) = &mut self.toast else {
            return;
        };
        toast.remaining -= delta_secs;
        if toast.remaining <= 0.0 {
            self.toast = None;
        }
    }

    /// 生成全部 HUD 四边形顶点，写入 `out`；`icons` 提供物品图标的图集 UV。
    pub fn build(&self, out: &mut Vec<HudVertex>, width: f32, height: f32, icons: &ItemTextureMap) {
        out.clear();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return;
        }
        if self.inventory_open {
            return;
        }

        self.push_health(out, width, height);
        self.push_hotbar(out, width, height, icons);
        self.push_tool_label(out, width, height);
        self.push_toast(out, width, height);
        self.push_progress(out, width, height);
        self.push_damage_flash(out, width, height);
        push_crosshair(out, width, height);
    }

    /// 心形生命值：满心、半心与空心按 2 点生命值换算。
    fn push_health(&self, out: &mut Vec<HudVertex>, width: f32, height: f32) {
        let viewport = (width, height);
        let hearts = self.max_health.div_ceil(2) as usize;
        if hearts == 0 {
            return;
        }
        let heart_size = HEART_PIXEL * HEART_PATTERN.len() as f32;
        let total_width =
            hearts as f32 * heart_size + (hearts.saturating_sub(1)) as f32 * HEART_GAP;
        let start_x = (width - total_width) * 0.5;
        let y = height - BOTTOM_MARGIN - SLOT_SIZE - HEALTH_OFFSET - heart_size;
        for index in 0..hearts {
            let x = start_x + index as f32 * (heart_size + HEART_GAP);
            let remaining = (self.health as i64 - (index as i64) * 2).clamp(0, 2);
            let fill = if remaining >= 2 {
                HeartFill::Full
            } else if remaining == 1 {
                HeartFill::Half
            } else {
                HeartFill::Empty
            };
            push_heart(out, viewport, x, y, fill);
        }
    }

    /// 受伤后覆盖屏幕四周的红色反馈，随时间淡出。
    fn push_damage_flash(&self, out: &mut Vec<HudVertex>, width: f32, height: f32) {
        if self.damage_flash_remaining <= 0.0 {
            return;
        }
        let alpha = (self.damage_flash_remaining / DAMAGE_FLASH_DURATION).clamp(0.0, 1.0);
        let mut color = DAMAGE_FLASH;
        color[3] *= alpha;
        let viewport = (width, height);
        let thickness = DAMAGE_FLASH_THICKNESS;
        push_quad(
            out,
            viewport,
            HudRect {
                x: 0.0,
                y: 0.0,
                width,
                height: thickness,
            },
            color,
        );
        push_quad(
            out,
            viewport,
            HudRect {
                x: 0.0,
                y: height - thickness,
                width,
                height: thickness,
            },
            color,
        );
        push_quad(
            out,
            viewport,
            HudRect {
                x: 0.0,
                y: thickness,
                width: thickness,
                height: height - thickness * 2.0,
            },
            color,
        );
        push_quad(
            out,
            viewport,
            HudRect {
                x: width - thickness,
                y: thickness,
                width: thickness,
                height: height - thickness * 2.0,
            },
            color,
        );
    }

    fn push_hotbar(
        &self,
        out: &mut Vec<HudVertex>,
        width: f32,
        height: f32,
        icons: &ItemTextureMap,
    ) {
        let viewport = (width, height);
        let total = HOTBAR_SLOTS as f32 * SLOT_SIZE + (HOTBAR_SLOTS - 1) as f32 * SLOT_GAP;
        let start_x = (width - total) * 0.5;
        let y = height - BOTTOM_MARGIN - SLOT_SIZE;

        for index in 0..HOTBAR_SLOTS {
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

            let inner = HudRect {
                x: x + SLOT_PADDING,
                y: y + SLOT_PADDING,
                width: SLOT_SIZE - SLOT_PADDING * 2.0,
                height: SLOT_SIZE - SLOT_PADDING * 2.0,
            };
            match self.slots[index] {
                None => push_quad(out, viewport, inner, SLOT_EMPTY),
                Some(stack) => {
                    push_item_icon(out, viewport, inner, stack.item(), icons);
                    push_slot_durability(out, viewport, x, y, stack);
                    if stack.count() > 1 {
                        let text = stack.count().to_string();
                        let text_width = font::text_width(&text, 2.0);
                        let text_y = y + SLOT_SIZE - 4.0 - font::GLYPH_HEIGHT as f32 * 2.0;
                        let text_x = x + SLOT_SIZE - 4.0 - text_width;
                        font::push_text(
                            out,
                            viewport,
                            text_x + 1.0,
                            text_y + 1.0,
                            2.0,
                            TEXT_SHADOW,
                            &text,
                        );
                        font::push_text(out, viewport, text_x, text_y, 2.0, COUNT_TEXT, &text);
                    }
                }
            }
        }
    }

    /// 选中工具的名称与损坏状态；普通方块物品不显示。
    fn push_tool_label(&self, out: &mut Vec<HudVertex>, width: f32, height: f32) {
        let Some(stack) = self.slots[self.selected_slot] else {
            return;
        };
        if !stack.is_tool() {
            return;
        }

        let text = if stack.is_broken() {
            format!("BROKEN {}", item_name(stack.item()))
        } else {
            item_name(stack.item()).to_string()
        };
        let color = if stack.is_broken() {
            TOOL_BROKEN_COLOR
        } else {
            TOOL_LABEL_COLOR
        };
        let text_width = font::text_width(&text, TOAST_PIXEL);
        let x = (width - text_width) * 0.5;
        let y = height - BOTTOM_MARGIN - SLOT_SIZE - TOOL_LABEL_OFFSET;
        font::push_text(
            out,
            (width, height),
            x + 1.0,
            y + 1.0,
            TOAST_PIXEL,
            [0.0, 0.0, 0.0, 0.85],
            &text,
        );
        font::push_text(out, (width, height), x, y, TOAST_PIXEL, color, &text);
    }

    fn push_toast(&self, out: &mut Vec<HudVertex>, width: f32, height: f32) {
        let Some(toast) = self.toast else {
            return;
        };
        let alpha = (toast.remaining / TOAST_FADE).clamp(0.0, 1.0);
        let (text, base_color) = match toast.kind {
            ToastKind::Pickup => (
                format!("+{} {}", toast.count, item_name(toast.item)),
                TOAST_TEXT,
            ),
            ToastKind::ToolBroken => (
                format!("{} BROKEN", item_name(toast.item)),
                TOOL_BROKEN_COLOR,
            ),
        };
        let text_width = font::text_width(&text, TOAST_PIXEL);
        let x = (width - text_width) * 0.5;
        let y = height - BOTTOM_MARGIN - SLOT_SIZE - TOAST_OFFSET;
        font::push_text(
            out,
            (width, height),
            x + 1.0,
            y + 1.0,
            TOAST_PIXEL,
            [0.0, 0.0, 0.0, 0.85 * alpha],
            &text,
        );
        let mut color = base_color;
        color[3] *= alpha;
        font::push_text(out, (width, height), x, y, TOAST_PIXEL, color, &text);
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

/// 快捷栏槽位底部的耐久条；普通方块物品不绘制。
fn push_slot_durability(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    x: f32,
    y: f32,
    stack: ItemStack,
) {
    let Some(max) = stack.max_durability() else {
        return;
    };
    if max == 0 {
        return;
    }

    let current = stack.durability().unwrap_or(max);
    let ratio = (current as f32 / max as f32).clamp(0.0, 1.0);
    let bar = HudRect {
        x: x + SLOT_PADDING,
        y: y + SLOT_SIZE - SLOT_PADDING - DURABILITY_BAR_HEIGHT,
        width: SLOT_SIZE - SLOT_PADDING * 2.0,
        height: DURABILITY_BAR_HEIGHT,
    };
    let background = if current == 0 {
        DURABILITY_BAR_BROKEN
    } else {
        DURABILITY_BAR_BACKGROUND
    };
    push_quad(out, viewport, bar, background);
    if ratio > 0.0 {
        push_quad(
            out,
            viewport,
            HudRect {
                width: bar.width * ratio,
                ..bar
            },
            durability_color(ratio),
        );
    }
}

/// 按剩余耐久比例选择颜色：低于 25% 警示，低于 50% 过渡。
pub(crate) fn durability_color(ratio: f32) -> [f32; 4] {
    if ratio <= DURABILITY_LOW_THRESHOLD {
        DURABILITY_LOW
    } else if ratio <= DURABILITY_MID_THRESHOLD {
        DURABILITY_MID
    } else {
        DURABILITY_HIGH
    }
}

/// 心形填充状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeartFill {
    Full,
    Half,
    Empty,
}

/// 绘制一颗 7×7 像素心形；半心只填充图案左半部分。
fn push_heart(out: &mut Vec<HudVertex>, viewport: (f32, f32), x: f32, y: f32, fill: HeartFill) {
    let heart_columns = HEART_PATTERN.len();
    for (row, bits) in HEART_PATTERN.iter().enumerate() {
        let mut column = 0;
        while column < heart_columns {
            let filled = bits & (1 << (heart_columns - 1 - column)) != 0;
            if !filled {
                column += 1;
                continue;
            }
            let start = column;
            while column < heart_columns
                && HEART_PATTERN[row] & (1 << (heart_columns - 1 - column)) != 0
            {
                column += 1;
            }
            let is_visible = match fill {
                HeartFill::Full => true,
                HeartFill::Half => start <= heart_columns / 2,
                HeartFill::Empty => false,
            };
            let color = if is_visible { HEART_FULL } else { HEART_EMPTY };
            let visible_end = match fill {
                HeartFill::Half => column.min(heart_columns / 2 + 1),
                _ => column,
            };
            if !is_visible {
                push_quad(
                    out,
                    viewport,
                    HudRect {
                        x: x + start as f32 * HEART_PIXEL,
                        y: y + row as f32 * HEART_PIXEL,
                        width: (column - start) as f32 * HEART_PIXEL,
                        height: HEART_PIXEL,
                    },
                    HEART_EMPTY,
                );
                continue;
            }
            push_quad(
                out,
                viewport,
                HudRect {
                    x: x + start as f32 * HEART_PIXEL,
                    y: y + row as f32 * HEART_PIXEL,
                    width: (visible_end - start) as f32 * HEART_PIXEL,
                    height: HEART_PIXEL,
                },
                color,
            );
        }
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
pub(crate) struct HudRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// 把像素坐标的矩形转为两个裁剪空间三角形，使用纯色绘制。
pub(crate) fn push_quad(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    color: [f32; 4],
) {
    push_quad_uv(out, viewport, rect, [SOLID_UV; 4], color);
}

/// 绘制一张物品图集中的图标；没有图标（未配置或显式隐藏）时不产生几何。
pub(crate) fn push_item_icon(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    item: ItemId,
    icons: &ItemTextureMap,
) {
    let Some(icon) = icons.icon(item) else {
        return;
    };
    push_textured_quad(out, viewport, rect, icon.uv, [1.0, 1.0, 1.0, 1.0]);
}

/// 按图集 UV 子区域采样绘制矩形；`uv` 为 `min` 左上、`max` 右下。
pub(crate) fn push_textured_quad(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    uv: UVRect,
    color: [f32; 4],
) {
    if uv.max[0] <= uv.min[0] || uv.max[1] <= uv.min[1] {
        return;
    }
    // 顶点顺序与 push_quad_uv 的四角一致，UV 与位置一一对应；纹理 v 轴向下。
    push_quad_uv(
        out,
        viewport,
        rect,
        [
            [uv.min[0], uv.min[1]],
            [uv.min[0], uv.max[1]],
            [uv.max[0], uv.max[1]],
            [uv.max[0], uv.min[1]],
        ],
        color,
    );
}

/// 顶点顺序：左上、左下、右下、左上、右下、右上。
fn push_quad_uv(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    uvs: [[f32; 2]; 4],
    color: [f32; 4],
) {
    let (width, height) = viewport;
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    let x0 = rect.x / width * 2.0 - 1.0;
    let x1 = (rect.x + rect.width) / width * 2.0 - 1.0;
    let y0 = 1.0 - rect.y / height * 2.0;
    let y1 = 1.0 - (rect.y + rect.height) / height * 2.0;

    for (px, py, uv) in [
        (x0, y0, uvs[0]),
        (x0, y1, uvs[1]),
        (x1, y1, uvs[2]),
        (x0, y0, uvs[0]),
        (x1, y1, uvs[2]),
        (x1, y0, uvs[3]),
    ] {
        out.push(HudVertex {
            position: [px, py],
            uv,
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stone(count: u32) -> ItemStack {
        ItemStack::new(ItemId::STONE, count).expect("合法堆叠")
    }

    fn pickaxe(durability: u32) -> ItemStack {
        ItemStack::new(ItemId::WOODEN_PICKAXE, 1)
            .expect("工具堆叠合法")
            .with_durability(durability)
    }

    /// 构造只包含石头图标的映射，UV 固定为图集中的一个子区域。
    fn stone_icons() -> ItemTextureMap {
        use crate::texture::item_texture::builtin_manifest;
        use crate::texture::texture_registry::{TextureRegistry, TextureSeed};
        use crate::texture::types::{TextureType, Transparency};

        let mut registry = TextureRegistry::new();
        let id = registry
            .register(TextureSeed {
                name: "stone".to_string(),
                texture_type: TextureType::Item,
                resource_path: "assets/textures/items/stone.png".to_string(),
                width: 16,
                height: 16,
                transparency: Transparency::Opaque,
                placeholder: false,
            })
            .expect("登记图标纹理");
        registry
            .apply_uv(
                id,
                UVRect {
                    min: [0.25, 0.5],
                    max: [0.5, 0.75],
                },
            )
            .expect("写入 UV");
        ItemTextureMap::from_manifest(&builtin_manifest(), &registry, id, &mut Vec::new())
            .expect("构建图标映射")
    }

    #[test]
    fn hotbar_draws_item_icons_with_atlas_uvs() {
        let mut slots = [None; HOTBAR_SLOTS];
        slots[0] = Some(stone(1));
        let mut vertices = Vec::new();
        HudModel {
            slots,
            ..HudModel::default()
        }
        .build(&mut vertices, 1280.0, 720.0, &stone_icons());

        let textured: Vec<&HudVertex> = vertices.iter().filter(|v| v.uv[0] >= 0.0).collect();
        assert!(!textured.is_empty(), "快捷栏必须采样物品图标");
        // 首个四边形按左上、左下、右下顺序生成，用于校验 UV 未被上下翻转。
        assert_eq!(textured[0].uv, [0.25, 0.5]);
        assert_eq!(textured[1].uv, [0.25, 0.75]);
        assert_eq!(textured[2].uv, [0.5, 0.75]);
        for vertex in textured {
            assert!((0.0..=1.0).contains(&vertex.uv[0]));
            assert!((0.0..=1.0).contains(&vertex.uv[1]));
        }
        assert!(
            vertices.iter().any(|v| v.uv == SOLID_UV),
            "槽位背景与数量文字仍应使用纯色绘制"
        );
    }

    #[test]
    fn a_slot_without_an_icon_draws_no_textured_geometry() {
        let mut slots = [None; HOTBAR_SLOTS];
        slots[0] = Some(stone(1));
        let mut vertices = Vec::new();
        HudModel {
            slots,
            ..HudModel::default()
        }
        .build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());
        assert!(vertices.iter().all(|vertex| vertex.uv == SOLID_UV));
    }

    #[test]
    fn hud_geometry_stays_inside_clip_space() {
        let mut vertices = Vec::new();
        let mut slots = [None; HOTBAR_SLOTS];
        slots[0] = Some(stone(3));
        let model = HudModel {
            mining_progress: Some(0.5),
            slots,
            ..HudModel::default()
        };
        model.build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());

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
        .build(&mut with_progress, 1280.0, 720.0, &ItemTextureMap::empty());
        HudModel::default().build(
            &mut without_progress,
            1280.0,
            720.0,
            &ItemTextureMap::empty(),
        );

        assert!(with_progress.len() > without_progress.len());
    }

    #[test]
    fn selected_slot_is_normalized() {
        let mut model = HudModel::default();
        model.set(100, [None; HOTBAR_SLOTS], Some(2.0), false);
        assert_eq!(model.selected_slot, HOTBAR_SLOTS - 1);
        assert_eq!(model.mining_progress, Some(2.0));
        model.set(0, [None; HOTBAR_SLOTS], Some(f32::NAN), false);
        assert_eq!(model.mining_progress, None);
    }

    #[test]
    fn invalid_viewport_produces_no_geometry() {
        let mut vertices = Vec::new();
        HudModel::default().build(&mut vertices, 0.0, 720.0, &ItemTextureMap::empty());
        assert!(vertices.is_empty());
        HudModel::default().build(&mut vertices, f32::NAN, 720.0, &ItemTextureMap::empty());
        assert!(vertices.is_empty());
    }

    #[test]
    fn an_open_inventory_hides_the_hud() {
        let mut vertices = Vec::new();
        let mut model = HudModel {
            mining_progress: Some(0.5),
            ..HudModel::default()
        };
        model.set(0, [None; HOTBAR_SLOTS], Some(0.5), true);
        model.build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());
        assert!(vertices.is_empty());
    }

    #[test]
    fn toast_expires_and_adds_geometry() {
        let mut model = HudModel::default();
        let mut without_toast = Vec::new();
        model.build(&mut without_toast, 1280.0, 720.0, &ItemTextureMap::empty());

        model.show_pickup(ItemId::STONE, 2);
        assert!(model.toast.is_some());
        let mut with_toast = Vec::new();
        model.build(&mut with_toast, 1280.0, 720.0, &ItemTextureMap::empty());
        assert!(with_toast.len() > without_toast.len());

        model.update(TOAST_DURATION + 0.1);
        assert!(model.toast.is_none());
    }

    #[test]
    fn durability_bars_only_apply_to_tools() {
        let viewport = (1280.0, 720.0);

        let mut block_vertices = Vec::new();
        push_slot_durability(&mut block_vertices, viewport, 0.0, 0.0, stone(1));
        assert!(block_vertices.is_empty(), "普通方块物品没有耐久条");

        let mut tool_vertices = Vec::new();
        push_slot_durability(&mut tool_vertices, viewport, 0.0, 0.0, pickaxe(30));
        assert_eq!(tool_vertices.len(), 12, "耐久条背景 + 填充");

        let mut broken_vertices = Vec::new();
        push_slot_durability(&mut broken_vertices, viewport, 0.0, 0.0, pickaxe(0));
        assert_eq!(broken_vertices.len(), 6, "损坏工具只绘制警示背景，没有填充");
    }

    #[test]
    fn durability_color_warns_at_low_and_medium_levels() {
        assert_eq!(durability_color(1.0), DURABILITY_HIGH);
        assert_eq!(durability_color(0.6), DURABILITY_HIGH);
        assert_eq!(durability_color(0.4), DURABILITY_MID);
        assert_eq!(durability_color(0.2), DURABILITY_LOW);
        assert_eq!(durability_color(0.0), DURABILITY_LOW);
    }

    #[test]
    fn the_selected_tool_shows_its_name_and_broken_state() {
        let mut slots = [None; HOTBAR_SLOTS];
        slots[0] = Some(pickaxe(60));
        let mut vertices = Vec::new();
        HudModel {
            slots,
            ..HudModel::default()
        }
        .build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());

        let mut empty_vertices = Vec::new();
        HudModel::default().build(&mut empty_vertices, 1280.0, 720.0, &ItemTextureMap::empty());
        assert!(
            vertices.len() > empty_vertices.len(),
            "选中工具必须显示名称"
        );

        let mut broken_slots = [None; HOTBAR_SLOTS];
        broken_slots[0] = Some(pickaxe(0));
        let mut broken_vertices = Vec::new();
        HudModel {
            slots: broken_slots,
            ..HudModel::default()
        }
        .build(
            &mut broken_vertices,
            1280.0,
            720.0,
            &ItemTextureMap::empty(),
        );
        assert!(
            broken_vertices.len() >= vertices.len(),
            "损坏状态仍显示名称与警示"
        );
    }

    #[test]
    fn health_is_clamped_and_defaults_are_safe() {
        let mut model = HudModel::default();
        model.set_health(50, 20);
        assert_eq!(model.health, 20);
        assert_eq!(model.max_health, 20);
        model.set_health(5, 0);
        assert_eq!(model.max_health, 20, "非法最大值回落到默认值");
        assert_eq!(model.health, 5);
    }

    #[test]
    fn half_hearts_render_a_partial_red_fill() {
        let mut vertices = Vec::new();
        let mut model = HudModel::default();
        model.set_health(19, 20);
        model.build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());

        assert!(
            vertices.iter().any(|vertex| vertex.color == HEART_EMPTY),
            "19/20 应出现半颗心，右半部分保持空心色"
        );
    }

    #[test]
    fn damage_flash_adds_edge_geometry_and_fades_out() {
        let mut model = HudModel::default();
        let mut plain = Vec::new();
        model.build(&mut plain, 1280.0, 720.0, &ItemTextureMap::empty());

        model.show_damage();
        let mut damaged = Vec::new();
        model.build(&mut damaged, 1280.0, 720.0, &ItemTextureMap::empty());
        assert!(damaged.len() > plain.len(), "受伤反馈应额外绘制屏幕边缘");

        model.update(DAMAGE_FLASH_DURATION + 0.1);
        let mut recovered = Vec::new();
        model.build(&mut recovered, 1280.0, 720.0, &ItemTextureMap::empty());
        assert_eq!(recovered.len(), plain.len(), "闪屏结束后恢复原 HUD");
    }
}

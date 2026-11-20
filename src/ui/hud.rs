//! HUD：屏幕空间准星、挖掘进度条、9 格快捷栏、选中物品名称/耐久、拾取提示与生命值。
//!
//! 全部内容由四边形与内联位图字体组成，直接写入共享的 UI 动态顶点缓冲；
//! 纯色四边形使用 UV 哨兵，物品图标则按图集 UV 采样 PNG。
//! 任何全屏界面（背包、合成、死亡）打开时隐藏 HUD，使界面焦点完全交给面板。

use super::animation::UiClock;
use super::draw::{push_item_icon, push_quad, push_ui_view, UiVertex};
use super::font;
use super::layout::{valid_viewport, UiRect};
use super::modules::hotbar_geometry;
use super::theme;
use super::widget::{self, SlotSkin, SlotState};
use crate::item::{item_name, ItemId, ItemStack, HOTBAR_SLOTS};
use crate::texture::{ItemTextureMap, UiTextureMap};

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
/// 快捷栏槽位边长（像素）；贴图按 16n 设计，此处 n = 3。
const SLOT_SIZE: f32 = 48.0;
/// 快捷栏距屏幕底部的距离（像素）。
const BOTTOM_MARGIN: f32 = 18.0;
/// 拾取提示字号（像素格）。
const TOAST_PIXEL: f32 = 2.0;
/// 提示显示时长（秒）。
const TOAST_DURATION: f32 = 1.2;
/// 提示淡出时长（秒）。
const TOAST_FADE: f32 = 0.3;
/// 提示图标边长（像素）。
const TOAST_ICON_SIZE: f32 = 22.0;
/// 选中物品名称与快捷栏的距离（像素）；位于状态条上方。
const TOOL_LABEL_OFFSET: f32 = 108.0;
/// 提示文字与快捷栏的距离（像素）；位于物品名称上方。
const TOAST_OFFSET: f32 = 132.0;
/// 状态条与快捷栏的距离（像素）。
const HEALTH_OFFSET: f32 = 28.0;
/// 状态条原图像素到屏幕像素的缩放倍率。
const STATE_SCALE: f32 = 3.0;
/// 状态条背景原图尺寸（106×19）。
const STATE_BG_WIDTH: f32 = 106.0;
const STATE_BG_HEIGHT: f32 = 19.0;
/// 单条填充原图尺寸（102×5）。
const STATE_FILL_WIDTH: f32 = 102.0;
const STATE_FILL_HEIGHT: f32 = 5.0;
/// 背景与填充之间的内边距（原图像素）。
const STATE_PADDING: f32 = 2.0;
/// 状态条缺失贴图时的回退颜色。
const STATE_BG_COLOR: [f32; 4] = [0.08, 0.09, 0.11, 0.85];
const STATE_HEART_COLOR: [f32; 4] = [0.86, 0.20, 0.24, 0.95];
const STATE_STAMINA_COLOR: [f32; 4] = [0.38, 0.76, 0.96, 0.95];
const STATE_HUNGER_COLOR: [f32; 4] = [0.80, 0.42, 0.16, 0.95];
/// 伤害闪屏持续时间（秒）。
const DAMAGE_FLASH_DURATION: f32 = 0.35;
/// 伤害闪屏边缘厚度（像素）。
const DAMAGE_FLASH_THICKNESS: f32 = 26.0;
/// 敌对生物名牌在停止攻击/离开准星后的显示时长（秒）。
const TARGET_DURATION: f32 = 2.5;
/// 敌对生物血条宽度（像素）。
const TARGET_BAR_WIDTH: f32 = 180.0;
/// 敌对生物血条高度（像素）。
const TARGET_BAR_HEIGHT: f32 = 9.0;
/// 敌对生物名牌与生命值之间的垂直距离（像素）。
const TARGET_GAP: f32 = 34.0;
/// 受击方向指示的显示时长（秒）。
const DAMAGE_DIRECTION_DURATION: f32 = 0.8;
/// 受击方向指示的单块边长（像素）。
const DAMAGE_DIRECTION_SIZE: f32 = 18.0;
/// 选中格光晕的呼吸速度。
const SELECTED_PULSE_SPEED: f32 = 4.0;

/// 提示消息显示时长（秒）。
const MESSAGE_DURATION: f32 = 2.5;
/// 世界时间文本颜色。
const TIME_TEXT: [f32; 4] = [0.82, 0.86, 0.95, 0.9];

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

/// 当前显示血量的敌对生物。
#[derive(Clone, Copy, Debug, PartialEq)]
struct TargetHealth {
    name: &'static str,
    health: u32,
    max_health: u32,
}

/// 每帧由上层写入的 HUD 数据。
#[derive(Clone, Debug, PartialEq)]
pub struct HudModel {
    pub selected_slot: usize,
    pub slots: [Option<ItemStack>; HOTBAR_SLOTS],
    /// 当前挖掘进度；`None` 表示不显示进度条。
    pub mining_progress: Option<f32>,
    /// 任意全屏界面是否打开；打开时不绘制战斗与快捷栏 HUD。
    pub ui_open: bool,
    /// 当前生命值（0–100）。
    pub health: u32,
    /// 最大生命值（默认 100）。
    pub max_health: u32,
    /// 当前饥饿值（0–100）。
    pub hunger: f32,
    /// 当前体力值（0–100）。
    pub stamina: f32,
    /// 当前昼夜阶段标签。
    pub time_label: &'static str,
    /// 进食进度；`None` 表示未进食。
    pub eating_progress: Option<f32>,
    /// 简短状态提示（睡眠失败、成功等）。
    message: Option<(&'static str, f32)>,
    damage_flash_remaining: f32,
    /// 当前显示血量的敌对生物；`None` 时不绘制名牌。
    target: Option<TargetHealth>,
    target_remaining: f32,
    /// 受击方向（弧度，0 为屏幕正前方，正值在右）；`None` 时不绘制。
    damage_direction: Option<f32>,
    damage_direction_remaining: f32,
    toast: Option<Toast>,
    clock: UiClock,
    /// UI 控件图集；为空时全部回退纯色绘制。
    ui_textures: UiTextureMap,
}

impl Default for HudModel {
    fn default() -> Self {
        Self {
            selected_slot: 0,
            slots: [None; HOTBAR_SLOTS],
            mining_progress: None,
            ui_open: false,
            health: 100,
            max_health: 100,
            hunger: 100.0,
            stamina: 100.0,
            time_label: "DAY",
            eating_progress: None,
            message: None,
            damage_flash_remaining: 0.0,
            target: None,
            target_remaining: 0.0,
            damage_direction: None,
            damage_direction_remaining: 0.0,
            toast: None,
            clock: UiClock::default(),
            ui_textures: UiTextureMap::empty(),
        }
    }
}

impl HudModel {
    /// 替换 UI 控件图集；资源加载或热重载后由渲染器调用。
    pub fn set_ui_textures(&mut self, textures: UiTextureMap) {
        self.ui_textures = textures;
    }

    /// 更新 HUD 数据；选中下标会被限制在当前槽位范围内。
    pub fn set(
        &mut self,
        selected_slot: usize,
        slots: [Option<ItemStack>; HOTBAR_SLOTS],
        mining_progress: Option<f32>,
        ui_open: bool,
    ) {
        self.selected_slot = selected_slot.min(HOTBAR_SLOTS - 1);
        self.slots = slots;
        self.mining_progress = mining_progress.filter(|progress| progress.is_finite());
        self.ui_open = ui_open;
    }

    /// 写入生命值；最大值为 0 时回落到默认 100 点。
    pub fn set_health(&mut self, health: u32, max_health: u32) {
        self.max_health = if max_health == 0 { 100 } else { max_health };
        self.health = health.min(self.max_health);
    }

    /// 写入饥饿、体力与昼夜阶段；数值越界时自动裁剪到 0–100。
    pub fn set_survival(&mut self, hunger: f32, stamina: f32, time_label: &'static str) {
        self.hunger = if hunger.is_finite() {
            hunger.clamp(0.0, 100.0)
        } else {
            100.0
        };
        self.stamina = if stamina.is_finite() {
            stamina.clamp(0.0, 100.0)
        } else {
            100.0
        };
        self.time_label = time_label;
    }

    /// 设置进食进度；`None` 表示未进食。
    pub fn set_eating_progress(&mut self, progress: Option<f32>) {
        self.eating_progress = progress.filter(|value| value.is_finite());
    }

    /// 显示一条短状态提示，覆盖旧提示并重新计时。
    pub fn show_message(&mut self, text: &'static str) {
        self.message = Some((text, MESSAGE_DURATION));
    }

    /// 受到伤害时触发短暂红色边缘反馈。
    pub fn show_damage(&mut self) {
        self.damage_flash_remaining = DAMAGE_FLASH_DURATION;
    }

    /// 显示敌对生物血量；持续 2.5 秒，重复调用会刷新计时。
    pub fn show_target(&mut self, name: &'static str, health: u32, max_health: u32) {
        if max_health == 0 {
            return;
        }
        self.target = Some(TargetHealth {
            name,
            health: health.min(max_health),
            max_health,
        });
        self.target_remaining = TARGET_DURATION;
    }

    /// 显示受击方向；`angle` 为相对镜头朝向的弧度（0 为正前方，正值在右）。
    pub fn show_damage_direction(&mut self, angle: f32) {
        if !angle.is_finite() {
            return;
        }
        self.damage_direction = Some(angle);
        self.damage_direction_remaining = DAMAGE_DIRECTION_DURATION;
    }

    /// 显示一条拾取提示；同一物品重复拾取会合并数量并刷新计时。
    pub fn show_pickup(&mut self, item: ItemId, count: u32) {
        if count == 0 || !item.is_valid() {
            return;
        }
        if let Some(toast) = &mut self.toast {
            if toast.kind == ToastKind::Pickup && toast.item == item {
                toast.count = toast.count.saturating_add(count);
                toast.remaining = TOAST_DURATION;
                return;
            }
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

    /// 推进提示、伤害闪屏与选中动画计时。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        self.clock.update(delta_secs);
        self.damage_flash_remaining = (self.damage_flash_remaining - delta_secs).max(0.0);
        if self.target_remaining > 0.0 {
            self.target_remaining -= delta_secs;
            if self.target_remaining <= 0.0 {
                self.target = None;
            }
        }
        if self.damage_direction_remaining > 0.0 {
            self.damage_direction_remaining -= delta_secs;
            if self.damage_direction_remaining <= 0.0 {
                self.damage_direction = None;
            }
        }
        if let Some((_, remaining)) = &mut self.message {
            *remaining -= delta_secs;
            if *remaining <= 0.0 {
                self.message = None;
            }
        }
        let Some(toast) = &mut self.toast else {
            return;
        };
        toast.remaining -= delta_secs;
        if toast.remaining <= 0.0 {
            self.toast = None;
        }
    }

    /// 生成全部 HUD 四边形顶点，写入 `out`；`icons` 提供物品图标的图集 UV。
    pub fn build(&self, out: &mut Vec<UiVertex>, width: f32, height: f32, icons: &ItemTextureMap) {
        out.clear();
        if !valid_viewport(width, height) || self.ui_open {
            return;
        }

        self.push_target_health(out, width, height);
        self.push_state(out, width, height);
        self.push_time_phase(out, width, height);
        self.push_message(out, width, height);
        self.push_hotbar(out, width, height, icons);
        self.push_selected_name(out, width, height);
        self.push_toast(out, width, height, icons);
        self.push_progress(out, width, height);
        self.push_damage_direction(out, width, height);
        self.push_damage_flash(out, width, height);
        push_crosshair(out, width, height, &self.ui_textures);
    }

    /// 敌对生物名牌与血条，绘制在生命值上方。
    fn push_target_health(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some(target) = self.target else {
            return;
        };
        if self.target_remaining <= 0.0 {
            return;
        }
        let viewport = (width, height);
        let alpha = (self.target_remaining / 0.35).clamp(0.0, 1.0);
        let state_top =
            height - BOTTOM_MARGIN - SLOT_SIZE - HEALTH_OFFSET - STATE_BG_HEIGHT * STATE_SCALE;
        let name_y = state_top - TARGET_GAP;

        let label = format!("{} {}/{}", target.name, target.health, target.max_health);
        let label_width = font::text_width(&label, 1.3);
        let mut label_color = theme::TEXT;
        label_color[3] *= alpha;
        font::push_text(
            out,
            viewport,
            (width - label_width) * 0.5,
            name_y,
            1.3,
            label_color,
            &label,
        );

        let x = (width - TARGET_BAR_WIDTH) * 0.5;
        let y = name_y + 14.0;
        let mut background = theme::TARGET_BAR_BACKGROUND;
        background[3] *= alpha;
        push_quad(
            out,
            viewport,
            UiRect::new(x, y, TARGET_BAR_WIDTH, TARGET_BAR_HEIGHT),
            background,
        );
        let ratio = target.health.min(target.max_health) as f32 / target.max_health as f32;
        if ratio > 0.0 {
            let mut fill = theme::TARGET_BAR_FILL;
            fill[3] *= alpha;
            push_quad(
                out,
                viewport,
                UiRect::new(x, y, TARGET_BAR_WIDTH * ratio, TARGET_BAR_HEIGHT),
                fill,
            );
        }
    }

    /// 受击方向：从屏幕中心向攻击来源方向偏移的红色标记。
    fn push_damage_direction(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some(angle) = self.damage_direction else {
            return;
        };
        if self.damage_direction_remaining <= 0.0 {
            return;
        }
        let viewport = (width, height);
        let alpha = (self.damage_direction_remaining / DAMAGE_DIRECTION_DURATION).clamp(0.0, 1.0);
        let (sin, cos) = angle.sin_cos();
        let direction = (sin, -cos);
        let center = (
            width * 0.5 + direction.0 * width * 0.32,
            height * 0.5 + direction.1 * height * 0.32,
        );
        for step in 0..3 {
            let offset = step as f32 * DAMAGE_DIRECTION_SIZE * 0.55;
            let size = DAMAGE_DIRECTION_SIZE * (1.0 - step as f32 * 0.25);
            let mut color = theme::DAMAGE_DIRECTION;
            color[3] *= alpha * (1.0 - step as f32 * 0.25);
            push_quad(
                out,
                viewport,
                UiRect::new(
                    center.0 + direction.0 * offset - size * 0.5,
                    center.1 + direction.1 * offset - size * 0.5,
                    size,
                    size,
                ),
                color,
            );
        }
    }

    /// 生命/体力/饥饿三行状态条：背景 106×19，填充 102×5，内边距 2 像素、间距 0。
    fn push_state(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let viewport = (width, height);
        let panel = UiRect::new(
            (width - STATE_BG_WIDTH * STATE_SCALE) * 0.5,
            height - BOTTOM_MARGIN - SLOT_SIZE - HEALTH_OFFSET - STATE_BG_HEIGHT * STATE_SCALE,
            STATE_BG_WIDTH * STATE_SCALE,
            STATE_BG_HEIGHT * STATE_SCALE,
        );
        if let Some(view) = self.ui_textures.get("state_bg") {
            push_ui_view(out, viewport, panel, view, [1.0; 4]);
        } else {
            push_quad(out, viewport, panel, STATE_BG_COLOR);
        }

        let health_ratio = if self.max_health == 0 {
            0.0
        } else {
            self.health as f32 / self.max_health as f32
        };
        let rows = [
            ("state_heart_fill", health_ratio, STATE_HEART_COLOR),
            (
                "state_stamina_fill",
                self.stamina / 100.0,
                STATE_STAMINA_COLOR,
            ),
            ("state_hunger_fill", self.hunger / 100.0, STATE_HUNGER_COLOR),
        ];
        let fill_x = panel.x + STATE_PADDING * STATE_SCALE;
        for (index, (name, ratio, fallback)) in rows.into_iter().enumerate() {
            let row_y = panel.y + (STATE_PADDING + index as f32 * STATE_FILL_HEIGHT) * STATE_SCALE;
            push_state_fill(
                out,
                viewport,
                (fill_x, row_y),
                (name, ratio, fallback),
                &self.ui_textures,
            );
        }
    }

    /// 右上角显示当前昼夜阶段。
    fn push_time_phase(&self, out: &mut Vec<UiVertex>, width: f32, _height: f32) {
        let label = format!("TIME {}", self.time_label);
        let text_width = font::text_width(&label, 1.8);
        font::push_text(
            out,
            (width, _height),
            width - text_width - 18.0,
            14.0,
            1.8,
            TIME_TEXT,
            &label,
        );
    }

    /// 屏幕中部的短状态提示。
    fn push_message(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some((text, remaining)) = self.message else {
            return;
        };
        if remaining <= 0.0 {
            return;
        }
        let viewport = (width, height);
        let alpha = (remaining / MESSAGE_DURATION).clamp(0.0, 1.0);
        let text_width = font::text_width(text, 2.0);
        let x = (width - text_width) * 0.5;
        let y = height * 0.62;
        let mut background = theme::OVERLAY_DARK;
        background[3] *= alpha;
        push_quad(
            out,
            viewport,
            UiRect::new(x - 10.0, y - 6.0, text_width + 20.0, 26.0),
            background,
        );
        let mut color = [1.0, 0.86, 0.44, 1.0];
        color[3] *= alpha;
        font::push_text(out, viewport, x, y, 2.0, color, text);
    }

    /// 受伤后覆盖屏幕四周的红色反馈，随时间淡出。
    fn push_damage_flash(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        if self.damage_flash_remaining <= 0.0 {
            return;
        }
        let alpha = (self.damage_flash_remaining / DAMAGE_FLASH_DURATION).clamp(0.0, 1.0);
        let viewport = (width, height);
        if let Some(view) = self.ui_textures.get("damage_vignette") {
            push_ui_view(
                out,
                viewport,
                UiRect::new(0.0, 0.0, width, height),
                view,
                [1.0, 1.0, 1.0, alpha],
            );
            return;
        }
        let mut color = theme::DAMAGE_FLASH;
        color[3] *= alpha;
        let thickness = DAMAGE_FLASH_THICKNESS;
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, 0.0, width, thickness),
            color,
        );
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, height - thickness, width, thickness),
            color,
        );
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, thickness, thickness, height - thickness * 2.0),
            color,
        );
        push_quad(
            out,
            viewport,
            UiRect::new(
                width - thickness,
                thickness,
                thickness,
                height - thickness * 2.0,
            ),
            color,
        );
    }

    fn push_hotbar(
        &self,
        out: &mut Vec<UiVertex>,
        width: f32,
        height: f32,
        icons: &ItemTextureMap,
    ) {
        let viewport = (width, height);
        let geometry = hotbar_geometry::HotbarGeometry::from_slot_size(SLOT_SIZE);
        let total = geometry.slots_width(HOTBAR_SLOTS);
        let start_x = (width - total) * 0.5;
        let y = height - BOTTOM_MARGIN - geometry.slot;
        let pulse = self.clock.pulse(SELECTED_PULSE_SPEED);

        if let Some(view) = self.ui_textures.get("hotbar_bg") {
            let first_slot = UiRect::new(start_x, y, geometry.slot, geometry.slot);
            let rect = geometry.background_rect(first_slot, HOTBAR_SLOTS);
            push_ui_view(out, viewport, rect, view, [1.0; 4]);
        }

        for index in 0..HOTBAR_SLOTS {
            let x = start_x + index as f32 * (geometry.slot + geometry.gap);
            let selected = index == self.selected_slot;
            let rect = UiRect::new(x, y, geometry.slot, geometry.slot);

            if selected {
                let expand = 1.0 + pulse * 2.0;
                let mut glow = theme::SLOT_GLOW;
                glow[3] = 0.12 + pulse * 0.20;
                push_quad(
                    out,
                    viewport,
                    UiRect::new(
                        rect.x - expand,
                        rect.y - expand,
                        rect.width + expand * 2.0,
                        rect.height + expand * 2.0,
                    ),
                    glow,
                );
            }

            widget::push_slot(
                out,
                viewport,
                rect,
                if selected {
                    SlotState::Selected
                } else {
                    SlotState::Normal
                },
                self.slots[index],
                icons,
                &self.ui_textures,
                SlotSkin::Hotbar,
                theme::HOTBAR_BACKGROUND,
            );
        }
    }

    /// 当前快捷栏物品的名称；工具损坏时改用警示色并注明状态。
    fn push_selected_name(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some(stack) = self.slots[self.selected_slot] else {
            return;
        };
        let text = if stack.is_broken() {
            format!("BROKEN {}", item_name(stack.item()))
        } else {
            item_name(stack.item()).to_string()
        };
        let color = if stack.is_broken() {
            theme::TOOL_BROKEN
        } else {
            theme::TOOL_LABEL
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
            theme::TEXT_SHADOW,
            &text,
        );
        font::push_text(out, (width, height), x, y, TOAST_PIXEL, color, &text);
    }

    fn push_toast(&self, out: &mut Vec<UiVertex>, width: f32, height: f32, icons: &ItemTextureMap) {
        let Some(toast) = self.toast else {
            return;
        };
        let alpha = (toast.remaining / TOAST_FADE).clamp(0.0, 1.0);
        let (text, base_color) = match toast.kind {
            ToastKind::Pickup => (
                format!("+{} {}", toast.count, item_name(toast.item)),
                theme::TOAST_TEXT,
            ),
            ToastKind::ToolBroken => (
                format!("{} BROKEN", item_name(toast.item)),
                theme::TOOL_BROKEN,
            ),
        };
        let has_icon = icons.icon(toast.item).is_some();
        let icon_span = if has_icon { TOAST_ICON_SIZE + 8.0 } else { 0.0 };
        let text_width = font::text_width(&text, TOAST_PIXEL);
        let total_width = icon_span + text_width;
        let x = (width - total_width) * 0.5;
        let y = height - BOTTOM_MARGIN - SLOT_SIZE - TOAST_OFFSET;

        if has_icon {
            push_item_icon(
                out,
                (width, height),
                UiRect::new(x, y - 2.0, TOAST_ICON_SIZE, TOAST_ICON_SIZE),
                toast.item,
                icons,
            );
        }
        let text_x = x + icon_span;
        font::push_text(
            out,
            (width, height),
            text_x + 1.0,
            y + 1.0,
            TOAST_PIXEL,
            [0.0, 0.0, 0.0, 0.85 * alpha],
            &text,
        );
        let mut color = base_color;
        color[3] *= alpha;
        font::push_text(out, (width, height), text_x, y, TOAST_PIXEL, color, &text);
    }

    fn push_progress(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some(progress) = self.eating_progress.or(self.mining_progress) else {
            return;
        };
        let viewport = (width, height);
        let progress = progress.clamp(0.0, 1.0);
        let x = (width - PROGRESS_WIDTH) * 0.5;
        let y = height * 0.5 + PROGRESS_OFFSET;
        let bar = UiRect::new(x, y, PROGRESS_WIDTH, PROGRESS_HEIGHT);

        if let Some(view) = self.ui_textures.get("progress_bg") {
            push_ui_view(out, viewport, bar, view, [1.0; 4]);
        } else {
            push_quad(out, viewport, bar, theme::PROGRESS_BACKGROUND);
        }

        let fill = UiRect::new(
            x + 2.0,
            y + 2.0,
            (PROGRESS_WIDTH - 4.0) * progress,
            PROGRESS_HEIGHT - 4.0,
        );
        if let Some(view) = self.ui_textures.get("progress_fill") {
            push_ui_view(out, viewport, fill, view, [1.0; 4]);
        } else {
            push_quad(out, viewport, fill, theme::PROGRESS_FILL);
        }
    }
}

/// 按百分比裁剪右侧像素绘制一条状态填充。
///
/// 102 像素对应 100 个 1% 档位：中间每档 1 列，首尾两档各占 2 列。
fn push_state_fill(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    anchor: (f32, f32),
    row: (&str, f32, [f32; 4]),
    ui: &UiTextureMap,
) {
    let (name, ratio, fallback) = row;
    let columns = state_fill_columns(ratio);
    if columns <= 0.0 {
        return;
    }
    let rect = UiRect::new(
        anchor.0,
        anchor.1,
        columns * STATE_SCALE,
        STATE_FILL_HEIGHT * STATE_SCALE,
    );
    if let Some(view) = ui.get(name) {
        let mut cropped = view;
        cropped.draw = crate::texture::ui_texture::UiDrawMode::Stretch;
        cropped.aspect = None;
        cropped.uv.max[0] = cropped.uv.min[0] + cropped.uv.width() * (columns / STATE_FILL_WIDTH);
        push_ui_view(out, viewport, rect, cropped, [1.0; 4]);
    } else {
        push_quad(out, viewport, rect, fallback);
    }
}

/// 百分比对应的原图像素列数：0 为 0 列，100% 为 102 列，中间每 1% 增加 1 列。
fn state_fill_columns(ratio: f32) -> f32 {
    let ratio = if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let percent = (ratio * 100.0).round();
    if percent <= 0.0 {
        0.0
    } else if percent >= 100.0 {
        STATE_FILL_WIDTH
    } else {
        percent + 1.0
    }
}

fn push_crosshair(out: &mut Vec<UiVertex>, width: f32, height: f32, ui: &UiTextureMap) {
    let center_x = width * 0.5;
    let center_y = height * 0.5;

    if let Some(view) = ui.get("crosshair") {
        let size = (CROSSHAIR_ARM + 4.0) * 2.0;
        push_ui_view(
            out,
            (width, height),
            UiRect::new(center_x - size * 0.5, center_y - size * 0.5, size, size),
            view,
            [1.0; 4],
        );
        return;
    }

    // 先画深色底衬，再画亮色线条，保证在草地与石头上都清晰可见。
    push_crosshair_layer(
        out,
        width,
        height,
        center_x,
        center_y,
        1.0,
        theme::CROSSHAIR_SHADOW,
    );
    push_crosshair_layer(
        out,
        width,
        height,
        center_x,
        center_y,
        0.0,
        theme::CROSSHAIR,
    );
}

fn push_crosshair_layer(
    out: &mut Vec<UiVertex>,
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
        UiRect::new(
            center_x - arm,
            center_y - thickness * 0.5,
            arm * 2.0,
            thickness,
        ),
        color,
    );
    push_quad(
        out,
        viewport,
        UiRect::new(
            center_x - thickness * 0.5,
            center_y - arm,
            thickness,
            arm * 2.0,
        ),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::draw::SOLID_UV;

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
        use crate::texture::types::{TextureType, Transparency, UVRect};

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

        let textured: Vec<&UiVertex> = vertices.iter().filter(|v| v.uv[0] >= 0.0).collect();
        assert!(!textured.is_empty(), "快捷栏必须采样物品图标");
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
    fn an_open_ui_hides_the_hud() {
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
    fn repeated_pickups_of_the_same_item_are_merged() {
        let mut model = HudModel::default();
        model.show_pickup(ItemId::STONE, 3);
        model.show_pickup(ItemId::STONE, 5);
        assert_eq!(model.toast.map(|toast| toast.count), Some(8));

        model.show_pickup(ItemId::DIRT, 1);
        assert_eq!(model.toast.map(|toast| toast.item), Some(ItemId::DIRT));
        assert_eq!(model.toast.map(|toast| toast.count), Some(1));
    }

    #[test]
    fn durability_bars_only_apply_to_tools() {
        let viewport = (1280.0, 720.0);
        let rect = UiRect::new(0.0, 0.0, SLOT_SIZE, SLOT_SIZE);

        let mut block_vertices = Vec::new();
        widget::push_durability_strip(&mut block_vertices, viewport, rect, stone(1));
        assert!(block_vertices.is_empty(), "普通方块物品没有耐久条");

        let mut tool_vertices = Vec::new();
        widget::push_durability_strip(&mut tool_vertices, viewport, rect, pickaxe(30));
        assert_eq!(tool_vertices.len(), 12, "耐久条背景 + 填充");

        let mut broken_vertices = Vec::new();
        widget::push_durability_strip(&mut broken_vertices, viewport, rect, pickaxe(0));
        assert_eq!(broken_vertices.len(), 6, "损坏工具只绘制警示背景，没有填充");
    }

    #[test]
    fn durability_color_warns_at_low_and_medium_levels() {
        assert_eq!(widget::durability_color(1.0), theme::DURABILITY_HIGH);
        assert_eq!(widget::durability_color(0.6), theme::DURABILITY_HIGH);
        assert_eq!(widget::durability_color(0.4), theme::DURABILITY_MID);
        assert_eq!(widget::durability_color(0.2), theme::DURABILITY_LOW);
        assert_eq!(widget::durability_color(0.0), theme::DURABILITY_LOW);
    }

    #[test]
    fn the_selected_item_shows_its_name_and_broken_state() {
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
            "选中物品必须显示名称"
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
    fn every_selected_item_has_a_name_not_only_tools() {
        let mut slots = [None; HOTBAR_SLOTS];
        slots[0] = Some(stone(4));
        let mut with_item = Vec::new();
        HudModel {
            slots,
            ..HudModel::default()
        }
        .build(&mut with_item, 1280.0, 720.0, &ItemTextureMap::empty());

        let mut empty = Vec::new();
        HudModel::default().build(&mut empty, 1280.0, 720.0, &ItemTextureMap::empty());
        assert!(with_item.len() > empty.len(), "方块物品同样显示名称");
    }

    #[test]
    fn state_values_are_clamped_and_defaults_are_safe() {
        let mut model = HudModel::default();
        model.set_health(150, 100);
        assert_eq!(model.health, 100);
        assert_eq!(model.max_health, 100);
        model.set_health(5, 0);
        assert_eq!(model.max_health, 100, "非法最大值回落到默认值");
        assert_eq!(model.health, 5);
        model.set_survival(120.0, -3.0, "DAY");
        assert_eq!(model.hunger, 100.0);
        assert_eq!(model.stamina, 0.0);
    }

    #[test]
    fn state_fill_columns_map_first_and_last_percent_to_two_columns() {
        assert_eq!(state_fill_columns(0.0), 0.0);
        assert_eq!(state_fill_columns(0.01), 2.0, "首个 1% 占两列");
        assert_eq!(state_fill_columns(0.5), 51.0);
        assert_eq!(state_fill_columns(0.99), 100.0, "最后一个 1% 占两列");
        assert_eq!(state_fill_columns(1.0), 102.0);
        assert_eq!(state_fill_columns(f32::NAN), 0.0);
    }

    #[test]
    fn full_state_bars_draw_three_fills_over_the_background() {
        let mut vertices = Vec::new();
        HudModel::default().build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());
        for color in [STATE_HEART_COLOR, STATE_STAMINA_COLOR, STATE_HUNGER_COLOR] {
            assert!(
                vertices.iter().any(|vertex| vertex.color == color),
                "满值时三条填充都应绘制"
            );
        }
    }

    #[test]
    fn empty_state_bars_skip_all_fills() {
        let mut vertices = Vec::new();
        HudModel {
            health: 0,
            hunger: 0.0,
            stamina: 0.0,
            ..HudModel::default()
        }
        .build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());
        for color in [STATE_HEART_COLOR, STATE_STAMINA_COLOR, STATE_HUNGER_COLOR] {
            assert!(
                !vertices.iter().any(|vertex| vertex.color == color),
                "空值不应绘制填充"
            );
        }
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

//! 死亡界面：全屏遮罩、死亡原因、重生与返回主菜单按钮。

use crate::player::DamageSource;
use crate::renderer::font;
use crate::renderer::hud::{push_quad, HudRect, HudVertex};

const OVERLAY: [f32; 4] = [0.16, 0.01, 0.02, 0.82];
const TITLE: [f32; 4] = [0.98, 0.86, 0.84, 1.0];
const MUTED: [f32; 4] = [0.82, 0.74, 0.74, 1.0];
const BUTTON: [f32; 4] = [0.28, 0.09, 0.10, 1.0];
const BUTTON_HOVER: [f32; 4] = [0.42, 0.14, 0.16, 1.0];
const BUTTON_BORDER: [f32; 4] = [0.80, 0.42, 0.42, 1.0];

const BUTTON_WIDTH: f32 = 250.0;
const BUTTON_HEIGHT: f32 = 48.0;
const BUTTON_GAP: f32 = 16.0;

/// 死亡界面可执行的动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathAction {
    None,
    Respawn,
    MainMenu,
}

/// 死亡界面状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeathUi {
    open: bool,
    cursor: (f32, f32),
    cause: DamageSource,
}

impl Default for DeathUi {
    fn default() -> Self {
        Self::new()
    }
}

impl DeathUi {
    pub fn new() -> Self {
        Self {
            open: false,
            cursor: (0.0, 0.0),
            cause: DamageSource::Custom,
        }
    }

    pub const fn is_open(self) -> bool {
        self.open
    }

    /// 打开死亡界面并记录致死来源。
    pub fn open(&mut self, cause: DamageSource) {
        self.open = true;
        self.cause = cause;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn set_cursor(&mut self, cursor: (f32, f32)) {
        if cursor.0.is_finite() && cursor.1.is_finite() {
            self.cursor = cursor;
        }
    }

    pub const fn cursor(self) -> (f32, f32) {
        self.cursor
    }

    /// 点击检测；一次点击最多返回一个动作。
    pub fn click_at(self, viewport: (f32, f32), cursor: (f32, f32)) -> DeathAction {
        if !self.open {
            return DeathAction::None;
        }
        let buttons = buttons(viewport);
        if contains(&buttons.0, cursor) {
            DeathAction::Respawn
        } else if contains(&buttons.1, cursor) {
            DeathAction::MainMenu
        } else {
            DeathAction::None
        }
    }

    /// 生成死亡界面顶点。
    pub fn build(&self, out: &mut Vec<HudVertex>, viewport: (f32, f32)) {
        if !self.open {
            return;
        }
        let (width, height) = viewport;
        push_quad(
            out,
            viewport,
            HudRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            OVERLAY,
        );

        let title = "YOU DIED";
        let title_scale = 5.0;
        font::push_text(
            out,
            viewport,
            (width - font::text_width(title, title_scale)) * 0.5,
            height * 0.24,
            title_scale,
            TITLE,
            title,
        );

        let cause = format!("CAUSE: {}", self.cause.label());
        font::push_text(
            out,
            viewport,
            (width - font::text_width(&cause, 2.0)) * 0.5,
            height * 0.24 + 52.0,
            2.0,
            MUTED,
            &cause,
        );

        let (respawn, main_menu) = buttons(viewport);
        push_button(
            out,
            viewport,
            respawn,
            "RESPAWN",
            contains(&respawn, self.cursor),
        );
        push_button(
            out,
            viewport,
            main_menu,
            "MAIN MENU",
            contains(&main_menu, self.cursor),
        );
    }
}

fn buttons(viewport: (f32, f32)) -> (HudRect, HudRect) {
    let (width, height) = viewport;
    let center_x = width * 0.5;
    let top = height * 0.52;
    let respawn = HudRect {
        x: center_x - BUTTON_WIDTH * 0.5,
        y: top,
        width: BUTTON_WIDTH,
        height: BUTTON_HEIGHT,
    };
    let main_menu = HudRect {
        x: respawn.x,
        y: top + BUTTON_HEIGHT + BUTTON_GAP,
        width: BUTTON_WIDTH,
        height: BUTTON_HEIGHT,
    };
    (respawn, main_menu)
}

fn push_button(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    label: &str,
    hovered: bool,
) {
    push_quad(
        out,
        viewport,
        rect,
        if hovered { BUTTON_HOVER } else { BUTTON },
    );
    push_border(out, viewport, rect, BUTTON_BORDER, 2.0);

    let scale = 2.0;
    let text_width = font::text_width(label, scale);
    font::push_text(
        out,
        viewport,
        rect.x + (rect.width - text_width) * 0.5,
        rect.y + (rect.height - font::GLYPH_HEIGHT as f32 * scale) * 0.5,
        scale,
        TITLE,
        label,
    );
}

fn push_border(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    color: [f32; 4],
    thickness: f32,
) {
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: thickness,
        },
        color,
    );
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x,
            y: rect.y + rect.height - thickness,
            width: rect.width,
            height: thickness,
        },
        color,
    );
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x,
            y: rect.y + thickness,
            width: thickness,
            height: rect.height - thickness * 2.0,
        },
        color,
    );
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x + rect.width - thickness,
            y: rect.y + thickness,
            width: thickness,
            height: rect.height - thickness * 2.0,
        },
        color,
    );
}

fn contains(rect: &HudRect, point: (f32, f32)) -> bool {
    point.0 >= rect.x
        && point.0 <= rect.x + rect.width
        && point.1 >= rect.y
        && point.1 <= rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> (f32, f32) {
        (1280.0, 720.0)
    }

    #[test]
    fn death_ui_opens_with_the_cause_and_closes() {
        let mut ui = DeathUi::new();
        assert!(!ui.is_open());
        ui.open(DamageSource::Fall);
        assert!(ui.is_open());
        assert_eq!(ui.cause, DamageSource::Fall);
        ui.close();
        assert!(!ui.is_open());
    }

    #[test]
    fn clicking_the_respawn_button_returns_respawn() {
        let mut ui = DeathUi::new();
        ui.open(DamageSource::Fall);
        let (respawn, _) = buttons(viewport());
        let action = ui.click_at(viewport(), (respawn.x + 5.0, respawn.y + 5.0));
        assert_eq!(action, DeathAction::Respawn);
    }

    #[test]
    fn clicking_the_main_menu_button_returns_main_menu() {
        let mut ui = DeathUi::new();
        ui.open(DamageSource::Fall);
        let (_, main_menu) = buttons(viewport());
        let action = ui.click_at(viewport(), (main_menu.x + 5.0, main_menu.y + 5.0));
        assert_eq!(action, DeathAction::MainMenu);
    }

    #[test]
    fn clicking_outside_buttons_does_nothing() {
        let mut ui = DeathUi::new();
        ui.open(DamageSource::Fall);
        assert_eq!(ui.click_at(viewport(), (1.0, 1.0)), DeathAction::None);
    }

    #[test]
    fn a_closed_death_ui_ignores_clicks_and_draws_nothing() {
        let ui = DeathUi::new();
        let mut vertices = Vec::new();
        ui.build(&mut vertices, viewport());
        assert!(vertices.is_empty());
        assert_eq!(ui.click_at(viewport(), (640.0, 400.0)), DeathAction::None);
    }

    #[test]
    fn the_death_screen_stays_inside_clip_space() {
        let mut ui = DeathUi::new();
        ui.open(DamageSource::Entity);
        let mut vertices = Vec::new();
        ui.build(&mut vertices, viewport());
        assert!(!vertices.is_empty());
        assert_eq!(vertices.len() % 6, 0);
        for vertex in &vertices {
            assert!((-1.0..=1.0).contains(&vertex.position[0]));
            assert!((-1.0..=1.0).contains(&vertex.position[1]));
        }
    }
}

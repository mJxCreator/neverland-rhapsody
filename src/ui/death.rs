//! 死亡界面：全屏遮罩、死亡原因、重生与返回主菜单按钮。
//!
//! 死亡是最高优先级界面：打开时其他界面全部关闭，键盘快捷键被吞掉，
//! 只保留鼠标对两个按钮的点击。

use super::draw::{push_border, push_quad, push_ui_view, UiVertex};
use super::font;
use super::layout::UiRect;
use super::theme;
use crate::player::DamageSource;
use crate::texture::UiTextureMap;

/// 死亡按钮使用的暗红底色。
const BUTTON: [f32; 4] = [0.28, 0.09, 0.10, 1.0];
/// 悬停时的提亮底色。
const BUTTON_HOVER: [f32; 4] = [0.42, 0.14, 0.16, 1.0];
/// 按钮边框。
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
#[derive(Clone, Debug, PartialEq)]
pub struct DeathUi {
    open: bool,
    cursor: (f32, f32),
    cause: DamageSource,
    ui_textures: UiTextureMap,
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
            ui_textures: UiTextureMap::empty(),
        }
    }

    /// 替换 UI 控件图集；资源加载或热重载后由状态管理器调用。
    pub fn set_ui_textures(&mut self, textures: UiTextureMap) {
        self.ui_textures = textures;
    }

    #[cfg(test)]
    pub const fn is_open(&self) -> bool {
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

    pub const fn cursor(&self) -> (f32, f32) {
        self.cursor
    }

    /// 点击检测；一次点击最多返回一个动作。
    pub fn click_at(&self, viewport: (f32, f32), cursor: (f32, f32)) -> DeathAction {
        if !self.open {
            return DeathAction::None;
        }
        let buttons = buttons(viewport);
        if buttons.0.contains(cursor.0, cursor.1) {
            DeathAction::Respawn
        } else if buttons.1.contains(cursor.0, cursor.1) {
            DeathAction::MainMenu
        } else {
            DeathAction::None
        }
    }

    /// 生成死亡界面顶点。
    pub fn build(&self, out: &mut Vec<UiVertex>, viewport: (f32, f32)) {
        if !self.open {
            return;
        }
        let (width, height) = viewport;
        if let Some(view) = self.ui_textures.get("death_overlay") {
            push_ui_view(
                out,
                viewport,
                UiRect::new(0.0, 0.0, width, height),
                view,
                [1.0; 4],
            );
        } else {
            push_quad(
                out,
                viewport,
                UiRect::new(0.0, 0.0, width, height),
                theme::DEATH_OVERLAY,
            );
        }

        if let Some(view) = self.ui_textures.get("death_panel") {
            let panel_width = (width - 80.0).min(720.0);
            push_ui_view(
                out,
                viewport,
                UiRect::new(
                    (width - panel_width) * 0.5,
                    height * 0.14,
                    panel_width,
                    height * 0.62,
                ),
                view,
                [1.0; 4],
            );
        }

        let title = "YOU DIED";
        let title_scale = 5.0;
        font::push_text(
            out,
            viewport,
            (width - font::text_width(title, title_scale)) * 0.5,
            height * 0.24,
            title_scale,
            theme::DEATH_TITLE,
            title,
        );

        let cause = format!("CAUSE: {}", self.cause.label());
        font::push_text(
            out,
            viewport,
            (width - font::text_width(&cause, 2.0)) * 0.5,
            height * 0.24 + 52.0,
            2.0,
            theme::TEXT_MUTED,
            &cause,
        );

        let (respawn, main_menu) = buttons(viewport);
        push_death_button(
            out,
            viewport,
            respawn,
            "RESPAWN",
            respawn.contains(self.cursor.0, self.cursor.1),
            &self.ui_textures,
        );
        push_death_button(
            out,
            viewport,
            main_menu,
            "MAIN MENU",
            main_menu.contains(self.cursor.0, self.cursor.1),
            &self.ui_textures,
        );
    }
}

fn buttons(viewport: (f32, f32)) -> (UiRect, UiRect) {
    let (width, height) = viewport;
    let center_x = width * 0.5;
    let top = height * 0.52;
    let respawn = UiRect::new(
        center_x - BUTTON_WIDTH * 0.5,
        top,
        BUTTON_WIDTH,
        BUTTON_HEIGHT,
    );
    let main_menu = UiRect::new(
        respawn.x,
        top + BUTTON_HEIGHT + BUTTON_GAP,
        BUTTON_WIDTH,
        BUTTON_HEIGHT,
    );
    (respawn, main_menu)
}

fn push_death_button(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    label: &str,
    hovered: bool,
    ui: &UiTextureMap,
) {
    let texture = if hovered { "button_hover" } else { "button" };
    if let Some(view) = ui.get(texture) {
        push_ui_view(out, viewport, rect, view, [1.0; 4]);
    } else {
        push_quad(
            out,
            viewport,
            rect,
            if hovered { BUTTON_HOVER } else { BUTTON },
        );
        push_border(out, viewport, rect, 2.0, BUTTON_BORDER);
    }

    let scale = 2.0;
    let text_width = font::text_width(label, scale);
    font::push_text(
        out,
        viewport,
        rect.x + (rect.width - text_width) * 0.5,
        rect.y + (rect.height - font::GLYPH_HEIGHT as f32 * scale) * 0.5,
        scale,
        theme::TEXT,
        label,
    );
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

    #[test]
    fn buttons_scale_with_the_viewport_height() {
        for height in [720.0, 900.0, 1080.0] {
            let (respawn, main_menu) = buttons((1280.0, height));
            assert!(respawn.y < height);
            assert!(main_menu.bottom() <= height);
        }
    }
}

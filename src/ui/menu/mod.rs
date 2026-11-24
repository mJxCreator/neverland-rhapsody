//! 菜单界面：主菜单、角色选择、世界选择与游戏内暂停菜单。
//!
//! 本模块只负责界面状态、布局与点击/按键解析，不读写磁盘：
//! 存档库由应用层驱动，菜单通过 [`MenuAction`] 把用户意图交回应用层执行。
//! 文案为中文；重命名输入同时接受 IME 提交文本与键盘文本事件。

use winit::keyboard::KeyCode;

use crate::text;
use crate::texture::UiTextureMap;
use crate::ui::core::draw::{push_border, push_quad, push_ui_view, UiVertex};
use crate::ui::core::font;
use crate::ui::core::layout::{centered_panel, valid_viewport, UiRect};
use crate::ui::core::theme;
use crate::ui::core::widget::{push_button, push_button_skinned, push_panel};

pub mod config;

use self::config::*;

/// 菜单中的角色/世界条目；应用层由存档库转换而来。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuEntry {
    pub id: String,
    pub name: String,
}

impl MenuEntry {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

/// 当前显示的菜单页面。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuScreen {
    Main,
    CharacterSelect,
    WorldSelect,
    Pause,
}

/// 菜单对外产生的动作；应用层据此操作存档或切换游戏状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    SinglePlayer,
    Quit,
    NewCharacter,
    SelectCharacter(String),
    EditCharacter(String),
    DeleteCharacter(String),
    NewWorld,
    SelectWorld(String),
    DeleteWorld(String),
    BackToMain,
    BackToCharacters,
    Resume,
    SaveAndQuit,
    ConfirmRename,
    CancelRename,
    /// 尚未实现的入口；文本直接作为提示显示。
    Unavailable(&'static str),
}

/// 重命名输入框状态。
#[derive(Clone, Debug, PartialEq, Eq)]
struct RenameState {
    character_id: String,
    text: String,
    /// IME 预编辑文本；只作预览，不写入最终名称。
    preedit: String,
}

/// 菜单状态与输入。
#[derive(Clone, Debug, PartialEq)]
pub struct MenuUi {
    screen: MenuScreen,
    cursor: (f32, f32),
    characters: Vec<MenuEntry>,
    worlds: Vec<MenuEntry>,
    selected_character: Option<MenuEntry>,
    rename: Option<RenameState>,
    notice: Option<&'static str>,
    ui_textures: UiTextureMap,
}

impl Default for MenuUi {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuUi {
    pub fn new() -> Self {
        Self {
            screen: MenuScreen::Main,
            cursor: (0.0, 0.0),
            characters: Vec::new(),
            worlds: Vec::new(),
            selected_character: None,
            rename: None,
            notice: None,
            ui_textures: UiTextureMap::empty(),
        }
    }

    /// 替换 UI 控件图集；资源加载或热重载后由应用层调用。
    pub fn set_ui_textures(&mut self, textures: &UiTextureMap) {
        self.ui_textures = textures.clone();
    }

    pub fn set_cursor(&mut self, x: f32, y: f32) {
        if x.is_finite() && y.is_finite() {
            self.cursor = (x, y);
        }
    }

    pub const fn cursor(&self) -> (f32, f32) {
        self.cursor
    }

    pub fn selected_character(&self) -> Option<&MenuEntry> {
        self.selected_character.as_ref()
    }

    #[allow(dead_code)]
    pub fn is_renaming(&self) -> bool {
        self.rename.is_some()
    }

    pub fn rename_text(&self) -> Option<&str> {
        self.rename.as_ref().map(|state| state.text.as_str())
    }

    /// IME 预编辑文本；为空表示没有正在组字。
    pub fn preedit_text(&self) -> Option<&str> {
        self.rename
            .as_ref()
            .map(|state| state.preedit.as_str())
            .filter(|text| !text.is_empty())
    }

    pub fn rename_character_id(&self) -> Option<&str> {
        self.rename
            .as_ref()
            .map(|state| state.character_id.as_str())
    }

    /// 显示一行提示；导航到其他页面时自动清除。
    pub fn show_notice(&mut self, notice: &'static str) {
        self.notice = Some(notice);
    }

    /// 打开主菜单。
    pub fn open_main(&mut self) {
        self.screen = MenuScreen::Main;
        self.selected_character = None;
        self.worlds.clear();
        self.rename = None;
        self.notice = None;
    }

    /// 打开角色选择；角色列表由应用层提供。
    pub fn open_characters(&mut self, characters: Vec<MenuEntry>) {
        self.screen = MenuScreen::CharacterSelect;
        self.characters = characters;
        self.rename = None;
        self.notice = None;
    }

    /// 进入世界选择，并记录当前角色。
    pub fn open_worlds(&mut self, character: MenuEntry, worlds: Vec<MenuEntry>) {
        self.screen = MenuScreen::WorldSelect;
        self.selected_character = Some(character);
        self.worlds = worlds;
        self.rename = None;
        self.notice = None;
    }

    /// 打开暂停菜单。
    pub fn open_pause(&mut self) {
        self.screen = MenuScreen::Pause;
        self.rename = None;
        self.notice = None;
    }

    /// 关闭暂停菜单；返回是否原本处于暂停页。
    pub fn close_pause(&mut self) -> bool {
        if self.screen != MenuScreen::Pause {
            return false;
        }
        self.notice = None;
        true
    }

    /// 原地刷新世界列表（新建/删除后）。
    pub fn set_worlds(&mut self, worlds: Vec<MenuEntry>) {
        self.worlds = worlds;
    }

    /// 进入重命名输入状态。
    pub fn begin_rename(&mut self, character_id: impl Into<String>, current_name: &str) {
        let text: String = current_name
            .chars()
            .filter(|character| !character.is_control())
            .take(RENAME_MAX_CHARS)
            .collect();
        self.rename = Some(RenameState {
            character_id: character_id.into(),
            text,
            preedit: String::new(),
        });
    }

    /// 追加 IME 提交文本；长度按字符数限制。
    pub fn ime_commit(&mut self, text: &str) {
        self.append_name_text(text);
    }

    /// 更新 IME 预编辑文本；只作预览。
    pub fn ime_preedit(&mut self, text: Option<&str>) {
        if let Some(state) = self.rename.as_mut() {
            state.preedit = text.unwrap_or_default().to_owned();
        }
    }

    /// 键盘文本事件：优先使用系统提交的文本，物理键位仅在文本缺失时回退。
    pub fn key_with_text(
        &mut self,
        code: KeyCode,
        pressed: bool,
        text: Option<&str>,
    ) -> MenuAction {
        if !pressed {
            return MenuAction::None;
        }
        if self.rename.is_none() {
            return self.key(code, pressed);
        }
        match code {
            KeyCode::Escape => MenuAction::CancelRename,
            KeyCode::Enter => MenuAction::ConfirmRename,
            KeyCode::Backspace => {
                if let Some(state) = self.rename.as_mut() {
                    state.text.pop();
                }
                MenuAction::None
            }
            _ => {
                if let Some(text) = text.filter(|text| !text.is_empty()) {
                    self.append_name_text(text);
                }
                MenuAction::None
            }
        }
    }

    fn append_name_text(&mut self, text: &str) {
        let Some(state) = self.rename.as_mut() else {
            return;
        };
        for character in text.chars().filter(|character| !character.is_control()) {
            if state.text.chars().count() >= RENAME_MAX_CHARS {
                break;
            }
            state.text.push(character);
        }
    }

    pub fn cancel_rename(&mut self) {
        self.rename = None;
    }

    /// 键盘输入：重命名输入优先，其次页面级快捷键。
    pub fn key(&mut self, code: KeyCode, pressed: bool) -> MenuAction {
        if !pressed {
            return MenuAction::None;
        }
        if self.rename.is_some() {
            return self.key_while_renaming(code);
        }
        match self.screen {
            MenuScreen::Pause => match code {
                KeyCode::Escape => MenuAction::Resume,
                _ => MenuAction::None,
            },
            MenuScreen::CharacterSelect => match code {
                KeyCode::Escape => MenuAction::BackToMain,
                _ => MenuAction::None,
            },
            MenuScreen::WorldSelect => match code {
                KeyCode::Escape => MenuAction::BackToCharacters,
                _ => MenuAction::None,
            },
            MenuScreen::Main => MenuAction::None,
        }
    }

    fn key_while_renaming(&mut self, code: KeyCode) -> MenuAction {
        match code {
            KeyCode::Escape => MenuAction::CancelRename,
            KeyCode::Enter => MenuAction::ConfirmRename,
            KeyCode::Backspace => {
                if let Some(state) = self.rename.as_mut() {
                    state.text.pop();
                }
                MenuAction::None
            }
            _ => {
                let Some(character) = key_to_char(code) else {
                    return MenuAction::None;
                };
                if let Some(state) = self.rename.as_mut() {
                    if state.text.chars().count() < RENAME_MAX_CHARS {
                        state.text.push(character);
                    }
                }
                MenuAction::None
            }
        }
    }

    /// 鼠标点击解析；一次点击最多返回一个动作。
    pub fn click_at(&self, viewport: (f32, f32), cursor: (f32, f32)) -> MenuAction {
        if !valid_viewport(viewport.0, viewport.1) {
            return MenuAction::None;
        }
        if self.rename.is_some() {
            let (confirm, cancel) = rename_buttons(viewport);
            if confirm.contains(cursor.0, cursor.1) {
                return MenuAction::ConfirmRename;
            }
            if cancel.contains(cursor.0, cursor.1) {
                return MenuAction::CancelRename;
            }
            return MenuAction::None;
        }
        match self.screen {
            MenuScreen::Main => {
                let buttons = main_buttons(viewport);
                let actions = [
                    MenuAction::SinglePlayer,
                    MenuAction::Unavailable(text::MULTIPLAYER_UNAVAILABLE),
                    MenuAction::Unavailable(text::ACHIEVEMENTS_UNAVAILABLE),
                    MenuAction::Unavailable(text::SETTINGS_UNAVAILABLE),
                    MenuAction::Unavailable(text::WIKI_UNAVAILABLE),
                    MenuAction::Unavailable(text::ABOUT_UNAVAILABLE),
                    MenuAction::Quit,
                ];
                for (index, rect) in buttons.iter().enumerate() {
                    if rect.contains(cursor.0, cursor.1) {
                        return actions[index].clone();
                    }
                }
                MenuAction::None
            }
            MenuScreen::CharacterSelect => {
                if back_button(viewport).contains(cursor.0, cursor.1) {
                    return MenuAction::BackToMain;
                }
                if new_button(viewport).contains(cursor.0, cursor.1) {
                    return MenuAction::NewCharacter;
                }
                for (index, entry) in self.characters.iter().enumerate() {
                    if index >= MAX_ROWS {
                        break;
                    }
                    let name = row_name_rect(viewport, index, 2);
                    let edit = row_action_rect(viewport, index, 2, 0);
                    let delete = row_action_rect(viewport, index, 2, 1);
                    if name.contains(cursor.0, cursor.1) {
                        return MenuAction::SelectCharacter(entry.id.clone());
                    }
                    if edit.contains(cursor.0, cursor.1) {
                        return MenuAction::EditCharacter(entry.id.clone());
                    }
                    if delete.contains(cursor.0, cursor.1) {
                        return MenuAction::DeleteCharacter(entry.id.clone());
                    }
                }
                MenuAction::None
            }
            MenuScreen::WorldSelect => {
                if back_button(viewport).contains(cursor.0, cursor.1) {
                    return MenuAction::BackToCharacters;
                }
                if new_button(viewport).contains(cursor.0, cursor.1) {
                    return MenuAction::NewWorld;
                }
                for (index, entry) in self.worlds.iter().enumerate() {
                    if index >= MAX_ROWS {
                        break;
                    }
                    let name = row_name_rect(viewport, index, 1);
                    let delete = row_action_rect(viewport, index, 1, 0);
                    if name.contains(cursor.0, cursor.1) {
                        return MenuAction::SelectWorld(entry.id.clone());
                    }
                    if delete.contains(cursor.0, cursor.1) {
                        return MenuAction::DeleteWorld(entry.id.clone());
                    }
                }
                MenuAction::None
            }
            MenuScreen::Pause => {
                let buttons = pause_buttons(viewport);
                let actions = [
                    MenuAction::Unavailable(text::SETTINGS_UNAVAILABLE),
                    MenuAction::Unavailable(text::CHARACTER_ACHIEVEMENTS_UNAVAILABLE),
                    MenuAction::Unavailable(text::WORLD_ACHIEVEMENTS_UNAVAILABLE),
                    MenuAction::SaveAndQuit,
                ];
                for (index, rect) in buttons.iter().enumerate() {
                    if rect.contains(cursor.0, cursor.1) {
                        return actions[index].clone();
                    }
                }
                MenuAction::None
            }
        }
    }

    /// 生成菜单顶点；调用方保证每帧清空输出列表。
    pub fn build(&self, out: &mut Vec<UiVertex>, viewport: (f32, f32)) {
        if !valid_viewport(viewport.0, viewport.1) {
            return;
        }
        let (width, height) = viewport;
        match self.screen {
            MenuScreen::Main => {
                self.draw_background(out, viewport);
                self.draw_title(out, viewport);
                let labels = text::MAIN_BUTTONS;
                for (index, rect) in main_buttons(viewport).iter().enumerate() {
                    let hovered = rect.contains(self.cursor.0, self.cursor.1);
                    push_button_skinned(
                        out,
                        viewport,
                        *rect,
                        labels[index],
                        hovered,
                        true,
                        BUTTON_TEXT_PIXEL,
                        &self.ui_textures,
                        "begin_button",
                        "begin_button_hover",
                    );
                }
                if let Some(notice) = self.notice {
                    let scale = 2.0;
                    font::push_text(
                        out,
                        viewport,
                        (width - font::text_width(notice, scale)) * 0.5,
                        10.0,
                        scale,
                        MENU_NOTICE,
                        notice,
                    );
                }
            }
            MenuScreen::CharacterSelect => {
                self.draw_background(out, viewport);
                let panel = list_panel(viewport);
                push_panel(out, viewport, panel, &self.ui_textures, "inventory_panel");
                push_header(out, viewport, panel, text::CHARACTER_SELECT_TITLE, None);
                if self.characters.is_empty() {
                    push_centered_text(
                        out,
                        viewport,
                        panel,
                        text::EMPTY_CHARACTERS,
                        MENU_TEXT_MUTED,
                    );
                }
                for (index, entry) in self.characters.iter().take(MAX_ROWS).enumerate() {
                    self.draw_list_row(out, viewport, index, &entry.name, 2);
                }
                self.draw_footer(out, viewport, text::NEW_CHARACTER, text::BACK);
                self.draw_notice(out, viewport, panel);
            }
            MenuScreen::WorldSelect => {
                self.draw_background(out, viewport);
                let panel = list_panel(viewport);
                push_panel(out, viewport, panel, &self.ui_textures, "inventory_panel");
                let subtitle = self.selected_character.as_ref().map(|entry| {
                    font::truncate_to_width(
                        &text::character_caption(&entry.name),
                        2.0,
                        panel.width - PANEL_PADDING * 2.0,
                    )
                });
                push_header(
                    out,
                    viewport,
                    panel,
                    text::WORLD_SELECT_TITLE,
                    subtitle.as_deref(),
                );
                if self.worlds.is_empty() {
                    push_centered_text(
                        out,
                        viewport,
                        panel,
                        text::EMPTY_WORLDS,
                        MENU_TEXT_MUTED,
                    );
                }
                for (index, entry) in self.worlds.iter().take(MAX_ROWS).enumerate() {
                    self.draw_list_row(out, viewport, index, &entry.name, 1);
                }
                self.draw_footer(out, viewport, text::NEW_WORLD, text::BACK);
                self.draw_notice(out, viewport, panel);
            }
            MenuScreen::Pause => {
                push_quad(
                    out,
                    viewport,
                    UiRect::new(0.0, 0.0, width, height),
                    theme::OVERLAY_DARK,
                );
                let panel = centered_panel(viewport, PANEL_WIDTH * 0.82, PANEL_HEIGHT);
                push_panel(out, viewport, panel, &self.ui_textures, "inventory_panel");
                push_header(out, viewport, panel, text::PAUSED_TITLE, None);
                let labels = text::PAUSE_BUTTONS;
                for (index, rect) in pause_buttons(viewport).iter().enumerate() {
                    let hovered = rect.contains(self.cursor.0, self.cursor.1);
                    push_button(
                        out,
                        viewport,
                        *rect,
                        labels[index],
                        hovered,
                        true,
                        BUTTON_TEXT_PIXEL,
                        &self.ui_textures,
                    );
                }
                let hint = text::RESUME_HINT;
                let scale = 2.0;
                font::push_text(
                    out,
                    viewport,
                    (width - font::text_width(hint, scale)) * 0.5,
                    panel.bottom() - PANEL_PADDING - font::line_height(scale),
                    scale,
                    MENU_TEXT_MUTED,
                    hint,
                );
                if let Some(notice) = self.notice {
                    font::push_text(
                        out,
                        viewport,
                        (width - font::text_width(notice, scale)) * 0.5,
                        panel.bottom() - PANEL_PADDING - font::line_height(scale) - 24.0,
                        scale,
                        MENU_NOTICE,
                        notice,
                    );
                }
            }
        }

        if self.rename.is_some() {
            self.draw_rename_modal(out, viewport);
        }
    }

    fn draw_background(&self, out: &mut Vec<UiVertex>, viewport: (f32, f32)) {
        let (width, height) = viewport;
        let rect = UiRect::new(0.0, 0.0, width, height);
        if let Some(view) = self.ui_textures.get("begin_bg") {
            push_ui_view(out, viewport, rect, view, [1.0; 4]);
        } else {
            push_quad(out, viewport, rect, MENU_BACKGROUND);
        }
    }

    fn draw_title(&self, out: &mut Vec<UiVertex>, viewport: (f32, f32)) {
        let (width, _height) = viewport;
        let area = title_area(viewport);
        if let Some(view) = self.ui_textures.get("title") {
            let source = view.source_size;
            if source[0] > 0 && source[1] > 0 {
                let ratio = source[0] as f32 / source[1] as f32;
                let rect_width = area.width.min(TITLE_MAX_WIDTH);
                let rect_height = rect_width / ratio;
                let rect = UiRect::new(
                    (width - rect_width) * 0.5,
                    area.y + (area.height - rect_height).max(0.0) * 0.5,
                    rect_width,
                    rect_height,
                );
                push_ui_view(out, viewport, rect, view, [1.0; 4]);
                return;
            }
        }
        let text_label = text::GAME_TITLE;
        let scale = 3.0;
        font::push_text(
            out,
            viewport,
            (width - font::text_width(text_label, scale)) * 0.5,
            area.y + (area.height - font::line_height(scale)) * 0.5,
            scale,
            theme::TEXT_TITLE,
            text_label,
        );
    }

    fn draw_list_row(
        &self,
        out: &mut Vec<UiVertex>,
        viewport: (f32, f32),
        index: usize,
        name: &str,
        actions: usize,
    ) {
        let name_rect = row_name_rect(viewport, index, actions);
        let hovered = name_rect.contains(self.cursor.0, self.cursor.1);
        push_row(out, viewport, name_rect, hovered);
        let scale = BUTTON_TEXT_PIXEL;
        let name = font::truncate_to_width(name, scale, name_rect.width - 24.0);
        font::push_text(
            out,
            viewport,
            name_rect.x + 12.0,
            name_rect.y + (name_rect.height - font::line_height(scale)) * 0.5,
            scale,
            MENU_TEXT,
            &name,
        );
        if actions >= 2 {
            let edit = row_action_rect(viewport, index, actions, 0);
            push_button(
                out,
                viewport,
                edit,
                text::EDIT,
                edit.contains(self.cursor.0, self.cursor.1),
                true,
                scale,
                &self.ui_textures,
            );
        }
        let delete = row_action_rect(viewport, index, actions, actions - 1);
        push_button(
            out,
            viewport,
            delete,
            text::DELETE,
            delete.contains(self.cursor.0, self.cursor.1),
            true,
            scale,
            &self.ui_textures,
        );
    }

    fn draw_footer(&self, out: &mut Vec<UiVertex>, viewport: (f32, f32), new_label: &str, back_label: &str) {
        let new = new_button(viewport);
        push_button(
            out,
            viewport,
            new,
            new_label,
            new.contains(self.cursor.0, self.cursor.1),
            true,
            BUTTON_TEXT_PIXEL,
            &self.ui_textures,
        );
        let back = back_button(viewport);
        push_button(
            out,
            viewport,
            back,
            back_label,
            back.contains(self.cursor.0, self.cursor.1),
            true,
            BUTTON_TEXT_PIXEL,
            &self.ui_textures,
        );
    }

    fn draw_notice(&self, out: &mut Vec<UiVertex>, viewport: (f32, f32), panel: UiRect) {
        let Some(notice) = self.notice else {
            return;
        };
        let scale = 2.0;
        font::push_text(
            out,
            viewport,
            (viewport.0 - font::text_width(notice, scale)) * 0.5,
            panel.bottom() - PANEL_PADDING - 58.0,
            scale,
            MENU_NOTICE,
            notice,
        );
    }

    fn draw_rename_modal(&self, out: &mut Vec<UiVertex>, viewport: (f32, f32)) {
        let (width, height) = viewport;
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, 0.0, width, height),
            [0.0, 0.0, 0.0, 0.55],
        );
        let panel = rename_panel(viewport);
        push_panel(out, viewport, panel, &self.ui_textures, "inventory_panel");
        let title = text::RENAME_TITLE;
        let scale = 3.0;
        font::push_text(
            out,
            viewport,
            panel.x + (panel.width - font::text_width(title, scale)) * 0.5,
            panel.y + 24.0,
            scale,
            MENU_TEXT,
            title,
        );
        let input = rename_input_rect(viewport);
        push_row(out, viewport, input, false);
        let text = self.rename_text().unwrap_or("");
        let text_scale = INPUT_TEXT_PIXEL;
        let text_x = input.x + 12.0;
        let text_y = input.y + (input.height - font::line_height(text_scale)) * 0.5;
        let max_width = (input.width - 24.0).max(0.0);
        let text = font::truncate_to_width(text, text_scale, max_width);
        let text_width = font::push_text(
            out,
            viewport,
            text_x,
            text_y,
            text_scale,
            MENU_TEXT,
            &text,
        );
        if let Some(preedit) = self.preedit_text() {
            let remaining = (max_width - text_width).max(0.0);
            let preedit = font::truncate_to_width(preedit, text_scale, remaining);
            font::push_text(
                out,
                viewport,
                text_x + text_width,
                text_y,
                text_scale,
                MENU_TEXT_MUTED,
                &preedit,
            );
        }
        let hint = text::RENAME_HINT;
        font::push_text(
            out,
            viewport,
            panel.x + (panel.width - font::text_width(hint, 1.0)) * 0.5,
            input.bottom() + 10.0,
            1.0,
            MENU_TEXT_MUTED,
            hint,
        );
        let (confirm, cancel) = rename_buttons(viewport);
        push_button(
            out,
            viewport,
            confirm,
            text::CONFIRM,
            confirm.contains(self.cursor.0, self.cursor.1),
            true,
            BUTTON_TEXT_PIXEL,
            &self.ui_textures,
        );
        push_button(
            out,
            viewport,
            cancel,
            text::CANCEL,
            cancel.contains(self.cursor.0, self.cursor.1),
            true,
            BUTTON_TEXT_PIXEL,
            &self.ui_textures,
        );
    }
}

fn key_to_char(code: KeyCode) -> Option<char> {
    Some(match code {
        KeyCode::KeyA => 'A',
        KeyCode::KeyB => 'B',
        KeyCode::KeyC => 'C',
        KeyCode::KeyD => 'D',
        KeyCode::KeyE => 'E',
        KeyCode::KeyF => 'F',
        KeyCode::KeyG => 'G',
        KeyCode::KeyH => 'H',
        KeyCode::KeyI => 'I',
        KeyCode::KeyJ => 'J',
        KeyCode::KeyK => 'K',
        KeyCode::KeyL => 'L',
        KeyCode::KeyM => 'M',
        KeyCode::KeyN => 'N',
        KeyCode::KeyO => 'O',
        KeyCode::KeyP => 'P',
        KeyCode::KeyQ => 'Q',
        KeyCode::KeyR => 'R',
        KeyCode::KeyS => 'S',
        KeyCode::KeyT => 'T',
        KeyCode::KeyU => 'U',
        KeyCode::KeyV => 'V',
        KeyCode::KeyW => 'W',
        KeyCode::KeyX => 'X',
        KeyCode::KeyY => 'Y',
        KeyCode::KeyZ => 'Z',
        KeyCode::Digit0 | KeyCode::Numpad0 => '0',
        KeyCode::Digit1 | KeyCode::Numpad1 => '1',
        KeyCode::Digit2 | KeyCode::Numpad2 => '2',
        KeyCode::Digit3 | KeyCode::Numpad3 => '3',
        KeyCode::Digit4 | KeyCode::Numpad4 => '4',
        KeyCode::Digit5 | KeyCode::Numpad5 => '5',
        KeyCode::Digit6 | KeyCode::Numpad6 => '6',
        KeyCode::Digit7 | KeyCode::Numpad7 => '7',
        KeyCode::Digit8 | KeyCode::Numpad8 => '8',
        KeyCode::Digit9 | KeyCode::Numpad9 => '9',
        KeyCode::Space => ' ',
        KeyCode::Minus | KeyCode::NumpadSubtract => '-',
        KeyCode::Period | KeyCode::NumpadDecimal => '.',
        _ => return None,
    })
}

fn title_area(viewport: (f32, f32)) -> UiRect {
    let (width, height) = viewport;
    let buttons = main_buttons(viewport);
    let bottom = buttons[0].y - TITLE_GAP;
    UiRect::new(0.0, 20.0, width, (bottom - 20.0).max(0.0).min(height))
}

fn main_buttons(viewport: (f32, f32)) -> [UiRect; 7] {
    let (width, height) = viewport;
    let count = 7.0;
    let total = count * MAIN_BUTTON_HEIGHT + (count - 1.0) * MAIN_BUTTON_GAP;
    let top = (height - MAIN_BOTTOM_MARGIN - total).max(20.0);
    let x = (width - MAIN_BUTTON_WIDTH) * 0.5;
    std::array::from_fn(|index| {
        UiRect::new(
            x,
            top + index as f32 * (MAIN_BUTTON_HEIGHT + MAIN_BUTTON_GAP),
            MAIN_BUTTON_WIDTH,
            MAIN_BUTTON_HEIGHT,
        )
    })
}

fn list_panel(viewport: (f32, f32)) -> UiRect {
    centered_panel(viewport, PANEL_WIDTH, PANEL_HEIGHT)
}

fn row_rect(viewport: (f32, f32), index: usize) -> UiRect {
    let panel = list_panel(viewport);
    let x = panel.x + PANEL_PADDING;
    let width = panel.width - PANEL_PADDING * 2.0;
    let y = panel.y + PANEL_HEADER_HEIGHT + index as f32 * (ROW_HEIGHT + ROW_GAP);
    UiRect::new(x, y, width, ROW_HEIGHT)
}

fn row_name_rect(viewport: (f32, f32), index: usize, actions: usize) -> UiRect {
    let row = row_rect(viewport, index);
    let reserved = actions as f32 * (ROW_BUTTON_WIDTH + ROW_BUTTON_GAP);
    UiRect::new(row.x, row.y, (row.width - reserved).max(0.0), row.height)
}

fn row_action_rect(viewport: (f32, f32), index: usize, actions: usize, action: usize) -> UiRect {
    let row = row_rect(viewport, index);
    let offset = (actions - 1 - action) as f32 * (ROW_BUTTON_WIDTH + ROW_BUTTON_GAP);
    UiRect::new(
        row.right() - offset - ROW_BUTTON_WIDTH,
        row.y,
        ROW_BUTTON_WIDTH,
        row.height,
    )
}

fn footer_y(viewport: (f32, f32)) -> f32 {
    let panel = list_panel(viewport);
    panel.bottom() - PANEL_PADDING - 38.0
}

fn new_button(viewport: (f32, f32)) -> UiRect {
    let panel = list_panel(viewport);
    UiRect::new(
        panel.x + PANEL_PADDING,
        footer_y(viewport),
        FOOTER_BUTTON_WIDTH,
        38.0,
    )
}

fn back_button(viewport: (f32, f32)) -> UiRect {
    let panel = list_panel(viewport);
    UiRect::new(
        panel.right() - PANEL_PADDING - FOOTER_BUTTON_WIDTH,
        footer_y(viewport),
        FOOTER_BUTTON_WIDTH,
        38.0,
    )
}

fn pause_buttons(viewport: (f32, f32)) -> [UiRect; 4] {
    let panel = centered_panel(viewport, PANEL_WIDTH * 0.82, PANEL_HEIGHT);
    let width = 320.0_f32.min(panel.width - PANEL_PADDING * 2.0);
    let height = 42.0;
    let gap = 12.0;
    let top = panel.y + PANEL_HEADER_HEIGHT;
    let x = panel.x + (panel.width - width) * 0.5;
    std::array::from_fn(|index| {
        UiRect::new(x, top + index as f32 * (height + gap), width, height)
    })
}

fn rename_panel(viewport: (f32, f32)) -> UiRect {
    centered_panel(viewport, 520.0, 240.0)
}

fn rename_input_rect(viewport: (f32, f32)) -> UiRect {
    let panel = rename_panel(viewport);
    UiRect::new(
        panel.x + PANEL_PADDING,
        panel.y + 64.0,
        panel.width - PANEL_PADDING * 2.0,
        42.0,
    )
}

fn rename_buttons(viewport: (f32, f32)) -> (UiRect, UiRect) {
    let panel = rename_panel(viewport);
    let width = 150.0;
    let height = 36.0;
    let y = panel.bottom() - PANEL_PADDING - height;
    let confirm = UiRect::new(panel.x + PANEL_PADDING, y, width, height);
    let cancel = UiRect::new(panel.right() - PANEL_PADDING - width, y, width, height);
    (confirm, cancel)
}

fn push_row(out: &mut Vec<UiVertex>, viewport: (f32, f32), rect: UiRect, hovered: bool) {
    push_quad(
        out,
        viewport,
        rect,
        if hovered { ROW_HOVER } else { ROW_BACKGROUND },
    );
    push_border(out, viewport, rect, 1.5, ROW_BORDER);
}

fn push_header(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    panel: UiRect,
    title: &str,
    subtitle: Option<&str>,
) {
    let scale = HEADER_TEXT_PIXEL;
    font::push_text(
        out,
        viewport,
        panel.x + (panel.width - font::text_width(title, scale)) * 0.5,
        panel.y + 18.0,
        scale,
        MENU_TEXT,
        title,
    );
    if let Some(subtitle) = subtitle {
        let small = 2.0;
        font::push_text(
            out,
            viewport,
            panel.x + (panel.width - font::text_width(subtitle, small)) * 0.5,
            panel.y + 18.0 + font::line_height(scale) + 6.0,
            small,
            MENU_TEXT_MUTED,
            subtitle,
        );
    }
}

fn push_centered_text(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    panel: UiRect,
    text: &str,
    color: [f32; 4],
) {
    let scale = 2.0;
    font::push_text(
        out,
        viewport,
        panel.x + (panel.width - font::text_width(text, scale)) * 0.5,
        panel.y + PANEL_HEADER_HEIGHT + 14.0,
        scale,
        color,
        text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> (f32, f32) {
        (1280.0, 720.0)
    }

    #[test]
    fn the_main_menu_routes_single_player_and_quit() {
        let menu = MenuUi::new();
        let buttons = main_buttons(viewport());
        assert_eq!(
            menu.click_at(viewport(), (buttons[0].x + 4.0, buttons[0].y + 4.0)),
            MenuAction::SinglePlayer
        );
        assert_eq!(
            menu.click_at(viewport(), (buttons[6].x + 4.0, buttons[6].y + 4.0)),
            MenuAction::Quit
        );
        assert_eq!(
            menu.click_at(viewport(), (buttons[1].x + 4.0, buttons[1].y + 4.0)),
            MenuAction::Unavailable("多人游戏暂未开放")
        );
    }

    #[test]
    fn selecting_a_character_opens_the_world_screen_with_the_player_name() {
        let mut menu = MenuUi::new();
        menu.open_characters(vec![MenuEntry::new("character_1", "ALICE")]);
        let row = row_name_rect(viewport(), 0, 2);
        assert_eq!(
            menu.click_at(viewport(), (row.x + 4.0, row.y + 4.0)),
            MenuAction::SelectCharacter("character_1".to_owned())
        );
        let edit = row_action_rect(viewport(), 0, 2, 0);
        assert_eq!(
            menu.click_at(viewport(), (edit.x + 4.0, edit.y + 4.0)),
            MenuAction::EditCharacter("character_1".to_owned())
        );
        let delete = row_action_rect(viewport(), 0, 2, 1);
        assert_eq!(
            menu.click_at(viewport(), (delete.x + 4.0, delete.y + 4.0)),
            MenuAction::DeleteCharacter("character_1".to_owned())
        );

        menu.open_worlds(
            MenuEntry::new("character_1", "ALICE"),
            vec![MenuEntry::new("world_1", "WORLD 1")],
        );
        let name = row_name_rect(viewport(), 0, 1);
        assert_eq!(
            menu.click_at(viewport(), (name.x + 4.0, name.y + 4.0)),
            MenuAction::SelectWorld("world_1".to_owned())
        );
        assert_eq!(menu.selected_character().map(|e| e.name.as_str()), Some("ALICE"));
    }

    #[test]
    fn the_pause_menu_resumes_and_saves_and_quits() {
        let mut menu = MenuUi::new();
        menu.open_pause();
        assert_eq!(
            menu.key(KeyCode::Escape, true),
            MenuAction::Resume
        );
        let buttons = pause_buttons(viewport());
        assert_eq!(
            menu.click_at(viewport(), (buttons[3].x + 4.0, buttons[3].y + 4.0)),
            MenuAction::SaveAndQuit
        );
        assert_eq!(
            menu.click_at(viewport(), (buttons[0].x + 4.0, buttons[0].y + 4.0)),
            MenuAction::Unavailable("设置暂未开放")
        );
    }

    #[test]
    fn rename_input_accepts_unicode_ime_text() {
        let mut menu = MenuUi::new();
        menu.open_characters(vec![MenuEntry::new("character_1", "ALICE")]);
        menu.begin_rename("character_1", "ALICE");
        assert!(menu.is_renaming());
        assert_eq!(menu.rename_text(), Some("ALICE"));

        assert_eq!(menu.key(KeyCode::Backspace, true), MenuAction::None);
        assert_eq!(menu.rename_text(), Some("ALIC"));
        // IME 提交中文：内部保存 Unicode，不做 ASCII 过滤。
        menu.ime_commit("小明");
        assert_eq!(menu.rename_text(), Some("ALIC小明"));
        // 预编辑只作预览，不写入名称。
        menu.ime_preedit(Some("zhong"));
        assert_eq!(menu.preedit_text(), Some("zhong"));
        assert_eq!(menu.rename_text(), Some("ALIC小明"));
        // 组字期间物理键位不产生字符，避免与 IME 重复输入。
        assert_eq!(menu.key_with_text(KeyCode::KeyZ, true, None), MenuAction::None);
        assert_eq!(menu.rename_text(), Some("ALIC小明"));
        assert_eq!(menu.key(KeyCode::Enter, true), MenuAction::ConfirmRename);
        assert_eq!(menu.key(KeyCode::Escape, true), MenuAction::CancelRename);
    }

    #[test]
    fn escape_navigates_back_from_lists() {
        let mut menu = MenuUi::new();
        menu.open_characters(Vec::new());
        assert_eq!(menu.key(KeyCode::Escape, true), MenuAction::BackToMain);
        menu.open_worlds(MenuEntry::new("character_1", "A"), Vec::new());
        assert_eq!(menu.key(KeyCode::Escape, true), MenuAction::BackToCharacters);
    }

    #[test]
    fn every_screen_stays_inside_clip_space() {
        let mut menu = MenuUi::new();
        let mut vertices = Vec::new();
        for screen in 0..4 {
            match screen {
                0 => menu.open_main(),
                1 => menu.open_characters(vec![MenuEntry::new("character_1", "ALICE")]),
                2 => menu.open_worlds(
                    MenuEntry::new("character_1", "ALICE"),
                    vec![MenuEntry::new("world_1", "WORLD 1")],
                ),
                _ => menu.open_pause(),
            }
            vertices.clear();
            menu.build(&mut vertices, viewport());
            assert!(!vertices.is_empty());
            assert_eq!(vertices.len() % 6, 0, "顶点必须成三角形对");
            for vertex in &vertices {
                assert!((-1.0..=1.0).contains(&vertex.position[0]));
                assert!((-1.0..=1.0).contains(&vertex.position[1]));
            }
        }
    }

    #[test]
    fn an_invalid_viewport_draws_nothing() {
        let menu = MenuUi::new();
        let mut vertices = Vec::new();
        menu.build(&mut vertices, (0.0, 720.0));
        assert!(vertices.is_empty());
    }
}

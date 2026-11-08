//! UI 状态管理器：统一维护界面状态、输入优先级与绘制顺序。
//!
//! 界面状态只有四种，且任意时刻最多一个全屏界面：
//! `Gameplay` → `Inventory` / `Crafting` → `Death`。
//! 死亡优先级最高，进入死亡会强制关闭其他界面；背包与合成互斥，
//! 打开其中一个不会自动切换到另一个。鼠标事件按当前状态路由，
//! 因此一次点击只可能作用在一个界面上。

use super::crafting::CraftingUi;
use super::death::{DeathAction, DeathUi};
use super::draw::UiVertex;
use super::input::ClickButton;
use super::inventory::InventoryUi;
use crate::item::{Inventory, Recipe, RecipeId};
use crate::player::DamageSource;
use crate::texture::{ItemTextureMap, UiTextureMap};

/// 全屏界面状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiState {
    Gameplay,
    Inventory,
    Crafting,
    Death,
}

impl UiState {
    /// 是否应当屏蔽世界输入与 HUD。
    pub const fn blocks_world_input(self) -> bool {
        !matches!(self, Self::Gameplay)
    }

    /// 调试与日志使用的状态名。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gameplay => "gameplay",
            Self::Inventory => "inventory",
            Self::Crafting => "crafting",
            Self::Death => "death",
        }
    }
}

/// 一次鼠标交互对外产生的副作用；界面内部的选择状态不在此列。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiPointerAction {
    #[default]
    None,
    /// 背包内容已被移动/合并/拆分，需要落盘。
    InventoryChanged,
    /// 玩家点击了合成按钮，需要由游戏层原子执行。
    Craft(RecipeId),
    /// 玩家点击了重生按钮。
    Respawn,
    /// 玩家点击了返回主菜单按钮。
    MainMenu,
}

/// 统一 UI 管理器。
#[derive(Clone, Debug, PartialEq)]
pub struct UiManager {
    state: UiState,
    inventory: InventoryUi,
    crafting: CraftingUi,
    death: DeathUi,
}

impl Default for UiManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UiManager {
    pub fn new() -> Self {
        Self {
            state: UiState::Gameplay,
            inventory: InventoryUi::new(),
            crafting: CraftingUi::new(),
            death: DeathUi::new(),
        }
    }

    pub const fn state(&self) -> UiState {
        self.state
    }

    /// 任意全屏界面是否打开；用于隐藏 HUD 与屏蔽世界输入。
    pub const fn blocks_world_input(&self) -> bool {
        self.state.blocks_world_input()
    }

    pub const fn is_gameplay(&self) -> bool {
        matches!(self.state, UiState::Gameplay)
    }

    pub fn crafting_mut(&mut self) -> &mut CraftingUi {
        &mut self.crafting
    }

    /// 把 UI 控件图集分发给各界面；资源加载或热重载后由游戏层调用。
    pub fn set_ui_textures(&mut self, textures: &UiTextureMap) {
        self.inventory.set_ui_textures(textures.clone());
        self.crafting.set_ui_textures(textures.clone());
        self.death.set_ui_textures(textures.clone());
    }

    /// 从游戏内打开背包；其他状态一律拒绝，避免界面叠加。
    pub fn open_inventory(&mut self) -> bool {
        if !self.is_gameplay() {
            return false;
        }
        self.inventory.open();
        self.state = UiState::Inventory;
        true
    }

    /// 关闭背包；拖动中的物品放不下时保持打开，返回 false。
    pub fn close_inventory(&mut self, inventory: &mut Inventory) -> bool {
        if self.state != UiState::Inventory {
            return false;
        }
        if !self.inventory.try_close(inventory) {
            return false;
        }
        self.state = UiState::Gameplay;
        true
    }

    /// 从游戏内打开合成界面。
    pub fn open_crafting(&mut self) -> bool {
        if !self.is_gameplay() {
            return false;
        }
        self.crafting.open();
        self.state = UiState::Crafting;
        true
    }

    /// 关闭合成界面。
    pub fn close_crafting(&mut self) -> bool {
        if self.state != UiState::Crafting {
            return false;
        }
        self.crafting.close();
        self.state = UiState::Gameplay;
        true
    }

    /// 进入死亡界面：强制关闭背包与合成，保证死亡界面独占输入。
    pub fn enter_death(&mut self, inventory: &mut Inventory, cause: DamageSource) {
        if self.state == UiState::Death {
            return;
        }
        if self.state == UiState::Inventory {
            self.inventory.force_close(inventory);
        }
        self.crafting.close();
        self.death.open(cause);
        self.state = UiState::Death;
    }

    /// 重生后回到游戏内。
    pub fn leave_death(&mut self) -> bool {
        if self.state != UiState::Death {
            return false;
        }
        self.death.close();
        self.state = UiState::Gameplay;
        true
    }

    /// 退出前关闭全部界面；拖动中的物品尽量放回背包。
    pub fn force_close(&mut self, inventory: &mut Inventory) {
        self.inventory.force_close(inventory);
        self.crafting.close();
        self.death.close();
        self.state = UiState::Gameplay;
    }

    /// 同步鼠标位置到所有子界面，切换界面时提示框与按钮状态立即正确。
    pub fn set_cursor(&mut self, x: f32, y: f32) {
        self.inventory.set_cursor(x, y);
        self.crafting.set_cursor(x, y);
        self.death.set_cursor((x, y));
    }

    /// 推进所有子界面的短动画与提示计时。
    pub fn update(&mut self, delta_secs: f32) {
        self.inventory.update(delta_secs);
        self.crafting.update(delta_secs);
    }

    /// 按当前状态路由一次鼠标点击。
    ///
    /// 返回的副作用由游戏层执行；未打开对应界面时直接忽略。
    pub fn click(
        &mut self,
        button: ClickButton,
        pressed: bool,
        inventory: &mut Inventory,
        viewport: (f32, f32),
        recipes: &[Recipe],
    ) -> UiPointerAction {
        if !pressed {
            return UiPointerAction::None;
        }
        match self.state {
            UiState::Gameplay => UiPointerAction::None,
            UiState::Death => {
                if button != ClickButton::Left {
                    return UiPointerAction::None;
                }
                let cursor = self.death.cursor();
                match self.death.click_at(viewport, cursor) {
                    DeathAction::Respawn => UiPointerAction::Respawn,
                    DeathAction::MainMenu => UiPointerAction::MainMenu,
                    DeathAction::None => UiPointerAction::None,
                }
            }
            UiState::Crafting => {
                if button != ClickButton::Left {
                    return UiPointerAction::None;
                }
                match self
                    .crafting
                    .click_at(recipes, viewport, self.crafting.cursor())
                {
                    Some(id) => UiPointerAction::Craft(id),
                    None => UiPointerAction::None,
                }
            }
            UiState::Inventory => {
                if self
                    .inventory
                    .click_at(inventory, button, self.inventory.cursor(), viewport)
                {
                    UiPointerAction::InventoryChanged
                } else {
                    UiPointerAction::None
                }
            }
        }
    }

    /// 生成当前界面的全部 UI 顶点；绘制顺序由状态保证。
    pub fn build(
        &self,
        out: &mut Vec<UiVertex>,
        inventory: &Inventory,
        selected_hotbar: usize,
        icons: &ItemTextureMap,
        viewport: (f32, f32),
        recipes: &[Recipe],
    ) {
        out.clear();
        match self.state {
            UiState::Gameplay => {}
            UiState::Inventory => {
                self.inventory
                    .build(out, inventory, selected_hotbar, icons, viewport);
            }
            UiState::Crafting => {
                self.crafting
                    .build(out, recipes, inventory, icons, viewport);
            }
            UiState::Death => self.death.build(out, viewport),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::recipe_registry;
    use crate::item::{ItemId, ItemStack};
    use crate::texture::ItemTextureMap;

    fn viewport() -> (f32, f32) {
        (1280.0, 720.0)
    }

    #[test]
    fn only_one_screen_can_be_open_at_a_time() {
        let mut ui = UiManager::new();
        assert!(ui.open_inventory());
        assert_eq!(ui.state(), UiState::Inventory);
        assert!(!ui.open_crafting(), "背包打开时不能叠加合成界面");
        assert_eq!(ui.state(), UiState::Inventory);

        let mut inventory = Inventory::new();
        assert!(ui.close_inventory(&mut inventory));
        assert!(ui.open_crafting());
        assert!(!ui.open_inventory(), "合成打开时不能叠加背包");
        assert_eq!(ui.state(), UiState::Crafting);
    }

    #[test]
    fn death_has_the_highest_priority() {
        let mut ui = UiManager::new();
        ui.open_inventory();
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(ItemStack::new(ItemId::STONE, 3).expect("合法堆叠")));

        ui.enter_death(&mut inventory, DamageSource::Fall);
        assert_eq!(ui.state(), UiState::Death);
        assert!(ui.blocks_world_input());

        assert!(!ui.open_inventory(), "死亡界面拒绝打开其他界面");
        assert!(!ui.open_crafting(), "死亡界面拒绝打开其他界面");
    }

    #[test]
    fn respawning_returns_to_gameplay() {
        let mut ui = UiManager::new();
        let mut inventory = Inventory::new();
        ui.enter_death(&mut inventory, DamageSource::Fall);
        assert!(ui.leave_death());
        assert_eq!(ui.state(), UiState::Gameplay);
        assert!(!ui.blocks_world_input());
        assert!(!ui.leave_death(), "重复重生不应改变状态");
    }

    #[test]
    fn closing_the_inventory_returns_to_gameplay() {
        let mut ui = UiManager::new();
        ui.open_inventory();
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(ItemStack::new(ItemId::STONE, 5).expect("合法堆叠")));
        assert!(ui.close_inventory(&mut inventory));
        assert_eq!(ui.state(), UiState::Gameplay);
        assert_eq!(inventory.count_of(ItemId::STONE), 5);
    }

    #[test]
    fn click_routing_is_state_scoped() {
        let mut ui = UiManager::new();
        let mut inventory = Inventory::new();
        assert_eq!(
            ui.click(
                ClickButton::Left,
                true,
                &mut inventory,
                viewport(),
                recipe_registry::all()
            ),
            UiPointerAction::None,
            "游戏内点击由世界层处理"
        );

        ui.open_inventory();
        assert_eq!(
            ui.click(
                ClickButton::Left,
                false,
                &mut inventory,
                viewport(),
                recipe_registry::all()
            ),
            UiPointerAction::None,
            "按键释放不触发背包操作"
        );
    }

    #[test]
    fn crafting_clicks_only_surface_the_craft_action() {
        let mut ui = UiManager::new();
        ui.open_crafting();
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(ItemStack::new(ItemId::WOOD, 4).expect("合法堆叠")));

        // 合成面板的按钮位置由布局决定；直接点击面板右侧下方区域可能命中按钮。
        // 这里通过重复点击扫描，确保命中时返回的是合成动作而不是背包变更。
        let mut found = None;
        for y in (40..680).step_by(20) {
            for x in (20..1260).step_by(20) {
                let action = ui.click(
                    ClickButton::Left,
                    true,
                    &mut inventory,
                    viewport(),
                    recipe_registry::all(),
                );
                if matches!(action, UiPointerAction::Craft(_)) {
                    found = Some(action);
                    break;
                }
                ui.set_cursor(x as f32, y as f32);
            }
            if found.is_some() {
                break;
            }
        }
        assert!(found.is_some(), "合成界面必须能通过点击触发合成");
        assert_eq!(inventory.count_of(ItemId::WOOD), 4, "界面层不消耗材料");
    }

    #[test]
    fn build_dispatches_by_state_and_uses_the_screen_rect() {
        let mut ui = UiManager::new();
        let inventory = Inventory::new();
        let mut vertices = Vec::new();
        ui.build(
            &mut vertices,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            viewport(),
            recipe_registry::all(),
        );
        assert!(vertices.is_empty(), "游戏内不产生全屏界面几何");

        ui.open_inventory();
        ui.build(
            &mut vertices,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            viewport(),
            recipe_registry::all(),
        );
        assert!(!vertices.is_empty());
        assert!(vertices
            .iter()
            .all(|vertex| (-1.0..=1.0).contains(&vertex.position[0])));

        ui.enter_death(&mut Inventory::new(), DamageSource::Entity);
        ui.build(
            &mut vertices,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            viewport(),
            recipe_registry::all(),
        );
        assert!(!vertices.is_empty(), "死亡界面覆盖整个屏幕");
    }

    #[test]
    fn force_close_clears_every_screen() {
        let mut ui = UiManager::new();
        ui.open_crafting();
        let mut inventory = Inventory::new();
        ui.force_close(&mut inventory);
        assert_eq!(ui.state(), UiState::Gameplay);
    }
}

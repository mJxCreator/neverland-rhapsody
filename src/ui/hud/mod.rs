//! 游戏内 HUD 组合层。
//!
//! HUD 本身不实现具体视觉，只负责按固定顺序组合各独立模块：
//! 状态条、快捷栏、准星、进度条、目标血条、受伤反馈、提示与昼夜文字。
//! 每个模块持有自己的 `config`，调整参数不需要改动本文件。

use crate::item::{ItemId, ItemStack, HOTBAR_SLOTS};
use crate::texture::{ItemTextureMap, UiTextureMap};
use crate::ui::core::draw::UiVertex;
use crate::ui::core::layout::valid_viewport;
use crate::ui::crosshair::CrosshairConfig;
use crate::ui::damage::DamageFeedback;
use crate::ui::hotbar::HotbarUi;
use crate::ui::notifications::Notifications;
use crate::ui::progress::ProgressBar;
use crate::ui::state::StateBars;
use crate::ui::target::TargetHud;
use crate::ui::time::TimePhase;

/// 每帧由上层写入的 HUD 数据。
#[derive(Clone, Debug, PartialEq)]
pub struct HudModel {
    /// 快捷栏模块。
    pub hotbar: HotbarUi,
    /// 生命/体力/饥饿状态条模块。
    pub state: StateBars,
    /// 挖掘/进食进度条模块。
    pub progress: ProgressBar,
    /// 敌对生物名牌与血条模块。
    pub target: TargetHud,
    /// 受伤反馈模块。
    pub damage: DamageFeedback,
    /// 短提示模块。
    pub notifications: Notifications,
    /// 昼夜阶段模块。
    pub time_phase: TimePhase,
    /// 准星参数。
    pub crosshair: CrosshairConfig,
    /// 任意全屏界面是否打开；打开时不绘制战斗与快捷栏 HUD。
    pub ui_open: bool,
    /// UI 控件图集；为空时各模块回退纯色绘制。
    ui_textures: UiTextureMap,
}

impl Default for HudModel {
    fn default() -> Self {
        Self {
            hotbar: HotbarUi::default(),
            state: StateBars::default(),
            progress: ProgressBar::default(),
            target: TargetHud::default(),
            damage: DamageFeedback::default(),
            notifications: Notifications::default(),
            time_phase: TimePhase::default(),
            crosshair: CrosshairConfig::default(),
            ui_open: false,
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
        self.hotbar.set(selected_slot, slots);
        self.progress.set_mining(mining_progress);
        self.ui_open = ui_open;
    }

    /// 写入生命值；最大值为 0 时回落到默认 100 点。
    pub fn set_health(&mut self, health: u32, max_health: u32) {
        self.state.set_health(health, max_health);
    }

    /// 写入饥饿、体力与昼夜阶段；数值越界时自动裁剪到 0–100。
    pub fn set_survival(&mut self, hunger: f32, stamina: f32, time_label: &'static str) {
        self.state.set_survival(hunger, stamina);
        self.time_phase.label = time_label;
    }

    /// 设置进食进度；`None` 表示未进食。
    pub fn set_eating_progress(&mut self, progress: Option<f32>) {
        self.progress.set_eating(progress);
    }

    /// 显示一条短状态提示，覆盖旧提示并重新计时。
    pub fn show_message(&mut self, text: &'static str) {
        self.notifications.show_message(text);
    }

    /// 受到伤害时触发短暂红色边缘反馈。
    pub fn show_damage(&mut self) {
        self.damage.show_flash();
    }

    /// 显示敌对生物血量；持续 2.5 秒，重复调用会刷新计时。
    pub fn show_target(&mut self, name: &'static str, health: u32, max_health: u32) {
        self.target.show(name, health, max_health);
    }

    /// 显示受击方向；`angle` 为相对镜头朝向的弧度（0 为正前方，正值在右）。
    pub fn show_damage_direction(&mut self, angle: f32) {
        self.damage.show_direction(angle);
    }

    /// 显示一条拾取提示；同一物品重复拾取会合并数量并刷新计时。
    pub fn show_pickup(&mut self, item: ItemId, count: u32) {
        self.notifications.show_pickup(item, count);
    }

    /// 显示工具损坏提示；这是耐久归零后的即时反馈。
    pub fn show_tool_broken(&mut self, item: ItemId) {
        self.notifications.show_tool_broken(item);
    }

    /// 推进提示、伤害反馈与选中动画计时。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        self.hotbar.update(delta_secs);
        self.target.update(delta_secs);
        self.damage.update(delta_secs);
        self.notifications.update(delta_secs);
    }

    /// 生成全部 HUD 四边形顶点，写入 `out`；`icons` 提供物品图标的图集 UV。
    pub fn build(&self, out: &mut Vec<UiVertex>, width: f32, height: f32, icons: &ItemTextureMap) {
        out.clear();
        if !valid_viewport(width, height) || self.ui_open {
            return;
        }

        let hotbar_top = height - self.hotbar.top_offset();
        let state_top = self.state.panel_top(hotbar_top);

        self.target
            .draw(out, width, height, state_top - self.target.config.gap);
        self.state
            .draw(out, width, height, hotbar_top, &self.ui_textures);
        self.time_phase.draw(out, width, height);
        self.hotbar
            .draw(out, width, height, icons, &self.ui_textures);
        self.notifications
            .draw(out, width, height, hotbar_top, icons);
        self.progress.draw(out, width, height, &self.ui_textures);
        self.damage.draw(out, width, height, &self.ui_textures);
        crate::ui::crosshair::draw(out, width, height, &self.crosshair, &self.ui_textures);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::ItemTextureMap;
    use crate::ui::core::draw::SOLID_UV;
    use crate::ui::core::layout::UiRect;
    use crate::ui::core::{theme, widget};

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
            hotbar: HotbarUi {
                slots,
                ..HotbarUi::default()
            },
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
            hotbar: HotbarUi {
                slots,
                ..HotbarUi::default()
            },
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
            hotbar: HotbarUi {
                slots,
                ..HotbarUi::default()
            },
            progress: ProgressBar {
                mining_progress: Some(0.5),
                ..ProgressBar::default()
            },
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
            progress: ProgressBar {
                mining_progress: Some(0.3),
                ..ProgressBar::default()
            },
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
        assert_eq!(model.hotbar.selected_slot, HOTBAR_SLOTS - 1);
        assert_eq!(model.progress.mining_progress, Some(2.0));
        model.set(0, [None; HOTBAR_SLOTS], Some(f32::NAN), false);
        assert_eq!(model.progress.mining_progress, None);
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
        let mut model = HudModel::default();
        model.set(0, [None; HOTBAR_SLOTS], Some(0.5), true);
        model.build(&mut vertices, 1280.0, 720.0, &ItemTextureMap::empty());
        assert!(vertices.is_empty());
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
    fn durability_bars_only_apply_to_tools() {
        let viewport = (1280.0, 720.0);
        let rect = UiRect::new(0.0, 0.0, 48.0, 48.0);

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
}

//! 玩家角色视觉层：模型实例、动画状态机、装备挂载与第一/第三人称表现。
//!
//! 该层只读取游戏状态与装备数据，不修改物理、伤害或存档；
//! 视角切换也仅改变相机与绘制内容。

pub mod animation_state;
pub mod first_person;
pub mod player_model;
pub mod third_person;

use std::collections::HashSet;

use glam::{Mat4, Quat, Vec3};

use self::animation_state::{AnimationContext, PlayerAnimationController, PlayerAnimationState};
use crate::attachment::AttachmentManager;
use crate::equipment::Equipment;
use crate::equipment_visual;
use crate::item::ItemStack;
use crate::model::types::Transform;
use crate::model::{
    instance_draws, static_draws, ModelDraw, ModelId, ModelInstance, ModelRegistry,
};

/// 相机视角模式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraMode {
    #[default]
    FirstPerson,
    ThirdPerson,
}

impl CameraMode {
    pub const fn toggle(self) -> Self {
        match self {
            Self::FirstPerson => Self::ThirdPerson,
            Self::ThirdPerson => Self::FirstPerson,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::FirstPerson => "FIRST PERSON",
            Self::ThirdPerson => "THIRD PERSON",
        }
    }
}

/// 驱动玩家视觉的一帧游戏状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerVisualState {
    /// 插值后的双脚位置。
    pub feet_position: Vec3,
    pub yaw: f32,
    pub velocity: Vec3,
    pub grounded: bool,
    pub health: u32,
    pub dead: bool,
    pub mining: bool,
    pub eating: bool,
    pub attacking: bool,
    /// 本帧真实落地事件。
    pub landed: bool,
}

/// 玩家角色视觉主体。
pub struct PlayerVisual {
    player_model: Option<ModelId>,
    instance: ModelInstance,
    fp_arms: Option<ModelId>,
    attachments: AttachmentManager,
    controller: PlayerAnimationController,
    state: PlayerAnimationState,
    root: Mat4,
    time: f32,
    time_in_state: f32,
    speed: f32,
    last_health: Option<u32>,
    warned_clips: HashSet<&'static str>,
    warned_models: HashSet<String>,
    held: Option<(ModelId, Transform)>,
}

impl PlayerVisual {
    /// 从注册表解析玩家与第一人称手臂模型；缺失时安全降级为空手/无角色。
    pub fn new(models: &ModelRegistry) -> Self {
        let player_model = models.by_name(player_model::PLAYER_MODEL);
        let fp_arms = models.by_name(player_model::FIRST_PERSON_ARMS_MODEL);
        Self {
            player_model,
            instance: ModelInstance::new(player_model.unwrap_or(ModelId::INVALID)),
            fp_arms,
            attachments: AttachmentManager::new(),
            controller: PlayerAnimationController::new(),
            state: PlayerAnimationState::Idle,
            root: Mat4::IDENTITY,
            time: 0.0,
            time_in_state: 0.0,
            speed: 0.0,
            last_health: None,
            warned_clips: HashSet::new(),
            warned_models: HashSet::new(),
            held: None,
        }
    }

    /// 重生或切换世界后恢复默认动画与外观。
    pub fn reset(&mut self) {
        self.controller.reset();
        self.state = PlayerAnimationState::Idle;
        self.time = 0.0;
        self.time_in_state = 0.0;
        self.speed = 0.0;
        self.last_health = None;
        self.attachments.clear();
        self.held = None;
    }

    /// 推进动画、姿态与装备挂载。返回本帧动画状态。
    pub fn update(
        &mut self,
        models: &ModelRegistry,
        equipment: &Equipment,
        held_stack: Option<ItemStack>,
        state: &PlayerVisualState,
        delta: f32,
    ) -> PlayerAnimationState {
        let delta = if delta.is_finite() {
            delta.max(0.0)
        } else {
            0.0
        };
        let damaged = self.last_health.is_some_and(|health| state.health < health);
        self.last_health = Some(state.health);
        self.speed = Vec3::new(state.velocity.x, 0.0, state.velocity.z).length();
        let context = AnimationContext {
            grounded: state.grounded,
            vertical_speed: state.velocity.y,
            horizontal_speed: self.speed,
            sprinting: self.speed >= 4.8,
            mining: state.mining,
            eating: state.eating,
            attacking: state.attacking,
            dead: state.dead,
            landed: state.landed,
            damaged,
        };
        let update = self.controller.update(&context, delta);
        if self.state != update.state {
            self.time_in_state = 0.0;
        } else {
            self.time_in_state += delta;
        }
        if update.restart {
            self.time_in_state = 0.0;
        }
        self.state = update.state;
        self.time = (self.time + delta) % 1000.0;

        self.root = Mat4::from_rotation_translation(
            // 模型本地 +Z 为正面，玩家水平前方为 -Z，因此偏航相差 180°。
            Quat::from_rotation_y(state.yaw + std::f32::consts::PI),
            state.feet_position,
        );

        if let Some(model) = self.player_model {
            if let Some(asset) = models.get(model) {
                if self.instance.model != model {
                    self.instance.set_model(model);
                }
                match resolve_clip(&asset, self.state) {
                    Some(index) => {
                        let looping = self.state.looping();
                        if update.restart {
                            self.instance.player.play(index, looping);
                        } else {
                            self.instance.play_clip(&asset, index, looping);
                        }
                        if asset.animation_index(self.state.clip_name()).is_none()
                            && self.warned_clips.insert(self.state.clip_name())
                        {
                            tracing::warn!(
                                model = %asset.name,
                                clip = self.state.clip_name(),
                                "玩家模型缺少动画片段，已回退到 idle 或首个片段"
                            );
                        }
                    }
                    None => {
                        if self.warned_clips.insert(self.state.clip_name()) {
                            tracing::warn!(
                                model = %asset.name,
                                clip = self.state.clip_name(),
                                "玩家模型没有任何动画片段，已保持绑定姿态"
                            );
                        }
                    }
                }
                self.instance.set_speed(update.speed);
                self.instance.update(&asset, delta);
            }
        }

        equipment_visual::apply_current(
            &mut self.attachments,
            equipment,
            held_stack,
            models,
            &mut self.warned_models,
        );
        self.held = held_stack.and_then(|stack| resolve_held(stack, models));
        self.state
    }

    /// 生成第三或第一人称的模型绘制批次。
    #[allow(clippy::too_many_arguments)]
    pub fn build_draws(
        &mut self,
        models: &ModelRegistry,
        mode: CameraMode,
        camera_position: Vec3,
        yaw: f32,
        pitch: f32,
        out: &mut Vec<ModelDraw>,
    ) {
        match mode {
            CameraMode::ThirdPerson => self.build_third_person(models, out),
            CameraMode::FirstPerson => {
                self.build_first_person(models, camera_position, yaw, pitch, out)
            }
        }
    }

    fn build_third_person(&mut self, models: &ModelRegistry, out: &mut Vec<ModelDraw>) {
        let Some(model) = self.player_model else {
            return;
        };
        let Some(asset) = models.get(model) else {
            return;
        };
        instance_draws(&asset, &self.instance, self.root, 0.0, out);
        if let Some(skeleton) = &asset.skeleton {
            self.attachments
                .build_draws(models, self.root, self.instance.pose(), skeleton, out);
        }
    }

    fn build_first_person(
        &self,
        models: &ModelRegistry,
        camera_position: Vec3,
        yaw: f32,
        pitch: f32,
        out: &mut Vec<ModelDraw>,
    ) {
        // 死亡时由死亡界面接管，避免第一人称手臂悬在半空。
        if self.state == PlayerAnimationState::Death {
            return;
        }
        if let Some(model) = self.fp_arms {
            if let Some(asset) = models.get(model) {
                let transform = first_person::arm_transform(
                    camera_position,
                    yaw,
                    pitch,
                    self.state,
                    self.speed,
                    self.time,
                    self.time_in_state,
                );
                static_draws(&asset, model, transform, 0.0, out);
            }
        }
        if let Some((model, local)) = self.held {
            if let Some(asset) = models.get(model) {
                let transform = first_person::held_transform(
                    camera_position,
                    yaw,
                    pitch,
                    self.state,
                    self.speed,
                    self.time,
                    self.time_in_state,
                    local,
                );
                static_draws(&asset, model, transform, 0.0, out);
            }
        }
    }
}

/// 解析当前手持物品的模型与第一人称局部变换。
fn resolve_held(stack: ItemStack, models: &ModelRegistry) -> Option<(ModelId, Transform)> {
    if stack.is_broken() {
        return None;
    }
    let definition = equipment_visual::definition(stack.item())?;
    if definition.display != equipment_visual::DisplayMode::Handheld {
        return None;
    }
    let part = definition.parts.first()?;
    let model = models.by_name(&part.model)?;
    Some((model, definition.first_person))
}

/// 按状态名查找片段，缺失时回退到 idle 或第一个片段。
fn resolve_clip(asset: &crate::model::ModelAsset, state: PlayerAnimationState) -> Option<usize> {
    asset
        .animation_index(state.clip_name())
        .or_else(|| asset.animation_index("idle"))
        .or(if asset.animations.is_empty() {
            None
        } else {
            Some(0)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::EquipmentSlot;
    use crate::item::ItemId;
    use crate::model::ModelRegistry;

    fn models() -> ModelRegistry {
        let mut models = ModelRegistry::new();
        models.register_builtin(player_model::PLAYER_MODEL, player_model::player());
        models.register_builtin(
            player_model::FIRST_PERSON_ARMS_MODEL,
            player_model::first_person_arms(),
        );
        equipment_visual::builtin::register(&mut models);
        models
    }

    fn state() -> PlayerVisualState {
        PlayerVisualState {
            feet_position: Vec3::ZERO,
            yaw: 0.0,
            velocity: Vec3::ZERO,
            grounded: true,
            health: 20,
            dead: false,
            mining: false,
            eating: false,
            attacking: false,
            landed: false,
        }
    }

    #[test]
    fn visual_resolves_builtin_models() {
        let models = models();
        let visual = PlayerVisual::new(&models);
        assert!(visual.player_model.is_some());
        assert!(visual.fp_arms.is_some());
    }

    #[test]
    fn missing_models_degrade_without_panicking() {
        let models = ModelRegistry::new();
        let mut visual = PlayerVisual::new(&models);
        let mut draws = Vec::new();
        visual.build_draws(
            &models,
            CameraMode::ThirdPerson,
            Vec3::ZERO,
            0.0,
            0.0,
            &mut draws,
        );
        assert!(draws.is_empty());
        let mut visual = PlayerVisual::new(&models);
        visual.build_draws(
            &models,
            CameraMode::FirstPerson,
            Vec3::ZERO,
            0.0,
            0.0,
            &mut draws,
        );
        assert!(draws.is_empty());
    }

    #[test]
    fn third_person_draws_body_attachments_and_held_item() {
        let models = models();
        let mut visual = PlayerVisual::new(&models);
        let mut equipment = Equipment::default();
        equipment
            .equip(
                EquipmentSlot::Head,
                ItemStack::new(ItemId::IRON_HELMET, 1).unwrap(),
            )
            .unwrap();
        let held = ItemStack::new(ItemId::IRON_SWORD, 1);
        visual.update(&models, &equipment, held, &state(), 0.016);
        assert_eq!(visual.attachments.len(), 2);
        let mut draws = Vec::new();
        visual.build_draws(
            &models,
            CameraMode::ThirdPerson,
            Vec3::new(0.0, 1.6, 4.0),
            0.0,
            0.0,
            &mut draws,
        );
        assert!(draws
            .iter()
            .any(|draw| draw.model == models.by_name("player").unwrap()));
        assert!(draws
            .iter()
            .any(|draw| draw.model == models.by_name("iron_helmet").unwrap()));
        assert!(draws
            .iter()
            .any(|draw| draw.model == models.by_name("iron_sword").unwrap()));
    }

    #[test]
    fn first_person_hides_body_but_draws_arms_and_item() {
        let models = models();
        let mut visual = PlayerVisual::new(&models);
        let equipment = Equipment::default();
        let held = ItemStack::new(ItemId::IRON_SWORD, 1);
        visual.update(&models, &equipment, held, &state(), 0.016);
        let mut draws = Vec::new();
        visual.build_draws(
            &models,
            CameraMode::FirstPerson,
            Vec3::ZERO,
            0.0,
            0.0,
            &mut draws,
        );
        assert!(draws
            .iter()
            .all(|draw| draw.model != models.by_name("player").unwrap()));
        assert!(draws.iter().any(|draw| {
            draw.model
                == models
                    .by_name(player_model::FIRST_PERSON_ARMS_MODEL)
                    .unwrap()
        }));
        assert!(draws
            .iter()
            .any(|draw| draw.model == models.by_name("iron_sword").unwrap()));
    }

    #[test]
    fn mining_and_death_drive_animation_state() {
        let models = models();
        let mut visual = PlayerVisual::new(&models);
        let equipment = Equipment::default();
        let mut ctx = state();
        ctx.mining = true;
        assert_eq!(
            visual.update(&models, &equipment, None, &ctx, 0.016),
            PlayerAnimationState::Mining
        );
        ctx.mining = false;
        ctx.dead = true;
        assert_eq!(
            visual.update(&models, &equipment, None, &ctx, 0.016),
            PlayerAnimationState::Death
        );
    }

    #[test]
    fn reset_clears_attachments_and_state() {
        let models = models();
        let mut visual = PlayerVisual::new(&models);
        let equipment = Equipment::default();
        let held = ItemStack::new(ItemId::IRON_SWORD, 1);
        visual.update(&models, &equipment, held, &state(), 0.016);
        assert!(!visual.attachments.is_empty());
        visual.reset();
        assert_eq!(visual.attachments.len(), 0);
        assert_eq!(visual.state, PlayerAnimationState::Idle);
    }

    #[test]
    fn camera_mode_toggles_between_two_states() {
        assert_eq!(CameraMode::FirstPerson.toggle(), CameraMode::ThirdPerson);
        assert_eq!(CameraMode::ThirdPerson.toggle(), CameraMode::FirstPerson);
        assert_eq!(CameraMode::FirstPerson.label(), "FIRST PERSON");
    }
}

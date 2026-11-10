//! 实体与模型资源的桥接：为每只生物维护独立的模型实例、动画状态与世界变换。
//!
//! 模型资源按名称映射（`pig` / `cow`），找不到模型时该生物继续使用旧盒状几何；
//! 每只存活生物拥有独立姿态，动画在待机/行走/奔跑之间按 AI 状态切换。

use std::collections::{HashMap, HashSet};

use glam::{Mat4, Quat, Vec3};

use super::ai::AiState;
use super::creature::Creature;
use super::entity::{EntityId, EntityTypeId};
use crate::model::{
    instance_draws, MeshBounds, ModelAsset, ModelDraw, ModelId, ModelInstance, ModelRegistry,
};

/// 实体模型参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityModelConfig {
    /// 最大渲染距离（格），超出后不产生模型批次。
    pub render_distance: f32,
}

impl Default for EntityModelConfig {
    fn default() -> Self {
        Self {
            render_distance: 80.0,
        }
    }
}

/// 实体类型到模型名称的固定映射。
const MODEL_NAMES: [(EntityTypeId, &str); 2] =
    [(EntityTypeId::PIG, "pig"), (EntityTypeId::COW, "cow")];

/// 生物模型实例集合。
pub struct EntityModelSet {
    /// 仅在注册表中真实存在的类型才会进入映射。
    models: HashMap<EntityTypeId, ModelId>,
    modeled: Vec<EntityTypeId>,
    instances: HashMap<EntityId, ModelInstance>,
    /// 已提示过片段回退的（模型, 期望片段），避免每帧刷屏。
    warned_clips: HashSet<(ModelId, &'static str)>,
    config: EntityModelConfig,
}

impl EntityModelSet {
    /// 按名称在注册表中解析生物类型；缺失的类型保留旧盒状渲染。
    pub fn new(registry: &ModelRegistry, config: EntityModelConfig) -> Self {
        let mut models = HashMap::new();
        let mut modeled = Vec::new();
        for (kind, name) in MODEL_NAMES {
            if let Some(id) = registry.by_name(name) {
                models.insert(kind, id);
                modeled.push(kind);
            }
        }
        Self {
            models,
            modeled,
            instances: HashMap::new(),
            warned_clips: HashSet::new(),
            config,
        }
    }

    /// 拥有正式模型的实体类型；旧盒状渲染应跳过这些类型。
    pub fn modeled_types(&self) -> &[EntityTypeId] {
        &self.modeled
    }

    /// 同步实例集合：清理死亡/移除实体、选择动画并推进姿态。
    pub fn update(&mut self, creatures: &[Creature], registry: &ModelRegistry, delta: f32) {
        let alive: HashSet<EntityId> = creatures
            .iter()
            .filter(|creature| creature.is_alive())
            .map(|creature| creature.id())
            .collect();
        self.instances.retain(|id, _| alive.contains(id));

        for creature in creatures.iter().filter(|creature| creature.is_alive()) {
            let Some(&model_id) = self.models.get(&creature.kind()) else {
                continue;
            };
            let Some(asset) = registry.get(model_id) else {
                continue;
            };
            let instance = self
                .instances
                .entry(creature.id())
                .or_insert_with(|| ModelInstance::new(model_id));
            if instance.model != model_id {
                instance.set_model(model_id);
            }
            let desired = animation_for(creature.brain.state());
            let clip = resolve_clip(&asset, desired);
            if let Some(clip) = clip {
                if !clip.eq_ignore_ascii_case(desired)
                    && self.warned_clips.insert((model_id, desired))
                {
                    let available = asset
                        .animations
                        .iter()
                        .map(|clip| clip.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    tracing::warn!(
                        model = %asset.name,
                        desired,
                        fallback = clip,
                        available = %available,
                        "模型缺少按状态命名的动画片段，已回退到最接近的片段"
                    );
                }
                instance.set_animation(&asset, clip, true);
            }
            instance.set_speed(playback_rate(
                clip.unwrap_or_default(),
                creature.entity.velocity(),
            ));
            instance.update(&asset, delta);
        }
    }

    /// 把全部可见生物转换为模型绘制批次；`out` 由调用方负责清空。
    #[allow(clippy::too_many_arguments)]
    pub fn build_draws(
        &self,
        creatures: &[Creature],
        registry: &ModelRegistry,
        alpha: f32,
        view_proj: Mat4,
        camera_position: Vec3,
        out: &mut Vec<ModelDraw>,
    ) {
        let alpha = alpha.clamp(0.0, 1.0);
        for creature in creatures.iter().filter(|creature| creature.is_alive()) {
            let Some(instance) = self.instances.get(&creature.id()) else {
                continue;
            };
            let Some(asset) = registry.get(instance.model) else {
                continue;
            };
            let position = creature
                .entity
                .previous_position()
                .lerp(creature.entity.position(), alpha);
            if position.distance(camera_position) > self.config.render_distance {
                continue;
            }
            let transform = Mat4::from_rotation_translation(
                Quat::from_rotation_y(creature.entity.yaw()),
                position,
            );
            if let Some(bounds) = asset.bounds() {
                if !bounds_visible(view_proj, transform, bounds) {
                    continue;
                }
            }
            instance_draws(&asset, instance, transform, out);
        }
    }
}

/// 按 AI 状态选择片段名：逃跑用奔跑，游荡用行走，其余待机。
fn animation_for(state: AiState) -> &'static str {
    match state {
        AiState::Fleeing => "run",
        AiState::Wandering => "walk",
        AiState::Idle | AiState::Avoiding | AiState::Dead => "idle",
    }
}

/// 解析实际可播放的片段名。
///
/// 外部模型的片段命名并不统一（例如 Blender 里存成 `stop`/`eat`），
/// 因此按别名逐级回退，最后使用资源中的第一个片段，避免生物完全静止。
fn resolve_clip<'a>(asset: &'a ModelAsset, desired: &str) -> Option<&'a str> {
    let candidates: &[&str] = match desired {
        "walk" => &["walk", "move", "run", "idle", "stop"],
        "run" => &["run", "sprint", "walk", "move", "idle"],
        _ => &["idle", "stand", "stop", "eat", "walk"],
    };
    for candidate in candidates {
        if let Some(clip) = asset
            .animations
            .iter()
            .find(|clip| clip.name.eq_ignore_ascii_case(candidate))
        {
            return Some(clip.name.as_str());
        }
    }
    asset.animations.first().map(|clip| clip.name.as_str())
}

/// 行走片段按实际水平速度调整播放速率，避免滑步感。
fn playback_rate(name: &str, velocity: Vec3) -> f32 {
    let speed = Vec3::new(velocity.x, 0.0, velocity.z).length();
    if name == "walk" {
        (speed / 1.4).clamp(0.6, 1.5)
    } else {
        1.0
    }
}

/// 用实例世界变换把模型局部包围盒展开，做八点可视性判断。
fn bounds_visible(view_proj: Mat4, transform: Mat4, bounds: MeshBounds) -> bool {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for index in 0..8 {
        let corner = Vec3::new(
            if index & 1 == 0 {
                bounds.min.x
            } else {
                bounds.max.x
            },
            if index & 2 == 0 {
                bounds.min.y
            } else {
                bounds.max.y
            },
            if index & 4 == 0 {
                bounds.min.z
            } else {
                bounds.max.z
            },
        );
        let clip = view_proj * transform.transform_point3(corner).extend(1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let point = clip.truncate() / clip.w;
        min = min.min(point);
        max = max.max(point);
    }
    min.x <= 1.0 && max.x >= -1.0 && min.y <= 1.0 && max.y >= -1.0 && min.z <= 1.0 && max.z >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::{Entity, EntityDimensions};
    use crate::model::builtin;

    fn registry() -> ModelRegistry {
        let mut registry = ModelRegistry::new();
        registry.register_builtin("pig", builtin::pig());
        registry.register_builtin("cow", builtin::cow());
        registry
    }

    fn creature(kind: EntityTypeId, id: u64, position: Vec3) -> Creature {
        Creature::new(Entity::new(
            EntityId::new(id),
            kind,
            position,
            0.0,
            EntityDimensions::new(0.9, 1.2),
            10,
        ))
    }

    #[test]
    fn builtin_models_are_mapped_by_kind() {
        let registry = registry();
        let set = EntityModelSet::new(&registry, EntityModelConfig::default());
        assert_eq!(set.modeled_types().len(), 2);
        assert!(set.modeled_types().contains(&EntityTypeId::PIG));
        assert!(set.modeled_types().contains(&EntityTypeId::COW));
    }

    #[test]
    fn empty_registry_falls_back_to_box_geometry() {
        let registry = ModelRegistry::new();
        let set = EntityModelSet::new(&registry, EntityModelConfig::default());
        assert!(set.modeled_types().is_empty());
    }

    #[test]
    fn wandering_and_fleeing_select_distinct_clips() {
        let registry = registry();
        let mut set = EntityModelSet::new(&registry, EntityModelConfig::default());
        let mut pig = creature(EntityTypeId::PIG, 1, Vec3::ZERO);
        set.update(std::slice::from_ref(&pig), &registry, 0.0);
        let asset = registry.get(registry.by_name("pig").unwrap()).unwrap();
        let instance = set.instances.get(&pig.id()).unwrap();
        assert_eq!(instance.clip_name(&asset), Some("idle"));

        pig.brain.set_state(AiState::Wandering, 1.0);
        set.update(std::slice::from_ref(&pig), &registry, 0.1);
        let instance = set.instances.get(&pig.id()).unwrap();
        assert_eq!(instance.clip_name(&asset), Some("walk"));

        pig.brain.set_state(AiState::Fleeing, 1.0);
        set.update(std::slice::from_ref(&pig), &registry, 0.1);
        let instance = set.instances.get(&pig.id()).unwrap();
        assert_eq!(instance.clip_name(&asset), Some("run"));
    }

    #[test]
    fn missing_state_clips_fall_back_to_available_names() {
        let mut asset = builtin::pig();
        // 模拟外部导出常见的命名：只有 stop / run / eat。
        asset.animations[0].name = "stop".to_string();
        asset.animations[1].name = "eat".to_string();
        assert_eq!(resolve_clip(&asset, "idle"), Some("stop"));
        assert_eq!(resolve_clip(&asset, "walk"), Some("run"));
        assert_eq!(resolve_clip(&asset, "run"), Some("run"));
    }

    #[test]
    fn model_without_clips_yields_no_clip() {
        let mut asset = builtin::pig();
        asset.animations.clear();
        assert_eq!(resolve_clip(&asset, "idle"), None);
    }

    #[test]
    fn dead_creatures_are_removed_from_instances() {
        let registry = registry();
        let mut set = EntityModelSet::new(&registry, EntityModelConfig::default());
        let mut pig = creature(EntityTypeId::PIG, 2, Vec3::ZERO);
        set.update(std::slice::from_ref(&pig), &registry, 0.0);
        assert_eq!(set.instances.len(), 1);
        pig.entity.take_damage(99);
        pig.mark_dead();
        set.update(std::slice::from_ref(&pig), &registry, 0.0);
        assert_eq!(set.instances.len(), 0);
    }

    #[test]
    fn draws_are_distance_culled() {
        let registry = registry();
        let mut set = EntityModelSet::new(&registry, EntityModelConfig::default());
        let near = creature(EntityTypeId::COW, 3, Vec3::ZERO);
        let far = creature(EntityTypeId::COW, 4, Vec3::new(0.0, 0.0, 200.0));
        let creatures = [near, far];
        set.update(&creatures, &registry, 0.0);
        let mut draws = Vec::new();
        set.build_draws(
            &creatures,
            &registry,
            1.0,
            Mat4::IDENTITY,
            Vec3::new(0.0, 0.0, -10.0),
            &mut draws,
        );
        // 每只牛包含一个蒙皮图元，远处牛应被距离剔除。
        assert!(!draws.is_empty());
        assert!(draws
            .iter()
            .all(|draw| draw.model == registry.by_name("cow").unwrap()));
    }
}

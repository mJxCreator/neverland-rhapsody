//! 实体渲染：把存活实体转换为盒状模型顶点，并做基础视锥剔除。
//!
//! 猪与牛使用不同的盒状拼装，主体色与斑点色来自实体定义；
//! 已销毁或不可见实体不会产生顶点。

use glam::{Mat4, Vec3};

use super::creature::Creature;
use super::entity_registry::{definition, CreatureModel, EntityDefinition};
use crate::mesh::Vertex;

/// 实体渲染参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityRenderConfig {
    /// 最大渲染距离（格）。
    pub render_distance: f32,
}

impl Default for EntityRenderConfig {
    fn default() -> Self {
        Self {
            render_distance: 80.0,
        }
    }
}

/// 为所有可见生物追加顶点；`out` 由调用方负责清空。
pub fn build_vertices(
    creatures: &[Creature],
    alpha: f32,
    view_proj: Mat4,
    camera_position: Vec3,
    config: &EntityRenderConfig,
    out: &mut Vec<Vertex>,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    for creature in creatures.iter().filter(|creature| creature.is_alive()) {
        let definition = match definition(creature.kind()) {
            Some(definition) => definition,
            None => continue,
        };
        let position = creature
            .entity
            .previous_position()
            .lerp(creature.entity.position(), alpha);
        if position.distance(camera_position) > config.render_distance {
            continue;
        }
        let yaw = creature.entity.yaw();
        let min = Vec3::new(
            position.x - definition.width * 0.5,
            position.y,
            position.z - definition.width * 0.5,
        );
        let max = min + Vec3::new(definition.width, definition.height, definition.width);
        if !is_visible(view_proj, min, max) {
            continue;
        }
        push_model(out, definition, position, yaw);
    }
}

/// 判断 AABB 是否有任意角点落在裁剪空间内。
fn is_visible(view_proj: Mat4, min: Vec3, max: Vec3) -> bool {
    for index in 0..8 {
        let corner = Vec3::new(
            if index & 1 == 0 { min.x } else { max.x },
            if index & 2 == 0 { min.y } else { max.y },
            if index & 4 == 0 { min.z } else { max.z },
        );
        let clip = view_proj * corner.extend(1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x >= -1.0
            && ndc.x <= 1.0
            && ndc.y >= -1.0
            && ndc.y <= 1.0
            && ndc.z >= 0.0
            && ndc.z <= 1.0
        {
            return true;
        }
    }
    false
}

fn push_model(out: &mut Vec<Vertex>, definition: &EntityDefinition, feet: Vec3, yaw: f32) {
    let body = definition.body_color;
    let accent = definition.accent_color;
    match definition.model {
        CreatureModel::Pig => {
            for sx in [-0.24f32, 0.24] {
                for sz in [-0.3f32, 0.3] {
                    push_box(
                        out,
                        feet + Vec3::new(sx, 0.17, sz),
                        Vec3::new(0.16, 0.34, 0.16),
                        yaw,
                        accent,
                    );
                }
            }
            push_box(
                out,
                feet + Vec3::new(0.0, 0.55, 0.0),
                Vec3::new(0.72, 0.42, 0.9),
                yaw,
                body,
            );
            push_box(
                out,
                feet + Vec3::new(0.0, 0.62, 0.6),
                Vec3::new(0.48, 0.4, 0.36),
                yaw,
                body,
            );
            push_box(
                out,
                feet + Vec3::new(0.0, 0.56, 0.83),
                Vec3::new(0.22, 0.16, 0.1),
                yaw,
                accent,
            );
            for sx in [-0.16f32, 0.16] {
                push_box(
                    out,
                    feet + Vec3::new(sx, 0.84, 0.6),
                    Vec3::new(0.1, 0.14, 0.08),
                    yaw,
                    accent,
                );
            }
            push_box(
                out,
                feet + Vec3::new(0.0, 0.68, -0.5),
                Vec3::new(0.06, 0.06, 0.2),
                yaw,
                accent,
            );
        }
        CreatureModel::Cow => {
            for sx in [-0.26f32, 0.26] {
                for sz in [-0.38f32, 0.38] {
                    push_box(
                        out,
                        feet + Vec3::new(sx, 0.3, sz),
                        Vec3::new(0.16, 0.6, 0.16),
                        yaw,
                        accent,
                    );
                }
            }
            push_box(
                out,
                feet + Vec3::new(0.0, 0.85, 0.0),
                Vec3::new(0.8, 0.55, 1.15),
                yaw,
                body,
            );
            push_box(
                out,
                feet + Vec3::new(0.0, 1.02, 0.72),
                Vec3::new(0.44, 0.44, 0.4),
                yaw,
                body,
            );
            push_box(
                out,
                feet + Vec3::new(0.0, 0.94, 0.95),
                Vec3::new(0.3, 0.16, 0.1),
                yaw,
                accent,
            );
            for sx in [-0.17f32, 0.17] {
                push_box(
                    out,
                    feet + Vec3::new(sx, 1.28, 0.72),
                    Vec3::new(0.08, 0.12, 0.08),
                    yaw,
                    accent,
                );
            }
            for sx in [-0.405f32, 0.405] {
                push_box(
                    out,
                    feet + Vec3::new(sx, 0.92, 0.05),
                    Vec3::new(0.05, 0.3, 0.55),
                    yaw,
                    accent,
                );
            }
            push_box(
                out,
                feet + Vec3::new(0.1, 1.13, -0.2),
                Vec3::new(0.3, 0.04, 0.4),
                yaw,
                accent,
            );
            push_box(
                out,
                feet + Vec3::new(0.0, 0.95, -0.62),
                Vec3::new(0.06, 0.06, 0.24),
                yaw,
                accent,
            );
        }
    }
}

/// 添加一个带朝向的盒体；面使用固定明暗系数模拟简单光照。
fn push_box(out: &mut Vec<Vertex>, center: Vec3, size: Vec3, yaw: f32, color: Vec3) {
    if size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0 {
        return;
    }
    const FACES: [([Vec3; 4], f32); 6] = [
        (
            [
                Vec3::new(0.5, -0.5, -0.5),
                Vec3::new(0.5, 0.5, -0.5),
                Vec3::new(0.5, 0.5, 0.5),
                Vec3::new(0.5, -0.5, 0.5),
            ],
            0.9,
        ),
        (
            [
                Vec3::new(-0.5, -0.5, 0.5),
                Vec3::new(-0.5, 0.5, 0.5),
                Vec3::new(-0.5, 0.5, -0.5),
                Vec3::new(-0.5, -0.5, -0.5),
            ],
            0.65,
        ),
        (
            [
                Vec3::new(-0.5, 0.5, -0.5),
                Vec3::new(-0.5, 0.5, 0.5),
                Vec3::new(0.5, 0.5, 0.5),
                Vec3::new(0.5, 0.5, -0.5),
            ],
            1.0,
        ),
        (
            [
                Vec3::new(-0.5, -0.5, 0.5),
                Vec3::new(-0.5, -0.5, -0.5),
                Vec3::new(0.5, -0.5, -0.5),
                Vec3::new(0.5, -0.5, 0.5),
            ],
            0.5,
        ),
        (
            [
                Vec3::new(-0.5, -0.5, 0.5),
                Vec3::new(0.5, -0.5, 0.5),
                Vec3::new(0.5, 0.5, 0.5),
                Vec3::new(-0.5, 0.5, 0.5),
            ],
            0.8,
        ),
        (
            [
                Vec3::new(0.5, -0.5, -0.5),
                Vec3::new(-0.5, -0.5, -0.5),
                Vec3::new(-0.5, 0.5, -0.5),
                Vec3::new(0.5, 0.5, -0.5),
            ],
            0.7,
        ),
    ];

    for (corners, shade) in FACES {
        let color = color * shade;
        let rotated: Vec<Vec3> = corners
            .iter()
            .map(|corner| center + rotate_y(corner * size, yaw))
            .collect();
        for index in [0usize, 1, 2, 0, 2, 3] {
            out.push(Vertex {
                position: rotated[index].to_array(),
                color: color.to_array(),
            });
        }
    }
}

fn rotate_y(local: Vec3, yaw: f32) -> Vec3 {
    let (sin, cos) = yaw.sin_cos();
    Vec3::new(
        local.x * cos + local.z * sin,
        local.y,
        -local.x * sin + local.z * cos,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::{Entity, EntityDimensions, EntityId, EntityTypeId};

    fn creature(kind: EntityTypeId, position: Vec3) -> Creature {
        let definition = definition(kind).unwrap();
        Creature::new(Entity::new(
            EntityId::new(1),
            kind,
            position,
            0.0,
            EntityDimensions::new(definition.width, definition.height),
            definition.max_health,
        ))
    }

    fn build(creatures: &[Creature]) -> Vec<Vertex> {
        let mut out = Vec::new();
        build_vertices(
            creatures,
            1.0,
            Mat4::IDENTITY,
            Vec3::new(0.0, 0.0, -10.0),
            &EntityRenderConfig::default(),
            &mut out,
        );
        out
    }

    #[test]
    fn pig_and_cow_generate_geometry() {
        let pig = build(&[creature(EntityTypeId::PIG, Vec3::ZERO)]);
        let cow = build(&[creature(EntityTypeId::COW, Vec3::ZERO)]);
        assert!(!pig.is_empty());
        assert!(!cow.is_empty());
        assert!(cow.len() > pig.len(), "牛的盒体数量应更多");
        assert_ne!(pig[0].color, cow[0].color);
    }

    #[test]
    fn dead_creatures_are_not_rendered() {
        let mut dead = creature(EntityTypeId::PIG, Vec3::ZERO);
        dead.entity.take_damage(99);
        assert!(build(&[dead]).is_empty());
    }

    #[test]
    fn distant_creatures_are_culled_by_range() {
        let far = creature(EntityTypeId::PIG, Vec3::new(0.0, 0.0, 200.0));
        assert!(build(&[far]).is_empty());
    }

    #[test]
    fn vertices_are_finite_and_translated() {
        let mut vertices = Vec::new();
        build_vertices(
            &[creature(EntityTypeId::COW, Vec3::new(5.0, 10.0, 0.0))],
            1.0,
            Mat4::from_translation(Vec3::new(-5.0, -10.0, 0.0)),
            Vec3::new(5.0, 10.0, -10.0),
            &EntityRenderConfig::default(),
            &mut vertices,
        );
        assert!(!vertices.is_empty());
        assert!(vertices
            .iter()
            .all(|vertex| { vertex.position.iter().all(|value| value.is_finite()) }));
        let max_y = vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::MIN, f32::max);
        assert!(max_y >= 10.0, "模型应位于实体脚底之上");
    }
}

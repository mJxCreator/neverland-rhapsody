//! AABB 与单位体素方块的分轴碰撞检测与位置修正。
//!
//! 移动按速度与时间步长细分为若干子步，每个子步依次处理 X、Z、Y 轴，
//! 保证高速移动也不会穿过一格厚的方块。

use glam::{IVec3, Vec3};

/// 判定贴合时使用的浮点容差，避免恰好接触被当成重叠。
pub const AABB_EPSILON: f32 = 1e-4;
/// 单个子步允许的最大位移（格），小于方块厚度以避免隧穿。
const MAX_SUBSTEP: f32 = 0.25;
/// 单帧允许的最大子步数，防止异常速度导致长时间循环。
const MAX_SUBSTEPS: u32 = 1024;
/// 单轴一次子步内最多修正的次数，防止异常数据导致死循环。
const MAX_RESOLVE_ITERATIONS: usize = 4;

/// 轴对齐包围盒。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// 以底部中心与宽高构造包围盒。
    pub fn from_feet(position: Vec3, width: f32, height: f32) -> Self {
        let half = width * 0.5;
        Self {
            min: Vec3::new(position.x - half, position.y, position.z - half),
            max: Vec3::new(position.x + half, position.y + height, position.z + half),
        }
    }

    /// 是否与一个单位方块真实重叠；恰好贴合或分离都返回 `false`。
    pub fn overlaps_block(&self, block: IVec3) -> bool {
        let min = block.as_vec3();
        let max = min + Vec3::ONE;
        self.min.x < max.x - AABB_EPSILON
            && self.max.x > min.x + AABB_EPSILON
            && self.min.y < max.y - AABB_EPSILON
            && self.max.y > min.y + AABB_EPSILON
            && self.min.z < max.z - AABB_EPSILON
            && self.max.z > min.z + AABB_EPSILON
    }

    /// 可能重叠的方块坐标范围（闭区间），用于避免遍历整个世界。
    fn voxel_bounds(&self) -> (IVec3, IVec3) {
        let lo = IVec3::new(
            (self.min.x + AABB_EPSILON).floor() as i32,
            (self.min.y + AABB_EPSILON).floor() as i32,
            (self.min.z + AABB_EPSILON).floor() as i32,
        );
        let hi = IVec3::new(
            (self.max.x - AABB_EPSILON).floor() as i32,
            (self.max.y - AABB_EPSILON).floor() as i32,
            (self.max.z - AABB_EPSILON).floor() as i32,
        );
        (lo, hi)
    }
}

/// 一次移动中发生的碰撞方向。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollisionFlags {
    /// 向下移动被阻挡，即站在地面或方块上。
    pub ground: bool,
    /// 向上移动被阻挡，即撞到天花板。
    pub ceiling: bool,
    /// 水平移动被阻挡，即撞到墙壁。
    pub horizontal: bool,
}

/// 按速度推进一个时间步，并在碰撞时修正位置、清除对应方向的速度。
///
/// `is_solid` 为方块实体判定，未加载区块应由调用方视为实体。
pub fn move_box(
    position: &mut Vec3,
    velocity: &mut Vec3,
    width: f32,
    height: f32,
    delta_secs: f32,
    is_solid: &dyn Fn(i32, i32, i32) -> bool,
) -> CollisionFlags {
    let mut flags = CollisionFlags::default();
    if delta_secs <= 0.0 || !velocity.is_finite() {
        return flags;
    }

    // 以本步最大速度为界拆分子步，使单步位移不超过 MAX_SUBSTEP。
    let max_speed = velocity.x.abs().max(velocity.y.abs()).max(velocity.z.abs());
    let steps = ((max_speed * delta_secs / MAX_SUBSTEP).ceil() as u32).clamp(1, MAX_SUBSTEPS);
    let sub_delta = delta_secs / steps as f32;

    for _ in 0..steps {
        if resolve_axis(
            position,
            velocity,
            Axis::X,
            sub_delta,
            width,
            height,
            is_solid,
        )
        .is_some()
        {
            flags.horizontal = true;
        }
        if resolve_axis(
            position,
            velocity,
            Axis::Z,
            sub_delta,
            width,
            height,
            is_solid,
        )
        .is_some()
        {
            flags.horizontal = true;
        }
        if let Some(hit) = resolve_axis(
            position,
            velocity,
            Axis::Y,
            sub_delta,
            width,
            height,
            is_solid,
        ) {
            if hit.positive {
                flags.ceiling = true;
            } else {
                flags.ground = true;
            }
        }
    }

    flags
}

/// 三个坐标轴。
#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }
}

/// 单轴碰撞命中记录。
struct AxisHit {
    /// 位移方向是否为正方向。
    positive: bool,
}

/// 推进单轴并修正重叠；返回本次是否发生碰撞及移动方向。
fn resolve_axis(
    position: &mut Vec3,
    velocity: &mut Vec3,
    axis: Axis,
    delta_secs: f32,
    width: f32,
    height: f32,
    is_solid: &dyn Fn(i32, i32, i32) -> bool,
) -> Option<AxisHit> {
    let index = axis.index();
    let velocity_value = component(velocity, index);
    let movement = velocity_value * delta_secs;
    if movement == 0.0 || !movement.is_finite() {
        return None;
    }

    add_component(position, index, movement);
    let positive = movement > 0.0;
    let mut collided = false;

    for _ in 0..MAX_RESOLVE_ITERATIONS {
        let aabb = Aabb::from_feet(*position, width, height);
        let (lo, hi) = aabb.voxel_bounds();
        let mut boundary: Option<f32> = None;

        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let block = IVec3::new(x, y, z);
                    if !is_solid(x, y, z) || !aabb.overlaps_block(block) {
                        continue;
                    }
                    let block_min = component(&block.as_vec3(), index);
                    let face = if positive { block_min } else { block_min + 1.0 };
                    boundary = Some(match boundary {
                        Some(current) if positive => current.min(face),
                        Some(current) => current.max(face),
                        None => face,
                    });
                }
            }
        }

        let Some(boundary) = boundary else {
            break;
        };

        match axis {
            Axis::X => {
                let half = width * 0.5;
                position.x = if positive {
                    boundary - half - AABB_EPSILON
                } else {
                    boundary + half + AABB_EPSILON
                };
            }
            Axis::Y => {
                position.y = if positive {
                    boundary - height - AABB_EPSILON
                } else {
                    boundary + AABB_EPSILON
                };
            }
            Axis::Z => {
                let half = width * 0.5;
                position.z = if positive {
                    boundary - half - AABB_EPSILON
                } else {
                    boundary + half + AABB_EPSILON
                };
            }
        }

        set_component(velocity, index, 0.0);
        collided = true;
    }

    if collided {
        Some(AxisHit { positive })
    } else {
        None
    }
}

fn component(vector: &Vec3, index: usize) -> f32 {
    match index {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

fn add_component(vector: &mut Vec3, index: usize, value: f32) {
    match index {
        0 => vector.x += value,
        1 => vector.y += value,
        _ => vector.z += value,
    }
}

fn set_component(vector: &mut Vec3, index: usize, value: f32) {
    match index {
        0 => vector.x = value,
        1 => vector.y = value,
        _ => vector.z = value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: f32 = 0.6;
    const HEIGHT: f32 = 1.8;

    fn floor_and_wall(x: i32, y: i32, _z: i32) -> bool {
        y <= 10 || x >= 3
    }

    #[test]
    fn touching_a_surface_is_not_an_overlap() {
        let aabb = Aabb::from_feet(Vec3::new(5.0, 11.0, 5.0), WIDTH, HEIGHT);
        assert!(!aabb.overlaps_block(IVec3::new(5, 10, 5)));
        assert!(aabb.overlaps_block(IVec3::new(5, 11, 5)));
    }

    #[test]
    fn horizontal_movement_stops_at_a_wall() {
        let mut position = Vec3::new(0.0, 11.0, 0.0);
        let mut velocity = Vec3::new(3.0, 0.0, 0.0);
        let flags = move_box(
            &mut position,
            &mut velocity,
            WIDTH,
            HEIGHT,
            1.0,
            &floor_and_wall,
        );

        assert!(flags.horizontal);
        assert!((position.x - (3.0 - WIDTH * 0.5 - AABB_EPSILON)).abs() < 1e-4);
        assert_eq!(velocity.x, 0.0);
        assert!(position.x > 2.0);
    }

    #[test]
    fn downward_movement_lands_on_the_floor() {
        let mut position = Vec3::new(1.0, 15.0, 1.0);
        let mut velocity = Vec3::new(0.0, -10.0, 0.0);
        let flags = move_box(
            &mut position,
            &mut velocity,
            WIDTH,
            HEIGHT,
            1.0,
            &floor_and_wall,
        );

        assert!(flags.ground);
        assert!(!flags.ceiling);
        assert!((position.y - (11.0 + AABB_EPSILON)).abs() < 1e-4);
        assert_eq!(velocity.y, 0.0);
    }

    #[test]
    fn upward_movement_hits_the_ceiling() {
        let mut position = Vec3::new(1.0, 11.0, 1.0);
        let mut velocity = Vec3::new(0.0, 5.0, 0.0);
        let flags = move_box(
            &mut position,
            &mut velocity,
            WIDTH,
            HEIGHT,
            0.5,
            &|x, y, _z| y <= 10 || ((13..14).contains(&y) && x < 2),
        );

        assert!(flags.ceiling);
        assert!(!flags.ground);
        assert!((position.y - (13.0 - HEIGHT - AABB_EPSILON)).abs() < 1e-4);
        assert_eq!(velocity.y, 0.0);
    }

    #[test]
    fn substepping_prevents_tunneling_through_a_thin_wall() {
        let mut position = Vec3::new(0.0, 11.0, 0.0);
        let mut velocity = Vec3::new(20.0, 0.0, 0.0);
        let flags = move_box(
            &mut position,
            &mut velocity,
            WIDTH,
            HEIGHT,
            1.0,
            &floor_and_wall,
        );

        assert!(flags.horizontal);
        assert!(position.x < 3.0);
        assert_eq!(velocity.x, 0.0);
    }

    #[test]
    fn zero_motion_changes_nothing() {
        let mut position = Vec3::new(1.0, 11.0, 1.0);
        let mut velocity = Vec3::ZERO;
        let flags = move_box(
            &mut position,
            &mut velocity,
            WIDTH,
            HEIGHT,
            0.0,
            &floor_and_wall,
        );
        assert_eq!(flags, CollisionFlags::default());
        assert_eq!(position, Vec3::new(1.0, 11.0, 1.0));
    }
}

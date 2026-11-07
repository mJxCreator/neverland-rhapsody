//! 体素射线检测：沿视线逐格步进，返回第一个命中的实体方块与命中面。

use glam::{IVec3, Vec3};

use crate::world::{Face, World};

/// 一次射线命中的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RayHit {
    /// 被选中方块的世界坐标。
    pub block: IVec3,
    /// 射线进入该方块时穿过的面，即面向观察者的那一面。
    pub face: Face,
}

/// 从 `origin` 沿 `direction` 步进最多 `max_distance`，返回第一个实体方块。
///
/// 采用 Amanatides–Woo 体素遍历，每次只跨越最近的网格边界，因此不会漏掉擦角方块。
pub fn raycast(world: &World, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RayHit> {
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO || max_distance <= 0.0 || !max_distance.is_finite() {
        return None;
    }

    let mut voxel = [
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    ];
    let (step, mut next, delta) = axis_steps(origin, direction, voxel)?;

    let enter_faces = [Face::NegX, Face::NegY, Face::NegZ];
    let leave_faces = [Face::PosX, Face::PosY, Face::PosZ];

    loop {
        // 选出最近的一条网格边界。
        let mut axis = 0;
        if next[1] < next[axis] {
            axis = 1;
        }
        if next[2] < next[axis] {
            axis = 2;
        }

        if next[axis] > max_distance {
            return None;
        }

        voxel[axis] += step[axis];
        next[axis] += delta[axis];

        let face = if step[axis] > 0 {
            enter_faces[axis]
        } else {
            leave_faces[axis]
        };

        if world.block_at(voxel[0], voxel[1], voxel[2]).is_solid() {
            return Some(RayHit {
                block: IVec3::new(voxel[0], voxel[1], voxel[2]),
                face,
            });
        }
    }
}

/// 计算三个轴上的步进方向、到下一条边界的距离，以及跨过一整格的距离。
///
/// 方向分量为 0 的轴上步进为 0、距离为无穷大，遍历永远不会选中该轴。
fn axis_steps(
    origin: Vec3,
    direction: Vec3,
    voxel: [i32; 3],
) -> Option<([i32; 3], [f32; 3], [f32; 3])> {
    let mut step = [0; 3];
    let mut next = [f32::INFINITY; 3];
    let mut delta = [f32::INFINITY; 3];
    let components = [
        (origin.x, direction.x, voxel[0]),
        (origin.y, direction.y, voxel[1]),
        (origin.z, direction.z, voxel[2]),
    ];

    for (axis, (position, direction, grid)) in components.into_iter().enumerate() {
        if direction == 0.0 {
            continue;
        }
        let boundary = if direction > 0.0 {
            (grid + 1) as f32
        } else {
            grid as f32
        };
        step[axis] = if direction > 0.0 { 1 } else { -1 };
        next[axis] = (boundary - position) / direction;
        delta[axis] = direction.abs().recip();
    }

    // 三个方向分量都为 0 时没有可行进方向，调用方应提前用 normalize 过滤。
    if step == [0; 3] {
        return None;
    }
    Some((step, next, delta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    const MAX_DISTANCE: f32 = 200.0;

    #[test]
    fn downward_ray_hits_the_surface_top_face() {
        let world = World::default();
        let (x, z) = (10, 10);
        let height = world.height_at(x, z);
        let origin = Vec3::new(x as f32 + 0.5, 40.0, z as f32 + 0.5);

        let hit = raycast(&world, origin, Vec3::NEG_Y, MAX_DISTANCE).expect("应命中地表");

        assert_eq!(hit.block, IVec3::new(x, height - 1, z));
        assert_eq!(hit.face, Face::PosY);
    }

    #[test]
    fn upward_ray_into_the_sky_misses() {
        let world = World::default();
        let origin = Vec3::new(10.5, 40.0, 10.5);
        assert_eq!(raycast(&world, origin, Vec3::Y, MAX_DISTANCE), None);
    }

    #[test]
    fn ray_stops_at_max_distance() {
        let world = World::default();
        let origin = Vec3::new(10.5, 40.0, 10.5);
        // 地表远在 1 格外，短距离射线不应命中。
        assert_eq!(raycast(&world, origin, Vec3::NEG_Y, 1.0), None);
    }

    #[test]
    fn zero_direction_and_invalid_distance_miss() {
        let world = World::default();
        let origin = Vec3::new(10.5, 40.0, 10.5);
        assert_eq!(raycast(&world, origin, Vec3::ZERO, MAX_DISTANCE), None);
        assert_eq!(raycast(&world, origin, Vec3::NEG_Y, 0.0), None);
    }

    #[test]
    fn ray_from_just_above_the_surface_hits_it() {
        let world = World::default();
        let (x, z) = (10, 10);
        let height = world.height_at(x, z);
        // 从地表上方 2 格处向下射入，进入面必然是顶面。
        let origin = Vec3::new(x as f32 + 0.5, height as f32 + 2.0, z as f32 + 0.5);
        let hit = raycast(&world, origin, Vec3::new(0.0, -3.0, 0.0), 5.0).expect("应命中地表");
        assert_eq!(hit.block, IVec3::new(x, height - 1, z));
        assert_eq!(hit.face, Face::PosY);
    }
}

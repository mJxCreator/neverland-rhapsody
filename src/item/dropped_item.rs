//! 世界掉落物：独立的运行时实体，负责重力、方块碰撞、合并、生命周期与存档快照。
//!
//! 掉落物按当前世界位置归属到区块；区块卸载前由上层取快照写入存档，
//! 区块重新加载时再从存档恢复，避免额外维护全局重复数据。

use std::collections::HashSet;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use super::item::{ItemId, ItemStack};
use super::item_registry;
use crate::mesh::Vertex;
use crate::world::{ChunkPos, World};

/// 掉落物行为参数；所有时序与半径集中在这里，避免散落在多个模块。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DroppedItemConfig {
    /// 重力加速度（格/秒²）。
    pub gravity: f32,
    /// 自动拾取半径（格）。
    pub pickup_radius: f32,
    /// 生成后不可拾取的时间（秒）。
    pub pickup_delay: f32,
    /// 生命周期（秒），到期后消失。
    pub lifetime: f32,
    /// 同类掉落物合并距离（格）。
    pub merge_radius: f32,
    /// 渲染与碰撞使用的半边长。
    pub half_size: f32,
    /// 速度上限，避免异常数值导致穿透。
    pub max_velocity: f32,
}

impl Default for DroppedItemConfig {
    fn default() -> Self {
        Self {
            gravity: 9.0,
            pickup_radius: 1.5,
            pickup_delay: 0.5,
            lifetime: 300.0,
            merge_radius: 0.9,
            half_size: 0.15,
            max_velocity: 8.0,
        }
    }
}

/// 存档中的一条掉落物记录。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DroppedItemRecord {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub item: u16,
    pub count: u32,
}

/// 世界中的一个掉落物实体。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DroppedItem {
    pub(crate) id: u64,
    pub(crate) stack: ItemStack,
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) age: f32,
}

impl DroppedItem {
    pub fn stack(&self) -> ItemStack {
        self.stack
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// 是否已过拾取延迟。
    pub fn is_pickable(&self, config: &DroppedItemConfig) -> bool {
        self.age >= config.pickup_delay
    }
}

/// 掉落物运行时容器。
#[derive(Debug, Default)]
pub struct DroppedItemStore {
    items: Vec<DroppedItem>,
    next_id: u64,
    /// 内容发生变化、需要写回存档的区块。
    dirty_chunks: HashSet<ChunkPos>,
}

impl DroppedItemStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前存活掉落物数量。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn items(&self) -> &[DroppedItem] {
        &self.items
    }

    /// 供拾取逻辑原地修改掉落物；调用方负责维持物品与数量合法性。
    pub fn items_mut(&mut self) -> &mut Vec<DroppedItem> {
        &mut self.items
    }

    /// 标记区块内容需要写回存档。
    pub fn mark_dirty_chunks(&mut self, chunks: &[ChunkPos]) {
        self.dirty_chunks.extend(chunks.iter().copied());
    }

    /// 生成一个掉落物；速度会被限制在上限内。
    pub fn spawn(
        &mut self,
        stack: ItemStack,
        position: Vec3,
        velocity: Vec3,
        config: &DroppedItemConfig,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let velocity = if velocity.is_finite() {
            velocity.clamp_length_max(config.max_velocity)
        } else {
            Vec3::ZERO
        };
        let position = if position.is_finite() {
            position
        } else {
            Vec3::ZERO
        };

        self.items.push(DroppedItem {
            id,
            stack,
            position,
            velocity,
            age: 0.0,
        });
        self.dirty_chunks.insert(chunk_of(position));
        id
    }

    /// 推进一帧：重力、方块碰撞、生命周期与同类合并。
    pub fn update(&mut self, delta_secs: f32, world: &World, config: &DroppedItemConfig) {
        let delta = if delta_secs.is_finite() {
            delta_secs.clamp(0.0, 0.25)
        } else {
            0.0
        };
        if delta == 0.0 {
            return;
        }

        let mut dirty = HashSet::new();
        for item in &mut self.items {
            let before = chunk_of(item.position);
            item.age += delta;
            item.velocity.y -= config.gravity * delta;
            item.velocity = item.velocity.clamp_length_max(config.max_velocity);
            Self::step(item, delta, world, config);

            let after = chunk_of(item.position);
            if before != after {
                dirty.insert(before);
                dirty.insert(after);
            }
        }

        // 过期或数值异常的掉落物直接移除，并同步其所属区块。
        self.items.retain(|item| {
            let keep = item.age < config.lifetime
                && item.position.is_finite()
                && item.velocity.is_finite();
            if !keep {
                dirty.insert(chunk_of(item.position));
            }
            keep
        });

        Self::merge_nearby(&mut self.items, config.merge_radius, &mut dirty);
        self.dirty_chunks.extend(dirty);
    }

    /// 按子步推进单个掉落物，降低高速下穿透地面的风险。
    fn step(item: &mut DroppedItem, delta: f32, world: &World, config: &DroppedItemConfig) {
        let travel = item.velocity.length() * delta;
        let steps = ((travel / 0.2).ceil() as usize).clamp(1, 8);
        let step_delta = delta / steps as f32;
        for _ in 0..steps {
            let anchor = item.position;
            resolve_axis(
                &mut item.position.x,
                &mut item.velocity.x,
                step_delta,
                Axis::X,
                anchor,
                world,
                config,
            );
            let anchor = item.position;
            resolve_axis(
                &mut item.position.y,
                &mut item.velocity.y,
                step_delta,
                Axis::Y,
                anchor,
                world,
                config,
            );
            let anchor = item.position;
            resolve_axis(
                &mut item.position.z,
                &mut item.velocity.z,
                step_delta,
                Axis::Z,
                anchor,
                world,
                config,
            );
        }
    }

    /// 合并相距很近的同类掉落物；超过堆叠上限的部分保留在原实体。
    fn merge_nearby(items: &mut Vec<DroppedItem>, radius: f32, dirty: &mut HashSet<ChunkPos>) {
        let radius_sq = radius * radius;
        let mut first = 0;
        while first < items.len() {
            let mut second = first + 1;
            while second < items.len() {
                let same_item = items[first].stack.item() == items[second].stack.item();
                let near = items[first]
                    .position
                    .distance_squared(items[second].position)
                    <= radius_sq;
                if !(same_item && near) {
                    second += 1;
                    continue;
                }

                dirty.insert(chunk_of(items[first].position));
                dirty.insert(chunk_of(items[second].position));

                let target = items[first].stack;
                let moved = target.room().min(items[second].stack.count());
                if moved == 0 {
                    second += 1;
                    continue;
                }
                items[first].stack = target
                    .with_count(target.count() + moved)
                    .expect("合并数量合法");

                let source = items[second].stack;
                if moved == source.count() {
                    items.remove(second);
                    continue;
                }
                items[second].stack = source
                    .with_count(source.count() - moved)
                    .expect("剩余数量应在堆叠上限内");
                second += 1;
            }
            first += 1;
        }
    }

    /// 取出并清空需要写回存档的区块列表。
    pub fn take_dirty_chunks(&mut self) -> Vec<ChunkPos> {
        let mut chunks: Vec<ChunkPos> = self.dirty_chunks.drain().collect();
        chunks.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        chunks
    }

    /// 指定区块当前全部掉落物的存档快照。
    pub fn snapshot_chunk(&self, pos: ChunkPos) -> Vec<DroppedItemRecord> {
        self.items
            .iter()
            .filter(|item| chunk_of(item.position) == pos)
            .map(|item| DroppedItemRecord {
                x: item.position.x,
                y: item.position.y,
                z: item.position.z,
                item: item.stack.item().raw(),
                count: item.stack.count(),
            })
            .collect()
    }

    /// 移除指定区块的掉落物并返回其存档快照。
    pub fn remove_chunk(&mut self, pos: ChunkPos) -> Vec<DroppedItemRecord> {
        let records = self.snapshot_chunk(pos);
        self.items.retain(|item| chunk_of(item.position) != pos);
        self.dirty_chunks.remove(&pos);
        records
    }

    /// 从存档恢复指定区块的掉落物；该区块已有运行时掉落物时跳过，避免重复生成。
    pub fn restore_chunk(&mut self, pos: ChunkPos, records: &[DroppedItemRecord]) -> usize {
        if self.items.iter().any(|item| chunk_of(item.position) == pos) {
            return 0;
        }

        let mut restored = 0;
        for record in records {
            let Some(item) = ItemId::from_raw(record.item) else {
                continue;
            };
            let Some(stack) = ItemStack::new(item, record.count) else {
                continue;
            };
            let position = Vec3::new(record.x, record.y, record.z);
            if !position.is_finite() {
                continue;
            }
            self.items.push(DroppedItem {
                id: self.next_id,
                stack,
                position,
                velocity: Vec3::ZERO,
                age: 0.0,
            });
            self.next_id = self.next_id.wrapping_add(1);
            restored += 1;
        }
        restored
    }

    /// 生成掉落物小立方体顶点；带轻微旋转与上下浮动。
    pub fn build_vertices(&self, out: &mut Vec<Vertex>) {
        out.clear();
        out.reserve(self.items.len() * DROPPED_VERTICES_PER_ITEM);
        for item in self.items() {
            let color = item_registry::icon_color(item.stack.item());
            push_item_cube(out, item, color);
        }
    }
}

/// 以当前坐标归属区块。
pub fn chunk_of(position: Vec3) -> ChunkPos {
    ChunkPos::from_world(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

/// 掉落物渲染的半边长。
const DROP_RENDER_SIZE: f32 = 0.3;
/// 每个掉落物的顶点数：6 个面 × 2 个三角形 × 3 个顶点。
const DROPPED_VERTICES_PER_ITEM: usize = 36;

/// 立方体 8 个角点，坐标范围 -0.5..0.5。
const CUBE_CORNERS: [[f32; 3]; 8] = [
    [-0.5, -0.5, -0.5],
    [0.5, -0.5, -0.5],
    [0.5, -0.5, 0.5],
    [-0.5, -0.5, 0.5],
    [-0.5, 0.5, -0.5],
    [0.5, 0.5, -0.5],
    [0.5, 0.5, 0.5],
    [-0.5, 0.5, 0.5],
];

/// 六个面的角点顺序（外侧逆时针）与基础明暗。
const CUBE_FACES: [([usize; 4], f32); 6] = [
    ([2, 1, 5, 6], 0.80),
    ([0, 3, 7, 4], 0.80),
    ([7, 6, 5, 4], 1.00),
    ([0, 1, 2, 3], 0.50),
    ([3, 2, 6, 7], 0.68),
    ([1, 0, 4, 5], 0.68),
];

fn push_item_cube(out: &mut Vec<Vertex>, item: &DroppedItem, color: Vec3) {
    let (sin, cos) = (item.age * 1.6).sin_cos();
    let bob = (item.age * 2.0).sin() * 0.06;
    let center = item.position + Vec3::Y * (0.12 + bob);
    let scale = DROP_RENDER_SIZE;

    let corner = |index: usize| {
        let [x, y, z] = CUBE_CORNERS[index];
        let x = x * scale;
        let z = z * scale;
        center + Vec3::new(x * cos - z * sin, y * scale, x * sin + z * cos)
    };

    for (face, shade) in CUBE_FACES {
        let points = [
            corner(face[0]),
            corner(face[1]),
            corner(face[2]),
            corner(face[3]),
        ];
        let shaded = (color * shade).to_array();
        for index in [0, 1, 2, 0, 2, 3] {
            out.push(Vertex {
                position: points[index].to_array(),
                color: shaded,
            });
        }
    }
}

/// 沿单个轴推进并处理与实体方块的碰撞；碰撞时把该轴速度清零。
fn resolve_axis(
    position: &mut f32,
    velocity: &mut f32,
    delta: f32,
    axis: Axis,
    anchor: Vec3,
    world: &World,
    config: &DroppedItemConfig,
) {
    let movement = *velocity * delta;
    if movement == 0.0 {
        return;
    }

    if axis == Axis::Y {
        let half = config.half_size;
        let next = *position + movement;
        let edge = if movement < 0.0 {
            next - half
        } else {
            next + half
        };
        let cell = edge.floor();
        let probe = Vec3::new(anchor.x, cell + 0.5, anchor.z);
        if is_solid_at(world, probe) {
            *position = if movement < 0.0 {
                cell + 1.0 + half
            } else {
                cell - half
            };
            *velocity = 0.0;
        } else {
            *position = next;
        }
        return;
    }

    let next = *position + movement;
    let probe = match axis {
        Axis::X => Vec3::new(
            next + movement.signum() * config.half_size,
            anchor.y,
            anchor.z,
        ),
        Axis::Z => Vec3::new(
            anchor.x,
            anchor.y,
            next + movement.signum() * config.half_size,
        ),
        Axis::Y => unreachable!(),
    };
    if is_solid_at(world, probe) {
        *velocity = 0.0;
    } else {
        *position = next;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

fn is_solid_at(world: &World, position: Vec3) -> bool {
    world
        .block_at(
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        )
        .is_solid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Block, Chunk, ChunkPos, DEFAULT_SEED};

    fn stone_floor_world() -> World {
        let mut world = World::new(DEFAULT_SEED);
        assert!(world.insert_chunk(Chunk::new(ChunkPos::new(0, 0, 0))));
        assert!(world.insert_chunk(Chunk::new(ChunkPos::new(0, 1, 0))));
        assert_eq!(
            world.set_block(8, 10, 8, Block::Stone),
            vec![ChunkPos::new(0, 0, 0)]
        );
        world
    }

    fn stone_stack(count: u32) -> ItemStack {
        ItemStack::new(ItemId::STONE, count).expect("合法物品堆叠")
    }

    #[test]
    fn spawning_generates_geometry_and_tracks_the_chunk() {
        let mut store = DroppedItemStore::new();
        let config = DroppedItemConfig::default();
        store.spawn(
            stone_stack(1),
            Vec3::new(8.5, 12.0, 8.5),
            Vec3::new(0.2, 1.0, 0.0),
            &config,
        );

        assert_eq!(store.len(), 1);
        let mut vertices = Vec::new();
        store.build_vertices(&mut vertices);
        assert_eq!(vertices.len(), DROPPED_VERTICES_PER_ITEM);
        assert_eq!(
            store.take_dirty_chunks(),
            vec![chunk_of(Vec3::new(8.5, 12.0, 8.5))]
        );
    }

    #[test]
    fn dropped_items_fall_and_rest_on_the_ground() {
        let world = stone_floor_world();
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        store.spawn(
            stone_stack(1),
            Vec3::new(8.5, 12.0, 8.5),
            Vec3::ZERO,
            &config,
        );

        for _ in 0..40 {
            store.update(0.05, &world, &config);
        }

        let item = store.items()[0];
        let expected = 11.0 + config.half_size;
        assert!((item.position().y - expected).abs() < 0.01, "应停在石面上");
        assert_eq!(item.velocity.y, 0.0);
    }

    #[test]
    fn items_expire_after_their_lifetime() {
        let world = World::default();
        let config = DroppedItemConfig {
            lifetime: 1.0,
            ..DroppedItemConfig::default()
        };
        let mut store = DroppedItemStore::new();
        store.spawn(
            stone_stack(1),
            Vec3::new(0.5, 2.0, 0.5),
            Vec3::ZERO,
            &config,
        );

        for _ in 0..6 {
            store.update(0.2, &world, &config);
        }
        assert_eq!(store.len(), 0, "超过生命周期的掉落物应消失");
    }

    #[test]
    fn nearby_same_items_merge_without_exceeding_the_limit() {
        let world = World::default();
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        store.spawn(
            stone_stack(30),
            Vec3::new(0.5, 2.0, 0.5),
            Vec3::ZERO,
            &config,
        );
        store.spawn(
            stone_stack(30),
            Vec3::new(0.7, 2.0, 0.7),
            Vec3::ZERO,
            &config,
        );
        store.spawn(
            stone_stack(30),
            Vec3::new(0.9, 2.0, 0.9),
            Vec3::ZERO,
            &config,
        );

        store.update(0.05, &world, &config);

        let total: u32 = store.items().iter().map(|item| item.stack().count()).sum();
        assert_eq!(total, 90, "合并不得改变总数量");
        for item in store.items() {
            assert!(item.stack().count() <= 64);
        }
    }

    #[test]
    fn different_items_never_merge() {
        let world = World::default();
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        store.spawn(
            stone_stack(1),
            Vec3::new(0.5, 2.0, 0.5),
            Vec3::ZERO,
            &config,
        );
        store.spawn(
            ItemStack::new(ItemId::DIRT, 1).expect("合法堆叠"),
            Vec3::new(0.6, 2.0, 0.6),
            Vec3::ZERO,
            &config,
        );
        store.update(0.05, &world, &config);
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn chunk_snapshots_round_trip_and_never_duplicate() {
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        store.spawn(
            stone_stack(5),
            Vec3::new(0.5, 2.0, 0.5),
            Vec3::ZERO,
            &config,
        );
        let pos = chunk_of(Vec3::new(0.5, 2.0, 0.5));

        let records = store.remove_chunk(pos);
        assert_eq!(records.len(), 1);
        assert_eq!(store.len(), 0);

        assert_eq!(store.restore_chunk(pos, &records), 1);
        assert_eq!(store.len(), 1);
        // 同一区块重复恢复不应生成两份。
        assert_eq!(store.restore_chunk(pos, &records), 0);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn invalid_records_are_ignored_on_restore() {
        let mut store = DroppedItemStore::new();
        let records = [
            DroppedItemRecord {
                x: 0.5,
                y: 2.0,
                z: 0.5,
                item: 99,
                count: 1,
            },
            DroppedItemRecord {
                x: 0.5,
                y: 2.0,
                z: 0.5,
                item: 1,
                count: 0,
            },
        ];
        assert_eq!(store.restore_chunk(ChunkPos::new(0, 0, 0), &records), 0);
        assert_eq!(store.len(), 0);
    }
}

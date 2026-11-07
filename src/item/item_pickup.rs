//! 掉落物拾取：检测玩家附近的掉落物并转移到背包，无法容纳的部分留在世界里。

use glam::Vec3;

use super::dropped_item::{chunk_of, DroppedItemConfig, DroppedItemStore};
use super::inventory::Inventory;
use super::item::ItemId;

/// 一次拾取的结果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickupOutcome {
    /// 成功放入背包的物品与数量。
    pub picked: Vec<(ItemId, u32)>,
    /// 本次拾取涉及的区块，需要同步持久化。
    pub dirty_chunks: Vec<crate::world::ChunkPos>,
}

impl PickupOutcome {
    pub fn is_empty(&self) -> bool {
        self.picked.is_empty()
    }

    /// 本次拾取中数量最多的一项，用于 HUD 提示。
    pub fn headline(&self) -> Option<(ItemId, u32)> {
        self.picked.iter().copied().max_by_key(|(_, count)| *count)
    }
}

/// 收集玩家中心点附近、已过拾取延迟的掉落物。
///
/// 背包放不下的部分保留在原掉落物上，不丢弃；被拾取的区块会记录到脏区块列表。
pub fn collect(
    store: &mut DroppedItemStore,
    inventory: &mut Inventory,
    player_center: Vec3,
    config: &DroppedItemConfig,
) -> PickupOutcome {
    let radius_sq = config.pickup_radius * config.pickup_radius;
    let mut outcome = PickupOutcome::default();

    let items = store.items_mut();
    items.retain_mut(|item| {
        if !item.is_pickable(config) {
            return true;
        }
        if item.position().distance_squared(player_center) > radius_sq {
            return true;
        }

        let stack = item.stack();
        let added = inventory.add_item(stack.item(), stack.count());
        if added.added == 0 {
            return true;
        }

        let chunk = chunk_of(item.position);
        if !outcome.dirty_chunks.contains(&chunk) {
            outcome.dirty_chunks.push(chunk);
        }
        outcome.picked.push((stack.item(), added.added));

        if added.remaining == 0 {
            return false;
        }
        item.stack = stack
            .with_count(added.remaining)
            .expect("剩余数量仍在堆叠上限内");
        true
    });

    store.mark_dirty_chunks(&outcome.dirty_chunks);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{DroppedItemStore, ItemStack};
    use crate::world::{Block, Chunk, ChunkPos, World, DEFAULT_SEED};
    use glam::Vec3;

    /// 带一块石质地面的世界，掉落物会停在 y = 11.0 表面上方。
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
        ItemStack::new(ItemId::STONE, count).expect("合法堆叠")
    }

    /// 让掉落物在石面上停稳并越过拾取延迟（单帧推进被限制在 0.25 秒内）。
    fn settle(store: &mut DroppedItemStore, world: &World, config: &DroppedItemConfig) {
        store.update(0.3, world, config);
        store.update(0.3, world, config);
    }

    #[test]
    fn pickups_respect_the_radius_and_delay() {
        let world = stone_floor_world();
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        let mut inventory = Inventory::new();
        let center = Vec3::new(8.5, 11.15, 8.5);

        store.spawn(
            stone_stack(2),
            Vec3::new(8.5, 12.0, 8.5),
            Vec3::ZERO,
            &config,
        );
        let outcome = collect(&mut store, &mut inventory, center, &config);
        assert!(outcome.is_empty(), "拾取延迟内不能拾取");
        assert_eq!(store.len(), 1);

        settle(&mut store, &world, &config);
        let outcome = collect(&mut store, &mut inventory, center, &config);
        assert_eq!(outcome.picked, vec![(ItemId::STONE, 2)]);
        assert_eq!(store.len(), 0);
        assert_eq!(inventory.count_of(ItemId::STONE), 2);
    }

    #[test]
    fn distant_items_are_not_picked_up() {
        let world = World::default();
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        let mut inventory = Inventory::new();
        store.spawn(
            stone_stack(1),
            Vec3::new(10.5, 1.0, 0.5),
            Vec3::ZERO,
            &config,
        );
        store.update(0.6, &world, &config);
        let outcome = collect(
            &mut store,
            &mut inventory,
            Vec3::new(0.5, 1.0, 0.5),
            &config,
        );
        assert!(outcome.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn a_full_inventory_keeps_unpicked_items_in_the_world() {
        let world = stone_floor_world();
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        let mut inventory = Inventory::new();
        for index in 0..crate::item::TOTAL_SLOTS {
            inventory.set_slot(index, Some(stone_stack(64)));
        }
        store.spawn(
            stone_stack(5),
            Vec3::new(8.5, 12.0, 8.5),
            Vec3::ZERO,
            &config,
        );
        settle(&mut store, &world, &config);

        let outcome = collect(
            &mut store,
            &mut inventory,
            Vec3::new(8.5, 11.15, 8.5),
            &config,
        );
        assert!(outcome.is_empty());
        assert_eq!(store.len(), 1);
        assert_eq!(store.items()[0].stack().count(), 5);
    }

    #[test]
    fn partial_pickups_keep_the_remainder() {
        let world = stone_floor_world();
        let config = DroppedItemConfig::default();
        let mut store = DroppedItemStore::new();
        let mut inventory = Inventory::new();
        // 只剩 1 个空位且已堆叠 63 个石头，只能再收 1 个。
        for index in 0..crate::item::TOTAL_SLOTS {
            inventory.set_slot(index, Some(stone_stack(64)));
        }
        inventory.set_slot(0, Some(stone_stack(63)));

        store.spawn(
            stone_stack(10),
            Vec3::new(8.5, 12.0, 8.5),
            Vec3::ZERO,
            &config,
        );
        settle(&mut store, &world, &config);
        let outcome = collect(
            &mut store,
            &mut inventory,
            Vec3::new(8.5, 11.15, 8.5),
            &config,
        );

        assert_eq!(outcome.picked, vec![(ItemId::STONE, 1)]);
        assert_eq!(store.items()[0].stack().count(), 9);
    }
}

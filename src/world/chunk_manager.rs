//! 区块生命周期管理：按玩家位置排队加载附近区块、卸载远离玩家的区块，
//! 并用每帧预算与去重集合避免重复生成、重复上传和过期任务。

use std::collections::HashSet;

use super::chunk::ChunkPos;
use super::world::{World, WORLD_CHUNKS_Y};

/// 区块加载与卸载参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkConfig {
    /// 水平加载半径（区块数）；默认 3，即玩家周围 7×7 的加载区域。
    pub load_radius: i32,
    /// 水平卸载半径；默认 4，大于加载半径以避免玩家在区块边界附近反复加载卸载。
    pub unload_radius: i32,
    /// 单帧最多生成的区块数，防止快速移动时单帧卡顿。
    pub max_loads_per_frame: usize,
    /// 单帧最多重建网格的区块数。
    pub max_meshes_per_frame: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            load_radius: 3,
            unload_radius: 4,
            max_loads_per_frame: 4,
            max_meshes_per_frame: 6,
        }
    }
}

/// 一帧内完成的区块加载与卸载结果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ChunkUpdate {
    pub loaded: Vec<ChunkPos>,
    pub unloaded: Vec<ChunkPos>,
}

/// 单线程区块调度器。
///
/// 生命周期通过集合表达：`pending` 对应 Queued，World 中存在对应 DataReady / Meshed / Uploaded，
/// `dirty_meshes` 对应待重建或待上传；玩家改动由上层直接重建并同步到渲染器。
pub struct ChunkManager {
    config: ChunkConfig,
    /// 已排队、尚未生成的区块，按 ChunkPos 去重。
    pending: HashSet<ChunkPos>,
    /// 需要重建网格的区块，按 ChunkPos 去重。
    dirty_meshes: HashSet<ChunkPos>,
}

impl ChunkManager {
    pub fn new(config: ChunkConfig) -> Self {
        Self {
            config,
            pending: HashSet::new(),
            dirty_meshes: HashSet::new(),
        }
    }

    /// 排队等待生成的区块数量。
    pub fn pending_generation(&self) -> usize {
        self.pending.len()
    }

    /// 等待重建网格的区块数量。
    pub fn pending_mesh(&self) -> usize {
        self.dirty_meshes.len()
    }

    /// 玩家所在区块周围应加载的区块：水平半径内 7×7，垂直固定两层。
    pub fn desired_positions(player: ChunkPos, radius: i32) -> Vec<ChunkPos> {
        let radius = radius.max(0);
        let mut positions = Vec::with_capacity(((radius * 2 + 1).pow(2) * WORLD_CHUNKS_Y) as usize);
        for chunk_y in 0..WORLD_CHUNKS_Y {
            for chunk_z in (player.z - radius)..=(player.z + radius) {
                for chunk_x in (player.x - radius)..=(player.x + radius) {
                    positions.push(ChunkPos::new(chunk_x, chunk_y, chunk_z));
                }
            }
        }
        positions
    }

    /// 为出生点一次性填满加载区域，返回本次生成的区块数。
    ///
    /// 启动阶段一次性完成可避免开局逐帧补充区块，网格由渲染器初始化时统一上传。
    pub fn preload(
        &mut self,
        player: ChunkPos,
        world: &mut World,
        apply_edits: &mut dyn FnMut(ChunkPos, &mut World) -> Vec<ChunkPos>,
    ) -> usize {
        let mut loaded = 0;
        loop {
            let update = self.update(player, world, apply_edits, &mut |_| true);
            loaded += update.loaded.len();
            if update.loaded.is_empty() {
                break;
            }
        }
        // 启动时渲染器会统一构建全部网格，无需再逐帧补建。
        self.dirty_meshes.clear();
        loaded
    }

    /// 推进一帧的区块生命周期，返回本帧完成的加载与卸载。
    ///
    /// `apply_edits` 在区块插入后立刻套用存档改动，保证先有数据和修改、再生成网格。
    /// `can_unload` 由上层确认持久化状态，返回 `false` 时保留区块并稍后重试。
    pub fn update(
        &mut self,
        player: ChunkPos,
        world: &mut World,
        apply_edits: &mut dyn FnMut(ChunkPos, &mut World) -> Vec<ChunkPos>,
        can_unload: &mut dyn FnMut(ChunkPos) -> bool,
    ) -> ChunkUpdate {
        let desired = Self::desired_positions(player, self.config.load_radius);
        let desired_set: HashSet<ChunkPos> = desired.iter().copied().collect();

        // 玩家快速移动后，已离开加载范围的排队任务直接取消，避免过期任务重新提交。
        self.pending.retain(|pos| desired_set.contains(pos));

        // 缺失区块入队；pending 集合保证同一区块不会重复排队。
        for &pos in &desired {
            if !world.contains_chunk(pos) {
                self.pending.insert(pos);
            }
        }

        // 近到远处理：先水平距离，再垂直层，最后按坐标稳定排序。
        let mut queue: Vec<ChunkPos> = self.pending.iter().copied().collect();
        queue.sort_by_key(|pos| (Self::distance_sq(*pos, player), pos.y, pos.x, pos.z));

        let mut update = ChunkUpdate::default();
        for pos in queue.into_iter().take(self.config.max_loads_per_frame) {
            self.pending.remove(&pos);
            if !world.insert_chunk(world.generate_chunk(pos)) {
                continue;
            }

            update.loaded.push(pos);
            self.dirty_meshes.insert(pos);
            for affected in apply_edits(pos, world) {
                if world.contains_chunk(affected) {
                    self.dirty_meshes.insert(affected);
                }
            }
            self.mark_loaded_neighbors_dirty(pos, world);
        }

        self.unload_far_chunks(player, world, can_unload, &mut update);
        update
    }

    /// 取出本帧允许重建的网格：已卸载或不再存在的区块直接丢弃，近处优先。
    pub fn take_meshes(&mut self, player: ChunkPos, world: &World) -> Vec<ChunkPos> {
        self.dirty_meshes.retain(|pos| world.contains_chunk(*pos));

        let mut queue: Vec<ChunkPos> = self.dirty_meshes.iter().copied().collect();
        queue.sort_by_key(|pos| (Self::distance_sq(*pos, player), pos.y, pos.x, pos.z));
        queue.truncate(self.config.max_meshes_per_frame);

        for pos in &queue {
            self.dirty_meshes.remove(pos);
        }
        queue
    }

    /// 上层已直接重建这些区块的网格，从脏集合移除避免重复上传。
    pub fn skip_dirty(&mut self, positions: &[ChunkPos]) {
        for pos in positions {
            self.dirty_meshes.remove(pos);
        }
    }

    fn unload_far_chunks(
        &mut self,
        player: ChunkPos,
        world: &mut World,
        can_unload: &mut dyn FnMut(ChunkPos) -> bool,
        update: &mut ChunkUpdate,
    ) {
        let mut victims: Vec<ChunkPos> = world
            .chunk_positions()
            .filter(|pos| Self::chunk_distance(*pos, player) > self.config.unload_radius)
            .collect();
        victims.sort_by_key(|pos| (pos.x, pos.y, pos.z));

        for pos in victims {
            if !can_unload(pos) {
                continue;
            }
            if world.remove_chunk(pos).is_none() {
                continue;
            }
            update.unloaded.push(pos);
            self.pending.remove(&pos);
            self.dirty_meshes.remove(&pos);
            // 邻居失去相邻区块后需要恢复边界面。
            self.mark_loaded_neighbors_dirty(pos, world);
        }
    }

    fn mark_loaded_neighbors_dirty(&mut self, pos: ChunkPos, world: &World) {
        for neighbor in Self::neighbors(pos) {
            if world.contains_chunk(neighbor) {
                self.dirty_meshes.insert(neighbor);
            }
        }
    }

    fn neighbors(pos: ChunkPos) -> [ChunkPos; 6] {
        [
            ChunkPos::new(pos.x - 1, pos.y, pos.z),
            ChunkPos::new(pos.x + 1, pos.y, pos.z),
            ChunkPos::new(pos.x, pos.y - 1, pos.z),
            ChunkPos::new(pos.x, pos.y + 1, pos.z),
            ChunkPos::new(pos.x, pos.y, pos.z - 1),
            ChunkPos::new(pos.x, pos.y, pos.z + 1),
        ]
    }

    /// 仅比较水平距离，垂直层不参与优先级。
    fn distance_sq(pos: ChunkPos, player: ChunkPos) -> i64 {
        Self::horizontal_distance_sq(pos, player)
    }

    /// 水平切比雪夫距离（区块数），与按方块半径划定的加载/卸载区域一致。
    ///
    /// 卸载判定必须使用该距离：若改用欧氏距离，加载方形区域的四个角会落在卸载圆外，
    /// 造成同一区块被反复加载与卸载。
    fn chunk_distance(pos: ChunkPos, player: ChunkPos) -> i32 {
        (i64::from(pos.x) - i64::from(player.x))
            .abs()
            .max((i64::from(pos.z) - i64::from(player.z)).abs()) as i32
    }

    fn horizontal_distance_sq(pos: ChunkPos, player: ChunkPos) -> i64 {
        let dx = i64::from(pos.x) - i64::from(player.x);
        let dz = i64::from(pos.z) - i64::from(player.z);
        dx * dx + dz * dz
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(load_radius: i32, unload_radius: i32, budget: usize) -> ChunkConfig {
        ChunkConfig {
            load_radius,
            unload_radius,
            max_loads_per_frame: budget,
            max_meshes_per_frame: 6,
        }
    }

    fn no_edits(_pos: ChunkPos, _world: &mut World) -> Vec<ChunkPos> {
        Vec::new()
    }

    fn always_unload(_pos: ChunkPos) -> bool {
        true
    }

    #[test]
    fn desired_area_has_two_vertical_layers() {
        let positions = ChunkManager::desired_positions(ChunkPos::new(0, 0, 0), 3);
        assert_eq!(positions.len(), 7 * 7 * 2);
        assert!(positions
            .iter()
            .all(|pos| (0..WORLD_CHUNKS_Y).contains(&pos.y)));
    }

    #[test]
    fn preload_fills_the_whole_radius_without_duplicates() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(1, 2, 2));
        let player = ChunkPos::new(0, 0, 0);

        let loaded = manager.preload(player, &mut world, &mut no_edits);

        assert_eq!(loaded, 3 * 3 * 2);
        assert_eq!(world.chunk_count(), loaded);
        assert_eq!(manager.pending_generation(), 0);

        let update = manager.update(player, &mut world, &mut no_edits, &mut always_unload);
        assert!(update.loaded.is_empty(), "已加载区块不应重复生成");
    }

    #[test]
    fn default_load_area_does_not_thrash_its_corners() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(ChunkConfig::default());
        let player = ChunkPos::new(0, 0, 0);

        let loaded = manager.preload(player, &mut world, &mut no_edits);
        assert_eq!(loaded, 7 * 7 * 2);

        // 加载区域四角到玩家的切比雪夫距离等于加载半径，必须落在卸载半径内，
        // 否则同一区块会被反复加载与卸载，预加载也将无法结束。
        for _ in 0..3 {
            let update = manager.update(player, &mut world, &mut no_edits, &mut always_unload);
            assert!(update.loaded.is_empty());
            assert!(update.unloaded.is_empty());
        }
    }

    #[test]
    fn per_frame_budget_limits_generation() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(1, 2, 2));
        let player = ChunkPos::new(0, 0, 0);

        let first = manager.update(player, &mut world, &mut no_edits, &mut always_unload);
        assert_eq!(first.loaded.len(), 2);
        assert_eq!(manager.pending_generation(), 16);

        let second = manager.update(player, &mut world, &mut no_edits, &mut always_unload);
        assert_eq!(second.loaded.len(), 2);
        assert_eq!(world.chunk_count(), 4);
    }

    #[test]
    fn nearest_chunks_are_generated_first() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(2, 3, 1));
        let player = ChunkPos::new(0, 0, 0);

        let first = manager.update(player, &mut world, &mut no_edits, &mut always_unload);
        assert_eq!(first.loaded, vec![ChunkPos::new(0, 0, 0)]);

        let second = manager.update(player, &mut world, &mut no_edits, &mut always_unload);
        assert_eq!(second.loaded, vec![ChunkPos::new(0, 1, 0)]);
    }

    #[test]
    fn moving_the_player_cancels_stale_pending_tasks() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(2, 3, 1));
        manager.update(
            ChunkPos::new(0, 0, 0),
            &mut world,
            &mut no_edits,
            &mut always_unload,
        );
        assert!(manager.pending_generation() > 0);

        // 玩家瞬移到远处：旧区域尚未生成的任务应全部取消。
        let moved = manager.update(
            ChunkPos::new(100, 0, 100),
            &mut world,
            &mut no_edits,
            &mut always_unload,
        );

        // 半径 2 的区域内，切比雪夫距离最大为 2。
        assert!(
            manager
                .pending
                .iter()
                .all(|pos| ChunkManager::chunk_distance(*pos, ChunkPos::new(100, 0, 100)) <= 2),
            "旧区域的过期任务仍留在队列中"
        );
        assert!(moved
            .loaded
            .iter()
            .all(|pos| ChunkManager::chunk_distance(*pos, ChunkPos::new(100, 0, 100)) <= 2));
    }

    #[test]
    fn chunks_beyond_unload_radius_are_removed() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(1, 2, 9));
        let origin = ChunkPos::new(0, 0, 0);
        manager.preload(origin, &mut world, &mut no_edits);

        let update = manager.update(
            ChunkPos::new(10, 0, 10),
            &mut world,
            &mut no_edits,
            &mut always_unload,
        );

        assert!(!update.unloaded.is_empty());
        assert!(world
            .chunk_positions()
            .all(|pos| ChunkManager::chunk_distance(pos, ChunkPos::new(10, 0, 10)) <= 2));
    }

    #[test]
    fn unload_is_vetoed_until_persistence_confirms() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(1, 2, 9));
        manager.preload(ChunkPos::new(0, 0, 0), &mut world, &mut no_edits);
        let before = world.chunk_count();

        let update = manager.update(
            ChunkPos::new(10, 0, 10),
            &mut world,
            &mut no_edits,
            &mut |_| false,
        );

        assert!(update.unloaded.is_empty());
        // 旧区块保留，新区域仍会加载；数量因此不少于原数量。
        assert!(world.chunk_count() >= before);
    }

    #[test]
    fn loading_a_chunk_marks_loaded_neighbors_for_meshing() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(0, 1, 1));
        manager.preload(ChunkPos::new(0, 0, 0), &mut world, &mut no_edits);
        manager.dirty_meshes.clear();

        let update = manager.update(
            ChunkPos::new(1, 0, 0),
            &mut world,
            &mut no_edits,
            &mut always_unload,
        );

        assert_eq!(update.loaded, vec![ChunkPos::new(1, 0, 0)]);
        assert!(manager.dirty_meshes.contains(&ChunkPos::new(1, 0, 0)));
        assert!(manager.dirty_meshes.contains(&ChunkPos::new(0, 0, 0)));
    }

    #[test]
    fn take_meshes_prioritizes_near_chunks_and_drops_unloaded_ones() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(0, 1, 1));
        let player = ChunkPos::new(0, 0, 0);
        manager.preload(player, &mut world, &mut no_edits);
        manager.dirty_meshes.clear();
        manager.dirty_meshes.insert(ChunkPos::new(5, 0, 0));
        manager.dirty_meshes.insert(ChunkPos::new(0, 1, 0));
        manager.dirty_meshes.insert(ChunkPos::new(0, 0, 0));

        let meshes = manager.take_meshes(player, &world);
        assert_eq!(meshes, vec![ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        assert!(!manager.dirty_meshes.contains(&ChunkPos::new(5, 0, 0)));
    }

    #[test]
    fn unloading_restores_neighbor_boundaries() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(1, 2, 9));
        manager.preload(ChunkPos::new(0, 0, 0), &mut world, &mut no_edits);
        manager.dirty_meshes.clear();

        // 玩家移动到 (3, 0, 0)：x = 0 的区块超出卸载半径被移除，
        // 相邻的 x = 1 区块仍在加载范围内，需要恢复其边界面。
        let update = manager.update(
            ChunkPos::new(3, 0, 0),
            &mut world,
            &mut no_edits,
            &mut always_unload,
        );

        assert!(update.unloaded.contains(&ChunkPos::new(0, 0, 0)));
        assert!(world.contains_chunk(ChunkPos::new(1, 0, 0)));
        assert!(manager.dirty_meshes.contains(&ChunkPos::new(1, 0, 0)));
    }

    #[test]
    fn edits_are_applied_when_a_chunk_loads() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(0, 1, 1));
        let mut applied = Vec::new();

        manager.preload(ChunkPos::new(0, 0, 0), &mut world, &mut |pos, _world| {
            applied.push(pos);
            Vec::new()
        });

        assert_eq!(applied.len(), WORLD_CHUNKS_Y as usize);
        assert!(applied.contains(&ChunkPos::new(0, 0, 0)));
        assert!(applied.contains(&ChunkPos::new(0, 1, 0)));
    }
}

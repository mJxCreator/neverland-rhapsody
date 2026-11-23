//! 区块生命周期管理：按玩家位置排队加载附近区块、卸载远离玩家的区块，
//! 并用每帧预算与去重集合避免重复生成、重复上传和过期任务。
//!
//! 生成入口唯一：所有需要程序化地形的系统都经本模块排队，游戏逻辑不直接调用
//! `World::generate_chunk`。生成本身是 `(种子, 版本, 坐标)` 的纯函数，守住这一点
//! 才能在需要时把生成整体搬到工作线程而不改动调用方。

use std::cmp::Reverse;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use super::chunk::ChunkPos;
use super::world::{World, WORLD_CHUNKS_Y};

/// 单帧区块生成的时间预算：超出后余下任务留到下一帧，避免跨越区块边界时集中生成造成卡顿。
const GENERATION_BUDGET: Duration = Duration::from_millis(2);
/// 单帧最多卸载的区块数：卸载按帧摊开，避免一次删除整列区块并连锁重建大量邻居网格。
const MAX_UNLOADS_PER_FRAME: usize = 8;

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
    /// 单帧生成区块的时间预算；连同数量上限一起限制单帧阻塞时长。
    pub load_time_budget: Duration,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            load_radius: 3,
            unload_radius: 4,
            max_loads_per_frame: 4,
            max_meshes_per_frame: 6,
            load_time_budget: GENERATION_BUDGET,
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
    /// 创建调度器，并规范化参数：
    /// 加载半径非负、卸载半径至少比加载半径大 1、单帧预算至少为 1。
    ///
    /// 这些约束保证加载区域始终落在卸载区域内部；否则同一区块会在同一帧被加载后立即卸载，
    /// 预加载将永远无法结束。
    pub fn new(config: ChunkConfig) -> Self {
        let load_radius = config.load_radius.max(0);
        Self {
            config: ChunkConfig {
                load_radius,
                unload_radius: config.unload_radius.max(load_radius + 1),
                max_loads_per_frame: config.max_loads_per_frame.max(1),
                max_meshes_per_frame: config.max_meshes_per_frame.max(1),
                // 时间预算为零会让任何一帧都放弃生成，归一化为至少 1ms。
                load_time_budget: config.load_time_budget.max(Duration::from_millis(1)),
            },
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

    /// 同步补齐指定半径内的缺失区块，返回本次加载的坐标。
    ///
    /// 仅供重生等必须立刻读取地形的场景使用：这里不套用每帧预算，生成入口仍集中在本模块，
    /// 生成的网格会进入脏集合，随后由常规流式加载按预算补建。
    pub fn load_now(
        &mut self,
        center: ChunkPos,
        radius: i32,
        world: &mut World,
        apply_edits: &mut dyn FnMut(ChunkPos, &mut World) -> Vec<ChunkPos>,
    ) -> Vec<ChunkPos> {
        let mut loaded = Vec::new();
        for pos in Self::desired_positions(center, radius) {
            if world.contains_chunk(pos) || !world.insert_chunk(world.generate_chunk(pos)) {
                continue;
            }
            self.pending.remove(&pos);
            loaded.push(pos);
            self.dirty_meshes.insert(pos);
            for affected in apply_edits(pos, world) {
                if world.contains_chunk(affected) {
                    self.dirty_meshes.insert(affected);
                }
            }
            self.mark_loaded_neighbors_dirty(pos, world);
        }
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
        let started = Instant::now();
        for pos in queue.into_iter().take(self.config.max_loads_per_frame) {
            // 数量上限之外再加时间上限：低预算帧只生成一个区块，避免长帧尖峰。
            if started.elapsed() >= self.config.load_time_budget {
                break;
            }
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

    /// 本帧来不及重建的区块放回脏集合，下一帧继续按距离优先处理。
    pub fn requeue_meshes(&mut self, positions: &[ChunkPos]) {
        for pos in positions {
            self.dirty_meshes.insert(*pos);
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
        // 最远的先卸载：被预算挤到下一帧的都是最不可能出现在视野里的区块。
        victims.sort_by_key(|pos| {
            (
                Reverse(Self::chunk_distance(*pos, player)),
                pos.x,
                pos.y,
                pos.z,
            )
        });

        let mut removed = Vec::new();
        for pos in victims.into_iter().take(MAX_UNLOADS_PER_FRAME) {
            if !can_unload(pos) {
                continue;
            }
            if world.remove_chunk(pos).is_none() {
                continue;
            }
            update.unloaded.push(pos);
            removed.push(pos);
            self.pending.remove(&pos);
            self.dirty_meshes.remove(&pos);
        }
        // 等本帧该卸载的全部移除后再标记邻居：一并卸载的邻居已不在世界中，
        // 会被 `contains_chunk` 自动跳过，不再产生无意义的脏网格。
        for pos in removed {
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
            // 测试关注数量上限，时间预算放大到不会干扰断言。
            load_time_budget: Duration::from_secs(1),
        }
    }

    fn no_edits(_pos: ChunkPos, _world: &mut World) -> Vec<ChunkPos> {
        Vec::new()
    }

    fn always_unload(_pos: ChunkPos) -> bool {
        true
    }

    #[test]
    fn desired_area_covers_every_vertical_layer() {
        let positions = ChunkManager::desired_positions(ChunkPos::new(0, 0, 0), 3);
        assert_eq!(positions.len(), 7 * 7 * WORLD_CHUNKS_Y as usize);
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

        assert_eq!(loaded, 3 * 3 * WORLD_CHUNKS_Y as usize);
        assert_eq!(world.chunk_count(), loaded);
        assert_eq!(manager.pending_generation(), 0);

        let update = manager.update(player, &mut world, &mut no_edits, &mut always_unload);
        assert!(update.loaded.is_empty(), "已加载区块不应重复生成");
    }

    #[test]
    fn invalid_config_is_normalized_to_prevent_thrashing() {
        let mut manager = ChunkManager::new(ChunkConfig {
            load_radius: -1,
            unload_radius: -3,
            max_loads_per_frame: 0,
            max_meshes_per_frame: 0,
            load_time_budget: Duration::ZERO,
        });
        assert_eq!(manager.config.load_radius, 0);
        assert_eq!(manager.config.unload_radius, 1);
        assert_eq!(manager.config.max_loads_per_frame, 1);
        assert_eq!(manager.config.max_meshes_per_frame, 1);
        assert_eq!(manager.config.load_time_budget, Duration::from_millis(1));

        let mut world = World::default();
        let loaded = manager.preload(ChunkPos::new(0, 0, 0), &mut world, &mut no_edits);
        assert_eq!(loaded, WORLD_CHUNKS_Y as usize);
        assert_eq!(world.chunk_count(), loaded);

        // 卸载半径小于加载半径的配置也必须收敛，而不是每帧加载后立刻卸载。
        let manager = ChunkManager::new(config(2, 1, 1));
        assert_eq!(manager.config.unload_radius, 3);
    }

    #[test]
    fn default_load_area_does_not_thrash_its_corners() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(ChunkConfig::default());
        let player = ChunkPos::new(0, 0, 0);

        let loaded = manager.preload(player, &mut world, &mut no_edits);
        assert_eq!(loaded, 7 * 7 * WORLD_CHUNKS_Y as usize);

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
        assert_eq!(
            manager.pending_generation(),
            9 * WORLD_CHUNKS_Y as usize - 2
        );

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

        let target = ChunkPos::new(10, 0, 10);
        let mut saw_unload = false;
        // 卸载按帧摊开，反复推进直到不再有区块超出卸载半径。
        for _ in 0..64 {
            let update = manager.update(target, &mut world, &mut no_edits, &mut always_unload);
            saw_unload |= !update.unloaded.is_empty();
            if update.unloaded.is_empty() {
                break;
            }
        }

        assert!(saw_unload);
        assert!(world
            .chunk_positions()
            .all(|pos| ChunkManager::chunk_distance(pos, target) <= 2));
    }

    #[test]
    fn unloads_are_spread_across_frames() {
        let mut world = World::default();
        let mut manager = ChunkManager::new(config(1, 2, 9));
        manager.preload(ChunkPos::new(0, 0, 0), &mut world, &mut no_edits);

        // 瞬移到远处：一帧最多卸载 MAX_UNLOADS_PER_FRAME 个区块，其余留到后续帧。
        let target = ChunkPos::new(10, 0, 10);
        let update = manager.update(target, &mut world, &mut no_edits, &mut always_unload);
        assert!(!update.unloaded.is_empty());
        assert!(update.unloaded.len() <= MAX_UNLOADS_PER_FRAME);
        assert!(
            world.chunk_count() > 9 * WORLD_CHUNKS_Y as usize,
            "旧区域不应在一帧内被全部删除"
        );
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
        // 卸载按帧摊开，反复推进直到目标区块被移除。
        let mut unloaded = Vec::new();
        for _ in 0..64 {
            let update = manager.update(
                ChunkPos::new(3, 0, 0),
                &mut world,
                &mut no_edits,
                &mut always_unload,
            );
            unloaded.extend(update.unloaded.iter().copied());
            if unloaded.contains(&ChunkPos::new(0, 0, 0)) {
                break;
            }
        }

        assert!(unloaded.contains(&ChunkPos::new(0, 0, 0)));
        assert!(world.contains_chunk(ChunkPos::new(1, 0, 0)));
        assert!(manager.dirty_meshes.contains(&ChunkPos::new(1, 0, 0)));
    }

    #[test]
    fn load_now_fills_the_area_immediately_and_marks_edits_dirty() {
        let mut world = World::default();
        // 单帧预算只有 1，load_now 仍应一次补齐整个半径，供重生立即读取地形。
        let mut manager = ChunkManager::new(config(1, 2, 1));
        let center = ChunkPos::new(0, 0, 0);

        let loaded = manager.load_now(center, 1, &mut world, &mut |pos, _world| {
            if pos == center {
                vec![center]
            } else {
                Vec::new()
            }
        });

        assert_eq!(loaded.len(), 3 * 3 * WORLD_CHUNKS_Y as usize);
        assert_eq!(world.chunk_count(), loaded.len());
        assert!(manager.dirty_meshes.contains(&center));
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

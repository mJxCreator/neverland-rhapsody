//! 应用主体：窗口创建与主循环。

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};
use tracing::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use super::config::AppConfig;
use super::time::FrameTimer;
use crate::interaction::{
    placement_cell, raycast, Hotbar, MiningOutcome, MiningState, MiningTarget, RayHit,
};
use crate::item::{
    block_for_item, collect_pickups, drops_for_block, DroppedItemConfig, DroppedItemStore,
    Inventory, ItemStack,
};
use crate::player::{PhysicsConfig, PhysicsSystem, PhysicsWorld, Player, PlayerController};
use crate::renderer::{spawn_pose, ClickButton, InventoryUi, RenderOutcome, Renderer};
use crate::save::{BlockEdit, WorldSave};
use crate::world::{Block, ChunkConfig, ChunkManager, ChunkPos, World};

/// 目标帧间隔，约 60 FPS。
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// 准星射线长度：超出该距离的方块不会被选中。
const SELECTION_REACH: f32 = 8.0;
/// 存档写入失败后的重试间隔，避免每帧都重复写盘。
const SAVE_RETRY_INTERVAL: Duration = Duration::from_secs(2);

/// 持有窗口、渲染器与主循环状态。
pub struct App {
    config: AppConfig,
    world: World,
    save: WorldSave,
    chunk_manager: ChunkManager,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    controller: PlayerController,
    player: Player,
    physics: PhysicsSystem,
    /// 持续挖掘状态：绑定当前目标方块与累计时间。
    mining: MiningState,
    /// 左键是否仍按住。
    mining_held: bool,
    /// 快捷栏仅保存选中下标，物品内容来自背包。
    hotbar: Hotbar,
    /// 玩家背包：27 格主背包 + 9 格快捷栏。
    inventory: Inventory,
    /// 背包界面状态（开关、鼠标位置、拖动中的物品）。
    inventory_ui: InventoryUi,
    /// 世界掉落物。
    dropped: DroppedItemStore,
    /// 掉落物行为参数。
    drop_config: DroppedItemConfig,
    /// 上一帧实际执行的物理步数，用于调试日志。
    physics_steps: u32,
    cursor_grabbed: bool,
    timer: FrameTimer,
    next_frame: Instant,
    /// 上一帧区块生成耗时（毫秒），用于调试日志。
    generation_ms: f64,
    /// 上一帧网格构建与上传耗时（毫秒）。
    mesh_ms: f64,
    /// 下一次允许重试写入存档的时刻。
    next_save_retry: Instant,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        let world = World::default();
        let save = WorldSave::load(WorldSave::default_path(), world.seed());
        // 物理参数集中管理，玩家碰撞体尺寸也由同一处提供。
        let physics = PhysicsSystem::new(PhysicsConfig::default());
        let (spawn, yaw, pitch) = spawn_pose(&world);
        let mut player = Player::new(spawn, physics.config().dimensions);
        player.rotate_look(yaw, pitch);

        // 旧档没有玩家数据时按空背包启动（v0.12 迁移策略）。
        let (inventory, selected_hotbar) = save
            .player()
            .map(Inventory::from_saved)
            .unwrap_or((Inventory::new(), 0));
        let mut hotbar = Hotbar::new();
        hotbar.set_selected(selected_hotbar);

        Self {
            config,
            world,
            save,
            chunk_manager: ChunkManager::new(ChunkConfig::default()),
            window: None,
            renderer: None,
            controller: PlayerController::new(),
            player,
            physics,
            mining: MiningState::new(),
            mining_held: false,
            hotbar,
            inventory,
            inventory_ui: InventoryUi::new(),
            dropped: DroppedItemStore::new(),
            drop_config: DroppedItemConfig::default(),
            physics_steps: 0,
            cursor_grabbed: false,
            timer: FrameTimer::new(),
            next_frame: Instant::now(),
            generation_ms: 0.0,
            mesh_ms: 0.0,
            next_save_retry: Instant::now(),
        }
    }

    /// 创建事件循环并进入主循环，窗口关闭或按下 Esc 时返回。
    pub fn run(config: AppConfig) -> anyhow::Result<()> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Wait);

        let mut app = Self::new(config);
        event_loop.run_app(&mut app)?;
        info!("事件循环已退出");
        Ok(())
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title(self.config.window_title.clone())
            .with_inner_size(PhysicalSize::new(
                self.config.window_width,
                self.config.window_height,
            ));

        match event_loop.create_window(attributes) {
            Ok(window) => {
                info!(
                    width = self.config.window_width,
                    height = self.config.window_height,
                    "窗口已创建"
                );

                let window = Arc::new(window);
                // 先按出生点填满初始加载区域，渲染器初始化时即可一次性上传网格。
                let spawn = self.player.position();
                let player_chunk =
                    ChunkPos::from_world(spawn.x.floor() as i32, 0, spawn.z.floor() as i32);
                let loaded =
                    self.chunk_manager
                        .preload(player_chunk, &mut self.world, &mut |pos, world| {
                            self.save.apply_chunk(pos, world)
                        });
                info!(chunks = self.world.chunk_count(), loaded, "初始区块已加载");

                // 存档中属于已加载区块的掉落物恢复为运行时实体。
                let loaded_chunks: Vec<ChunkPos> = self.world.chunk_positions().collect();
                for pos in loaded_chunks {
                    let records = self.save.chunk_drops(pos).to_vec();
                    self.dropped.restore_chunk(pos, &records);
                }

                match Renderer::new(
                    window.clone(),
                    window.inner_size(),
                    &self.world,
                    self.player.eye_position(),
                    self.player.yaw(),
                    self.player.pitch(),
                ) {
                    Ok(renderer) => {
                        info!("渲染器已初始化");
                        self.renderer = Some(renderer);
                    }
                    Err(err) => {
                        error!(error = %format!("{err:#}"), "渲染器初始化失败");
                        event_loop.exit();
                    }
                }

                self.window = Some(window);
                self.timer = FrameTimer::new();
                self.next_frame = Instant::now();
            }
            Err(err) => {
                error!(error = %err, "窗口创建失败");
                event_loop.exit();
            }
        }
    }

    /// 捕获鼠标用于第一人称视角；优先锁定光标，失败时退回窗口内约束。
    fn grab_cursor(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let result = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        match result {
            Ok(()) => {
                window.set_cursor_visible(false);
                self.cursor_grabbed = true;
                info!("鼠标已捕获，按 Esc 释放");
            }
            Err(err) => warn!(error = %err, "鼠标捕获失败"),
        }
    }

    /// 释放鼠标并清空输入状态，避免按键与视角残留。
    fn release_cursor(&mut self) {
        if let Some(window) = self.window.as_ref() {
            if let Err(err) = window.set_cursor_grab(CursorGrabMode::None) {
                warn!(error = %err, "释放鼠标捕获失败");
            }
            window.set_cursor_visible(true);
        }
        self.cursor_grabbed = false;
        self.mining_held = false;
        self.mining.cancel();
        self.controller.reset();
    }

    /// 打开背包界面：暂停视角与挖掘输入，但世界物理与区块流送继续运行。
    fn open_inventory(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            if let Err(err) = window.set_cursor_grab(CursorGrabMode::None) {
                warn!(error = %err, "打开背包时释放鼠标捕获失败");
            }
            window.set_cursor_visible(true);
        }
        self.cursor_grabbed = false;
        self.mining_held = false;
        self.mining.cancel();
        self.controller.reset();
        self.inventory_ui.open();
        info!("背包已打开，按 E 或 Esc 关闭");
    }

    /// 关闭背包界面：拖动中的物品先归位，再恢复鼠标捕获。
    fn close_inventory(&mut self) {
        if !self.inventory_ui.try_close(&mut self.inventory) {
            warn!("背包已满，无法放下拖动中的物品，界面保持打开");
            return;
        }
        self.persist_player();
        self.grab_cursor();
        info!("背包已关闭");
    }

    /// 当前准星命中的方块。
    fn targeted_block(&self) -> Option<RayHit> {
        let renderer = self.renderer.as_ref()?;
        let camera = renderer.camera();
        raycast(
            &self.world,
            camera.position(),
            camera.forward(),
            SELECTION_REACH,
        )
    }

    /// 当前命中方块对应的挖掘目标；空气不可挖掘，因此无需进入状态机。
    fn mining_target(&self, hit: Option<RayHit>) -> Option<MiningTarget> {
        let hit = hit?;
        let block_type = self.world.block_at(hit.block.x, hit.block.y, hit.block.z);
        if block_type == Block::Air {
            return None;
        }
        Some(MiningTarget {
            block: hit.block,
            block_type,
        })
    }

    /// 破坏指定方块：写世界、存存档、重建网格并生成碎块。
    fn break_block(&mut self, block: IVec3, block_type: Block) {
        let affected = self.world.set_block(block.x, block.y, block.z, Block::Air);
        if affected.is_empty() {
            return;
        }

        info!(block = ?block, block_type = ?block_type, "方块已破坏");
        self.remember_edit(BlockEdit {
            x: block.x,
            y: block.y,
            z: block.z,
            block: Block::Air,
        });
        self.spawn_block_drops(block, block_type);
        self.rebuild_chunks(affected);
        self.mining.cancel();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.spawn_block_particles(block_type, block);
            renderer.set_selection(None);
        }
    }

    /// 按方块掉落表生成掉落物；初始速度带轻微抛出效果。
    fn spawn_block_drops(&mut self, block: IVec3, block_type: Block) {
        let center = Vec3::new(
            block.x as f32 + 0.5,
            block.y as f32 + 0.5,
            block.z as f32 + 0.5,
        );
        for (index, drop) in drops_for_block(block_type).iter().enumerate() {
            let Some(stack) = ItemStack::new(drop.item, drop.count) else {
                continue;
            };
            let direction = if index % 2 == 0 { 1.0 } else { -1.0 };
            let velocity = Vec3::new(0.18 * direction, 1.2, -0.08 * direction);
            self.dropped
                .spawn(stack, center, velocity, &self.drop_config);
        }
    }

    /// 右键放置：在命中面外侧的空格放下快捷栏当前选中的方块物品。
    fn place_block(&mut self) {
        let Some(hit) = self.targeted_block() else {
            return;
        };
        let Some(stack) = self.inventory.hotbar_stack(self.hotbar.selected_index()) else {
            debug!("当前快捷栏槽位为空，无法放置");
            return;
        };
        let Some(block) = block_for_item(stack.item()) else {
            debug!(item = ?stack.item(), "该物品不能放置为方块");
            return;
        };
        let target = match placement_cell(&self.world, &hit, &self.player, block) {
            Ok(target) => target,
            Err(reason) => {
                debug!(?reason, hit = ?hit.block, "方块放置被拒绝");
                return;
            }
        };

        let affected = self.world.set_block(target.x, target.y, target.z, block);
        if affected.is_empty() {
            return;
        }

        // 世界写入成功后才扣减物品，避免放置失败时损失库存。
        self.inventory.remove_item(stack.item(), 1);
        self.persist_player();

        info!(block = ?target, block_type = ?block, "方块已放置");
        self.remember_edit(BlockEdit {
            x: target.x,
            y: target.y,
            z: target.z,
            block,
        });
        self.rebuild_chunks(affected);
    }

    /// 方块变化后重建受影响区块的网格。
    fn rebuild_chunks(&mut self, affected: Vec<ChunkPos>) {
        if affected.is_empty() {
            return;
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_chunks(&self.world, &affected);
        }
        // 已直接重建，避免调度器重复上传同一批区块。
        self.chunk_manager.skip_dirty(&affected);
    }

    /// 推进动态区块生命周期：加载/卸载区块、应用存档改动、按预算重建网格。
    fn stream_chunks(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        // 以玩家实体位置而非摄像机计算目标区块，摄像机只做渲染跟随。
        let position = self.player.position();
        let player_chunk =
            ChunkPos::from_world(position.x.floor() as i32, 0, position.z.floor() as i32);

        let start = Instant::now();
        // 写盘失败后按固定间隔重试，避免每帧反复读写整个存档文件。
        let now = Instant::now();
        if now >= self.next_save_retry {
            self.next_save_retry = now + SAVE_RETRY_INTERVAL;
            if let Err(err) = self.save.ensure_persisted() {
                warn!(error = %format!("{err:#}"), "世界存档重试写入失败");
            }
        }
        // 即使写盘失败，改动仍完整保留在 WorldSave 的内存待写入队列中，
        // 卸载区块不会丢失数据；若此时拒绝卸载，持续写盘失败会让已探索区块无界增长。
        //
        // 卸载保护：保留玩家所在区块及其水平一圈邻居。玩家碰撞体最多横跨一格，
        // 该范围覆盖任何可能的碰撞查询，避免依赖的方块数据先于玩家离开被卸载。
        let update = self.chunk_manager.update(
            player_chunk,
            &mut self.world,
            &mut |pos, world| self.save.apply_chunk(pos, world),
            &mut |pos: ChunkPos| {
                (pos.x - player_chunk.x)
                    .abs()
                    .max((pos.z - player_chunk.z).abs())
                    > 1
            },
        );
        self.generation_ms = start.elapsed().as_secs_f64() * 1000.0;
        if !update.loaded.is_empty() || !update.unloaded.is_empty() {
            debug!(
                loaded = update.loaded.len(),
                unloaded = update.unloaded.len(),
                "区块生命周期更新"
            );
        }

        // 区块卸载前保存所属掉落物，重新加载的区块则从存档恢复，二者互不重复。
        for pos in &update.loaded {
            let records = self.save.chunk_drops(*pos).to_vec();
            self.dropped.restore_chunk(*pos, &records);
        }
        if !update.unloaded.is_empty() {
            for pos in &update.unloaded {
                let records = self.dropped.remove_chunk(*pos);
                self.save.record_chunk_drops(*pos, records);
            }
            if let Err(err) = self.save.ensure_persisted() {
                warn!(error = %format!("{err:#}"), "卸载区块时写入存档失败");
            }
        }

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // 先释放已卸载区块的 GPU 资源，再补建本帧允许的网格。
        renderer.remove_chunks(&update.unloaded);

        let start = Instant::now();
        let meshes = self.chunk_manager.take_meshes(player_chunk, &self.world);
        renderer.update_chunks(&self.world, &meshes);
        self.mesh_ms = start.elapsed().as_secs_f64() * 1000.0;
    }

    /// 退出前确保内存中的方块改动、掉落物与玩家背包已经落盘。
    fn flush_save(&mut self) {
        // 拖动中的物品先归位，避免关闭界面时丢失。
        let _ = self.inventory_ui.try_close(&mut self.inventory);
        self.persist_player();

        for pos in self.world.chunk_positions() {
            let records = self.dropped.snapshot_chunk(pos);
            self.save.record_chunk_drops(pos, records);
        }
        if let Err(err) = self.save.ensure_persisted() {
            warn!(error = %format!("{err:#}"), "退出前写入世界存档失败");
        }
    }

    /// 把当前背包与快捷栏选择写入存档；失败只记录日志，不中断游戏。
    fn persist_player(&mut self) {
        let saved = self.inventory.to_saved(self.hotbar.selected_index());
        if let Err(err) = self.save.set_player(saved) {
            warn!(error = %format!("{err:#}"), "玩家背包写入存档失败");
        }
    }

    /// 把发生变化的区块掉落物快照写回存档。
    fn persist_dirty_drops(&mut self) {
        let chunks = self.dropped.take_dirty_chunks();
        if chunks.is_empty() {
            return;
        }
        for pos in chunks {
            let records = self.dropped.snapshot_chunk(pos);
            self.save.record_chunk_drops(pos, records);
        }
        if let Err(err) = self.save.ensure_persisted() {
            warn!(error = %format!("{err:#}"), "掉落物变化写入存档失败");
        }
    }

    /// 把一次改动写入存档；失败时只记录日志，不影响继续游戏。
    fn remember_edit(&mut self, edit: BlockEdit) {
        if let Err(err) = self.save.record(edit) {
            warn!(error = %format!("{err:#}"), "世界存档写入失败");
        }
    }

    fn draw_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        let delta_secs = delta.as_secs_f32();
        // 视角旋转与物理位移解耦：鼠标视角立即生效，位移走固定步长物理。
        let look = self.controller.take_look();
        self.player.rotate_look(look.0, look.1);

        let intent = self.controller.intent();
        let physics_world = PhysicsWorld::new(&self.world);
        let update = self
            .physics
            .update(&mut self.player, &intent, &physics_world, delta_secs);
        self.physics_steps = update.steps;

        // 摄像机用物理插值系数跟随玩家，避免固定步长与渲染帧率不同步产生抖动。
        let alpha = update.alpha;
        let eye = self.player.interpolated_eye_position(alpha);
        let yaw = self.player.yaw();
        let pitch = self.player.pitch();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.camera_mut().set_pose(eye, yaw, pitch);
        }

        // 物理移动后先补齐区块，再让射线检测基于最新的已加载世界。
        self.stream_chunks();

        // 掉落物物理与自动拾取；拾取后立即同步玩家背包与所属区块存档。
        self.dropped
            .update(delta_secs, &self.world, &self.drop_config);
        // 以玩家身体中心作为拾取判定点。
        let center = self.player.position() + Vec3::Y * 0.9;
        let picked = collect_pickups(
            &mut self.dropped,
            &mut self.inventory,
            center,
            &self.drop_config,
        );
        self.persist_dirty_drops();
        if !picked.is_empty() {
            if let Some(renderer) = self.renderer.as_mut() {
                if let Some((item, count)) = picked.headline() {
                    renderer.hud_mut().show_pickup(item, count);
                }
            }
            self.persist_player();
        }

        let inventory_open = self.inventory_ui.is_open();
        let hit = self.targeted_block();
        let mining_target = self.mining_target(hit);
        let outcome = self
            .mining
            .update(self.mining_held, mining_target, delta_secs);

        let mut selection = hit.map(|hit| hit.block);
        if let MiningOutcome::Completed { block } = outcome {
            let block_type = self.world.block_at(block.x, block.y, block.z);
            self.break_block(block, block_type);
            // 方块已变成空气，本帧不应再高亮旧目标。
            selection = None;
        }

        let slots = self.inventory.hotbar_slots();
        let selected_slot = self.hotbar.selected_index();
        let progress = self.mining.progress();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.hud_mut().update(delta_secs);
            renderer.set_selection(if inventory_open { None } else { selection });
            renderer
                .hud_mut()
                .set(selected_slot, slots, progress, inventory_open);
            renderer.update_particles(delta_secs);
            self.dropped.build_vertices(renderer.drop_vertices_mut());
            let viewport = renderer.viewport();
            let mut ui_vertices = Vec::new();
            self.inventory_ui
                .build(&mut ui_vertices, &self.inventory, selected_slot, viewport);
            renderer.set_ui_vertices(ui_vertices);
        }

        let outcome = match self.renderer.as_mut() {
            Some(renderer) => renderer.render(),
            None => return,
        };

        match outcome {
            RenderOutcome::Presented | RenderOutcome::Skipped => {}
            RenderOutcome::Reconfigure => {
                warn!("surface 已失效，重新配置");
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.reconfigure();
                }
            }
            RenderOutcome::Fatal(err) => {
                error!(error = %format!("{err:#}"), "渲染失败，退出");
                event_loop.exit();
            }
        }
    }
}

/// 数字键 1-9 对应的快捷栏槽位；其他按键返回 `None`。
fn hotbar_slot_for_key(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::Digit1 => Some(0),
        KeyCode::Digit2 => Some(1),
        KeyCode::Digit3 => Some(2),
        KeyCode::Digit4 => Some(3),
        KeyCode::Digit5 => Some(4),
        KeyCode::Digit6 => Some(5),
        KeyCode::Digit7 => Some(6),
        KeyCode::Digit8 => Some(7),
        KeyCode::Digit9 => Some(8),
        _ => None,
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.create_window(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                info!("收到关闭请求，退出");
                self.flush_save();
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if code == KeyCode::Escape {
                    if pressed {
                        if self.inventory_ui.is_open() {
                            self.close_inventory();
                        } else if self.cursor_grabbed {
                            self.release_cursor();
                        } else {
                            info!("按下 Esc，退出");
                            self.flush_save();
                            event_loop.exit();
                        }
                    }
                    return;
                }
                if code == KeyCode::KeyE {
                    if pressed {
                        if self.inventory_ui.is_open() {
                            self.close_inventory();
                        } else {
                            self.open_inventory();
                        }
                    }
                    return;
                }
                // 背包打开时只响应开关界面，暂停移动与视角输入。
                if self.inventory_ui.is_open() {
                    return;
                }
                // 未捕获鼠标时键盘不驱动相机，避免释放鼠标后误移动。
                if !self.cursor_grabbed {
                    return;
                }
                if let Some(slot) = hotbar_slot_for_key(code) {
                    if pressed && self.hotbar.select_slot(slot) {
                        self.persist_player();
                    }
                    return;
                }
                self.controller.handle_key(code, pressed);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.inventory_ui
                    .set_cursor(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.inventory_ui.is_open() {
                    if state == ElementState::Pressed {
                        let viewport = self
                            .renderer
                            .as_ref()
                            .map_or((1280.0, 720.0), Renderer::viewport);
                        let click = match button {
                            MouseButton::Left => Some(ClickButton::Left),
                            MouseButton::Right => Some(ClickButton::Right),
                            _ => None,
                        };
                        if let Some(click) = click {
                            let cursor = self.inventory_ui.cursor();
                            if self.inventory_ui.click_at(
                                &mut self.inventory,
                                click,
                                cursor,
                                viewport,
                            ) {
                                self.persist_player();
                            }
                        }
                    }
                    return;
                }
                if !self.cursor_grabbed {
                    // 首次点击只用于捕获鼠标，避免误触挖掘。
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.grab_cursor();
                    }
                    return;
                }
                match (button, state) {
                    // 左键按住持续挖掘，松开立即中断并清空进度。
                    (MouseButton::Left, ElementState::Pressed) => self.mining_held = true,
                    (MouseButton::Left, ElementState::Released) => {
                        self.mining_held = false;
                        self.mining.cancel();
                    }
                    // 右键只在按下瞬间放置一块，避免长按连续刷方块。
                    (MouseButton::Right, ElementState::Pressed) => self.place_block(),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !self.cursor_grabbed && !self.inventory_ui.is_open() {
                    return;
                }
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                let before = self.hotbar.selected_index();
                self.hotbar.scroll(amount);
                if before != self.hotbar.selected_index() {
                    self.persist_player();
                }
            }
            WindowEvent::Focused(false) => {
                if self.cursor_grabbed {
                    warn!("窗口失去焦点，释放鼠标捕获");
                    self.release_cursor();
                } else {
                    self.controller.reset();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                self.next_frame = Instant::now();
            }
            WindowEvent::RedrawRequested => {
                let stats = self.timer.tick();
                let frame_start = Instant::now();
                self.draw_frame(stats.delta, event_loop);

                if let Some(fps) = stats.fps {
                    let player = self.player.position();
                    let player_chunk =
                        ChunkPos::from_world(player.x.floor() as i32, 0, player.z.floor() as i32);
                    let gpu_chunks = self
                        .renderer
                        .as_ref()
                        .map_or(0, crate::renderer::Renderer::chunk_count);
                    info!(
                        fps = f64::from(fps),
                        delta_ms = stats.delta.as_secs_f64() * 1000.0,
                        player_x = player.x,
                        player_y = player.y,
                        player_z = player.z,
                        player_vy = self.player.velocity().y,
                        player_ay = self.player.acceleration().y,
                        player_grounded = self.player.is_grounded(),
                        player_state = ?self.player.movement_state(),
                        physics_steps = self.physics_steps,
                        chunk_x = player_chunk.x,
                        chunk_z = player_chunk.z,
                        loaded_chunks = self.world.chunk_count(),
                        gpu_chunks,
                        pending_generation = self.chunk_manager.pending_generation(),
                        pending_meshes = self.chunk_manager.pending_mesh(),
                        generation_ms = self.generation_ms,
                        mesh_ms = self.mesh_ms,
                        selected_hotbar = self.hotbar.selected_index(),
                        inventory_open = self.inventory_ui.is_open(),
                        dropped_items = self.dropped.len(),
                        mining_active = self.mining.is_mining(),
                        mining_target = ?self.mining.target(),
                        mining_progress = self.mining.progress(),
                        mining_elapsed = self.mining.elapsed_time(),
                        mining_required = self.mining.required_time(),
                        "帧率统计"
                    );
                }
                // 以帧起点为基准排下一帧，避免与 vsync 叠加成双重限帧。
                self.next_frame = frame_start + FRAME_INTERVAL;
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if !self.cursor_grabbed {
            return;
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            self.controller.add_look(delta.0 as f32, delta.1 as f32);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
    }
}

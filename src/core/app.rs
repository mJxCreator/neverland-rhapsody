//! 应用主体：窗口创建与主循环。

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{IVec3, Vec3};
use tracing::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use super::config::AppConfig;
use super::time::FrameTimer;
use crate::interaction::raycast::{placement_target, raycast, RayHit};
use crate::player::PlayerController;
use crate::renderer::{spawn_position, RenderOutcome, Renderer};
use crate::save::{BlockEdit, WorldSave};
use crate::world::{Block, ChunkConfig, ChunkManager, ChunkPos, World};

/// 目标帧间隔，约 60 FPS。
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// 单帧参与摄像机更新的最大时长，避免窗口卡顿后位置跳变。
const MAX_CAMERA_DELTA: f32 = 0.05;
/// 准星射线长度：超出该距离的方块不会被选中。
const SELECTION_REACH: f32 = 8.0;
/// 右键放置的方块类型。
const PLACE_BLOCK: Block = Block::Grass;
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

        Self {
            config,
            world,
            save,
            chunk_manager: ChunkManager::new(ChunkConfig::default()),
            window: None,
            renderer: None,
            controller: PlayerController::new(),
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
                let spawn = spawn_position(&self.world);
                let player_chunk =
                    ChunkPos::from_world(spawn.x.floor() as i32, 0, spawn.z.floor() as i32);
                let loaded =
                    self.chunk_manager
                        .preload(player_chunk, &mut self.world, &mut |pos, world| {
                            self.save.apply_chunk(pos, world)
                        });
                info!(chunks = self.world.chunk_count(), loaded, "初始区块已加载");

                match Renderer::new(window.clone(), window.inner_size(), &self.world) {
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
        self.controller.reset();
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

    /// 摄像机所在方块坐标，用于阻止把方块放进自己所在的位置。
    fn camera_cell(&self) -> Option<IVec3> {
        let position = self.renderer.as_ref()?.camera().position();
        Some(IVec3::new(
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        ))
    }

    /// 左键破坏：把准星选中的方块变成空气。
    fn break_block(&mut self) {
        let Some(hit) = self.targeted_block() else {
            return;
        };
        let affected = self
            .world
            .set_block(hit.block.x, hit.block.y, hit.block.z, Block::Air);
        if !affected.is_empty() {
            info!(block = ?hit.block, "方块已破坏");
            self.remember_edit(BlockEdit {
                x: hit.block.x,
                y: hit.block.y,
                z: hit.block.z,
                block: Block::Air,
            });
        }
        self.rebuild_chunks(affected);
    }

    /// 右键放置：在命中面外侧的空格放下一块草方块。
    fn place_block(&mut self) {
        let Some(hit) = self.targeted_block() else {
            return;
        };
        let target = placement_target(&hit);
        if !self.world.contains(target.x, target.y, target.z) {
            return;
        }
        if self.world.block_at(target.x, target.y, target.z).is_solid() {
            return;
        }
        if self.camera_cell() == Some(target) {
            return;
        }
        let affected = self
            .world
            .set_block(target.x, target.y, target.z, PLACE_BLOCK);
        if !affected.is_empty() {
            info!(block = ?target, "方块已放置");
            self.remember_edit(BlockEdit {
                x: target.x,
                y: target.y,
                z: target.z,
                block: PLACE_BLOCK,
            });
        }
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
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let position = renderer.camera().position();
        let player = ChunkPos::from_world(position.x.floor() as i32, 0, position.z.floor() as i32);

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
        let update = self.chunk_manager.update(
            player,
            &mut self.world,
            &mut |pos, world| self.save.apply_chunk(pos, world),
            &mut |_| true,
        );
        self.generation_ms = start.elapsed().as_secs_f64() * 1000.0;
        if !update.loaded.is_empty() || !update.unloaded.is_empty() {
            debug!(
                loaded = update.loaded.len(),
                unloaded = update.unloaded.len(),
                "区块生命周期更新"
            );
        }

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // 先释放已卸载区块的 GPU 资源，再补建本帧允许的网格。
        renderer.remove_chunks(&update.unloaded);

        let start = Instant::now();
        let meshes = self.chunk_manager.take_meshes(player, &self.world);
        renderer.update_chunks(&self.world, &meshes);
        self.mesh_ms = start.elapsed().as_secs_f64() * 1000.0;
    }

    /// 退出前确保内存中的方块改动已经落盘。
    fn flush_save(&mut self) {
        if let Err(err) = self.save.ensure_persisted() {
            warn!(error = %format!("{err:#}"), "退出前写入世界存档失败");
        }
    }

    /// 把一次改动写入存档；失败时只记录日志，不影响继续游戏。
    fn remember_edit(&mut self, edit: BlockEdit) {
        if let Err(err) = self.save.record(edit) {
            warn!(error = %format!("{err:#}"), "世界存档写入失败");
        }
    }

    fn draw_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        let delta_secs = delta.as_secs_f32().min(MAX_CAMERA_DELTA);
        if let Some(renderer) = self.renderer.as_mut() {
            self.controller.update(renderer.camera_mut(), delta_secs);
        }

        // 摄像机移动后先补齐区块，再让射线检测基于最新的已加载世界。
        self.stream_chunks();

        let hit = self.targeted_block();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_selection(hit.map(|hit| hit.block));
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
                        if self.cursor_grabbed {
                            self.release_cursor();
                        } else {
                            info!("按下 Esc，退出");
                            self.flush_save();
                            event_loop.exit();
                        }
                    }
                    return;
                }
                // 未捕获鼠标时键盘不驱动相机，避免释放鼠标后误移动。
                if !self.cursor_grabbed {
                    return;
                }
                self.controller.handle_key(code, pressed);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if !self.cursor_grabbed {
                    // 首次点击只用于捕获鼠标，避免误触挖掘。
                    if button == MouseButton::Left {
                        self.grab_cursor();
                    }
                    return;
                }
                match button {
                    MouseButton::Left => self.break_block(),
                    MouseButton::Right => self.place_block(),
                    _ => {}
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
                    let (player, player_chunk, gpu_chunks) = match self.renderer.as_ref() {
                        Some(renderer) => {
                            let position = renderer.camera().position();
                            (
                                position,
                                ChunkPos::from_world(
                                    position.x.floor() as i32,
                                    0,
                                    position.z.floor() as i32,
                                ),
                                renderer.chunk_count(),
                            )
                        }
                        None => (Vec3::ZERO, ChunkPos::new(0, 0, 0), 0),
                    };
                    info!(
                        fps = f64::from(fps),
                        delta_ms = stats.delta.as_secs_f64() * 1000.0,
                        player_x = player.x,
                        player_y = player.y,
                        player_z = player.z,
                        chunk_x = player_chunk.x,
                        chunk_z = player_chunk.z,
                        loaded_chunks = self.world.chunk_count(),
                        gpu_chunks,
                        pending_generation = self.chunk_manager.pending_generation(),
                        pending_meshes = self.chunk_manager.pending_mesh(),
                        generation_ms = self.generation_ms,
                        mesh_ms = self.mesh_ms,
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

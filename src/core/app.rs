//! 应用主体：窗口创建与主循环。

use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use super::config::AppConfig;
use super::time::FrameTimer;
use crate::interaction::raycast::raycast;
use crate::player::PlayerController;
use crate::renderer::{RenderOutcome, Renderer};
use crate::world::World;

/// 目标帧间隔，约 60 FPS。
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// 单帧参与摄像机更新的最大时长，避免窗口卡顿后位置跳变。
const MAX_CAMERA_DELTA: f32 = 0.05;
/// 准星射线长度：超出该距离的方块不会被选中。
const SELECTION_REACH: f32 = 8.0;

/// 持有窗口、渲染器与主循环状态。
pub struct App {
    config: AppConfig,
    world: World,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    controller: PlayerController,
    cursor_grabbed: bool,
    timer: FrameTimer,
    next_frame: Instant,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            world: World::default(),
            window: None,
            renderer: None,
            controller: PlayerController::new(),
            cursor_grabbed: false,
            timer: FrameTimer::new(),
            next_frame: Instant::now(),
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

    fn draw_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        if let Some(renderer) = self.renderer.as_mut() {
            self.controller.update(
                renderer.camera_mut(),
                delta.as_secs_f32().min(MAX_CAMERA_DELTA),
            );

            let camera = renderer.camera_mut();
            let hit = raycast(
                &self.world,
                camera.position(),
                camera.forward(),
                SELECTION_REACH,
            );
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
                            event_loop.exit();
                        }
                    }
                    return;
                }
                self.controller.handle_key(code, pressed);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !self.cursor_grabbed => self.grab_cursor(),
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
                    info!(
                        fps = f64::from(fps),
                        delta_ms = stats.delta.as_secs_f64() * 1000.0,
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

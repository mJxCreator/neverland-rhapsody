//! 应用主体：窗口创建与主循环。

use std::time::{Duration, Instant};

use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use super::config::AppConfig;
use super::time::FrameTimer;

/// 目标帧间隔，约 60 FPS。
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

/// 持有窗口与主循环状态。
pub struct App {
    config: AppConfig,
    window: Option<Window>,
    timer: FrameTimer,
    next_frame: Instant,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            window: None,
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
                self.window = Some(window);
                self.next_frame = Instant::now();
            }
            Err(err) => {
                error!(error = %err, "窗口创建失败");
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
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                info!("按下 Esc，退出");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let stats = self.timer.tick();
                if let Some(fps) = stats.fps {
                    info!(
                        fps = f64::from(fps),
                        delta_ms = stats.delta.as_secs_f64() * 1000.0,
                        "帧率统计"
                    );
                }
                self.next_frame = Instant::now() + FRAME_INTERVAL;
            }
            _ => {}
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

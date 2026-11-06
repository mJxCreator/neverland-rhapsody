//! wgpu 渲染器：初始化 GPU 资源并逐帧绘制示例场景。

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use glam::{Mat4, Vec3};
use tracing::info;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::camera::Camera;
use super::pipeline::{self, Vertex, DEPTH_FORMAT};

/// 天空清屏色。
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.53,
    g: 0.71,
    b: 0.94,
    a: 1.0,
};

/// 示例方块：每面一个颜色，便于观察 3D 空间关系。
const CUBE_VERTICES: [Vertex; 24] = [
    // +X
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.52, 0.41, 0.29],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [0.52, 0.41, 0.29],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [0.52, 0.41, 0.29],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.52, 0.41, 0.29],
    },
    // -X
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [0.46, 0.36, 0.26],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.46, 0.36, 0.26],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.46, 0.36, 0.26],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.46, 0.36, 0.26],
    },
    // +Y（顶面）
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.36, 0.62, 0.30],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [0.36, 0.62, 0.30],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.36, 0.62, 0.30],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.36, 0.62, 0.30],
    },
    // -Y（底面）
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.32, 0.25, 0.19],
    },
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.32, 0.25, 0.19],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [0.32, 0.25, 0.19],
    },
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [0.32, 0.25, 0.19],
    },
    // +Z
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [0.55, 0.43, 0.31],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [0.55, 0.43, 0.31],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [0.55, 0.43, 0.31],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.55, 0.43, 0.31],
    },
    // -Z
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.48, 0.38, 0.27],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.48, 0.38, 0.27],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.48, 0.38, 0.27],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.48, 0.38, 0.27],
    },
];

const CUBE_INDICES: [u16; 36] = [
    0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17, 18,
    16, 18, 19, 20, 21, 22, 20, 22, 23,
];

/// 单帧渲染结果，决定上层如何继续。
pub enum RenderOutcome {
    /// 已提交并呈现。
    Presented,
    /// 本帧跳过（窗口被遮挡或获取超时）。
    Skipped,
    /// surface 需要重新配置。
    Reconfigure,
    /// 无法恢复的错误。
    Fatal(anyhow::Error),
}

/// 持有 GPU 资源并负责绘制。
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    camera: Camera,
    elapsed: f32,
}

impl Renderer {
    /// 创建 surface/device 与绘制资源；失败时返回错误由上层退出。
    pub fn new(window: Arc<Window>, size: PhysicalSize<u32>) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window)?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))?;

        let adapter_info = adapter.get_info();
        info!(gpu = %adapter_info.name, backend = ?adapter_info.backend, "GPU 适配器已就绪");

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("neverland-rhapsody device"),
                ..Default::default()
            }))?;

        let width = size.width.max(1);
        let height = size.height.max(1);
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| anyhow!("surface 不支持任何可用配置"))?;
        surface.configure(&device, &config);

        let depth_view = create_depth_view(&device, &config);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline = pipeline::create_pipeline(&device, config.format, &bind_group_layout);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck::cast_slice(&CUBE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: bytemuck::cast_slice(&CUBE_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene uniforms"),
            size: std::mem::size_of::<[[f32; 4]; 4]>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let camera = Camera::new(Vec3::new(2.2, 1.8, 3.2), width as f32 / height as f32);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth_view,
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            bind_group,
            camera,
            elapsed: 0.0,
        })
    }

    /// 视口变化后重建交换链配置与深度附件。
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, &self.config);
        self.camera
            .set_aspect(size.width as f32 / size.height as f32);
    }

    /// 按当前配置重新配置 surface。
    pub fn reconfigure(&self) {
        self.surface.configure(&self.device, &self.config);
    }

    /// 绘制一帧。`delta` 用于推进示例方块的旋转。
    pub fn render(&mut self, delta: Duration) -> RenderOutcome {
        self.elapsed += delta.as_secs_f32();

        let model =
            Mat4::from_rotation_y(self.elapsed * 0.6) * Mat4::from_rotation_x(self.elapsed * 0.25);
        let mvp = (self.camera.view_proj() * model).to_cols_array_2d();
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&mvp));

        let (surface_texture, needs_reconfigure) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return RenderOutcome::Skipped;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return RenderOutcome::Reconfigure;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return RenderOutcome::Fatal(anyhow!("获取 surface 纹理时触发验证错误"));
            }
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..CUBE_INDICES.len() as u32, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(surface_texture);

        if needs_reconfigure {
            self.reconfigure();
        }

        RenderOutcome::Presented
    }
}

fn create_depth_view(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

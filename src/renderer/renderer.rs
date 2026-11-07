//! wgpu 渲染器：初始化 GPU 资源并逐帧绘制区块网格、破坏粒子、选中线框与 HUD。

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use glam::{IVec3, Mat4, Vec3};
use tracing::{debug, info};
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::camera::Camera;
use super::hud::{HudModel, HudVertex};
use super::particles::ParticleSystem;
use super::pipeline::{self, DEPTH_FORMAT};
use crate::mesh::{build_chunk_mesh, ChunkMesh, Vertex};
use crate::world::{Block, ChunkPos, World};

/// 天空清屏色。
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.53,
    g: 0.71,
    b: 0.94,
    a: 1.0,
};

/// 出生点在世界平面上的位置：初始区域中央偏后，面向 -Z 观察山地。
const SPAWN_X: f32 = 48.0;
const SPAWN_Z: f32 = 84.0;
/// 出生时的俯仰角，略微俯视。
const SPAWN_PITCH: f32 = -0.45;

/// 出生姿态：玩家双脚位置、偏航角与俯仰角。
///
/// 双脚落在地表高度上，物理系统接管后由重力与碰撞保持贴地。
pub fn spawn_pose(world: &World) -> (Vec3, f32, f32) {
    let feet = Vec3::new(
        SPAWN_X,
        world.height_at(SPAWN_X as i32, SPAWN_Z as i32) as f32,
        SPAWN_Z,
    );
    (feet, 0.0, SPAWN_PITCH)
}

/// 选中线框的棱数（立方体 12 条棱），每条棱两个顶点。
const SELECTION_EDGE_COUNT: usize = 12;
/// 选中线框的顶点总数。
const SELECTION_VERTEX_COUNT: u32 = (SELECTION_EDGE_COUNT * 2) as u32;
/// 线框相对方块表面的外扩量，避免与方块面共面导致闪烁。
const SELECTION_EXPAND: f32 = 0.003;

/// HUD 动态顶点缓冲容量；正常布局约 200 个顶点，保留充足余量。
const HUD_VERTEX_CAPACITY: usize = 1024;
/// 破坏粒子动态顶点缓冲容量（约 100 个碎块）。
const PARTICLE_VERTEX_CAPACITY: usize = 4096;

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

/// 一个区块的 GPU 顶点与索引缓冲。
struct ChunkBuffers {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// 持有 GPU 资源，并把世界中的区块网格提交绘制。
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    selection_pipeline: wgpu::RenderPipeline,
    selection_buffer: wgpu::Buffer,
    selection: Option<IVec3>,
    hud_pipeline: wgpu::RenderPipeline,
    hud_buffer: wgpu::Buffer,
    hud_model: HudModel,
    hud_vertices: Vec<HudVertex>,
    particle_buffer: wgpu::Buffer,
    particles: ParticleSystem,
    particle_vertices: Vec<Vertex>,
    chunks: HashMap<ChunkPos, ChunkBuffers>,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    camera: Camera,
}

impl Renderer {
    /// 创建 surface/device 与绘制资源；`world` 提供待渲染的区块。
    ///
    /// 摄像机初始位置与朝向来自玩家实体，由上层按固定步长物理结果持续同步。
    pub fn new(
        window: Arc<Window>,
        size: PhysicalSize<u32>,
        world: &World,
        eye_position: Vec3,
        yaw: f32,
        pitch: f32,
    ) -> Result<Self> {
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
        let selection_pipeline =
            pipeline::create_selection_pipeline(&device, config.format, &bind_group_layout);
        let hud_pipeline = pipeline::create_hud_pipeline(&device, config.format);
        let hud_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hud vertices"),
            size: (HUD_VERTEX_CAPACITY * std::mem::size_of::<HudVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let particle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle vertices"),
            size: (PARTICLE_VERTEX_CAPACITY * std::mem::size_of::<Vertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let selection_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("selection vertices"),
            size: (SELECTION_VERTEX_COUNT as usize * std::mem::size_of::<[f32; 3]>())
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let chunks = upload_chunk_meshes(&device, world);

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

        let camera = Camera::new(eye_position, yaw, pitch, width as f32 / height as f32);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth_view,
            pipeline,
            selection_pipeline,
            selection_buffer,
            selection: None,
            hud_pipeline,
            hud_buffer,
            hud_model: HudModel::default(),
            hud_vertices: Vec::with_capacity(HUD_VERTEX_CAPACITY),
            particle_buffer,
            particles: ParticleSystem::new(),
            particle_vertices: Vec::with_capacity(PARTICLE_VERTEX_CAPACITY),
            chunks,
            uniform_buffer,
            bind_group,
            camera,
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

    /// 供输入控制器逐帧更新摄像机姿态。
    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    /// 供上层读取摄像机位置与朝向。
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// 更新准星选中的方块，`None` 表示当前没有可高亮的方块。
    pub fn set_selection(&mut self, block: Option<IVec3>) {
        self.selection = block;
    }

    /// 上层每帧写入 HUD 状态（快捷栏选择、挖掘进度等）。
    pub fn hud_mut(&mut self) -> &mut HudModel {
        &mut self.hud_model
    }

    /// 在指定方块位置生成破坏碎块。
    pub fn spawn_block_particles(&mut self, block: Block, block_pos: IVec3) {
        self.particles.spawn_block(block, block_pos);
    }

    /// 推进破坏粒子的生命周期。
    pub fn update_particles(&mut self, delta_secs: f32) {
        self.particles.update(delta_secs);
    }

    /// 方块被修改后，重建受影响区块的网格并替换对应的 GPU 缓冲。
    pub fn update_chunks(&mut self, world: &World, positions: &[ChunkPos]) {
        for &pos in positions {
            let mesh = match world.chunk_at(pos) {
                Some(chunk) => build_chunk_mesh(chunk, |x, y, z| world.block_at(x, y, z)),
                None => {
                    // 区块已卸载，释放对应 GPU 资源。
                    self.chunks.remove(&pos);
                    continue;
                }
            };
            match chunk_buffers(&self.device, &mesh) {
                Some(buffers) => {
                    self.chunks.insert(pos, buffers);
                }
                None => {
                    // 区块已被挖空，移除其绘制缓冲。
                    self.chunks.remove(&pos);
                }
            }
        }
    }

    /// 释放已卸载区块的 GPU 缓冲。
    pub fn remove_chunks(&mut self, positions: &[ChunkPos]) {
        for pos in positions {
            self.chunks.remove(pos);
        }
    }

    /// 当前持有 GPU 网格的区块数量，用于调试统计。
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// 绘制一帧。
    pub fn render(&mut self) -> RenderOutcome {
        let view_proj: Mat4 = self.camera.view_proj();
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&view_proj.to_cols_array_2d()),
        );
        if let Some(block) = self.selection {
            self.queue.write_buffer(
                &self.selection_buffer,
                0,
                bytemuck::cast_slice(&selection_vertices(block)),
            );
        }

        self.particles.build_vertices(&mut self.particle_vertices);
        let particle_count = self.particle_vertices.len().min(PARTICLE_VERTEX_CAPACITY) as u32;
        if particle_count > 0 {
            self.queue.write_buffer(
                &self.particle_buffer,
                0,
                bytemuck::cast_slice(&self.particle_vertices[..particle_count as usize]),
            );
        }

        self.hud_model.build(
            &mut self.hud_vertices,
            self.config.width as f32,
            self.config.height as f32,
        );
        let hud_count = self.hud_vertices.len().min(HUD_VERTEX_CAPACITY) as u32;
        if hud_count > 0 {
            self.queue.write_buffer(
                &self.hud_buffer,
                0,
                bytemuck::cast_slice(&self.hud_vertices[..hud_count as usize]),
            );
        }

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
            for chunk in self.chunks.values() {
                render_pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            }

            if particle_count > 0 {
                render_pass.set_pipeline(&self.pipeline);
                render_pass.set_bind_group(0, &self.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.particle_buffer.slice(..));
                render_pass.draw(0..particle_count, 0..1);
            }

            if self.selection.is_some() {
                render_pass.set_pipeline(&self.selection_pipeline);
                render_pass.set_bind_group(0, &self.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.selection_buffer.slice(..));
                render_pass.draw(0..SELECTION_VERTEX_COUNT, 0..1);
            }

            if hud_count > 0 {
                render_pass.set_pipeline(&self.hud_pipeline);
                render_pass.set_vertex_buffer(0, self.hud_buffer.slice(..));
                render_pass.draw(0..hud_count, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(surface_texture);

        if needs_reconfigure {
            self.reconfigure();
        }

        RenderOutcome::Presented
    }
}

/// 把每个非空区块的网格上传为顶点/索引缓冲。
fn upload_chunk_meshes(device: &wgpu::Device, world: &World) -> HashMap<ChunkPos, ChunkBuffers> {
    world
        .chunks()
        .filter_map(|chunk| {
            let mesh = build_chunk_mesh(chunk, |x, y, z| world.block_at(x, y, z));
            let buffers = chunk_buffers(device, &mesh)?;

            debug!(
                chunk = ?chunk.pos(),
                vertices = mesh.vertices.len(),
                triangles = mesh.indices.len() / 3,
                "区块网格已上传"
            );
            Some((chunk.pos(), buffers))
        })
        .collect()
}

/// 为一份网格创建 GPU 缓冲；空网格返回 `None`。
fn chunk_buffers(device: &wgpu::Device, mesh: &ChunkMesh) -> Option<ChunkBuffers> {
    if mesh.is_empty() {
        return None;
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("chunk vertices"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("chunk indices"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    Some(ChunkBuffers {
        vertex_buffer,
        index_buffer,
        index_count: mesh.indices.len() as u32,
    })
}

/// 立方体的 12 条棱，用顶点下标成对表示。
const CUBE_EDGES: [(usize, usize); SELECTION_EDGE_COUNT] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// 生成选中方块的线框顶点（12 条棱 × 2 个端点），位置为世界坐标。
fn selection_vertices(block: IVec3) -> [[f32; 3]; SELECTION_EDGE_COUNT * 2] {
    let min = block.as_vec3() - Vec3::splat(SELECTION_EXPAND);
    let max = min + Vec3::splat(1.0 + SELECTION_EXPAND * 2.0);
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];

    let mut vertices = [[0.0; 3]; SELECTION_EDGE_COUNT * 2];
    for (index, (start, end)) in CUBE_EDGES.iter().enumerate() {
        vertices[index * 2] = corners[*start].to_array();
        vertices[index * 2 + 1] = corners[*end].to_array();
    }
    vertices
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_outline_wraps_the_whole_block() {
        let block = IVec3::new(2, 3, 4);
        let vertices = selection_vertices(block);

        assert_eq!(vertices.len(), SELECTION_VERTEX_COUNT as usize);
        for [x, y, z] in vertices {
            for (value, axis_min) in [(x, 2.0), (y, 3.0), (z, 4.0)] {
                // 允许 f32 舍入误差。
                let lower = axis_min - SELECTION_EXPAND - 1e-4;
                let upper = axis_min + 1.0 + SELECTION_EXPAND + 1e-4;
                assert!((lower..=upper).contains(&value), "坐标越界: {value}");
            }
        }
    }
}

//! 模型 GPU 资源管理：纹理绑定组、静态网格缓存、动态蒙皮顶点与实例矩阵。
//!
//! 每帧只更新实例矩阵（256 字节对齐的动态偏移）与蒙皮顶点；
//! 静态网格按（模型、网格、图元）缓存，多个实例共享同一份 GPU 缓冲。

use std::collections::HashMap;

use wgpu::util::DeviceExt;

use super::pipeline::{create_model_bind_group_layout, create_model_pipeline, ModelPipelineKind};
use crate::model::{AlphaMode, ModelAsset, ModelDraw, ModelGeometry, ModelId, ModelVertex};
use crate::texture::{upload_texture_image, GpuTexture};

/// 每个实例的参数块大小：4×4 矩阵（64 字节）+ 参数向量（16 字节）。
const MODEL_UNIFORM_SIZE: u64 = 80;
/// 动态 uniform 偏移对齐；WebGPU 保证 256 字节。
const UNIFORM_SLOT_SIZE: usize = 256;
const INITIAL_UNIFORM_SLOTS: usize = 64;
const INITIAL_DYNAMIC_VERTICES: usize = 16_384;

struct StaticMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

#[derive(Clone, Copy)]
enum PreparedGeometry {
    Static {
        mesh: usize,
        primitive: usize,
    },
    Dynamic {
        first_vertex: u32,
        vertex_count: u32,
    },
}

struct PreparedDraw {
    model: ModelId,
    material: usize,
    alpha: AlphaMode,
    uniform_offset: u32,
    geometry: PreparedGeometry,
    /// 第一人称覆盖层批次，绘制时关闭深度测试。
    overlay: bool,
}

/// 模型渲染资源与批次。
pub struct ModelRenderer {
    pipeline_opaque: wgpu::RenderPipeline,
    pipeline_mask: wgpu::RenderPipeline,
    pipeline_blend: wgpu::RenderPipeline,
    pipeline_overlay: wgpu::RenderPipeline,
    model_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    uniform_slots: usize,
    textures: HashMap<(ModelId, usize), GpuTexture>,
    bind_groups: HashMap<(ModelId, usize), wgpu::BindGroup>,
    white_texture: GpuTexture,
    white_bind_group: wgpu::BindGroup,
    static_meshes: HashMap<(ModelId, usize, usize), StaticMesh>,
    dynamic_buffer: wgpu::Buffer,
    dynamic_capacity: usize,
    dynamic_vertices: Vec<ModelVertex>,
    draws: Vec<ModelDraw>,
    overlay_draws: Vec<ModelDraw>,
    prepared: Vec<PreparedDraw>,
}

impl ModelRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        scene_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let model_layout = create_model_bind_group_layout(device);
        let pipeline_opaque = create_model_pipeline(
            device,
            format,
            scene_layout,
            &model_layout,
            ModelPipelineKind::Opaque,
        );
        let pipeline_mask = create_model_pipeline(
            device,
            format,
            scene_layout,
            &model_layout,
            ModelPipelineKind::Mask,
        );
        let pipeline_blend = create_model_pipeline(
            device,
            format,
            scene_layout,
            &model_layout,
            ModelPipelineKind::Blend,
        );
        let pipeline_overlay = create_model_pipeline(
            device,
            format,
            scene_layout,
            &model_layout,
            ModelPipelineKind::Overlay,
        );
        let uniform_buffer = create_uniform_buffer(device, INITIAL_UNIFORM_SLOTS);
        let white_texture = upload_texture_image(
            device,
            queue,
            &crate::texture::types::TextureImage::new(1, 1, vec![255; 4]).expect("1×1 白色纹理"),
            "model white texture",
        );
        let white_bind_group =
            create_texture_bind_group(device, &model_layout, &uniform_buffer, &white_texture);
        let dynamic_buffer = create_vertex_buffer(device, INITIAL_DYNAMIC_VERTICES);
        Self {
            pipeline_opaque,
            pipeline_mask,
            pipeline_blend,
            pipeline_overlay,
            model_layout,
            uniform_buffer,
            uniform_slots: INITIAL_UNIFORM_SLOTS,
            textures: HashMap::new(),
            bind_groups: HashMap::new(),
            white_texture,
            white_bind_group,
            static_meshes: HashMap::new(),
            dynamic_buffer,
            dynamic_capacity: INITIAL_DYNAMIC_VERTICES,
            dynamic_vertices: Vec::new(),
            draws: Vec::new(),
            overlay_draws: Vec::new(),
            prepared: Vec::new(),
        }
    }

    /// 上传模型的纹理与静态网格；重复调用会替换该模型旧资源（用于热重载）。
    pub fn upload_model(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: ModelId,
        asset: &ModelAsset,
    ) {
        self.textures.retain(|(model, _), _| *model != id);
        self.bind_groups.retain(|(model, _), _| *model != id);
        self.static_meshes.retain(|(model, _, _), _| *model != id);
        // 绑定组按材质下标索引：同一材质可复用纹理图，也可能完全没有贴图。
        for (material_index, material) in asset.materials.iter().enumerate() {
            let Some(texture_index) = material.base_color_texture else {
                continue;
            };
            let Some(texture) = asset.textures.get(texture_index) else {
                continue;
            };
            let gpu = upload_texture_image(
                device,
                queue,
                &texture.image,
                &format!("model {id:?} material {material_index} texture"),
            );
            let bind_group =
                create_texture_bind_group(device, &self.model_layout, &self.uniform_buffer, &gpu);
            self.bind_groups.insert((id, material_index), bind_group);
            self.textures.insert((id, material_index), gpu);
        }
        for (mesh_index, mesh) in asset.meshes.iter().enumerate() {
            for (primitive_index, primitive) in mesh.primitives.iter().enumerate() {
                if primitive.indices.is_empty() {
                    continue;
                }
                if primitive.is_skinned() && asset.skeleton.is_some() {
                    continue;
                }
                let material = asset
                    .materials
                    .get(primitive.material)
                    .cloned()
                    .unwrap_or_default();
                let vertices = ModelVertex::from_primitive(primitive, material.base_color);
                let static_mesh = StaticMesh {
                    vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("model static vertex buffer"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("model static index buffer"),
                        contents: bytemuck::cast_slice(&primitive.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    index_count: primitive.indices.len() as u32,
                };
                self.static_meshes
                    .insert((id, mesh_index, primitive_index), static_mesh);
            }
        }
    }

    /// 设置本帧模型绘制批次；真正的 GPU 写入在 [`Self::prepare`] 中完成。
    pub fn set_draws(&mut self, draws: Vec<ModelDraw>) {
        self.draws = draws;
    }

    /// 设置第一人称覆盖层绘制批次；这些批次在最后绘制且不参与深度测试。
    pub fn set_overlay_draws(&mut self, draws: Vec<ModelDraw>) {
        self.overlay_draws = draws;
    }

    /// 写入实例矩阵与动态蒙皮顶点；普通批次与第一人称覆盖层共用同一块动态缓冲。
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.prepared.clear();
        self.dynamic_vertices.clear();
        let regular_count = self.draws.len();
        let total_count = regular_count + self.overlay_draws.len();
        if total_count == 0 {
            return;
        }
        self.ensure_uniform_capacity(device, total_count);
        let mut uniform_bytes = vec![0u8; total_count * UNIFORM_SLOT_SIZE];
        for (slot, draw) in self
            .draws
            .iter()
            .chain(self.overlay_draws.iter())
            .enumerate()
        {
            let base = slot * UNIFORM_SLOT_SIZE;
            let matrix = draw.transform.to_cols_array();
            uniform_bytes[base..base + 64].copy_from_slice(bytemuck::cast_slice(&matrix));
            uniform_bytes[base + 64..base + 68].copy_from_slice(&draw.alpha.cutoff().to_le_bytes());
            // params.y = 受击闪红强度，其余分量保留给后续效果。
            uniform_bytes[base + 68..base + 72].copy_from_slice(&draw.flash.to_le_bytes());
        }
        queue.write_buffer(&self.uniform_buffer, 0, &uniform_bytes);

        let total_vertices: usize = self
            .draws
            .iter()
            .chain(self.overlay_draws.iter())
            .map(|draw| match &draw.geometry {
                ModelGeometry::Dynamic(vertices) => vertices.len(),
                ModelGeometry::Static { .. } => 0,
            })
            .sum();
        self.ensure_dynamic_capacity(device, total_vertices);
        for (slot, draw) in self
            .draws
            .iter()
            .chain(self.overlay_draws.iter())
            .enumerate()
        {
            let geometry = match &draw.geometry {
                ModelGeometry::Static { mesh, primitive } => PreparedGeometry::Static {
                    mesh: *mesh,
                    primitive: *primitive,
                },
                ModelGeometry::Dynamic(vertices) => {
                    let first_vertex = self.dynamic_vertices.len() as u32;
                    self.dynamic_vertices.extend_from_slice(vertices);
                    PreparedGeometry::Dynamic {
                        first_vertex,
                        vertex_count: vertices.len() as u32,
                    }
                }
            };
            self.prepared.push(PreparedDraw {
                model: draw.model,
                material: draw.material,
                alpha: draw.alpha,
                uniform_offset: (slot * UNIFORM_SLOT_SIZE) as u32,
                geometry,
                overlay: slot >= regular_count,
            });
        }
        if !self.dynamic_vertices.is_empty() {
            queue.write_buffer(
                &self.dynamic_buffer,
                0,
                bytemuck::cast_slice(&self.dynamic_vertices),
            );
        }
    }

    /// 在已有渲染通道中绘制普通模型批次（参与深度测试）。
    pub fn draw<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        scene_bind_group: &'pass wgpu::BindGroup,
    ) {
        let mut current_pipeline: Option<ModelPipelineKind> = None;
        for draw in self.prepared.iter().filter(|draw| !draw.overlay) {
            let kind = if draw.alpha.is_blend() {
                ModelPipelineKind::Blend
            } else if draw.alpha.is_mask() {
                ModelPipelineKind::Mask
            } else {
                ModelPipelineKind::Opaque
            };
            if current_pipeline != Some(kind) {
                let pipeline = match kind {
                    ModelPipelineKind::Opaque => &self.pipeline_opaque,
                    ModelPipelineKind::Mask => &self.pipeline_mask,
                    ModelPipelineKind::Blend => &self.pipeline_blend,
                    ModelPipelineKind::Overlay => &self.pipeline_overlay,
                };
                render_pass.set_pipeline(pipeline);
                current_pipeline = Some(kind);
            }
            self.draw_prepared(render_pass, scene_bind_group, draw);
        }
    }

    /// 绘制第一人称覆盖层：关闭深度测试与深度写入，避免被世界几何遮挡。
    pub fn draw_overlay<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        scene_bind_group: &'pass wgpu::BindGroup,
    ) {
        let mut pipeline_set = false;
        for draw in self.prepared.iter().filter(|draw| draw.overlay) {
            if !pipeline_set {
                render_pass.set_pipeline(&self.pipeline_overlay);
                pipeline_set = true;
            }
            self.draw_prepared(render_pass, scene_bind_group, draw);
        }
    }

    fn draw_prepared<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        scene_bind_group: &'pass wgpu::BindGroup,
        draw: &PreparedDraw,
    ) {
        render_pass.set_bind_group(0, scene_bind_group, &[]);
        let texture_bind_group = self
            .bind_groups
            .get(&(draw.model, draw.material))
            .unwrap_or(&self.white_bind_group);
        render_pass.set_bind_group(1, texture_bind_group, &[draw.uniform_offset]);
        match draw.geometry {
            PreparedGeometry::Static { mesh, primitive } => {
                if let Some(static_mesh) = self.static_meshes.get(&(draw.model, mesh, primitive)) {
                    render_pass.set_vertex_buffer(0, static_mesh.vertex_buffer.slice(..));
                    render_pass.set_index_buffer(
                        static_mesh.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    render_pass.draw_indexed(0..static_mesh.index_count, 0, 0..1);
                }
            }
            PreparedGeometry::Dynamic {
                first_vertex,
                vertex_count,
            } => {
                if vertex_count == 0 {
                    return;
                }
                let stride = std::mem::size_of::<ModelVertex>() as u64;
                let start = u64::from(first_vertex) * stride;
                let end = start + u64::from(vertex_count) * stride;
                render_pass.set_vertex_buffer(0, self.dynamic_buffer.slice(start..end));
                render_pass.draw(0..vertex_count, 0..1);
            }
        }
    }

    fn ensure_uniform_capacity(&mut self, device: &wgpu::Device, required: usize) {
        let required = required.max(1);
        if required <= self.uniform_slots {
            return;
        }
        let slots = required.next_power_of_two();
        self.uniform_buffer = create_uniform_buffer(device, slots);
        self.uniform_slots = slots;
        // 绑定组持有 uniform 缓冲句柄；重建缓冲后必须重建绑定组。
        self.white_bind_group = create_texture_bind_group(
            device,
            &self.model_layout,
            &self.uniform_buffer,
            &self.white_texture,
        );
        for ((model, material), texture) in &self.textures {
            let bind_group = create_texture_bind_group(
                device,
                &self.model_layout,
                &self.uniform_buffer,
                texture,
            );
            self.bind_groups.insert((*model, *material), bind_group);
        }
    }

    fn ensure_dynamic_capacity(&mut self, device: &wgpu::Device, required: usize) {
        if required <= self.dynamic_capacity {
            return;
        }
        let capacity = required.next_power_of_two();
        self.dynamic_buffer = create_vertex_buffer(device, capacity);
        self.dynamic_capacity = capacity;
    }
}

fn create_uniform_buffer(device: &wgpu::Device, slots: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model instance uniform buffer"),
        size: (slots.max(1) * UNIFORM_SLOT_SIZE) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_vertex_buffer(device: &wgpu::Device, vertices: usize) -> wgpu::Buffer {
    let size = vertices.max(1) as u64 * std::mem::size_of::<ModelVertex>() as u64;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model dynamic vertex buffer"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    texture: &GpuTexture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("model texture bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                // 动态偏移按槽推进，绑定大小固定为单个实例参数块（矩阵 64 + 参数 16）。
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(MODEL_UNIFORM_SIZE),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&texture.sampler),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::super::pipeline::model_vertex_buffer_layout;
    use super::*;

    #[test]
    fn uniform_slot_alignment_is_256_bytes() {
        assert_eq!(UNIFORM_SLOT_SIZE, 256);
        // 实例矩阵（64 字节）与参数（16 字节）合计 80 字节，仍落在单个槽内。
    }

    #[test]
    fn vertex_layout_stride_matches_struct() {
        let layout = model_vertex_buffer_layout();
        assert_eq!(
            layout.array_stride as usize,
            std::mem::size_of::<ModelVertex>()
        );
    }
}

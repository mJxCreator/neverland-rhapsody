//! glTF/GLB 到运行时模型资源的转换。
//!
//! 坐标约定：glTF 与引擎都使用右手系、Y 轴向上、模型 +Z 为前方，单位为米/方块，
//! 因此导入阶段不做坐标翻转，只把节点变换烘焙进静态网格顶点。
//! 蒙皮网格保留局部坐标，由实例姿态每帧变形后通过实例世界矩阵放置。

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use glam::{Mat3, Mat4, Quat, Vec3};

use super::animation::{
    AnimationChannel, AnimationClip, AnimationPath, Interpolation, KeyframeValues,
};
use super::glb::{parse_glb, Gltf, GltfAccessor, GltfNode};
use super::material::{AlphaMode, ModelMaterial};
use super::mesh::{MeshBounds, ModelMesh, ModelPrimitive};
use super::skeleton::{Joint, Skeleton};
use super::types::{ModelAsset, ModelTexture, Transform};
use super::ModelLoadError;
use crate::texture::texture_loader::decode_png_bytes;

/// 从磁盘加载 `.glb`（或 `.gltf` JSON，缓冲区需为外部 `.bin`）。
pub fn load_model(path: &Path) -> Result<ModelAsset, ModelLoadError> {
    let bytes = fs::read(path).map_err(|error| ModelLoadError::io(path, error))?;
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("model")
        .to_string();
    let base_dir = path.parent().unwrap_or(Path::new("."));
    load_model_from_bytes(&bytes, base_dir, &name)
}

/// 从字节加载模型；`base_dir` 用于解析相对路径的外部资源。
pub fn load_model_from_bytes(
    bytes: &[u8],
    base_dir: &Path,
    name: &str,
) -> Result<ModelAsset, ModelLoadError> {
    if bytes.starts_with(b"glTF") {
        let document = parse_glb(bytes, base_dir)?;
        Loader::new(&document.gltf, &document.bin, base_dir, name)?.build()
    } else {
        let gltf: Gltf = serde_json::from_slice(bytes)
            .map_err(|error| ModelLoadError::format(format!("glTF JSON 解析失败: {error}")))?;
        Loader::new(&gltf, &[], base_dir, name)?.build()
    }
}

struct Loader<'a> {
    gltf: &'a Gltf,
    base_dir: &'a Path,
    buffers: Vec<Vec<u8>>,
    name: String,
}

impl<'a> Loader<'a> {
    fn new(
        gltf: &'a Gltf,
        bin: &[u8],
        base_dir: &'a Path,
        name: &str,
    ) -> Result<Self, ModelLoadError> {
        let mut buffers = Vec::new();
        for (index, buffer) in gltf
            .buffers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let data = match &buffer.uri {
                Some(uri) if uri.starts_with("data:") => {
                    return Err(ModelLoadError::unsupported(
                        "暂不支持 data URI 缓冲区，请使用 GLB 内嵌 BIN 或外部 .bin",
                    ));
                }
                Some(uri) => {
                    let path = base_dir.join(uri);
                    fs::read(&path).map_err(|error| ModelLoadError::io(&path, error))?
                }
                None => bin.to_vec(),
            };
            if data.len() < buffer.byte_length {
                return Err(ModelLoadError::format(format!(
                    "缓冲区 {index} 声明 {} 字节，实际只有 {} 字节",
                    buffer.byte_length,
                    data.len()
                )));
            }
            buffers.push(data);
        }
        if gltf.buffers.is_none() && !bin.is_empty() {
            buffers.push(bin.to_vec());
        }
        Ok(Self {
            gltf,
            base_dir,
            buffers,
            name: name.to_string(),
        })
    }

    fn build(&self) -> Result<ModelAsset, ModelLoadError> {
        let nodes = self.gltf.nodes.as_deref().unwrap_or_default();
        let worlds = node_world_matrices(nodes)?;
        let mesh_nodes = self.scene_mesh_nodes(nodes, &worlds)?;
        if mesh_nodes.is_empty() {
            return Err(ModelLoadError::format("模型不包含可渲染网格"));
        }
        let (materials, textures) = self.build_materials()?;
        let skeleton = self.build_skeleton(nodes, &worlds, &mesh_nodes)?;
        let meshes = self.build_meshes(nodes, &mesh_nodes, skeleton.as_ref())?;
        let animations = self.build_animations(&mesh_nodes, skeleton.as_ref())?;
        Ok(ModelAsset {
            name: self.name.clone(),
            meshes,
            materials,
            textures,
            skeleton,
            animations,
        })
    }

    fn build_materials(&self) -> Result<(Vec<ModelMaterial>, Vec<ModelTexture>), ModelLoadError> {
        let (textures, remap) = self.build_textures()?;
        let materials = self
            .gltf
            .materials
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|material| {
                let pbr = material.pbr_metallic_roughness.as_ref();
                let mut placeholder = false;
                let texture_index = pbr
                    .and_then(|pbr| pbr.base_color_texture.as_ref())
                    .and_then(|reference| {
                        if reference.tex_coord.unwrap_or(0) != 0 {
                            placeholder = true;
                            tracing::warn!(
                                model = %self.name,
                                "材质引用了 TEXCOORD_{}，运行时只支持 UV0，将忽略纹理",
                                reference.tex_coord.unwrap_or(0)
                            );
                            return None;
                        }
                        let mapped = remap.get(reference.index).copied().flatten();
                        if mapped.is_none() {
                            placeholder = true;
                        }
                        mapped
                    });
                let (alpha_mode, known) = AlphaMode::from_gltf(
                    material.alpha_mode.as_deref().unwrap_or("OPAQUE"),
                    material.alpha_cutoff,
                );
                if !known {
                    tracing::warn!(
                        model = %self.name,
                        "未知 alphaMode {:?}，按 OPAQUE 处理",
                        material.alpha_mode
                    );
                }
                ModelMaterial {
                    name: material
                        .name
                        .clone()
                        .unwrap_or_else(|| "material".to_string()),
                    base_color: pbr
                        .and_then(|pbr| pbr.base_color_factor)
                        .unwrap_or([1.0; 4]),
                    base_color_texture: texture_index,
                    metallic: pbr.and_then(|pbr| pbr.metallic_factor).unwrap_or(0.0),
                    roughness: pbr.and_then(|pbr| pbr.roughness_factor).unwrap_or(1.0),
                    alpha_mode,
                    double_sided: material.double_sided.unwrap_or(false),
                    placeholder,
                }
            })
            .collect::<Vec<_>>();
        // 没有材质时使用默认纯白材质，避免所有图元退化成占位材质。
        let materials = if materials.is_empty() {
            vec![ModelMaterial {
                name: "default".to_string(),
                placeholder: false,
                ..ModelMaterial::default()
            }]
        } else {
            materials
        };
        Ok((materials, textures))
    }

    /// 解码所有纹理，并返回“glTF 纹理下标 → 运行时纹理下标”的映射。
    ///
    /// 同一图片被多张 glTF 纹理引用时只解码一次；解码失败只影响对应材质，
    /// 由材质层标记占位，不中断整个模型加载。
    fn build_textures(&self) -> Result<(Vec<ModelTexture>, Vec<Option<usize>>), ModelLoadError> {
        let gltf_textures = self.gltf.textures.as_deref().unwrap_or_default();
        let images = self.gltf.images.as_deref().unwrap_or_default();
        let mut decoded: HashMap<usize, Option<crate::texture::types::TextureImage>> =
            HashMap::new();
        let mut textures = Vec::new();
        let mut remap = Vec::with_capacity(gltf_textures.len());
        for texture in gltf_textures {
            let runtime = texture.source.and_then(|image_index| {
                let image = decoded
                    .entry(image_index)
                    .or_insert_with(|| match images.get(image_index) {
                        Some(image) => self
                            .decode_image(image_index, image)
                            .map_err(|error| {
                                tracing::warn!(
                                    model = %self.name,
                                    "图片 {} 解码失败，使用占位材质: {error}",
                                    image_index
                                );
                            })
                            .ok(),
                        None => None,
                    })
                    .clone()?;
                let index = textures.len();
                textures.push(ModelTexture {
                    name: texture
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("texture_{image_index}")),
                    image,
                });
                Some(index)
            });
            remap.push(runtime);
        }
        Ok((textures, remap))
    }

    fn decode_image(
        &self,
        image_index: usize,
        image: &super::glb::GltfImage,
    ) -> Result<crate::texture::types::TextureImage, ModelLoadError> {
        let bytes = if let Some(view_index) = image.buffer_view {
            self.read_buffer_view_bytes(view_index)?.to_vec()
        } else if let Some(uri) = &image.uri {
            if uri.starts_with("data:") {
                return Err(ModelLoadError::unsupported(
                    "暂不支持 data URI 纹理，请使用内嵌 bufferView 或外部 PNG",
                ));
            }
            let path = self.base_dir.join(uri);
            fs::read(&path).map_err(|error| ModelLoadError::io(&path, error))?
        } else {
            return Err(ModelLoadError::format(format!(
                "图片 {image_index} 既没有 uri 也没有 bufferView"
            )));
        };
        decode_png_bytes(&bytes, &format!("{}#image{image_index}", self.name))
            .map_err(|error| ModelLoadError::format(error.to_string()))
    }

    fn build_skeleton(
        &self,
        nodes: &[GltfNode],
        worlds: &[Mat4],
        mesh_nodes: &[SceneMeshNode],
    ) -> Result<Option<Skeleton>, ModelLoadError> {
        let skin_indices: BTreeSet<usize> =
            mesh_nodes.iter().filter_map(|node| node.skin).collect();
        if skin_indices.is_empty() {
            return Ok(None);
        }
        if skin_indices.len() > 1 {
            return Err(ModelLoadError::unsupported(
                "一个模型包含多个 Skin，本版本只支持单骨骼集",
            ));
        }
        let skin_index = *skin_indices.iter().next().unwrap_or(&0);
        let skin = self
            .gltf
            .skins
            .as_deref()
            .and_then(|skins| skins.get(skin_index))
            .ok_or_else(|| ModelLoadError::format(format!("Skin {skin_index} 不存在")))?;
        if skin.joints.is_empty() {
            return Err(ModelLoadError::format("Skin 没有关节"));
        }
        let joint_lookup: HashMap<usize, usize> = skin
            .joints
            .iter()
            .enumerate()
            .map(|(joint, node)| (*node, joint))
            .collect();
        let parents = node_parents(nodes)?;

        let mut joints = Vec::with_capacity(skin.joints.len());
        for (joint_index, node_index) in skin.joints.iter().enumerate() {
            let node = nodes.get(*node_index).ok_or_else(|| {
                ModelLoadError::format(format!("Skin 关节引用不存在的节点 {node_index}"))
            })?;
            // 找到最近的“也是关节”的祖先，把中间的非关节节点变换折叠进局部变换。
            let mut cursor = parents[*node_index];
            let mut anchor = None;
            while let Some(parent) = cursor {
                if joint_lookup.contains_key(&parent) {
                    anchor = Some(parent);
                    break;
                }
                cursor = parents[parent];
            }
            let local_matrix = node_local_matrix(node)?;
            let parent_node = parents[*node_index];
            let prefix_matrix = match parent_node {
                Some(parent_node) => {
                    let anchor_world = anchor
                        .map(|anchor| worlds[anchor].inverse())
                        .unwrap_or(Mat4::IDENTITY);
                    anchor_world * worlds[parent_node]
                }
                None => Mat4::IDENTITY,
            };
            let parent_joint = anchor.and_then(|parent| joint_lookup.get(&parent).copied());
            joints.push(Joint {
                name: node
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("joint_{joint_index}")),
                parent: parent_joint,
                local: Transform::from_matrix(local_matrix),
                prefix: Transform::from_matrix(prefix_matrix),
            });
        }

        let inverse_bind = if let Some(accessor_index) = skin.inverse_bind_matrices {
            let values = self.read_accessor_f32(accessor_index)?;
            if values.len() != skin.joints.len() * 16 {
                return Err(ModelLoadError::format(format!(
                    "逆绑定矩阵数量 {} 与关节数量 {} 不一致",
                    values.len() / 16,
                    skin.joints.len()
                )));
            }
            values.chunks_exact(16).map(matrix_from_slice).collect()
        } else {
            skin.joints
                .iter()
                .map(|node| worlds[*node].inverse())
                .collect()
        };
        let skeleton = Skeleton {
            joints,
            inverse_bind,
        };
        skeleton.validate().map_err(ModelLoadError::format)?;
        Ok(Some(skeleton))
    }

    fn build_meshes(
        &self,
        nodes: &[GltfNode],
        mesh_nodes: &[SceneMeshNode],
        skeleton: Option<&Skeleton>,
    ) -> Result<Vec<ModelMesh>, ModelLoadError> {
        let gltf_meshes = self.gltf.meshes.as_deref().unwrap_or_default();
        let mut meshes = Vec::new();
        for mesh_node in mesh_nodes {
            let name = nodes[mesh_node.node]
                .name
                .clone()
                .unwrap_or_else(|| format!("node_{}", mesh_node.node));
            let Some(mesh) = gltf_meshes.get(mesh_node.mesh) else {
                return Err(ModelLoadError::format(format!(
                    "节点 {} 引用了不存在的网格 {}",
                    mesh_node.node, mesh_node.mesh
                )));
            };
            let mut primitives = Vec::with_capacity(mesh.primitives.len());
            for (primitive_index, primitive) in mesh.primitives.iter().enumerate() {
                let material = primitive.material.unwrap_or(0);
                let mut built = self.build_primitive(primitive)?;
                built.material = material;
                let skinned = built.is_skinned() && skeleton.is_some() && mesh_node.skin.is_some();
                if !skinned {
                    bake_into_local(&mut built, &mesh_node.world);
                }
                built.validate().map_err(|error| {
                    ModelLoadError::format(format!(
                        "网格 {}.{} 数据非法: {error}",
                        mesh_node.mesh, primitive_index
                    ))
                })?;
                primitives.push(built);
            }
            let bounds = primitives
                .iter()
                .filter_map(|primitive| MeshBounds::from_positions(&primitive.positions))
                .reduce(MeshBounds::union)
                .unwrap_or(MeshBounds::new(Vec3::ZERO, Vec3::ZERO));
            meshes.push(ModelMesh {
                name: if mesh.name.is_some() {
                    format!("{}#{}", name, mesh_node.mesh)
                } else {
                    name.clone()
                },
                primitives,
                bounds,
            });
        }
        Ok(meshes)
    }

    fn build_primitive(
        &self,
        primitive: &super::glb::GltfPrimitive,
    ) -> Result<ModelPrimitive, ModelLoadError> {
        let mode = primitive.mode.unwrap_or(4);
        if mode != 4 {
            return Err(ModelLoadError::unsupported(format!(
                "图元模式 {mode} 未支持，仅支持 TRIANGLES(4)"
            )));
        }
        let positions_accessor = primitive
            .attributes
            .get("POSITION")
            .copied()
            .ok_or_else(|| ModelLoadError::format("图元缺少 POSITION 属性"))?;
        let positions: Vec<[f32; 3]> = self
            .read_accessor_f32(positions_accessor)?
            .chunks_exact(3)
            .map(|chunk| [chunk[0], chunk[1], chunk[2]])
            .collect();
        if positions.is_empty() {
            return Err(ModelLoadError::format("图元没有任何顶点"));
        }
        let indices = if let Some(index) = primitive.indices {
            self.read_indices(index)?
        } else {
            (0..positions.len() as u32).collect()
        };
        let normals = if let Some(accessor) = primitive.attributes.get("NORMAL") {
            self.read_accessor_f32(*accessor)?
                .chunks_exact(3)
                .map(|chunk| [chunk[0], chunk[1], chunk[2]])
                .collect()
        } else {
            ModelPrimitive::compute_flat_normals(&positions, &indices)
        };
        let uvs = if let Some(accessor) = primitive.attributes.get("TEXCOORD_0") {
            self.read_accessor_f32(*accessor)?
                .chunks_exact(2)
                .map(|chunk| [chunk[0], chunk[1]])
                .collect()
        } else {
            vec![[0.0, 0.0]; positions.len()]
        };
        let tangents = primitive
            .attributes
            .get("TANGENT")
            .map(|accessor| {
                Ok(self
                    .read_accessor_f32(*accessor)?
                    .chunks_exact(4)
                    .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
                    .collect())
            })
            .transpose()?;
        let (joints, weights) = match (
            primitive.attributes.get("JOINTS_0"),
            primitive.attributes.get("WEIGHTS_0"),
        ) {
            (Some(joint_accessor), Some(weight_accessor)) => {
                let joints: Vec<[u16; 4]> = self
                    .read_accessor_f32(*joint_accessor)?
                    .chunks_exact(4)
                    .map(|chunk| {
                        [
                            round_u16(chunk[0]),
                            round_u16(chunk[1]),
                            round_u16(chunk[2]),
                            round_u16(chunk[3]),
                        ]
                    })
                    .collect();
                let mut weights: Vec<[f32; 4]> = self
                    .read_accessor_f32(*weight_accessor)?
                    .chunks_exact(4)
                    .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
                    .collect();
                normalize_weights(&mut weights);
                (Some(joints), Some(weights))
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(ModelLoadError::format("JOINTS_0 与 WEIGHTS_0 必须同时存在"));
            }
            (None, None) => (None, None),
        };
        Ok(ModelPrimitive {
            positions,
            normals,
            uvs,
            tangents,
            joints,
            weights,
            indices,
            material: 0,
        })
    }

    fn build_animations(
        &self,
        mesh_nodes: &[SceneMeshNode],
        skeleton: Option<&Skeleton>,
    ) -> Result<Vec<AnimationClip>, ModelLoadError> {
        if skeleton.is_none() {
            return Ok(Vec::new());
        }
        let Some(animations) = self.gltf.animations.as_deref() else {
            return Ok(Vec::new());
        };
        let joint_nodes: HashMap<usize, usize> = mesh_nodes
            .iter()
            .filter_map(|node| node.skin)
            .next()
            .and_then(|skin| self.gltf.skins.as_deref().and_then(|skins| skins.get(skin)))
            .map(|skin| {
                skin.joints
                    .iter()
                    .enumerate()
                    .map(|(joint, node)| (*node, joint))
                    .collect()
            })
            .unwrap_or_default();
        let mut clips = Vec::with_capacity(animations.len());
        for (clip_index, animation) in animations.iter().enumerate() {
            let mut channels = Vec::new();
            for animation_channel in &animation.channels {
                let Some(node_index) = animation_channel.target.node else {
                    continue;
                };
                let Some(joint) = joint_nodes.get(&node_index).copied() else {
                    // 非关节节点通道本版本不参与骨骼姿态，显式提示而不是静默失败。
                    tracing::warn!(
                        model = %self.name,
                        "动画通道目标节点 {} 不是 Skin 关节，已跳过",
                        node_index
                    );
                    continue;
                };
                let Some(path) = AnimationPath::from_gltf(&animation_channel.target.path) else {
                    tracing::warn!(
                        model = %self.name,
                        "未知动画路径 {:?}，已跳过",
                        animation_channel.target.path
                    );
                    continue;
                };
                let sampler = animation
                    .samplers
                    .get(animation_channel.sampler)
                    .ok_or_else(|| {
                        ModelLoadError::format(format!(
                            "动画 {} 引用了不存在的采样器",
                            animation_channel.sampler
                        ))
                    })?;
                let (interpolation, known) =
                    Interpolation::from_gltf(sampler.interpolation.as_deref());
                if !known {
                    return Err(ModelLoadError::unsupported(format!(
                        "未知动画插值 {:?}",
                        sampler.interpolation
                    )));
                }
                let times = self.read_accessor_f32(sampler.input)?;
                let values = self.read_accessor_f32(sampler.output)?;
                let values = build_keyframes(path, interpolation, &values)?;
                let channel = AnimationChannel {
                    joint,
                    path,
                    interpolation,
                    times,
                    values,
                };
                channel.validate().map_err(|error| {
                    ModelLoadError::format(format!("动画 {clip_index} 通道非法: {error}"))
                })?;
                channels.push(channel);
            }
            let duration = channels
                .iter()
                .filter_map(|channel| channel.times.last().copied())
                .fold(0.0_f32, f32::max);
            let clip = AnimationClip {
                name: animation
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("clip_{clip_index}")),
                duration,
                channels,
            };
            clip.validate().map_err(ModelLoadError::format)?;
            clips.push(clip);
        }
        Ok(clips)
    }

    fn scene_mesh_nodes(
        &self,
        nodes: &[GltfNode],
        worlds: &[Mat4],
    ) -> Result<Vec<SceneMeshNode>, ModelLoadError> {
        let parents = node_parents(nodes)?;
        let roots = scene_roots(self.gltf, &parents)?;
        let mut result = Vec::new();
        let mut visited = vec![false; nodes.len()];
        let mut stack = roots;
        while let Some(index) = stack.pop() {
            if index >= nodes.len() {
                return Err(ModelLoadError::format(format!(
                    "场景引用了不存在的节点 {index}"
                )));
            }
            if visited[index] {
                continue;
            }
            visited[index] = true;
            let node = &nodes[index];
            if let Some(mesh) = node.mesh {
                result.push(SceneMeshNode {
                    node: index,
                    mesh,
                    skin: node.skin,
                    world: worlds[index],
                });
            }
            if let Some(children) = &node.children {
                stack.extend(children.iter().copied());
            }
        }
        Ok(result)
    }

    fn read_buffer_view_bytes(&self, view_index: usize) -> Result<&[u8], ModelLoadError> {
        let view = self
            .gltf
            .buffer_views
            .as_deref()
            .and_then(|views| views.get(view_index))
            .ok_or_else(|| ModelLoadError::format(format!("bufferView {view_index} 不存在")))?;
        let buffer = self.buffers.get(view.buffer).ok_or_else(|| {
            ModelLoadError::format(format!("bufferView {view_index} 引用的缓冲区不存在"))
        })?;
        let start = view.byte_offset.unwrap_or(0);
        let end = start
            .checked_add(view.byte_length)
            .ok_or_else(|| ModelLoadError::format("bufferView 范围溢出"))?;
        buffer.get(start..end).ok_or_else(|| {
            ModelLoadError::format(format!(
                "bufferView {view_index} 超出缓冲区长度 {}（需要 {end}）",
                buffer.len()
            ))
        })
    }

    fn read_accessor_f32(&self, index: usize) -> Result<Vec<f32>, ModelLoadError> {
        let info = self.accessor_info(index)?;
        if info.count == 0 {
            return Ok(Vec::new());
        }
        if info.zeroed {
            return Ok(vec![0.0; info.count * info.components]);
        }
        let buffer = &self.buffers[info.buffer];
        let mut values = Vec::with_capacity(info.count * info.components);
        for element in 0..info.count {
            let element_start = info.start + element * info.stride;
            for component in 0..info.components {
                let offset = element_start + component * info.component_size;
                let value = component_to_f32(buffer, offset, info.component_type, info.normalized)?;
                values.push(value);
            }
        }
        Ok(values)
    }

    fn read_indices(&self, index: usize) -> Result<Vec<u32>, ModelLoadError> {
        let info = self.accessor_info(index)?;
        if info.kind != "SCALAR" {
            return Err(ModelLoadError::format(format!(
                "索引 Accessor {index} 类型必须是 SCALAR，实际为 {}",
                info.kind
            )));
        }
        if !matches!(info.component_type, 5121 | 5123 | 5125) {
            return Err(ModelLoadError::format(format!(
                "索引 Accessor {index} 组件类型 {} 非法，应为 UNSIGNED_BYTE/SHORT/INT",
                info.component_type
            )));
        }
        if info.zeroed {
            return Ok(vec![0; info.count]);
        }
        let buffer = &self.buffers[info.buffer];
        let mut indices = Vec::with_capacity(info.count);
        for element in 0..info.count {
            let offset = info.start + element * info.stride;
            let value = match info.component_type {
                5121 => u32::from(read_u8(buffer, offset)?),
                5123 => u32::from(read_u16(buffer, offset)?),
                _ => read_u32(buffer, offset)?,
            };
            indices.push(value);
        }
        Ok(indices)
    }

    fn accessor_info(&self, index: usize) -> Result<AccessorInfo, ModelLoadError> {
        let accessor: &GltfAccessor = self
            .gltf
            .accessors
            .as_deref()
            .and_then(|accessors| accessors.get(index))
            .ok_or_else(|| ModelLoadError::format(format!("Accessor {index} 不存在")))?;
        if accessor.sparse.is_some() {
            return Err(ModelLoadError::unsupported(format!(
                "Accessor {index} 使用 sparse 数据，本版本不支持"
            )));
        }
        let components = accessor_components(&accessor.kind).ok_or_else(|| {
            ModelLoadError::format(format!("Accessor {index} 类型 {:?} 非法", accessor.kind))
        })?;
        let component_size = component_size(accessor.component_type).ok_or_else(|| {
            ModelLoadError::format(format!(
                "Accessor {index} 组件类型 {} 非法",
                accessor.component_type
            ))
        })?;
        let Some(view_index) = accessor.buffer_view else {
            // 没有 bufferView 的 Accessor 在 glTF 中表示全零数据。
            return Ok(AccessorInfo {
                count: accessor.count,
                components,
                component_size,
                component_type: accessor.component_type,
                normalized: accessor.normalized.unwrap_or(false),
                kind: accessor.kind.clone(),
                buffer: 0,
                start: 0,
                stride: component_size * components,
                zeroed: true,
            });
        };
        let view = self
            .gltf
            .buffer_views
            .as_deref()
            .and_then(|views| views.get(view_index))
            .ok_or_else(|| ModelLoadError::format(format!("bufferView {view_index} 不存在")))?;
        let buffer = self.buffers.get(view.buffer).ok_or_else(|| {
            ModelLoadError::format(format!("bufferView {view_index} 引用的缓冲区不存在"))
        })?;
        let stride = view.byte_stride.unwrap_or(components * component_size);
        if stride < components * component_size {
            return Err(ModelLoadError::format(format!(
                "bufferView {view_index} 的 byteStride {stride} 小于元素大小"
            )));
        }
        let start = view.byte_offset.unwrap_or(0) + accessor.byte_offset.unwrap_or(0);
        let required_end = if accessor.count == 0 {
            start
        } else {
            start + (accessor.count - 1) * stride + components * component_size
        };
        let view_end = view.byte_offset.unwrap_or(0) + view.byte_length;
        if required_end > view_end || required_end > buffer.len() {
            return Err(ModelLoadError::format(format!(
                "Accessor {index} 数据超出 bufferView 或缓冲区范围"
            )));
        }
        Ok(AccessorInfo {
            count: accessor.count,
            components,
            component_size,
            component_type: accessor.component_type,
            normalized: accessor.normalized.unwrap_or(false),
            kind: accessor.kind.clone(),
            buffer: view.buffer,
            start,
            stride,
            zeroed: false,
        })
    }
}

struct AccessorInfo {
    count: usize,
    components: usize,
    component_size: usize,
    component_type: u32,
    normalized: bool,
    kind: String,
    buffer: usize,
    start: usize,
    stride: usize,
    zeroed: bool,
}

#[derive(Clone, Debug)]
struct SceneMeshNode {
    node: usize,
    mesh: usize,
    skin: Option<usize>,
    world: Mat4,
}

fn scene_roots(gltf: &Gltf, parents: &[Option<usize>]) -> Result<Vec<usize>, ModelLoadError> {
    if let Some(scene_index) = gltf.scene {
        let scene = gltf
            .scenes
            .as_deref()
            .and_then(|scenes| scenes.get(scene_index))
            .ok_or_else(|| ModelLoadError::format(format!("场景 {scene_index} 不存在")))?;
        if let Some(roots) = &scene.nodes {
            return Ok(roots.clone());
        }
    }
    Ok(parents
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
        .collect())
}

fn node_parents(nodes: &[GltfNode]) -> Result<Vec<Option<usize>>, ModelLoadError> {
    let mut parents = vec![None; nodes.len()];
    for (parent, node) in nodes.iter().enumerate() {
        for child in node.children.as_deref().unwrap_or_default() {
            if *child >= nodes.len() {
                return Err(ModelLoadError::format(format!(
                    "节点 {parent} 引用了不存在的子节点 {child}"
                )));
            }
            if parents[*child].is_some() {
                return Err(ModelLoadError::format(format!("节点 {child} 有多个父节点")));
            }
            parents[*child] = Some(parent);
        }
    }
    Ok(parents)
}

fn node_world_matrices(nodes: &[GltfNode]) -> Result<Vec<Mat4>, ModelLoadError> {
    let parents = node_parents(nodes)?;
    let mut worlds = vec![Mat4::IDENTITY; nodes.len()];
    let mut resolved = vec![false; nodes.len()];
    for index in 0..nodes.len() {
        resolve_node_world(index, nodes, &parents, &mut worlds, &mut resolved)?;
    }
    Ok(worlds)
}

fn resolve_node_world(
    index: usize,
    nodes: &[GltfNode],
    parents: &[Option<usize>],
    worlds: &mut [Mat4],
    resolved: &mut [bool],
) -> Result<(), ModelLoadError> {
    if resolved[index] {
        return Ok(());
    }
    let local = node_local_matrix(&nodes[index])?;
    let world = match parents[index] {
        Some(parent) => {
            resolve_node_world(parent, nodes, parents, worlds, resolved)?;
            worlds[parent] * local
        }
        None => local,
    };
    worlds[index] = world;
    resolved[index] = true;
    Ok(())
}

fn node_local_matrix(node: &GltfNode) -> Result<Mat4, ModelLoadError> {
    if let Some(matrix) = node.matrix {
        return Ok(Mat4::from_cols_array(&matrix));
    }
    let translation = node.translation.unwrap_or([0.0; 3]);
    let rotation = node.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let scale = node.scale.unwrap_or([1.0; 3]);
    let rotation = Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
    if !rotation.is_finite() || rotation.length_squared() <= f32::EPSILON {
        return Err(ModelLoadError::format("节点旋转四元数非法"));
    }
    Ok(Transform::new(
        Vec3::from_array(translation),
        rotation.normalize(),
        Vec3::from_array(scale),
    )
    .to_matrix())
}

fn bake_into_local(primitive: &mut ModelPrimitive, matrix: &Mat4) {
    let normal_matrix = Mat3::from_cols(
        matrix.x_axis.truncate(),
        matrix.y_axis.truncate(),
        matrix.z_axis.truncate(),
    );
    let normal_matrix = if normal_matrix.determinant().abs() > 1.0e-9 {
        normal_matrix.inverse().transpose()
    } else {
        Mat3::IDENTITY
    };
    for position in &mut primitive.positions {
        let value = *matrix * Vec3::from_array(*position).extend(1.0);
        *position = value.truncate().to_array();
    }
    for normal in &mut primitive.normals {
        let value = (normal_matrix * Vec3::from_array(*normal)).normalize_or_zero();
        *normal = value.to_array();
    }
    if let Some(tangents) = &mut primitive.tangents {
        for tangent in tangents {
            let value = (normal_matrix * Vec3::from_array([tangent[0], tangent[1], tangent[2]]))
                .normalize_or_zero();
            tangent[0] = value.x;
            tangent[1] = value.y;
            tangent[2] = value.z;
        }
    }
}

fn build_keyframes(
    path: AnimationPath,
    interpolation: Interpolation,
    raw: &[f32],
) -> Result<KeyframeValues, ModelLoadError> {
    let multiplier = if interpolation == Interpolation::CubicSpline {
        3
    } else {
        1
    };
    let values = match path {
        AnimationPath::Rotation => {
            if !raw.len().is_multiple_of(4 * multiplier) {
                return Err(ModelLoadError::format(
                    "旋转动画输出数量不是四元数帧的整数倍",
                ));
            }
            KeyframeValues::Quat(
                raw.chunks_exact(4)
                    .map(|chunk| {
                        Quat::from_xyzw(chunk[0], chunk[1], chunk[2], chunk[3]).normalize()
                    })
                    .collect(),
            )
        }
        AnimationPath::Translation | AnimationPath::Scale => {
            if !raw.len().is_multiple_of(3 * multiplier) {
                return Err(ModelLoadError::format(
                    "位移/缩放动画输出数量不是 VEC3 帧的整数倍",
                ));
            }
            KeyframeValues::Vec3(
                raw.chunks_exact(3)
                    .map(|chunk| Vec3::new(chunk[0], chunk[1], chunk[2]))
                    .collect(),
            )
        }
    };
    Ok(values)
}

fn normalize_weights(weights: &mut [[f32; 4]]) {
    for weight in weights {
        let total: f32 = weight
            .iter()
            .filter(|value| value.is_finite() && **value > 0.0)
            .sum();
        if total > 0.0 {
            for value in weight.iter_mut() {
                *value = if value.is_finite() && *value > 0.0 {
                    *value / total
                } else {
                    0.0
                };
            }
        } else {
            *weight = [1.0, 0.0, 0.0, 0.0];
        }
    }
}

fn accessor_components(kind: &str) -> Option<usize> {
    match kind {
        "SCALAR" => Some(1),
        "VEC2" => Some(2),
        "VEC3" => Some(3),
        "VEC4" => Some(4),
        "MAT2" => Some(4),
        "MAT3" => Some(9),
        "MAT4" => Some(16),
        _ => None,
    }
}

fn component_size(component_type: u32) -> Option<usize> {
    match component_type {
        5120 | 5121 => Some(1),
        5122 | 5123 => Some(2),
        5125 | 5126 => Some(4),
        _ => None,
    }
}

fn component_to_f32(
    buffer: &[u8],
    offset: usize,
    component_type: u32,
    normalized: bool,
) -> Result<f32, ModelLoadError> {
    let value = match component_type {
        5120 => f32::from(read_i8(buffer, offset)?),
        5121 => f32::from(read_u8(buffer, offset)?),
        5122 => f32::from(read_i16(buffer, offset)?),
        5123 => f32::from(read_u16(buffer, offset)?),
        5125 => {
            let raw = read_u32(buffer, offset)?;
            if normalized {
                raw as f32 / u32::MAX as f32
            } else {
                raw as f32
            }
        }
        5126 => {
            let bytes = buffer
                .get(offset..offset + 4)
                .ok_or_else(|| ModelLoadError::format("Accessor 读取 f32 越界"))?;
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        _ => return Err(ModelLoadError::format("未知组件类型")),
    };
    if !normalized {
        return Ok(value);
    }
    Ok(match component_type {
        5120 => (value / i8::MAX as f32).clamp(-1.0, 1.0),
        5121 => (value / u8::MAX as f32).clamp(0.0, 1.0),
        5122 => (value / i16::MAX as f32).clamp(-1.0, 1.0),
        5123 => (value / u16::MAX as f32).clamp(0.0, 1.0),
        _ => value,
    })
}

fn read_u8(buffer: &[u8], offset: usize) -> Result<u8, ModelLoadError> {
    buffer
        .get(offset)
        .copied()
        .ok_or_else(|| ModelLoadError::format("Accessor 读取 u8 越界"))
}

fn read_i8(buffer: &[u8], offset: usize) -> Result<i8, ModelLoadError> {
    read_u8(buffer, offset).map(|value| value as i8)
}

fn read_u16(buffer: &[u8], offset: usize) -> Result<u16, ModelLoadError> {
    let bytes = buffer
        .get(offset..offset + 2)
        .ok_or_else(|| ModelLoadError::format("Accessor 读取 u16 越界"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_i16(buffer: &[u8], offset: usize) -> Result<i16, ModelLoadError> {
    read_u16(buffer, offset).map(|value| value as i16)
}

fn read_u32(buffer: &[u8], offset: usize) -> Result<u32, ModelLoadError> {
    let bytes = buffer
        .get(offset..offset + 4)
        .ok_or_else(|| ModelLoadError::format("Accessor 读取 u32 越界"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn round_u16(value: f32) -> u16 {
    if value.is_finite() {
        value.round().clamp(0.0, u16::MAX as f32) as u16
    } else {
        0
    }
}

fn matrix_from_slice(slice: &[f32]) -> Mat4 {
    Mat4::from_cols_array(&[
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7], slice[8],
        slice[9], slice[10], slice[11], slice[12], slice[13], slice[14], slice[15],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个包含单个三角形与内嵌 BIN 的最小 GLB。
    fn triangle_glb() -> Vec<u8> {
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices: [u16; 3] = [0, 1, 2];
        let mut bin = Vec::new();
        for position in positions {
            for value in position {
                bin.extend_from_slice(&value.to_le_bytes());
            }
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let index_offset = bin.len();
        for index in indices {
            bin.extend_from_slice(&index.to_le_bytes());
        }
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "tri", "mesh": 0}],
            "meshes": [{"primitives": [{
                "attributes": {"POSITION": 0},
                "indices": 1,
                "material": 0
            }]}],
            "materials": [{"name": "mat", "pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.0, 0.0, 1.0]}}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": index_offset, "byteLength": 6}
            ],
            "buffers": [{"byteLength": bin.len()}]
        });
        build_glb(json.to_string().as_bytes(), &bin)
    }

    fn build_glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
        let json_padded = (json.len() + 3) & !3;
        let bin_padded = (bin.len() + 3) & !3;
        let total = 12 + 8 + json_padded + 8 + bin_padded;
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(b"glTF");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(total as u32).to_le_bytes());
        bytes.extend_from_slice(&(json_padded as u32).to_le_bytes());
        bytes.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        bytes.extend_from_slice(json);
        bytes.resize(12 + 8 + json_padded, b' ');
        bytes.extend_from_slice(&(bin_padded as u32).to_le_bytes());
        bytes.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        bytes.extend_from_slice(bin);
        bytes.resize(total, 0);
        bytes
    }

    #[test]
    fn loads_minimal_triangle() {
        let bytes = triangle_glb();
        let asset = load_model_from_bytes(&bytes, Path::new("."), "tri").expect("应加载成功");
        assert_eq!(asset.name, "tri");
        assert_eq!(asset.meshes.len(), 1);
        assert_eq!(asset.materials.len(), 1);
        assert_eq!(asset.materials[0].base_color, [1.0, 0.0, 0.0, 1.0]);
        let primitive = &asset.meshes[0].primitives[0];
        assert_eq!(primitive.vertex_count(), 3);
        assert_eq!(primitive.triangle_count(), 1);
        assert!(primitive.validate().is_ok());
        assert!(asset.skeleton.is_none());
        assert!(asset.animations.is_empty());
    }

    #[test]
    fn broken_glb_has_readable_error() {
        let error = load_model_from_bytes(b"not a glb", Path::new("."), "bad").unwrap_err();
        assert!(error.to_string().contains("JSON"));
    }

    #[test]
    fn missing_position_is_rejected() {
        // 构造无 POSITION 的 JSON 应得到明确错误。
        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {}}]}],
            "accessors": [],
            "bufferViews": [],
            "buffers": [{"byteLength": 0}]
        });
        let bytes = build_glb(json.to_string().as_bytes(), &[]);
        let error = load_model_from_bytes(&bytes, Path::new("."), "bad").unwrap_err();
        assert!(error.to_string().contains("POSITION"));
    }

    /// 按 4 字节对齐追加二进制数据，并返回 (偏移, 长度)。
    #[derive(Default)]
    struct Bin {
        bytes: Vec<u8>,
    }

    impl Bin {
        fn align4(&mut self) {
            while !self.bytes.len().is_multiple_of(4) {
                self.bytes.push(0);
            }
        }

        fn push_f32(&mut self, values: &[f32]) -> (usize, usize) {
            self.align4();
            let offset = self.bytes.len();
            for value in values {
                self.bytes.extend_from_slice(&value.to_le_bytes());
            }
            (offset, self.bytes.len() - offset)
        }

        fn push_u16(&mut self, values: &[u16]) -> (usize, usize) {
            self.align4();
            let offset = self.bytes.len();
            for value in values {
                self.bytes.extend_from_slice(&value.to_le_bytes());
            }
            (offset, self.bytes.len() - offset)
        }

        fn push_bytes(&mut self, values: &[u8]) -> (usize, usize) {
            self.align4();
            let offset = self.bytes.len();
            self.bytes.extend_from_slice(values);
            (offset, self.bytes.len() - offset)
        }
    }

    fn one_pixel_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG 头");
            writer
                .write_image_data(&[200, 120, 80, 255])
                .expect("PNG 数据");
        }
        bytes
    }

    #[test]
    fn loads_multi_mesh_materials_uv_and_texture() {
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let uvs: [[f32; 2]; 3] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let indices: [u16; 3] = [0, 1, 2];
        let png = one_pixel_png();
        let mut bin = Bin::default();
        let (position_offset, position_length) = bin.push_f32(
            &positions
                .iter()
                .flat_map(|position| position.iter().copied())
                .collect::<Vec<_>>(),
        );
        let (uv_offset, uv_length) = bin.push_f32(
            &uvs.iter()
                .flat_map(|uv| uv.iter().copied())
                .collect::<Vec<_>>(),
        );
        let (index_offset, index_length) = bin.push_u16(&indices);
        let (image_offset, image_length) = bin.push_bytes(&png);

        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [
                {"name": "root", "translation": [1.0, 0.0, 0.0], "children": [1, 2]},
                {"name": "left", "mesh": 0},
                {"name": "right", "mesh": 1}
            ],
            "meshes": [
                {"name": "quad", "primitives": [{
                    "attributes": {"POSITION": 0, "TEXCOORD_0": 1},
                    "indices": 2,
                    "material": 0
                }]},
                {"name": "quad", "primitives": [{
                    "attributes": {"POSITION": 0, "TEXCOORD_0": 1},
                    "indices": 2,
                    "material": 1
                }]}
            ],
            "materials": [
                {
                    "name": "textured",
                    "alphaMode": "MASK",
                    "alphaCutoff": 0.5,
                    "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}
                },
                {
                    "name": "plain",
                    "pbrMetallicRoughness": {"baseColorFactor": [0.2, 0.8, 0.4, 1.0]}
                }
            ],
            "textures": [{"source": 0}],
            "images": [{"bufferView": 3, "mimeType": "image/png"}],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"},
                {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": position_offset, "byteLength": position_length},
                {"buffer": 0, "byteOffset": uv_offset, "byteLength": uv_length},
                {"buffer": 0, "byteOffset": index_offset, "byteLength": index_length},
                {"buffer": 0, "byteOffset": image_offset, "byteLength": image_length}
            ],
            "buffers": [{"byteLength": bin.bytes.len()}]
        });
        let bytes = build_glb(json.to_string().as_bytes(), &bin.bytes);
        let asset = load_model_from_bytes(&bytes, Path::new("."), "multi").expect("应加载成功");

        assert_eq!(asset.meshes.len(), 2, "两个网格节点应产生两个网格");
        assert_eq!(asset.materials.len(), 2);
        assert_eq!(asset.materials[0].base_color_texture, Some(0));
        assert_eq!(asset.materials[1].base_color_texture, None);
        assert!(matches!(
            asset.materials[0].alpha_mode,
            AlphaMode::Mask(cutoff) if (cutoff - 0.5).abs() < 1e-6
        ));
        assert_eq!(asset.textures.len(), 1);
        assert_eq!(asset.textures[0].image.width, 1);
        assert_eq!(asset.textures[0].image.height, 1);

        let primitive = &asset.meshes[0].primitives[0];
        assert_eq!(primitive.uvs[1], [1.0, 0.0]);
        // 父节点平移应烘焙进静态网格。
        assert!((primitive.positions[0][0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn loads_skinned_animation() {
        let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let joints: [u16; 12] = [0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0];
        let weights: [f32; 12] = [0.5, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0];
        let indices: [u16; 3] = [0, 1, 2];
        let inverse_bind: [f32; 32] = [
            // 关节 0：单位矩阵
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            // 关节 1：沿 X 反向平移 1
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0,
        ];
        let times: [f32; 2] = [0.0, 1.0];
        let rotations: [f32; 8] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.707_106_77, 0.707_106_77];

        let mut bin = Bin::default();
        let (position_offset, position_length) = bin.push_f32(
            &positions
                .iter()
                .flat_map(|position| position.iter().copied())
                .collect::<Vec<_>>(),
        );
        let (joint_offset, joint_length) = bin.push_u16(&joints);
        let (weight_offset, weight_length) = bin.push_f32(&weights);
        let (index_offset, index_length) = bin.push_u16(&indices);
        let (bind_offset, bind_length) = bin.push_f32(&inverse_bind);
        let (time_offset, time_length) = bin.push_f32(&times);
        let (rotation_offset, rotation_length) = bin.push_f32(&rotations);

        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [3]}],
            "nodes": [
                {"name": "root", "children": [1]},
                {"name": "joint_root", "children": [2]},
                {"name": "joint_tip", "translation": [1.0, 0.0, 0.0]},
                {"name": "skinned", "mesh": 0, "skin": 0}
            ],
            "skins": [{
                "joints": [1, 2],
                "skeleton": 1,
                "inverseBindMatrices": 3
            }],
            "meshes": [{"primitives": [{
                "attributes": {
                    "POSITION": 0,
                    "JOINTS_0": 1,
                    "WEIGHTS_0": 2
                },
                "indices": 4
            }]}],
            "animations": [{
                "name": "wave",
                "channels": [{"sampler": 0, "target": {"node": 2, "path": "rotation"}}],
                "samplers": [{"input": 5, "output": 6, "interpolation": "LINEAR"}]
            }],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5123, "count": 3, "type": "VEC4"},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4"},
                {"bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4"},
                {"bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR"},
                {"bufferView": 5, "componentType": 5126, "count": 2, "type": "SCALAR"},
                {"bufferView": 6, "componentType": 5126, "count": 2, "type": "VEC4"}
            ],
            "bufferViews": [
                {"buffer": 0, "byteOffset": position_offset, "byteLength": position_length},
                {"buffer": 0, "byteOffset": joint_offset, "byteLength": joint_length},
                {"buffer": 0, "byteOffset": weight_offset, "byteLength": weight_length},
                {"buffer": 0, "byteOffset": index_offset, "byteLength": index_length},
                {"buffer": 0, "byteOffset": bind_offset, "byteLength": bind_length},
                {"buffer": 0, "byteOffset": time_offset, "byteLength": time_length},
                {"buffer": 0, "byteOffset": rotation_offset, "byteLength": rotation_length}
            ],
            "buffers": [{"byteLength": bin.bytes.len()}]
        });
        let bytes = build_glb(json.to_string().as_bytes(), &bin.bytes);
        let asset = load_model_from_bytes(&bytes, Path::new("."), "skinned").expect("应加载成功");

        let skeleton = asset.skeleton.as_ref().expect("应有骨骼");
        assert_eq!(skeleton.joints.len(), 2);
        assert_eq!(skeleton.joints[1].parent, Some(0));
        assert_eq!(skeleton.joints[1].local.translation.x, 1.0);

        let primitive = &asset.meshes[0].primitives[0];
        assert!(primitive.is_skinned());
        assert_eq!(primitive.joints.as_ref().unwrap()[1], [1, 0, 0, 0]);

        assert_eq!(asset.animations.len(), 1);
        assert_eq!(asset.animations[0].name, "wave");
        assert!((asset.animations[0].duration - 1.0).abs() < 1e-6);
        assert_eq!(asset.animation_index("WAVE"), Some(0));

        let pose = super::super::skeleton::Pose::from_locals(skeleton, &skeleton.bind_locals());
        assert_eq!(pose.skin.len(), 2);
        // 绑定姿态下关节 1 的世界位置为 (1,0,0)。
        let tip = super::super::skeleton::joint_position(&pose, 1);
        assert!((tip.x - 1.0).abs() < 1e-5);
    }
}

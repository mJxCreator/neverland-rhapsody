//! 内置低多边形猪/牛/僵尸模型：不依赖外部资产即可展示完整 3D 生物。
//!
//! 这些模型与 GLB 导入产物使用完全相同的运行时结构（网格 + 骨骼 + 动画 + UV 纹理），
//! 因此可以作为资源缺失时的回退，也用于验证动画/蒙皮链路。
//! 模型坐标约定：脚底位于 `y = 0`，中心在原点，面向 `+Z`。

use glam::{Quat, Vec3};

use super::animation::{
    AnimationChannel, AnimationClip, AnimationPath, Interpolation, KeyframeValues,
};
use super::material::{AlphaMode, ModelMaterial};
use super::mesh::{MeshBounds, ModelMesh, ModelPrimitive};
use super::skeleton::{Joint, Skeleton};
use super::types::{ModelAsset, ModelTexture, Transform};
use crate::texture::types::TextureImage;

/// 生物配色与尺寸参数。
struct AnimalSpec {
    name: &'static str,
    body_color: [u8; 4],
    accent_color: [u8; 4],
    leg_color: [u8; 4],
    head_color: [u8; 4],
    body_center: Vec3,
    body_half: Vec3,
    head_center: Vec3,
    head_half: Vec3,
    leg_half: Vec3,
    leg_offset_x: f32,
    leg_offset_z: f32,
    has_horns: bool,
    spots: bool,
}

/// 用外部 PNG 替换内置模型默认生成的 UV 图集。
///
/// 分区约定保持不变：左上身体、右上强调色（牛为斑点）、左下腿、右下头/脸。
/// 图片尺寸任意，采样时按归一化 UV 缩放；建议保持 64×64 版式以匹配默认像素风。
pub fn override_texture(asset: &mut ModelAsset, image: TextureImage) {
    if let Some(texture) = asset.textures.first_mut() {
        texture.image = image;
    }
}

/// 内置猪模型。
pub fn pig() -> ModelAsset {
    build(AnimalSpec {
        name: "pig",
        body_color: [234, 151, 166, 255],
        accent_color: [198, 108, 126, 255],
        leg_color: [214, 131, 146, 255],
        head_color: [238, 160, 174, 255],
        body_center: Vec3::new(0.0, 0.55, 0.0),
        body_half: Vec3::new(0.35, 0.25, 0.55),
        head_center: Vec3::new(0.0, 0.75, 0.62),
        head_half: Vec3::new(0.25, 0.22, 0.25),
        leg_half: Vec3::new(0.09, 0.16, 0.09),
        leg_offset_x: 0.22,
        leg_offset_z: 0.35,
        has_horns: false,
        spots: false,
    })
}

/// 内置牛模型。
pub fn cow() -> ModelAsset {
    build(AnimalSpec {
        name: "cow",
        body_color: [235, 232, 225, 255],
        accent_color: [42, 42, 48, 255],
        leg_color: [224, 220, 212, 255],
        head_color: [240, 238, 232, 255],
        body_center: Vec3::new(0.0, 0.8, 0.0),
        body_half: Vec3::new(0.5, 0.35, 0.75),
        head_center: Vec3::new(0.0, 1.1, 0.85),
        head_half: Vec3::new(0.27, 0.24, 0.3),
        leg_half: Vec3::new(0.11, 0.3, 0.11),
        leg_offset_x: 0.35,
        leg_offset_z: 0.45,
        has_horns: true,
        spots: true,
    })
}

/// 僵尸人形模型：双腿、躯干、头部与前伸双臂。
pub fn zombie() -> ModelAsset {
    let texture = generate_zombie_texture();
    let material = ModelMaterial {
        name: "zombie_skin".to_string(),
        base_color: [1.0; 4],
        base_color_texture: Some(0),
        metallic: 0.0,
        roughness: 1.0,
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        placeholder: false,
    };
    let (primitive, bounds) = build_zombie_primitive();
    let mesh = ModelMesh {
        name: "zombie".to_string(),
        primitives: vec![primitive],
        bounds,
    };
    let skeleton = build_zombie_skeleton();
    let animations = build_zombie_animations(&skeleton);
    ModelAsset {
        name: "zombie".to_string(),
        meshes: vec![mesh],
        materials: vec![material],
        textures: vec![ModelTexture {
            name: "zombie_skin".to_string(),
            image: texture,
        }],
        skeleton: Some(skeleton),
        animations,
    }
}

fn build(spec: AnimalSpec) -> ModelAsset {
    let texture = generate_texture(&spec);
    let material = ModelMaterial {
        name: format!("{}_skin", spec.name),
        base_color: [1.0; 4],
        base_color_texture: Some(0),
        metallic: 0.0,
        roughness: 1.0,
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        placeholder: false,
    };
    let (primitive, bounds) = build_primitive(&spec);
    let mesh = ModelMesh {
        name: spec.name.to_string(),
        primitives: vec![primitive],
        bounds,
    };
    let skeleton = build_skeleton(&spec);
    let animations = build_animations(&skeleton, &spec);
    ModelAsset {
        name: spec.name.to_string(),
        meshes: vec![mesh],
        materials: vec![material],
        textures: vec![ModelTexture {
            name: format!("{}_skin", spec.name),
            image: texture,
        }],
        skeleton: Some(skeleton),
        animations,
    }
}

/// 纹理区域：左侧身体、右侧强调色、左下腿部、右下头部。
const REGION_BODY: UVRect = UVRect::new(0.0, 0.0, 0.5, 0.5);
const REGION_ACCENT: UVRect = UVRect::new(0.5, 0.0, 1.0, 0.5);
const REGION_LEG: UVRect = UVRect::new(0.0, 0.5, 0.5, 1.0);
const REGION_HEAD: UVRect = UVRect::new(0.5, 0.5, 0.75, 1.0);
const REGION_FACE: UVRect = UVRect::new(0.75, 0.5, 1.0, 1.0);

#[derive(Clone, Copy)]
struct UVRect {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
}

impl UVRect {
    const fn new(u0: f32, v0: f32, u1: f32, v1: f32) -> Self {
        Self { u0, v0, u1, v1 }
    }

    fn corners(self) -> [[f32; 2]; 4] {
        [
            [self.u0, self.v0],
            [self.u1, self.v0],
            [self.u1, self.v1],
            [self.u0, self.v1],
        ]
    }
}

fn build_primitive(spec: &AnimalSpec) -> (ModelPrimitive, MeshBounds) {
    let mut primitive = ModelPrimitive {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        tangents: None,
        joints: Some(Vec::new()),
        weights: Some(Vec::new()),
        indices: Vec::new(),
        material: 0,
    };

    let body_joint = 1;
    let head_joint = 2;
    let tail_joint = 3;
    let leg_joints = [4, 5, 6, 7];

    add_box(
        &mut primitive,
        spec.body_center,
        spec.body_half,
        REGION_BODY,
        REGION_BODY,
        body_joint,
    );
    add_box(
        &mut primitive,
        spec.head_center,
        spec.head_half,
        REGION_HEAD,
        REGION_FACE,
        head_joint,
    );
    // 口鼻
    add_box(
        &mut primitive,
        Vec3::new(
            spec.head_center.x,
            spec.head_center.y - 0.06,
            spec.head_center.z + spec.head_half.z + 0.06,
        ),
        Vec3::new(0.13, 0.11, 0.07),
        REGION_ACCENT,
        REGION_ACCENT,
        head_joint,
    );
    // 耳朵
    for side in [-1.0, 1.0] {
        add_box(
            &mut primitive,
            Vec3::new(
                spec.head_center.x + side * 0.18,
                spec.head_center.y + spec.head_half.y + 0.06,
                spec.head_center.z - 0.05,
            ),
            Vec3::new(0.08, 0.1, 0.06),
            REGION_ACCENT,
            REGION_ACCENT,
            head_joint,
        );
    }
    if spec.has_horns {
        for side in [-1.0, 1.0] {
            add_box(
                &mut primitive,
                Vec3::new(
                    spec.head_center.x + side * 0.22,
                    spec.head_center.y + spec.head_half.y + 0.12,
                    spec.head_center.z,
                ),
                Vec3::new(0.05, 0.12, 0.05),
                REGION_ACCENT,
                REGION_ACCENT,
                head_joint,
            );
        }
    }
    // 四条腿
    for (leg, (x, z)) in [(-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
        .into_iter()
        .enumerate()
    {
        add_box(
            &mut primitive,
            Vec3::new(
                x * spec.leg_offset_x,
                spec.leg_half.y,
                z * spec.leg_offset_z,
            ),
            spec.leg_half,
            REGION_LEG,
            REGION_LEG,
            leg_joints[leg],
        );
    }
    // 尾巴
    add_box(
        &mut primitive,
        Vec3::new(0.0, spec.body_center.y + 0.12, -spec.body_half.z - 0.1),
        Vec3::new(0.04, 0.04, 0.12),
        REGION_ACCENT,
        REGION_ACCENT,
        tail_joint,
    );

    let bounds = MeshBounds::from_positions(&primitive.positions)
        .unwrap_or(MeshBounds::new(Vec3::ZERO, spec.body_half));
    (primitive, bounds)
}

/// 添加一个箱体：六个面共享 `uv_all`，`+Z` 正面可用 `uv_front` 覆盖（用于脸部）。
fn add_box(
    primitive: &mut ModelPrimitive,
    center: Vec3,
    half: Vec3,
    uv_all: UVRect,
    uv_front: UVRect,
    joint: u16,
) {
    let min = center - half;
    let max = center + half;
    let faces = [
        // +X
        (
            [
                [max.x, min.y, max.z],
                [max.x, min.y, min.z],
                [max.x, max.y, min.z],
                [max.x, max.y, max.z],
            ],
            [1.0, 0.0, 0.0],
            uv_all,
        ),
        // -X
        (
            [
                [min.x, min.y, min.z],
                [min.x, min.y, max.z],
                [min.x, max.y, max.z],
                [min.x, max.y, min.z],
            ],
            [-1.0, 0.0, 0.0],
            uv_all,
        ),
        // +Y
        (
            [
                [min.x, max.y, max.z],
                [max.x, max.y, max.z],
                [max.x, max.y, min.z],
                [min.x, max.y, min.z],
            ],
            [0.0, 1.0, 0.0],
            uv_all,
        ),
        // -Y
        (
            [
                [min.x, min.y, min.z],
                [max.x, min.y, min.z],
                [max.x, min.y, max.z],
                [min.x, min.y, max.z],
            ],
            [0.0, -1.0, 0.0],
            uv_all,
        ),
        // +Z（正面）
        (
            [
                [min.x, min.y, max.z],
                [max.x, min.y, max.z],
                [max.x, max.y, max.z],
                [min.x, max.y, max.z],
            ],
            [0.0, 0.0, 1.0],
            uv_front,
        ),
        // -Z
        (
            [
                [max.x, min.y, min.z],
                [min.x, min.y, min.z],
                [min.x, max.y, min.z],
                [max.x, max.y, min.z],
            ],
            [0.0, 0.0, -1.0],
            uv_all,
        ),
    ];
    for (corners, normal, uv) in faces.iter() {
        let base = primitive.positions.len() as u32;
        let uvs = uv.corners();
        for (corner, uv) in corners.iter().zip(uvs) {
            primitive.positions.push(*corner);
            primitive.normals.push(*normal);
            primitive.uvs.push(uv);
            primitive
                .joints
                .as_mut()
                .expect("内置模型带骨骼")
                .push([joint, 0, 0, 0]);
            primitive
                .weights
                .as_mut()
                .expect("内置模型带权重")
                .push([1.0, 0.0, 0.0, 0.0]);
        }
        primitive
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn build_skeleton(spec: &AnimalSpec) -> Skeleton {
    let body = spec.body_center;
    let joints = vec![
        Joint {
            name: "root".to_string(),
            parent: None,
            local: Transform::IDENTITY,
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "body".to_string(),
            parent: Some(0),
            local: Transform::new(body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "head".to_string(),
            parent: Some(1),
            local: Transform::new(spec.head_center - body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "tail".to_string(),
            parent: Some(1),
            local: Transform::new(
                Vec3::new(0.0, body.y + 0.12, -spec.body_half.z - 0.1) - body,
                Quat::IDENTITY,
                Vec3::ONE,
            ),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "leg_front_left".to_string(),
            parent: Some(1),
            local: Transform::new(
                Vec3::new(-spec.leg_offset_x, spec.leg_half.y, spec.leg_offset_z) - body,
                Quat::IDENTITY,
                Vec3::ONE,
            ),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "leg_front_right".to_string(),
            parent: Some(1),
            local: Transform::new(
                Vec3::new(spec.leg_offset_x, spec.leg_half.y, spec.leg_offset_z) - body,
                Quat::IDENTITY,
                Vec3::ONE,
            ),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "leg_back_left".to_string(),
            parent: Some(1),
            local: Transform::new(
                Vec3::new(-spec.leg_offset_x, spec.leg_half.y, -spec.leg_offset_z) - body,
                Quat::IDENTITY,
                Vec3::ONE,
            ),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "leg_back_right".to_string(),
            parent: Some(1),
            local: Transform::new(
                Vec3::new(spec.leg_offset_x, spec.leg_half.y, -spec.leg_offset_z) - body,
                Quat::IDENTITY,
                Vec3::ONE,
            ),
            prefix: Transform::IDENTITY,
        },
    ];
    let skeleton = Skeleton {
        joints,
        inverse_bind: Vec::new(),
    };
    let bind_globals = skeleton.global_matrices(&skeleton.bind_locals());
    Skeleton {
        joints: skeleton.joints,
        inverse_bind: bind_globals.iter().map(|matrix| matrix.inverse()).collect(),
    }
}

fn build_animations(skeleton: &Skeleton, spec: &AnimalSpec) -> Vec<AnimationClip> {
    let body = skeleton.joint_index("body").unwrap_or(1);
    let head = skeleton.joint_index("head").unwrap_or(2);
    let tail = skeleton.joint_index("tail").unwrap_or(3);
    let legs = [
        skeleton.joint_index("leg_front_left").unwrap_or(4),
        skeleton.joint_index("leg_front_right").unwrap_or(5),
        skeleton.joint_index("leg_back_left").unwrap_or(6),
        skeleton.joint_index("leg_back_right").unwrap_or(7),
    ];

    let idle = {
        let channels = vec![
            rotation_channel(
                head,
                &[0.0, 1.0, 2.0],
                vec![
                    Quat::from_rotation_x(-0.04),
                    Quat::from_rotation_x(0.04),
                    Quat::from_rotation_x(-0.04),
                ],
            ),
            rotation_channel(
                tail,
                &[0.0, 1.0, 2.0],
                vec![
                    Quat::from_rotation_y(0.12),
                    Quat::from_rotation_y(-0.12),
                    Quat::from_rotation_y(0.12),
                ],
            ),
            translation_channel(
                body,
                &[0.0, 1.0, 2.0],
                vec![
                    spec.body_center,
                    spec.body_center - Vec3::new(0.0, 0.02, 0.0),
                    spec.body_center,
                ],
            ),
        ];
        AnimationClip {
            name: "idle".to_string(),
            duration: 2.0,
            channels,
        }
    };

    let walk = gait_clip(spec, body, &legs, "walk", 1.0, 0.35);
    let run = gait_clip(spec, body, &legs, "run", 0.6, 0.6);
    vec![idle, walk, run]
}

fn gait_clip(
    spec: &AnimalSpec,
    body: usize,
    legs: &[usize; 4],
    name: &str,
    duration: f32,
    amplitude: f32,
) -> AnimationClip {
    let times = [
        0.0,
        duration * 0.25,
        duration * 0.5,
        duration * 0.75,
        duration,
    ];
    let mut channels = Vec::new();
    for (index, joint) in legs.iter().enumerate() {
        // 对角腿同相位，相邻腿反相位。
        let sign = if index == 0 || index == 3 { 1.0 } else { -1.0 };
        let angles = [
            amplitude * sign,
            0.0,
            -amplitude * sign,
            0.0,
            amplitude * sign,
        ];
        channels.push(rotation_channel(
            *joint,
            &times,
            angles
                .iter()
                .map(|angle| Quat::from_rotation_x(*angle))
                .collect(),
        ));
    }
    channels.push(translation_channel(
        body,
        &times,
        vec![
            spec.body_center,
            spec.body_center + Vec3::new(0.0, 0.03, 0.0),
            spec.body_center,
            spec.body_center + Vec3::new(0.0, 0.03, 0.0),
            spec.body_center,
        ],
    ));
    AnimationClip {
        name: name.to_string(),
        duration,
        channels,
    }
}

fn rotation_channel(joint: usize, times: &[f32], values: Vec<Quat>) -> AnimationChannel {
    AnimationChannel {
        joint,
        path: AnimationPath::Rotation,
        interpolation: Interpolation::Linear,
        times: times.to_vec(),
        values: KeyframeValues::Quat(values),
    }
}

fn translation_channel(joint: usize, times: &[f32], values: Vec<Vec3>) -> AnimationChannel {
    AnimationChannel {
        joint,
        path: AnimationPath::Translation,
        interpolation: Interpolation::Linear,
        times: times.to_vec(),
        values: KeyframeValues::Vec3(values),
    }
}

/// 生成 64×64 UV 纹理：四个基础区域 + 脸部眼睛；牛的身体区域带黑色斑块。
fn generate_texture(spec: &AnimalSpec) -> TextureImage {
    let size = 64usize;
    let mut pixels = vec![0u8; size * size * 4];
    let fill = |pixels: &mut [u8], rect: UVRect, color: [u8; 4]| {
        let x0 = (rect.u0 * size as f32) as usize;
        let x1 = (rect.u1 * size as f32) as usize;
        let y0 = (rect.v0 * size as f32) as usize;
        let y1 = (rect.v1 * size as f32) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let index = (y * size + x) * 4;
                pixels[index..index + 4].copy_from_slice(&color);
            }
        }
    };
    fill(&mut pixels, REGION_BODY, spec.body_color);
    fill(&mut pixels, REGION_ACCENT, spec.accent_color);
    fill(&mut pixels, REGION_LEG, spec.leg_color);
    fill(&mut pixels, REGION_HEAD, spec.head_color);
    fill(&mut pixels, REGION_FACE, spec.head_color);

    // 脸部两只眼睛。
    let face_x0 = (REGION_FACE.u0 * size as f32) as usize;
    let face_y0 = (REGION_FACE.v0 * size as f32) as usize;
    let eye_color = [24, 20, 24, 255];
    for eye_x in [face_x0 + 3, face_x0 + 10] {
        for dy in 0..4 {
            for dx in 0..3 {
                let index = ((face_y0 + 6 + dy) * size + eye_x + dx) * 4;
                pixels[index..index + 4].copy_from_slice(&eye_color);
            }
        }
    }
    if spec.spots {
        // 身体区域用黑色矩形形成简单的奶牛斑块。
        let body_x0 = (REGION_BODY.u0 * size as f32) as usize;
        let body_y0 = (REGION_BODY.v0 * size as f32) as usize;
        for (sx, sy, w, h) in [(4usize, 5usize, 8usize, 6usize), (20, 16, 7, 5)] {
            for y in sy..sy + h {
                for x in sx..sx + w {
                    let index = ((body_y0 + y) * size + body_x0 + x) * 4;
                    pixels[index..index + 4].copy_from_slice(&spec.accent_color);
                }
            }
        }
    }
    TextureImage::new(size as u32, size as u32, pixels).expect("内置纹理尺寸与缓冲一致")
}

/// 僵尸网格：躯干、头部、前伸双臂与双腿，脚底位于 `y = 0`。
fn build_zombie_primitive() -> (ModelPrimitive, MeshBounds) {
    let mut primitive = ModelPrimitive {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        tangents: None,
        joints: Some(Vec::new()),
        weights: Some(Vec::new()),
        indices: Vec::new(),
        material: 0,
    };

    let body_joint = 1;
    let head_joint = 2;
    // 躯干
    add_box(
        &mut primitive,
        Vec3::new(0.0, 1.15, 0.0),
        Vec3::new(0.3, 0.35, 0.17),
        REGION_BODY,
        REGION_BODY,
        body_joint,
    );
    // 头部（正面用脸部区域画眼睛）
    add_box(
        &mut primitive,
        Vec3::new(0.0, 1.79, 0.0),
        Vec3::new(0.25, 0.25, 0.24),
        REGION_HEAD,
        REGION_FACE,
        head_joint,
    );
    // 双臂：向前伸出，符合僵尸姿态
    for (joint, x) in [3u16, 4].into_iter().zip([-0.42f32, 0.42]) {
        add_box(
            &mut primitive,
            Vec3::new(x, 1.32, 0.22),
            Vec3::new(0.12, 0.32, 0.12),
            REGION_ACCENT,
            REGION_ACCENT,
            joint,
        );
    }
    // 双腿
    for (joint, x) in [5u16, 6].into_iter().zip([-0.16f32, 0.16]) {
        add_box(
            &mut primitive,
            Vec3::new(x, 0.39, 0.0),
            Vec3::new(0.13, 0.39, 0.14),
            REGION_LEG,
            REGION_LEG,
            joint,
        );
    }

    let bounds = MeshBounds::from_positions(&primitive.positions)
        .unwrap_or(MeshBounds::new(Vec3::ZERO, Vec3::new(0.5, 2.0, 0.5)));
    (primitive, bounds)
}

fn build_zombie_skeleton() -> Skeleton {
    let body = Vec3::new(0.0, 1.15, 0.0);
    let head = Vec3::new(0.0, 1.8, 0.0);
    let arm_left = Vec3::new(-0.42, 1.4, 0.1);
    let arm_right = Vec3::new(0.42, 1.4, 0.1);
    let leg_left = Vec3::new(-0.16, 0.78, 0.0);
    let leg_right = Vec3::new(0.16, 0.78, 0.0);
    let joints = vec![
        Joint {
            name: "root".to_string(),
            parent: None,
            local: Transform::IDENTITY,
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "body".to_string(),
            parent: Some(0),
            local: Transform::new(body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "head".to_string(),
            parent: Some(1),
            local: Transform::new(head - body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "arm_left".to_string(),
            parent: Some(1),
            local: Transform::new(arm_left - body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "arm_right".to_string(),
            parent: Some(1),
            local: Transform::new(arm_right - body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "leg_left".to_string(),
            parent: Some(1),
            local: Transform::new(leg_left - body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
        Joint {
            name: "leg_right".to_string(),
            parent: Some(1),
            local: Transform::new(leg_right - body, Quat::IDENTITY, Vec3::ONE),
            prefix: Transform::IDENTITY,
        },
    ];
    let skeleton = Skeleton {
        joints,
        inverse_bind: Vec::new(),
    };
    let bind_globals = skeleton.global_matrices(&skeleton.bind_locals());
    Skeleton {
        joints: skeleton.joints,
        inverse_bind: bind_globals.iter().map(|matrix| matrix.inverse()).collect(),
    }
}

fn build_zombie_animations(skeleton: &Skeleton) -> Vec<AnimationClip> {
    let body = skeleton.joint_index("body").unwrap_or(1);
    let head = skeleton.joint_index("head").unwrap_or(2);
    let arm_left = skeleton.joint_index("arm_left").unwrap_or(3);
    let arm_right = skeleton.joint_index("arm_right").unwrap_or(4);
    let leg_left = skeleton.joint_index("leg_left").unwrap_or(5);
    let leg_right = skeleton.joint_index("leg_right").unwrap_or(6);
    let body_local = Vec3::new(0.0, 1.15, 0.0);
    let arms = [arm_left, arm_right];
    let legs = [leg_left, leg_right];

    let idle = AnimationClip {
        name: "idle".to_string(),
        duration: 2.0,
        channels: vec![
            rotation_channel(
                head,
                &[0.0, 1.0, 2.0],
                vec![
                    Quat::from_rotation_z(-0.06),
                    Quat::from_rotation_z(0.06),
                    Quat::from_rotation_z(-0.06),
                ],
            ),
            rotation_channel(
                arm_left,
                &[0.0, 1.0, 2.0],
                vec![
                    Quat::from_rotation_x(0.08),
                    Quat::from_rotation_x(-0.06),
                    Quat::from_rotation_x(0.08),
                ],
            ),
            rotation_channel(
                arm_right,
                &[0.0, 1.0, 2.0],
                vec![
                    Quat::from_rotation_x(-0.06),
                    Quat::from_rotation_x(0.08),
                    Quat::from_rotation_x(-0.06),
                ],
            ),
            translation_channel(
                body,
                &[0.0, 1.0, 2.0],
                vec![body_local, body_local - Vec3::Y * 0.02, body_local],
            ),
        ],
    };

    let walk = zombie_gait(body, &legs, &arms, "walk", 1.0, 0.5, 0.35);
    let run = zombie_gait(body, &legs, &arms, "run", 0.6, 0.75, 0.55);

    // 攻击：双臂后摆蓄力后向前劈下，再回到自然姿态。
    let attack = {
        let times = [0.0, 0.16, 0.32, 0.5];
        let angles = [0.35, -1.25, -0.9, 0.0];
        let channels = vec![
            rotation_channel(
                arm_left,
                &times,
                angles.iter().map(|a| Quat::from_rotation_x(*a)).collect(),
            ),
            rotation_channel(
                arm_right,
                &times,
                angles.iter().map(|a| Quat::from_rotation_x(*a)).collect(),
            ),
            rotation_channel(
                head,
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(0.12),
                    Quat::from_rotation_x(0.2),
                    Quat::from_rotation_x(0.0),
                ],
            ),
        ];
        AnimationClip {
            name: "attack".to_string(),
            duration: 0.5,
            channels,
        }
    };

    // 受击：上半身后仰，双臂外张，快速回到原位。
    let hurt = {
        let times = [0.0, 0.12, 0.35];
        let channels = vec![
            rotation_channel(
                body,
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_x(-0.28), Quat::IDENTITY],
            ),
            rotation_channel(
                head,
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_x(-0.3), Quat::IDENTITY],
            ),
            rotation_channel(
                arm_left,
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_z(-0.5), Quat::IDENTITY],
            ),
            rotation_channel(
                arm_right,
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_z(0.5), Quat::IDENTITY],
            ),
        ];
        AnimationClip {
            name: "hurt".to_string(),
            duration: 0.35,
            channels,
        }
    };

    // 死亡：整体向前倒地并下沉。
    let death = {
        let times = [0.0, 0.35, 0.7, 1.2];
        let channels = vec![
            rotation_channel(
                body,
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_x(0.45),
                    Quat::from_rotation_x(1.15),
                    Quat::from_rotation_x(1.5),
                ],
            ),
            rotation_channel(
                head,
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_x(0.2),
                    Quat::from_rotation_x(0.3),
                    Quat::from_rotation_x(0.4),
                ],
            ),
            translation_channel(
                body,
                &times,
                vec![
                    body_local,
                    body_local + Vec3::new(0.0, -0.15, 0.1),
                    body_local + Vec3::new(0.0, -0.45, 0.25),
                    body_local + Vec3::new(0.0, -0.55, 0.35),
                ],
            ),
        ];
        AnimationClip {
            name: "death".to_string(),
            duration: 1.2,
            channels,
        }
    };

    vec![idle, walk, run, attack, hurt, death]
}

fn zombie_gait(
    body: usize,
    legs: &[usize; 2],
    arms: &[usize; 2],
    name: &str,
    duration: f32,
    leg_amplitude: f32,
    arm_amplitude: f32,
) -> AnimationClip {
    let body_local = Vec3::new(0.0, 1.15, 0.0);
    let times = [
        0.0,
        duration * 0.25,
        duration * 0.5,
        duration * 0.75,
        duration,
    ];
    let mut channels = Vec::new();
    for (index, joint) in legs.iter().enumerate() {
        let sign = if index == 0 { 1.0 } else { -1.0 };
        let angles = [
            leg_amplitude * sign,
            0.0,
            -leg_amplitude * sign,
            0.0,
            leg_amplitude * sign,
        ];
        channels.push(rotation_channel(
            *joint,
            &times,
            angles
                .iter()
                .map(|angle| Quat::from_rotation_x(*angle))
                .collect(),
        ));
    }
    for (index, joint) in arms.iter().enumerate() {
        // 手臂与同侧腿反相。
        let sign = if index == 0 { -1.0 } else { 1.0 };
        let angles = [
            arm_amplitude * sign,
            0.0,
            -arm_amplitude * sign,
            0.0,
            arm_amplitude * sign,
        ];
        channels.push(rotation_channel(
            *joint,
            &times,
            angles
                .iter()
                .map(|angle| Quat::from_rotation_x(*angle))
                .collect(),
        ));
    }
    channels.push(translation_channel(
        body,
        &times,
        vec![
            body_local,
            body_local + Vec3::Y * 0.03,
            body_local,
            body_local + Vec3::Y * 0.03,
            body_local,
        ],
    ));
    AnimationClip {
        name: name.to_string(),
        duration,
        channels,
    }
}

/// 生成僵尸 64×64 纹理：上衣、皮肤、裤子分区，脸部带双眼。
fn generate_zombie_texture() -> TextureImage {
    let size = 64usize;
    let mut pixels = vec![0u8; size * size * 4];
    let fill = |pixels: &mut [u8], rect: UVRect, color: [u8; 4]| {
        let x0 = (rect.u0 * size as f32) as usize;
        let x1 = (rect.u1 * size as f32) as usize;
        let y0 = (rect.v0 * size as f32) as usize;
        let y1 = (rect.v1 * size as f32) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let index = (y * size + x) * 4;
                pixels[index..index + 4].copy_from_slice(&color);
            }
        }
    };
    let skin = [94, 140, 78, 255];
    fill(&mut pixels, REGION_BODY, [54, 100, 118, 255]);
    fill(&mut pixels, REGION_ACCENT, skin);
    fill(&mut pixels, REGION_LEG, [56, 58, 92, 255]);
    fill(&mut pixels, REGION_HEAD, skin);
    fill(&mut pixels, REGION_FACE, skin);

    let face_x0 = (REGION_FACE.u0 * size as f32) as usize;
    let face_y0 = (REGION_FACE.v0 * size as f32) as usize;
    let eye_color = [20, 18, 20, 255];
    for eye_x in [face_x0 + 3, face_x0 + 10] {
        for dy in 0..4 {
            for dx in 0..3 {
                let index = ((face_y0 + 6 + dy) * size + eye_x + dx) * 4;
                pixels[index..index + 4].copy_from_slice(&eye_color);
            }
        }
    }
    TextureImage::new(size as u32, size as u32, pixels).expect("内置纹理尺寸与缓冲一致")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_valid(asset: &ModelAsset) {
        assert_eq!(asset.meshes.len(), 1);
        assert!(!asset.textures.is_empty());
        let primitive = &asset.meshes[0].primitives[0];
        assert!(primitive.validate().is_ok());
        assert!(primitive.is_skinned());
        let skeleton = asset.skeleton.as_ref().expect("内置模型带骨骼");
        assert!(skeleton.validate().is_ok());
        for matrix in skeleton.bind_skin_matrices() {
            let identity = glam::Mat4::IDENTITY.to_cols_array();
            for (actual, expected) in matrix.to_cols_array().iter().zip(identity.iter()) {
                assert!((actual - expected).abs() < 1e-4);
            }
        }
        assert!(asset.animation_index("idle").is_some());
        assert!(asset.animation_index("walk").is_some());
        assert!(asset.animation_index("run").is_some());
    }

    #[test]
    fn pig_model_is_complete() {
        let asset = pig();
        assert_valid(&asset);
        let bounds = asset.bounds().expect("应有包围盒");
        assert!(bounds.min.y.abs() < 1e-4, "脚底应接近 y=0");
        assert!(bounds.size().y > 0.8);
        assert!(asset.triangle_count() >= 100);
    }

    #[test]
    fn cow_model_is_larger_than_pig() {
        let pig = pig();
        let cow = cow();
        assert_valid(&cow);
        let pig_bounds = pig.bounds().expect("pig bounds");
        let cow_bounds = cow.bounds().expect("cow bounds");
        assert!(cow_bounds.size().y > pig_bounds.size().y);
    }

    #[test]
    fn zombie_model_is_humanoid_with_combat_clips() {
        let asset = zombie();
        assert_valid(&asset);
        let bounds = asset.bounds().expect("应有包围盒");
        assert!(bounds.min.y.abs() < 1e-4, "脚底应接近 y=0");
        assert!(bounds.size().y > 1.9, "僵尸应接近两格高");
        for clip in ["attack", "hurt", "death"] {
            assert!(asset.animation_index(clip).is_some(), "缺少动画片段 {clip}");
        }
    }

    #[test]
    fn builtin_animations_keep_loop_endpoints() {
        for asset in [pig(), cow(), zombie()] {
            for clip in &asset.animations {
                for channel in &clip.channels {
                    let first = channel.times.first().copied().unwrap_or(0.0);
                    let last = channel.times.last().copied().unwrap_or(0.0);
                    assert!(first == 0.0 && (last - clip.duration).abs() < 1e-5);
                }
            }
        }
    }

    #[test]
    fn texture_has_expected_size() {
        let image = &pig().textures[0].image;
        assert_eq!(image.width, 64);
        assert_eq!(image.height, 64);
        assert_eq!(image.pixels.len(), 64 * 64 * 4);
    }

    #[test]
    fn external_texture_overrides_generated_atlas() {
        let mut asset = pig();
        let image = TextureImage::new(2, 2, vec![255u8; 2 * 2 * 4]).expect("有效尺寸");
        override_texture(&mut asset, image);
        assert_eq!(asset.textures[0].image.width, 2);
        assert_eq!(asset.textures[0].image.height, 2);
        assert_eq!(asset.textures[0].image.pixels.len(), 2 * 2 * 4);
        // 覆盖只影响纹理，网格、骨骼与动画保持不变。
        assert!(asset.skeleton.is_some());
        assert_eq!(asset.meshes.len(), 1);
    }
}

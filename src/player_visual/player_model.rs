//! 内置玩家角色模型与第一人称手臂模型。
//!
//! 坐标约定与全部模型资源一致：脚底在 `y = 0`，模型面向 `+Z`，单位为米。
//! 骨骼名称固定为 `Attachment` 模块声明的 20 根骨骼；磁盘上的 `player.glb`
//! 若同名注册会覆盖这里的程序化模型，因此美术可以直接替换外观。

use glam::{Quat, Vec3};

use crate::attachment::PLAYER_BONES;
use crate::model::animation::{
    AnimationChannel, AnimationClip, AnimationPath, Interpolation, KeyframeValues,
};
use crate::model::material::{AlphaMode, ModelMaterial};
use crate::model::mesh::{MeshBounds, ModelMesh, ModelPrimitive};
use crate::model::skeleton::{Joint, Skeleton};
use crate::model::types::{ModelAsset, ModelTexture, Transform};
use crate::texture::types::TextureImage;

/// 内置玩家模型的稳定注册名。
pub const PLAYER_MODEL: &str = "player";
/// 第一人称手臂模型的稳定注册名。
pub const FIRST_PERSON_ARMS_MODEL: &str = "player_fp_arms";

const ROOT: usize = 0;
const PELVIS: usize = 1;
const SPINE: usize = 2;
const CHEST: usize = 3;
const NECK: usize = 4;
const HEAD: usize = 5;
const LEFT_SHOULDER: usize = 6;
const LEFT_ARM: usize = 7;
const LEFT_FOREARM: usize = 8;
const LEFT_HAND: usize = 9;
const RIGHT_SHOULDER: usize = 10;
const RIGHT_ARM: usize = 11;
const RIGHT_FOREARM: usize = 12;
const RIGHT_HAND: usize = 13;
const LEFT_LEG: usize = 14;
const LEFT_KNEE: usize = 15;
const LEFT_FOOT: usize = 16;
const RIGHT_LEG: usize = 17;
const RIGHT_KNEE: usize = 18;
const RIGHT_FOOT: usize = 19;

const PELVIS_POS: Vec3 = Vec3::new(0.0, 0.92, 0.0);

/// 生成完整玩家模型。
pub fn player() -> ModelAsset {
    let texture = generate_texture();
    let material = ModelMaterial {
        name: "player_skin".to_string(),
        base_color: [1.0; 4],
        base_color_texture: Some(0),
        metallic: 0.0,
        roughness: 0.95,
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        placeholder: false,
    };
    let (primitive, bounds) = build_primitive();
    let skeleton = build_skeleton();
    let animations = build_animations(&skeleton);
    ModelAsset {
        name: PLAYER_MODEL.to_string(),
        meshes: vec![ModelMesh {
            name: PLAYER_MODEL.to_string(),
            primitives: vec![primitive],
            bounds,
        }],
        materials: vec![material],
        textures: vec![ModelTexture {
            name: "player_skin".to_string(),
            image: texture,
        }],
        skeleton: Some(skeleton),
        animations,
    }
}

/// 第一人称手臂：右前臂、手掌与袖口，沿本地 `-Z` 向前伸展。
pub fn first_person_arms() -> ModelAsset {
    let skin = ModelMaterial {
        name: "fp_skin".to_string(),
        base_color: [0.86, 0.70, 0.55, 1.0],
        base_color_texture: None,
        metallic: 0.0,
        roughness: 0.9,
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        placeholder: false,
    };
    let sleeve = ModelMaterial {
        name: "fp_sleeve".to_string(),
        base_color: [0.25, 0.43, 0.67, 1.0],
        base_color_texture: None,
        metallic: 0.0,
        roughness: 0.95,
        alpha_mode: AlphaMode::Opaque,
        double_sided: false,
        placeholder: false,
    };
    let mut forearm = empty_static_primitive(0);
    push_box(
        &mut forearm,
        Vec3::new(0.0, 0.0, -0.06),
        Vec3::new(0.05, 0.05, 0.20),
        REGION_SKIN,
        REGION_SKIN,
        None,
    );
    push_box(
        &mut forearm,
        Vec3::new(0.0, 0.0, -0.27),
        Vec3::new(0.055, 0.055, 0.07),
        REGION_SKIN,
        REGION_SKIN,
        None,
    );
    let mut sleeve_primitive = empty_static_primitive(1);
    push_box(
        &mut sleeve_primitive,
        Vec3::new(0.0, 0.0, 0.13),
        Vec3::new(0.065, 0.065, 0.14),
        REGION_SHIRT,
        REGION_SHIRT,
        None,
    );
    let bounds = MeshBounds::new(Vec3::new(-0.07, -0.07, -0.35), Vec3::new(0.07, 0.07, 0.28));
    ModelAsset {
        name: FIRST_PERSON_ARMS_MODEL.to_string(),
        meshes: vec![ModelMesh {
            name: FIRST_PERSON_ARMS_MODEL.to_string(),
            primitives: vec![forearm, sleeve_primitive],
            bounds,
        }],
        materials: vec![skin, sleeve],
        textures: Vec::new(),
        skeleton: None,
        animations: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// 纹理区域
// ---------------------------------------------------------------------------

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

const REGION_SHIRT: UVRect = UVRect::new(0.0, 0.0, 0.5, 0.5);
const REGION_SKIN: UVRect = UVRect::new(0.5, 0.0, 1.0, 0.5);
const REGION_PANTS: UVRect = UVRect::new(0.0, 0.5, 0.5, 0.75);
const REGION_HAIR: UVRect = UVRect::new(0.5, 0.5, 0.75, 0.75);
const REGION_FACE: UVRect = UVRect::new(0.75, 0.5, 1.0, 0.75);
const REGION_HEAD: UVRect = UVRect::new(0.5, 0.75, 0.75, 1.0);
const REGION_SHOE: UVRect = UVRect::new(0.75, 0.75, 1.0, 1.0);

// ---------------------------------------------------------------------------
// 网格
// ---------------------------------------------------------------------------

fn build_primitive() -> (ModelPrimitive, MeshBounds) {
    let mut primitive = empty_skinned_primitive();
    for side in [-1.0f32, 1.0] {
        // 模型面向 +Z 时，角色右手在本地 -X；负侧分配右侧关节。
        let shoulder = if side < 0.0 {
            RIGHT_SHOULDER
        } else {
            LEFT_SHOULDER
        };
        let arm = if side < 0.0 { RIGHT_ARM } else { LEFT_ARM };
        let forearm = if side < 0.0 {
            RIGHT_FOREARM
        } else {
            LEFT_FOREARM
        };
        let hand = if side < 0.0 { RIGHT_HAND } else { LEFT_HAND };
        let leg = if side < 0.0 { RIGHT_LEG } else { LEFT_LEG };
        let knee = if side < 0.0 { RIGHT_KNEE } else { LEFT_KNEE };
        let foot = if side < 0.0 { RIGHT_FOOT } else { LEFT_FOOT };

        push_box(
            &mut primitive,
            Vec3::new(side * 0.20, 1.42, 0.0),
            Vec3::new(0.075, 0.06, 0.085),
            REGION_SHIRT,
            REGION_SHIRT,
            Some(shoulder as u16),
        );
        push_box(
            &mut primitive,
            Vec3::new(side * 0.30, 1.26, 0.0),
            Vec3::new(0.06, 0.14, 0.07),
            REGION_SHIRT,
            REGION_SHIRT,
            Some(arm as u16),
        );
        push_box(
            &mut primitive,
            Vec3::new(side * 0.30, 1.00, 0.0),
            Vec3::new(0.055, 0.12, 0.065),
            REGION_SKIN,
            REGION_SKIN,
            Some(forearm as u16),
        );
        push_box(
            &mut primitive,
            Vec3::new(side * 0.30, 0.855, 0.0),
            Vec3::new(0.06, 0.06, 0.07),
            REGION_SKIN,
            REGION_SKIN,
            Some(hand as u16),
        );
        push_box(
            &mut primitive,
            Vec3::new(side * 0.13, 0.68, 0.0),
            Vec3::new(0.075, 0.18, 0.09),
            REGION_PANTS,
            REGION_PANTS,
            Some(leg as u16),
        );
        push_box(
            &mut primitive,
            Vec3::new(side * 0.13, 0.26, 0.0),
            Vec3::new(0.07, 0.18, 0.075),
            REGION_PANTS,
            REGION_PANTS,
            Some(knee as u16),
        );
        push_box(
            &mut primitive,
            Vec3::new(side * 0.13, 0.045, 0.035),
            Vec3::new(0.08, 0.045, 0.135),
            REGION_SHOE,
            REGION_SHOE,
            Some(foot as u16),
        );
    }

    push_box(
        &mut primitive,
        Vec3::new(0.0, 0.95, 0.0),
        Vec3::new(0.16, 0.10, 0.11),
        REGION_PANTS,
        REGION_PANTS,
        Some(PELVIS as u16),
    );
    push_box(
        &mut primitive,
        Vec3::new(0.0, 1.10, 0.0),
        Vec3::new(0.19, 0.10, 0.11),
        REGION_SHIRT,
        REGION_SHIRT,
        Some(SPINE as u16),
    );
    push_box(
        &mut primitive,
        Vec3::new(0.0, 1.30, 0.0),
        Vec3::new(0.20, 0.10, 0.125),
        REGION_SHIRT,
        REGION_SHIRT,
        Some(CHEST as u16),
    );
    push_box(
        &mut primitive,
        Vec3::new(0.0, 1.50, 0.0),
        Vec3::new(0.055, 0.05, 0.055),
        REGION_SKIN,
        REGION_SKIN,
        Some(NECK as u16),
    );
    push_box(
        &mut primitive,
        Vec3::new(0.0, 1.665, 0.0),
        Vec3::new(0.135, 0.125, 0.13),
        REGION_HEAD,
        REGION_FACE,
        Some(HEAD as u16),
    );
    push_box(
        &mut primitive,
        Vec3::new(0.0, 1.78, -0.01),
        Vec3::new(0.14, 0.02, 0.135),
        REGION_HAIR,
        REGION_HAIR,
        Some(HEAD as u16),
    );
    push_box(
        &mut primitive,
        Vec3::new(0.0, 1.70, -0.135),
        Vec3::new(0.135, 0.08, 0.02),
        REGION_HAIR,
        REGION_HAIR,
        Some(HEAD as u16),
    );

    let bounds = MeshBounds::from_positions(&primitive.positions)
        .unwrap_or(MeshBounds::new(Vec3::ZERO, Vec3::new(0.4, 1.8, 0.3)));
    (primitive, bounds)
}

fn empty_skinned_primitive() -> ModelPrimitive {
    ModelPrimitive {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        tangents: None,
        joints: Some(Vec::new()),
        weights: Some(Vec::new()),
        indices: Vec::new(),
        material: 0,
    }
}

fn empty_static_primitive(material: usize) -> ModelPrimitive {
    ModelPrimitive {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        tangents: None,
        joints: None,
        weights: None,
        indices: Vec::new(),
        material,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_box(
    primitive: &mut ModelPrimitive,
    center: Vec3,
    half: Vec3,
    uv_all: UVRect,
    uv_front: UVRect,
    joint: Option<u16>,
) {
    let min = center - half;
    let max = center + half;
    let faces = [
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
        (
            [
                [min.x, min.y, min.z],
                [min.x, min.y, max.z],
                [max.x, min.y, max.z],
                [max.x, min.y, min.z],
            ],
            [0.0, -1.0, 0.0],
            uv_all,
        ),
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
    for (corners, normal, uv) in faces {
        let base = primitive.positions.len() as u32;
        for (corner, uv) in corners.iter().zip(uv.corners()) {
            primitive.positions.push(*corner);
            primitive.normals.push(normal);
            primitive.uvs.push(uv);
            if let Some(joints) = primitive.joints.as_mut() {
                joints.push([joint.unwrap_or(0), 0, 0, 0]);
            }
            if let Some(weights) = primitive.weights.as_mut() {
                weights.push([1.0, 0.0, 0.0, 0.0]);
            }
        }
        primitive
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

// ---------------------------------------------------------------------------
// 骨骼
// ---------------------------------------------------------------------------

fn build_skeleton() -> Skeleton {
    let joints = vec![
        joint("Root", None, Vec3::ZERO),
        joint("Pelvis", Some(ROOT), PELVIS_POS),
        joint("Spine", Some(PELVIS), Vec3::new(0.0, 0.18, 0.0)),
        joint("Chest", Some(SPINE), Vec3::new(0.0, 0.20, 0.0)),
        joint("Neck", Some(CHEST), Vec3::new(0.0, 0.16, 0.0)),
        joint("Head", Some(NECK), Vec3::new(0.0, 0.12, 0.0)),
        joint("LeftShoulder", Some(CHEST), Vec3::new(0.20, 0.12, 0.0)),
        joint("LeftArm", Some(LEFT_SHOULDER), Vec3::new(0.10, -0.02, 0.0)),
        joint("LeftForearm", Some(LEFT_ARM), Vec3::new(0.0, -0.28, 0.0)),
        joint("LeftHand", Some(LEFT_FOREARM), Vec3::new(0.0, -0.26, 0.0)),
        joint("RightShoulder", Some(CHEST), Vec3::new(-0.20, 0.12, 0.0)),
        joint(
            "RightArm",
            Some(RIGHT_SHOULDER),
            Vec3::new(-0.10, -0.02, 0.0),
        ),
        joint("RightForearm", Some(RIGHT_ARM), Vec3::new(0.0, -0.28, 0.0)),
        joint("RightHand", Some(RIGHT_FOREARM), Vec3::new(0.0, -0.26, 0.0)),
        joint("LeftLeg", Some(PELVIS), Vec3::new(0.13, -0.06, 0.0)),
        joint("LeftKnee", Some(LEFT_LEG), Vec3::new(0.0, -0.42, 0.0)),
        joint("LeftFoot", Some(LEFT_KNEE), Vec3::new(0.0, -0.40, 0.0)),
        joint("RightLeg", Some(PELVIS), Vec3::new(-0.13, -0.06, 0.0)),
        joint("RightKnee", Some(RIGHT_LEG), Vec3::new(0.0, -0.42, 0.0)),
        joint("RightFoot", Some(RIGHT_KNEE), Vec3::new(0.0, -0.40, 0.0)),
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

fn joint(name: &str, parent: Option<usize>, translation: Vec3) -> Joint {
    debug_assert!(PLAYER_BONES.contains(&name));
    Joint {
        name: name.to_string(),
        parent,
        local: Transform::new(translation, Quat::IDENTITY, Vec3::ONE),
        prefix: Transform::IDENTITY,
    }
}

// ---------------------------------------------------------------------------
// 动画
// ---------------------------------------------------------------------------

fn build_animations(skeleton: &Skeleton) -> Vec<AnimationClip> {
    let index = |name: &str| {
        skeleton
            .joint_index(name)
            .expect("内置骨架包含全部动画关节")
    };
    vec![
        idle(
            index("Spine"),
            index("Head"),
            index("LeftArm"),
            index("RightArm"),
            index("Pelvis"),
        ),
        gait(skeleton, "walk", 1.0, 0.55, 0.40, 0.55, 0.04, 0.018),
        gait(skeleton, "run", 0.62, 0.90, 0.80, 0.95, 0.18, 0.045),
        jump(skeleton),
        fall(skeleton),
        land(skeleton),
        attack(skeleton),
        mining(skeleton),
        eat(skeleton),
        hurt(skeleton),
        death(skeleton),
    ]
}

fn idle(
    spine: usize,
    head: usize,
    left_arm: usize,
    right_arm: usize,
    pelvis: usize,
) -> AnimationClip {
    let times = [0.0, 1.0, 2.0];
    AnimationClip {
        name: "idle".to_string(),
        duration: 2.0,
        channels: vec![
            rotation_channel(
                spine,
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(0.025),
                    Quat::from_rotation_x(0.0),
                ],
            ),
            rotation_channel(
                head,
                &times,
                vec![
                    Quat::from_rotation_y(-0.06),
                    Quat::from_rotation_y(0.06),
                    Quat::from_rotation_y(-0.06),
                ],
            ),
            rotation_channel(
                left_arm,
                &times,
                vec![
                    Quat::from_rotation_x(0.04),
                    Quat::from_rotation_x(-0.02),
                    Quat::from_rotation_x(0.04),
                ],
            ),
            rotation_channel(
                right_arm,
                &times,
                vec![
                    Quat::from_rotation_x(-0.02),
                    Quat::from_rotation_x(0.04),
                    Quat::from_rotation_x(-0.02),
                ],
            ),
            translation_channel(
                pelvis,
                &times,
                vec![
                    PELVIS_POS,
                    PELVIS_POS - Vec3::new(0.0, 0.004, 0.0),
                    PELVIS_POS,
                ],
            ),
        ],
    }
}

#[allow(clippy::too_many_arguments)]
fn gait(
    skeleton: &Skeleton,
    name: &str,
    duration: f32,
    leg_amplitude: f32,
    arm_amplitude: f32,
    knee_amplitude: f32,
    spine_pitch: f32,
    bob: f32,
) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("行走骨架完整");
    let times = [
        0.0,
        duration * 0.25,
        duration * 0.5,
        duration * 0.75,
        duration,
    ];
    let mut channels = Vec::new();
    for (side, sign) in [(index("LeftLeg"), 1.0f32), (index("RightLeg"), -1.0)] {
        channels.push(rotation_channel(
            side,
            &times,
            [1.0, 0.0, -1.0, 0.0, 1.0]
                .iter()
                .map(|value| Quat::from_rotation_x(value * leg_amplitude * sign))
                .collect(),
        ));
    }
    for (side, sign) in [(index("LeftKnee"), 1.0f32), (index("RightKnee"), -1.0)] {
        let angles = if sign > 0.0 {
            [0.08, knee_amplitude, 0.08, 0.02, 0.08]
        } else {
            [0.08, 0.02, 0.08, knee_amplitude, 0.08]
        };
        channels.push(rotation_channel(
            side,
            &times,
            angles
                .iter()
                .map(|angle| Quat::from_rotation_x(*angle))
                .collect(),
        ));
    }
    for (side, sign) in [(index("LeftArm"), -1.0f32), (index("RightArm"), 1.0)] {
        channels.push(rotation_channel(
            side,
            &times,
            [1.0, 0.0, -1.0, 0.0, 1.0]
                .iter()
                .map(|value| Quat::from_rotation_x(value * arm_amplitude * sign))
                .collect(),
        ));
    }
    for side in [index("LeftForearm"), index("RightForearm")] {
        channels.push(rotation_channel(
            side,
            &times,
            vec![Quat::from_rotation_x(0.25); 5],
        ));
    }
    channels.push(rotation_channel(
        index("Spine"),
        &times,
        vec![Quat::from_rotation_x(spine_pitch); 5],
    ));
    channels.push(rotation_channel(
        index("Head"),
        &times,
        vec![Quat::from_rotation_x(-spine_pitch * 0.6); 5],
    ));
    channels.push(translation_channel(
        index("Pelvis"),
        &times,
        vec![
            PELVIS_POS,
            PELVIS_POS + Vec3::Y * bob,
            PELVIS_POS,
            PELVIS_POS + Vec3::Y * bob,
            PELVIS_POS,
        ],
    ));
    AnimationClip {
        name: name.to_string(),
        duration,
        channels,
    }
}

fn jump(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("跳跃骨架完整");
    let times = [0.0, 0.12, 0.35];
    AnimationClip {
        name: "jump".to_string(),
        duration: 0.35,
        channels: vec![
            rotation_channel(
                index("LeftLeg"),
                &times,
                vec![
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(-0.6),
                    Quat::from_rotation_x(-0.45),
                ],
            ),
            rotation_channel(
                index("RightLeg"),
                &times,
                vec![
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(-0.6),
                    Quat::from_rotation_x(-0.45),
                ],
            ),
            rotation_channel(
                index("LeftKnee"),
                &times,
                vec![
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(1.0),
                    Quat::from_rotation_x(0.8),
                ],
            ),
            rotation_channel(
                index("RightKnee"),
                &times,
                vec![
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(1.0),
                    Quat::from_rotation_x(0.8),
                ],
            ),
            rotation_channel(
                index("LeftArm"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(-1.2),
                    Quat::from_rotation_x(-0.9),
                ],
            ),
            rotation_channel(
                index("RightArm"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(-1.2),
                    Quat::from_rotation_x(-0.9),
                ],
            ),
            rotation_channel(
                index("Spine"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(0.16),
                    Quat::from_rotation_x(0.1),
                ],
            ),
        ],
    }
}

fn fall(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("下落骨架完整");
    let times = [0.0, 0.25, 0.5];
    AnimationClip {
        name: "fall".to_string(),
        duration: 0.5,
        channels: vec![
            rotation_channel(
                index("LeftArm"),
                &times,
                vec![
                    Quat::from_rotation_z(-0.7),
                    Quat::from_rotation_z(-0.9),
                    Quat::from_rotation_z(-0.7),
                ],
            ),
            rotation_channel(
                index("RightArm"),
                &times,
                vec![
                    Quat::from_rotation_z(0.7),
                    Quat::from_rotation_z(0.9),
                    Quat::from_rotation_z(0.7),
                ],
            ),
            rotation_channel(
                index("LeftLeg"),
                &times,
                vec![
                    Quat::from_rotation_x(-0.2),
                    Quat::from_rotation_x(-0.35),
                    Quat::from_rotation_x(-0.2),
                ],
            ),
            rotation_channel(
                index("RightLeg"),
                &times,
                vec![
                    Quat::from_rotation_x(0.15),
                    Quat::from_rotation_x(0.3),
                    Quat::from_rotation_x(0.15),
                ],
            ),
            rotation_channel(
                index("Spine"),
                &times,
                vec![
                    Quat::from_rotation_x(-0.05),
                    Quat::from_rotation_x(-0.12),
                    Quat::from_rotation_x(-0.05),
                ],
            ),
        ],
    }
}

fn land(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("落地骨架完整");
    let times = [0.0, 0.08, 0.25];
    AnimationClip {
        name: "land".to_string(),
        duration: 0.25,
        channels: vec![
            rotation_channel(
                index("LeftKnee"),
                &times,
                vec![
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(0.85),
                    Quat::from_rotation_x(0.2),
                ],
            ),
            rotation_channel(
                index("RightKnee"),
                &times,
                vec![
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(0.85),
                    Quat::from_rotation_x(0.2),
                ],
            ),
            rotation_channel(
                index("LeftLeg"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(-0.4),
                    Quat::from_rotation_x(-0.05),
                ],
            ),
            rotation_channel(
                index("RightLeg"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(-0.4),
                    Quat::from_rotation_x(-0.05),
                ],
            ),
            rotation_channel(
                index("Spine"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(0.28),
                    Quat::from_rotation_x(0.05),
                ],
            ),
            rotation_channel(
                index("LeftArm"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(-0.55),
                    Quat::from_rotation_x(0.0),
                ],
            ),
            rotation_channel(
                index("RightArm"),
                &times,
                vec![
                    Quat::from_rotation_x(0.0),
                    Quat::from_rotation_x(-0.55),
                    Quat::from_rotation_x(0.0),
                ],
            ),
            translation_channel(
                index("Pelvis"),
                &times,
                vec![
                    PELVIS_POS,
                    PELVIS_POS - Vec3::new(0.0, 0.10, 0.0),
                    PELVIS_POS,
                ],
            ),
        ],
    }
}

fn attack(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("攻击骨架完整");
    let times = [0.0, 0.15, 0.30, 0.48, 0.65];
    AnimationClip {
        name: "attack".to_string(),
        duration: 0.65,
        channels: vec![
            rotation_channel(
                index("RightArm"),
                &times,
                vec![
                    Quat::from_rotation_x(0.2),
                    Quat::from_rotation_x(-1.6),
                    Quat::from_rotation_x(-2.0),
                    Quat::from_rotation_x(-1.0),
                    Quat::from_rotation_x(0.2),
                ],
            ),
            rotation_channel(
                index("RightForearm"),
                &times,
                vec![
                    Quat::from_rotation_x(0.2),
                    Quat::from_rotation_x(-0.5),
                    Quat::from_rotation_x(-0.2),
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(0.2),
                ],
            ),
            rotation_channel(
                index("LeftArm"),
                &times,
                vec![
                    Quat::from_rotation_x(-0.2),
                    Quat::from_rotation_x(0.3),
                    Quat::from_rotation_x(0.2),
                    Quat::from_rotation_x(0.1),
                    Quat::from_rotation_x(-0.2),
                ],
            ),
            rotation_channel(
                index("Spine"),
                &times,
                vec![
                    Quat::from_rotation_y(0.0),
                    Quat::from_rotation_y(0.25),
                    Quat::from_rotation_y(-0.25),
                    Quat::from_rotation_y(-0.1),
                    Quat::from_rotation_y(0.0),
                ],
            ),
            rotation_channel(
                index("Chest"),
                &times,
                vec![
                    Quat::from_rotation_y(0.0),
                    Quat::from_rotation_y(0.15),
                    Quat::from_rotation_y(-0.15),
                    Quat::from_rotation_y(0.0),
                    Quat::from_rotation_y(0.0),
                ],
            ),
            rotation_channel(
                index("Head"),
                &times,
                vec![
                    Quat::from_rotation_y(0.0),
                    Quat::from_rotation_y(-0.2),
                    Quat::from_rotation_y(0.15),
                    Quat::from_rotation_y(0.05),
                    Quat::from_rotation_y(0.0),
                ],
            ),
        ],
    }
}

fn mining(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("挖掘骨架完整");
    let times = [0.0, 0.25, 0.5];
    AnimationClip {
        name: "mining".to_string(),
        duration: 0.5,
        channels: vec![
            rotation_channel(
                index("RightArm"),
                &times,
                vec![
                    Quat::from_rotation_x(-1.4),
                    Quat::from_rotation_x(-0.1),
                    Quat::from_rotation_x(-1.4),
                ],
            ),
            rotation_channel(
                index("RightForearm"),
                &times,
                vec![
                    Quat::from_rotation_x(-0.3),
                    Quat::from_rotation_x(-0.1),
                    Quat::from_rotation_x(-0.3),
                ],
            ),
            rotation_channel(
                index("LeftArm"),
                &times,
                vec![
                    Quat::from_rotation_x(-0.5),
                    Quat::from_rotation_x(-0.25),
                    Quat::from_rotation_x(-0.5),
                ],
            ),
            rotation_channel(
                index("Spine"),
                &times,
                vec![
                    Quat::from_rotation_x(0.12),
                    Quat::from_rotation_x(0.3),
                    Quat::from_rotation_x(0.12),
                ],
            ),
            rotation_channel(
                index("Head"),
                &times,
                vec![
                    Quat::from_rotation_x(0.06),
                    Quat::from_rotation_x(0.14),
                    Quat::from_rotation_x(0.06),
                ],
            ),
        ],
    }
}

fn eat(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("进食骨架完整");
    let times = [0.0, 0.3, 0.6];
    AnimationClip {
        name: "eat".to_string(),
        duration: 0.6,
        channels: vec![
            rotation_channel(
                index("RightArm"),
                &times,
                vec![
                    Quat::from_rotation_x(-1.5),
                    Quat::from_rotation_x(-1.7),
                    Quat::from_rotation_x(-1.5),
                ],
            ),
            rotation_channel(
                index("RightForearm"),
                &times,
                vec![
                    Quat::from_rotation_x(-2.0),
                    Quat::from_rotation_x(-2.2),
                    Quat::from_rotation_x(-2.0),
                ],
            ),
            rotation_channel(
                index("Head"),
                &times,
                vec![
                    Quat::from_rotation_x(0.08),
                    Quat::from_rotation_x(0.16),
                    Quat::from_rotation_x(0.08),
                ],
            ),
            rotation_channel(
                index("Spine"),
                &times,
                vec![
                    Quat::from_rotation_x(0.05),
                    Quat::from_rotation_x(0.08),
                    Quat::from_rotation_x(0.05),
                ],
            ),
        ],
    }
}

fn hurt(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("受击骨架完整");
    let times = [0.0, 0.12, 0.35];
    AnimationClip {
        name: "hurt".to_string(),
        duration: 0.35,
        channels: vec![
            rotation_channel(
                index("Spine"),
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_x(-0.3), Quat::IDENTITY],
            ),
            rotation_channel(
                index("Chest"),
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_x(-0.15), Quat::IDENTITY],
            ),
            rotation_channel(
                index("Head"),
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_x(-0.25), Quat::IDENTITY],
            ),
            rotation_channel(
                index("LeftArm"),
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_z(-0.35), Quat::IDENTITY],
            ),
            rotation_channel(
                index("RightArm"),
                &times,
                vec![Quat::IDENTITY, Quat::from_rotation_z(0.35), Quat::IDENTITY],
            ),
        ],
    }
}

fn death(skeleton: &Skeleton) -> AnimationClip {
    let index = |joint: &str| skeleton.joint_index(joint).expect("死亡骨架完整");
    let times = [0.0, 0.35, 0.7, 1.2];
    AnimationClip {
        name: "death".to_string(),
        duration: 1.2,
        channels: vec![
            rotation_channel(
                index("Spine"),
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_x(0.5),
                    Quat::from_rotation_x(1.1),
                    Quat::from_rotation_x(1.35),
                ],
            ),
            rotation_channel(
                index("Head"),
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_x(0.2),
                    Quat::from_rotation_x(0.3),
                    Quat::from_rotation_x(0.4),
                ],
            ),
            rotation_channel(
                index("LeftArm"),
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_z(-0.5),
                    Quat::from_rotation_z(-0.8),
                    Quat::from_rotation_z(-1.0),
                ],
            ),
            rotation_channel(
                index("RightArm"),
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_z(0.5),
                    Quat::from_rotation_z(0.8),
                    Quat::from_rotation_z(1.0),
                ],
            ),
            rotation_channel(
                index("LeftKnee"),
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_x(0.4),
                    Quat::from_rotation_x(0.8),
                    Quat::from_rotation_x(0.9),
                ],
            ),
            rotation_channel(
                index("RightKnee"),
                &times,
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_x(0.4),
                    Quat::from_rotation_x(0.8),
                    Quat::from_rotation_x(0.9),
                ],
            ),
            translation_channel(
                index("Pelvis"),
                &times,
                vec![
                    PELVIS_POS,
                    PELVIS_POS - Vec3::new(0.0, 0.10, 0.0),
                    PELVIS_POS - Vec3::new(0.0, 0.35, 0.0),
                    PELVIS_POS - Vec3::new(0.0, 0.55, 0.0),
                ],
            ),
        ],
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

// ---------------------------------------------------------------------------
// 纹理
// ---------------------------------------------------------------------------

fn generate_texture() -> TextureImage {
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
    fill(&mut pixels, REGION_SHIRT, [64, 106, 168, 255]);
    fill(&mut pixels, REGION_SKIN, [226, 184, 146, 255]);
    fill(&mut pixels, REGION_PANTS, [58, 66, 96, 255]);
    fill(&mut pixels, REGION_HAIR, [72, 50, 36, 255]);
    fill(&mut pixels, REGION_FACE, [230, 188, 150, 255]);
    fill(&mut pixels, REGION_HEAD, [226, 184, 146, 255]);
    fill(&mut pixels, REGION_SHOE, [54, 46, 40, 255]);

    // 正面脸部两只眼睛。
    let face_x0 = (REGION_FACE.u0 * size as f32) as usize;
    let face_y0 = (REGION_FACE.v0 * size as f32) as usize;
    let eye_color = [32, 28, 30, 255];
    for eye_x in [face_x0 + 2, face_x0 + 8] {
        for dy in 0..3 {
            for dx in 0..3 {
                let index = ((face_y0 + 4 + dy) * size + eye_x + dx) * 4;
                pixels[index..index + 4].copy_from_slice(&eye_color);
            }
        }
    }
    TextureImage::new(size as u32, size as u32, pixels).expect("内置纹理尺寸与缓冲一致")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachment::PLAYER_BONES;

    #[test]
    fn player_model_has_the_complete_contract() {
        let asset = player();
        assert_eq!(asset.name, PLAYER_MODEL);
        let primitive = &asset.meshes[0].primitives[0];
        assert!(primitive.validate().is_ok());
        assert!(primitive.is_skinned());
        let skeleton = asset.skeleton.as_ref().expect("玩家带骨骼");
        assert!(skeleton.validate().is_ok());
        assert_eq!(skeleton.joints.len(), PLAYER_BONES.len());
        for name in PLAYER_BONES {
            assert!(skeleton.joint_index(name).is_some(), "缺少契约骨骼 {name}");
        }
        for matrix in skeleton.bind_skin_matrices() {
            assert!(
                matrix.abs_diff_eq(glam::Mat4::IDENTITY, 1e-4),
                "绑定姿态蒙皮矩阵应为单位矩阵"
            );
        }
        let bounds = asset.bounds().expect("玩家包围盒");
        assert!(bounds.min.y.abs() < 1e-4, "脚底应贴地");
        assert!((bounds.size().y - 1.8).abs() < 0.05, "玩家应约 1.8 格高");
    }

    #[test]
    fn right_hand_is_on_the_characters_right_side() {
        use crate::attachment::attachment_point::BoneId;
        use crate::model::skeleton::Pose;

        let asset = player();
        let skeleton = asset.skeleton.as_ref().expect("玩家带骨骼");
        let pose = Pose::from_locals(skeleton, &skeleton.bind_locals());
        let right = crate::attachment::attachment_transform::bone_global(
            &pose,
            skeleton,
            BoneId::RightHand,
        )
        .expect("右手骨骼")
        .w_axis
        .x;
        let left =
            crate::attachment::attachment_transform::bone_global(&pose, skeleton, BoneId::LeftHand)
                .expect("左手骨骼")
                .w_axis
                .x;
        // 模型面向 +Z 时，角色右手在本地 -X；绕 Y 旋转 180° 后出现在世界 +X。
        assert!(right < 0.0, "右手应在模型本地 -X");
        assert!(left > 0.0, "左手应在模型本地 +X");
        let root = glam::Mat4::from_rotation_y(std::f32::consts::PI);
        let world_right = (root
            * crate::attachment::attachment_transform::bone_global(
                &pose,
                skeleton,
                BoneId::RightHand,
            )
            .expect("右手骨骼"))
        .w_axis
        .x;
        assert!(world_right > 0.0, "面向 -Z 时右手应位于世界 +X");
    }

    #[test]
    fn all_required_animations_exist_and_validate() {
        let asset = player();
        for name in [
            "idle", "walk", "run", "jump", "fall", "land", "attack", "mining", "eat", "hurt",
            "death",
        ] {
            let index = asset.animation_index(name).unwrap_or_else(|| {
                panic!("缺少动画 {name}");
            });
            asset.animations[index]
                .validate()
                .unwrap_or_else(|error| panic!("动画 {name} 非法: {error}"));
        }
    }

    #[test]
    fn first_person_arms_are_static_and_forward_facing() {
        let asset = first_person_arms();
        assert_eq!(asset.name, FIRST_PERSON_ARMS_MODEL);
        assert!(asset.skeleton.is_none());
        for primitive in asset.meshes[0].primitives.iter() {
            assert!(primitive.validate().is_ok());
            assert!(!primitive.is_skinned());
        }
        let bounds = asset.bounds().expect("手臂包围盒");
        assert!(bounds.min.z < -0.2, "手臂应沿 -Z 向前伸展");
    }
}

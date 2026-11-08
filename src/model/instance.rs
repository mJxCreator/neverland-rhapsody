//! 模型实例：位置/朝向由外部驱动，动画播放与骨骼姿态保存在实例自身。
//!
//! 同一模型的多个实例共享 [`super::types::ModelAsset`]，
//! 但每个实例拥有独立姿态，互不影响。

use super::animation::AnimationPlayer;
use super::skeleton::Pose;
use super::types::{ModelAsset, ModelId, Transform};

/// 动画切换时的淡入状态，避免姿态瞬间跳变。
#[derive(Clone, Debug)]
struct FadeState {
    from: Vec<Transform>,
    elapsed: f32,
    duration: f32,
}

/// 单个模型实例的动画状态。
#[derive(Clone, Debug)]
pub struct ModelInstance {
    /// 绑定的模型资源。
    pub model: ModelId,
    pub player: AnimationPlayer,
    /// 当前骨骼姿态；无骨骼模型为空姿态。
    pose: Pose,
    fade: Option<FadeState>,
}

impl ModelInstance {
    /// 创建实例；此时尚未绑定任何片段。
    pub fn new(model: ModelId) -> Self {
        Self {
            model,
            player: AnimationPlayer::new(),
            pose: Pose {
                locals: Vec::new(),
                globals: Vec::new(),
                skin: Vec::new(),
            },
            fade: None,
        }
    }

    /// 切换模型，重置动画与姿态。
    pub fn set_model(&mut self, model: ModelId) {
        self.model = model;
        self.player.stop();
        self.pose = Pose {
            locals: Vec::new(),
            globals: Vec::new(),
            skin: Vec::new(),
        };
        self.fade = None;
    }

    /// 当前姿态。
    pub fn pose(&self) -> &Pose {
        &self.pose
    }

    /// 当前播放的片段名。
    pub fn clip_name<'a>(&self, asset: &'a ModelAsset) -> Option<&'a str> {
        self.player
            .clip()
            .and_then(|index| asset.animations.get(index))
            .map(|clip| clip.name.as_str())
    }

    /// 按名称切换动画；名称不存在时返回 `false` 且保持原状态。
    /// 相同片段再次调用不会重置播放进度。
    pub fn set_animation(&mut self, asset: &ModelAsset, name: &str, looping: bool) -> bool {
        let Some(index) = asset.animation_index(name) else {
            return false;
        };
        if self.player.clip() == Some(index) {
            return true;
        }
        // 记录切换瞬间的姿态作为淡入起点；首次播放没有可混合姿态。
        if !self.pose.locals.is_empty() {
            self.fade = Some(FadeState {
                from: self.pose.locals.clone(),
                elapsed: 0.0,
                duration: 0.2,
            });
        }
        self.player.play(index, looping);
        true
    }

    /// 直接播放片段下标。
    pub fn play_clip(&mut self, asset: &ModelAsset, index: usize, looping: bool) {
        if index >= asset.animations.len() {
            return;
        }
        if self.player.clip() == Some(index) {
            return;
        }
        if !self.pose.locals.is_empty() {
            self.fade = Some(FadeState {
                from: self.pose.locals.clone(),
                elapsed: 0.0,
                duration: 0.2,
            });
        }
        self.player.play(index, looping);
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.player.set_speed(speed);
    }

    pub fn pause(&mut self) {
        self.player.pause();
    }

    pub fn resume(&mut self) {
        self.player.resume();
    }

    /// 推进动画并重算姿态。
    pub fn update(&mut self, asset: &ModelAsset, delta: f32) {
        let Some(skeleton) = &asset.skeleton else {
            self.pose = Pose {
                locals: Vec::new(),
                globals: Vec::new(),
                skin: Vec::new(),
            };
            return;
        };
        let sample_time = self.player.advance(delta, &asset.animations);
        let mut locals = skeleton.bind_locals();
        if let Some(clip) = self
            .player
            .clip()
            .and_then(|index| asset.animations.get(index))
        {
            clip.sample(sample_time, &mut locals);
        }
        if let Some(fade) = &mut self.fade {
            let weight = if fade.duration > 0.0 {
                (fade.elapsed / fade.duration).clamp(0.0, 1.0)
            } else {
                1.0
            };
            locals = fade
                .from
                .iter()
                .zip(&locals)
                .map(|(from, to)| Transform::lerp(*from, *to, weight))
                .collect();
            fade.elapsed += delta.max(0.0);
            if weight >= 1.0 {
                self.fade = None;
            }
        }
        self.pose = Pose::from_locals(skeleton, &locals);
    }

    /// 设置绑定姿态（无动画时的静态外观）。
    pub fn apply_bind_pose(&mut self, asset: &ModelAsset) {
        if let Some(skeleton) = &asset.skeleton {
            self.pose = Pose::bind(skeleton);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::animation::{
        AnimationChannel, AnimationClip, AnimationPath, Interpolation, KeyframeValues,
    };
    use crate::model::skeleton::{Joint, Skeleton};
    use glam::Vec3;

    fn asset() -> ModelAsset {
        let joints = vec![Joint {
            name: "body".to_string(),
            parent: None,
            local: Transform::IDENTITY,
        }];
        let mut skeleton = Skeleton {
            joints,
            inverse_bind: vec![glam::Mat4::IDENTITY],
        };
        skeleton.inverse_bind = skeleton
            .global_matrices(&skeleton.bind_locals())
            .iter()
            .map(|matrix| matrix.inverse())
            .collect();
        let clip = AnimationClip {
            name: "walk".to_string(),
            duration: 1.0,
            channels: vec![AnimationChannel {
                joint: 0,
                path: AnimationPath::Translation,
                interpolation: Interpolation::Linear,
                times: vec![0.0, 1.0],
                values: KeyframeValues::Vec3(vec![Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0)]),
            }],
        };
        ModelAsset {
            name: "test".to_string(),
            meshes: Vec::new(),
            materials: Vec::new(),
            textures: Vec::new(),
            skeleton: Some(skeleton),
            animations: vec![clip],
        }
    }

    #[test]
    fn update_samples_animation_into_pose() {
        let asset = asset();
        let mut instance = ModelInstance::new(ModelId::new(0));
        assert!(instance.set_animation(&asset, "walk", true));
        instance.update(&asset, 0.5);
        assert!((instance.pose().locals[0].translation.z - 0.5).abs() < 1e-4);
    }

    #[test]
    fn missing_animation_keeps_existing_state() {
        let asset = asset();
        let mut instance = ModelInstance::new(ModelId::new(0));
        assert!(!instance.set_animation(&asset, "missing", true));
        assert_eq!(instance.player.clip(), None);
    }

    #[test]
    fn instances_do_not_share_pose() {
        let asset = asset();
        let mut first = ModelInstance::new(ModelId::new(0));
        let mut second = ModelInstance::new(ModelId::new(0));
        first.set_animation(&asset, "walk", true);
        first.update(&asset, 0.5);
        second.update(&asset, 0.0);
        assert!((first.pose().locals[0].translation.z - 0.5).abs() < 1e-4);
        assert!(second.pose().locals[0].translation.z.abs() < 1e-4);
    }

    #[test]
    fn switching_animation_fades_without_jump() {
        let asset = asset();
        let mut instance = ModelInstance::new(ModelId::new(0));
        instance.set_animation(&asset, "walk", true);
        instance.update(&asset, 0.5);
        assert!((instance.pose().locals[0].translation.z - 0.5).abs() < 1e-4);
        // 切换片段时旧姿态作为淡入起点，首帧不会直接跳到新片段起点。
        instance.player.stop();
        instance.set_animation(&asset, "walk", true);
        instance.update(&asset, 0.0);
        assert!(instance.pose().locals[0].translation.z > 0.4);
    }
}

//! 动画：关键帧数据、插值、片段采样与播放状态机。
//!
//! 支持 TRANSLATION/ROTATION/SCALE 通道与 LINEAR/STEP/CUBICSPLINE 插值。
//! 播放器只推进时间，姿态混合由模型实例负责。

use glam::{Quat, Vec3};

use super::types::Transform;

/// 动画通道作用的属性。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationPath {
    Translation,
    Rotation,
    Scale,
}

impl AnimationPath {
    pub fn from_gltf(value: &str) -> Option<Self> {
        match value {
            "translation" => Some(Self::Translation),
            "rotation" => Some(Self::Rotation),
            "scale" => Some(Self::Scale),
            _ => None,
        }
    }
}

/// 关键帧插值方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

impl Interpolation {
    pub fn from_gltf(value: Option<&str>) -> (Self, bool) {
        match value.unwrap_or("LINEAR") {
            "LINEAR" => (Self::Linear, true),
            "STEP" => (Self::Step, true),
            "CUBICSPLINE" => (Self::CubicSpline, true),
            _ => (Self::Linear, false),
        }
    }
}

/// 关键帧数据；旋转使用四元数，位移/缩放使用向量。
#[derive(Clone, Debug, PartialEq)]
pub enum KeyframeValues {
    Vec3(Vec<Vec3>),
    Quat(Vec<Quat>),
}

impl KeyframeValues {
    pub fn len(&self) -> usize {
        match self {
            Self::Vec3(values) => values.len(),
            Self::Quat(values) => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 一条动画通道。
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationChannel {
    /// 目标关节下标。
    pub joint: usize,
    pub path: AnimationPath,
    pub interpolation: Interpolation,
    /// 关键帧时间（秒，升序）。
    pub times: Vec<f32>,
    pub values: KeyframeValues,
}

impl AnimationChannel {
    /// CUBICSPLINE 的每个关键帧有三组值（入切线、值、出切线）。
    fn value_multiplier(&self) -> usize {
        if self.interpolation == Interpolation::CubicSpline {
            3
        } else {
            1
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.times.is_empty() {
            return Err("动画通道没有关键帧".to_string());
        }
        let expected = self.times.len() * self.value_multiplier();
        if self.values.len() != expected {
            return Err(format!(
                "关键帧数量与数值数量不一致：{} 个时间，{} 个值，插值 {:?}",
                self.times.len(),
                self.values.len(),
                self.interpolation
            ));
        }
        if self.times.windows(2).any(|pair| pair[0] > pair[1]) {
            return Err("关键帧时间必须升序".to_string());
        }
        for time in &self.times {
            if !time.is_finite() || *time < 0.0 {
                return Err("关键帧时间非法".to_string());
            }
        }
        match (self.path, &self.values) {
            (AnimationPath::Rotation, KeyframeValues::Quat(_)) => {}
            (AnimationPath::Translation | AnimationPath::Scale, KeyframeValues::Vec3(_)) => {}
            _ => return Err("通道属性与数值类型不一致".to_string()),
        }
        Ok(())
    }

    /// 在指定时间采样，写入目标局部变换。
    pub fn apply(&self, time: f32, target: &mut Transform) {
        let Some((index, weight)) = self.frame_at(time) else {
            return;
        };
        match (&self.values, self.path) {
            (KeyframeValues::Vec3(values), AnimationPath::Translation) => {
                target.translation = sample_vec3(
                    values,
                    index,
                    weight,
                    self.interpolation,
                    self.times[index],
                    self.next_time(index),
                );
            }
            (KeyframeValues::Vec3(values), AnimationPath::Scale) => {
                target.scale = sample_vec3(
                    values,
                    index,
                    weight,
                    self.interpolation,
                    self.times[index],
                    self.next_time(index),
                );
            }
            (KeyframeValues::Quat(values), AnimationPath::Rotation) => {
                target.rotation = sample_quat(
                    values,
                    index,
                    weight,
                    self.interpolation,
                    self.times[index],
                    self.next_time(index),
                );
            }
            _ => {}
        }
    }

    fn next_time(&self, index: usize) -> f32 {
        self.times
            .get(index + 1)
            .copied()
            .unwrap_or_else(|| self.times.get(index).copied().unwrap_or(0.0))
    }

    /// 找到时间所在的关键帧下标与帧内比例。
    fn frame_at(&self, time: f32) -> Option<(usize, f32)> {
        if self.times.is_empty() {
            return None;
        }
        let index = self
            .times
            .partition_point(|value| *value <= time)
            .saturating_sub(1);
        let start = self.times[index];
        let end = self.next_time(index);
        let weight = if end > start {
            ((time - start) / (end - start)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some((index, weight))
    }
}

fn sample_vec3(
    values: &[Vec3],
    index: usize,
    weight: f32,
    interpolation: Interpolation,
    start: f32,
    end: f32,
) -> Vec3 {
    match interpolation {
        Interpolation::Step => values[index],
        Interpolation::Linear => {
            let next = values.get(index + 1).copied().unwrap_or(values[index]);
            values[index].lerp(next, weight)
        }
        Interpolation::CubicSpline => {
            let value = values[(index * 3 + 1).min(values.len() - 1)];
            let out_tangent = values[(index * 3 + 2).min(values.len() - 1)];
            let next_value = values.get(index * 3 + 4).copied().unwrap_or(value);
            let in_tangent = values.get(index * 3 + 3).copied().unwrap_or(Vec3::ZERO);
            cubic_vec3(
                value,
                out_tangent,
                next_value,
                in_tangent,
                weight,
                end - start,
            )
        }
    }
}

fn sample_quat(
    values: &[Quat],
    index: usize,
    weight: f32,
    interpolation: Interpolation,
    start: f32,
    end: f32,
) -> Quat {
    match interpolation {
        Interpolation::Step => values[index],
        Interpolation::Linear => {
            let next = values.get(index + 1).copied().unwrap_or(values[index]);
            values[index].normalize().lerp(next.normalize(), weight)
        }
        Interpolation::CubicSpline => {
            let value = values[(index * 3 + 1).min(values.len() - 1)].normalize();
            let out_tangent = values[(index * 3 + 2).min(values.len() - 1)];
            let next_value = values
                .get(index * 3 + 4)
                .copied()
                .unwrap_or(value)
                .normalize();
            let in_tangent = values.get(index * 3 + 3).copied().unwrap_or(Quat::IDENTITY);
            // 四元数三次插值用分量 Hermite 后重新归一化，足够避免明显跳变。
            let value = Quat::from_xyzw(
                cubic_scalar(
                    value.x,
                    out_tangent.x,
                    next_value.x,
                    in_tangent.x,
                    weight,
                    end - start,
                ),
                cubic_scalar(
                    value.y,
                    out_tangent.y,
                    next_value.y,
                    in_tangent.y,
                    weight,
                    end - start,
                ),
                cubic_scalar(
                    value.z,
                    out_tangent.z,
                    next_value.z,
                    in_tangent.z,
                    weight,
                    end - start,
                ),
                cubic_scalar(
                    value.w,
                    out_tangent.w,
                    next_value.w,
                    in_tangent.w,
                    weight,
                    end - start,
                ),
            );
            value.normalize()
        }
    }
}

fn cubic_vec3(
    value: Vec3,
    out_tangent: Vec3,
    next_value: Vec3,
    in_tangent: Vec3,
    weight: f32,
    duration: f32,
) -> Vec3 {
    Vec3::new(
        cubic_scalar(
            value.x,
            out_tangent.x,
            next_value.x,
            in_tangent.x,
            weight,
            duration,
        ),
        cubic_scalar(
            value.y,
            out_tangent.y,
            next_value.y,
            in_tangent.y,
            weight,
            duration,
        ),
        cubic_scalar(
            value.z,
            out_tangent.z,
            next_value.z,
            in_tangent.z,
            weight,
            duration,
        ),
    )
}

/// 三次 Hermite 基函数；tangent 已按 glTF 规范缩放为值/秒。
fn cubic_scalar(
    start: f32,
    out_tangent: f32,
    end: f32,
    in_tangent: f32,
    weight: f32,
    duration: f32,
) -> f32 {
    if duration <= 0.0 {
        return start;
    }
    let weight2 = weight * weight;
    let weight3 = weight2 * weight;
    let h00 = 2.0 * weight3 - 3.0 * weight2 + 1.0;
    let h10 = weight3 - 2.0 * weight2 + weight;
    let h01 = -2.0 * weight3 + 3.0 * weight2;
    let h11 = weight3 - weight2;
    h00 * start + h10 * duration * out_tangent + h01 * end + h11 * duration * in_tangent
}

/// 一个动画片段。
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationClip {
    pub name: String,
    /// 片段时长（秒），由最长通道决定。
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
}

impl AnimationClip {
    /// 采样到目标局部变换列表；未受影响的关节保持原值。
    pub fn sample(&self, time: f32, locals: &mut [Transform]) {
        let time = clamp_time(time, self.duration);
        for channel in &self.channels {
            if let Some(target) = locals.get_mut(channel.joint) {
                channel.apply(time, target);
            }
        }
    }

    /// 通道作用于的关节是否覆盖全部关节；用于统计。
    pub fn targeted_joints(&self) -> usize {
        self.channels
            .iter()
            .map(|channel| channel.joint)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.duration < 0.0 || !self.duration.is_finite() {
            return Err("动画片段时长非法".to_string());
        }
        for channel in &self.channels {
            channel.validate()?;
        }
        Ok(())
    }
}

fn clamp_time(time: f32, duration: f32) -> f32 {
    if !time.is_finite() {
        return 0.0;
    }
    time.clamp(0.0, duration.max(0.0))
}

/// 动画播放状态：时间推进、循环、暂停与速度。
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationPlayer {
    clip: Option<usize>,
    time: f32,
    speed: f32,
    playing: bool,
    looping: bool,
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self {
            clip: None,
            time: 0.0,
            speed: 1.0,
            playing: false,
            looping: true,
        }
    }
}

impl AnimationPlayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前片段下标。
    pub fn clip(&self) -> Option<usize> {
        self.clip
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_looping(&self) -> bool {
        self.looping
    }

    /// 切换到指定片段并从头播放。
    pub fn play(&mut self, clip: usize, looping: bool) {
        self.clip = Some(clip);
        self.time = 0.0;
        self.playing = true;
        self.looping = looping;
    }

    /// 保持当前时间继续播放。
    pub fn resume(&mut self) {
        if self.clip.is_some() {
            self.playing = true;
        }
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// 停止并清空片段。
    pub fn stop(&mut self) {
        self.playing = false;
        self.clip = None;
        self.time = 0.0;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.max(0.0);
    }

    pub fn set_time(&mut self, time: f32) {
        self.time = time.max(0.0);
    }

    /// 推进时间；返回采样时间。非循环片段到末尾后自动停止。
    pub fn advance(&mut self, delta: f32, clips: &[AnimationClip]) -> f32 {
        let Some(index) = self.clip else {
            return 0.0;
        };
        let Some(clip) = clips.get(index) else {
            self.stop();
            return 0.0;
        };
        if self.playing {
            self.time += delta.max(0.0) * self.speed;
            let duration = clip.duration.max(0.0);
            if duration > 0.0 {
                if self.looping {
                    self.time %= duration;
                } else if self.time >= duration {
                    self.time = duration;
                    self.playing = false;
                }
            } else {
                self.time = 0.0;
            }
        }
        clamp_time(self.time, clip.duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_channel() -> AnimationChannel {
        AnimationChannel {
            joint: 0,
            path: AnimationPath::Translation,
            interpolation: Interpolation::Linear,
            times: vec![0.0, 1.0],
            values: KeyframeValues::Vec3(vec![Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0)]),
        }
    }

    fn clip(name: &str, duration: f32) -> AnimationClip {
        AnimationClip {
            name: name.to_string(),
            duration,
            channels: vec![linear_channel()],
        }
    }

    #[test]
    fn linear_interpolation_blends_between_keys() {
        let mut locals = [Transform::IDENTITY];
        linear_channel().apply(0.5, &mut locals[0]);
        assert!((locals[0].translation - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn step_interpolation_holds_the_first_key() {
        let mut channel = linear_channel();
        channel.interpolation = Interpolation::Step;
        let mut locals = [Transform::IDENTITY];
        channel.apply(0.99, &mut locals[0]);
        assert_eq!(locals[0].translation, Vec3::ZERO);
    }

    #[test]
    fn cubic_spline_uses_three_values_per_key() {
        let channel = AnimationChannel {
            joint: 0,
            path: AnimationPath::Translation,
            interpolation: Interpolation::CubicSpline,
            times: vec![0.0, 1.0],
            // 两帧 × 3 个值：入切线、值、出切线；出切线为 2 时中点明显快于线性。
            values: KeyframeValues::Vec3(vec![
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::ZERO,
            ]),
        };
        assert!(channel.validate().is_ok());
        let mut locals = [Transform::IDENTITY];
        channel.apply(0.5, &mut locals[0]);
        assert!((locals[0].translation.x - 0.75).abs() < 1e-4);
    }

    #[test]
    fn invalid_channel_data_is_reported() {
        let mut channel = linear_channel();
        channel.values = KeyframeValues::Vec3(vec![Vec3::ZERO]);
        assert!(channel.validate().is_err());
    }

    #[test]
    fn player_loops_and_stops_at_end() {
        let clips = vec![clip("walk", 2.0)];
        let mut player = AnimationPlayer::new();
        player.play(0, true);
        player.advance(2.5, &clips);
        assert!(player.time() < 2.0, "循环播放应回绕");
        player.play(0, false);
        player.advance(3.0, &clips);
        assert!((player.time() - 2.0).abs() < 1e-5);
        assert!(!player.is_playing());
    }

    #[test]
    fn pause_and_resume_preserve_time() {
        let clips = vec![clip("idle", 1.0)];
        let mut player = AnimationPlayer::new();
        player.play(0, true);
        player.advance(0.3, &clips);
        player.pause();
        player.advance(0.5, &clips);
        assert!((player.time() - 0.3).abs() < 1e-5);
        player.resume();
        player.advance(0.2, &clips);
        assert!((player.time() - 0.5).abs() < 1e-5);
    }
}

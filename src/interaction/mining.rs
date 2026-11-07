//! 持续挖掘：把按住左键的时间累计为目标方块的实际挖掘进度。
//!
//! 进度只依赖真实时间与方块硬度，与渲染帧率无关；目标方块一旦变化或失效，
//! 已累计的进度立即清零，不会错误继承到下一个方块。

use glam::IVec3;

use crate::world::Block;

/// 基础挖掘系数：硬度 1.0 的方块需要 1 秒。
pub const SECONDS_PER_HARDNESS: f32 = 1.0;

/// 当前待挖掘的目标：世界坐标与方块类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningTarget {
    pub block: IVec3,
    pub block_type: Block,
}

/// 一次挖掘状态推进的结果。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MiningOutcome {
    /// 未在挖掘：松开按键、没有目标或目标不可破坏。
    Idle,
    /// 正在挖掘，进度范围 0..1。
    Mining { progress: f32 },
    /// 进度达到 100%，调用方应破坏该方块。
    Completed { block: IVec3 },
}

/// 与目标方块绑定的挖掘状态。
#[derive(Debug, Default)]
pub struct MiningState {
    target: Option<IVec3>,
    target_block: Option<Block>,
    elapsed: f32,
    required: f32,
}

impl MiningState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前是否正在挖掘有效目标。
    pub fn is_mining(&self) -> bool {
        self.target.is_some()
    }

    /// 当前目标方块的世界坐标。
    pub fn target(&self) -> Option<IVec3> {
        self.target
    }

    /// 已累计的挖掘时间（秒）。
    pub fn elapsed_time(&self) -> f32 {
        self.elapsed
    }

    /// 当前目标所需的挖掘时间（秒）。
    pub fn required_time(&self) -> f32 {
        self.required
    }

    /// 当前挖掘进度；未在挖掘时返回 `None`。
    pub fn progress(&self) -> Option<f32> {
        self.target?;
        if self.required <= 0.0 {
            return None;
        }
        Some((self.elapsed / self.required).clamp(0.0, 1.0))
    }

    /// 立即取消当前挖掘并清空进度。
    pub fn cancel(&mut self) {
        self.target = None;
        self.target_block = None;
        self.elapsed = 0.0;
        self.required = 0.0;
    }

    /// 推进一帧挖掘状态。
    ///
    /// `held` 表示左键是否仍按住，`target` 是本帧准星命中的方块（含类型）。
    /// `delta_secs` 是本帧实际经过的秒数。
    pub fn update(
        &mut self,
        held: bool,
        target: Option<MiningTarget>,
        delta_secs: f32,
    ) -> MiningOutcome {
        if !held {
            self.cancel();
            return MiningOutcome::Idle;
        }

        let Some(target) = target else {
            self.cancel();
            return MiningOutcome::Idle;
        };

        if !target.block_type.is_breakable() {
            self.cancel();
            return MiningOutcome::Idle;
        }

        let required = target.block_type.mining_time(SECONDS_PER_HARDNESS);
        if required <= 0.0 {
            self.cancel();
            return MiningOutcome::Idle;
        }

        // 目标方块或方块类型变化时，进度必须从零开始，避免继承上一个目标的进度。
        if self.target != Some(target.block) || self.target_block != Some(target.block_type) {
            self.target = Some(target.block);
            self.target_block = Some(target.block_type);
            self.elapsed = 0.0;
        }
        self.required = required;

        let delta = if delta_secs.is_finite() {
            delta_secs.max(0.0)
        } else {
            0.0
        };
        self.elapsed += delta;

        if self.elapsed >= self.required {
            let block = target.block;
            self.cancel();
            return MiningOutcome::Completed { block };
        }

        MiningOutcome::Mining {
            progress: (self.elapsed / self.required).clamp(0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Block;

    fn target(block: IVec3, block_type: Block) -> Option<MiningTarget> {
        Some(MiningTarget { block, block_type })
    }

    #[test]
    fn mining_accumulates_only_while_held() {
        let mut state = MiningState::new();
        let dirt = target(IVec3::new(4, 5, 6), Block::Dirt);

        assert_eq!(
            state.update(true, dirt, 0.2),
            MiningOutcome::Mining { progress: 0.4 }
        );
        assert!(state.is_mining());
        assert!((state.elapsed_time() - 0.2).abs() < 1e-6);

        // 松开左键立即取消，且不保留进度。
        assert_eq!(state.update(false, dirt, 0.2), MiningOutcome::Idle);
        assert!(!state.is_mining());
        assert_eq!(state.elapsed_time(), 0.0);
    }

    #[test]
    fn different_hardness_needs_different_time() {
        let mut state = MiningState::new();
        let dirt = target(IVec3::ZERO, Block::Dirt);
        state.update(true, dirt, 0.1);

        let mut stone_state = MiningState::new();
        let stone = target(IVec3::ZERO, Block::Stone);
        stone_state.update(true, stone, 0.1);

        assert_eq!(state.required_time(), 0.5);
        assert_eq!(stone_state.required_time(), 1.5);
        assert_eq!(state.progress(), Some(0.2));
        assert!((stone_state.progress().unwrap() - 0.1 / 1.5).abs() < 1e-6);
    }

    #[test]
    fn reaching_full_time_completes_and_resets() {
        let mut state = MiningState::new();
        let dirt = target(IVec3::new(1, 2, 3), Block::Dirt);

        state.update(true, dirt, 0.3);
        assert!(
            matches!(state.update(true, dirt, 0.2), MiningOutcome::Completed { block } if block == IVec3::new(1, 2, 3))
        );
        assert!(!state.is_mining());
        assert_eq!(state.progress(), None);
    }

    #[test]
    fn switching_target_resets_progress() {
        let mut state = MiningState::new();
        let first = target(IVec3::new(1, 2, 3), Block::Dirt);
        let second = target(IVec3::new(2, 2, 3), Block::Stone);

        state.update(true, first, 0.3);
        assert_eq!(state.progress(), Some(0.6));

        let outcome = state.update(true, second, 0.1);
        assert_eq!(
            outcome,
            MiningOutcome::Mining {
                progress: 0.1 / 1.5
            }
        );
        assert!((state.elapsed_time() - 0.1).abs() < 1e-6);
    }

    #[test]
    fn losing_the_target_cancels_mining() {
        let mut state = MiningState::new();
        let dirt = target(IVec3::new(1, 2, 3), Block::Dirt);
        state.update(true, dirt, 0.2);

        assert_eq!(state.update(true, None, 0.2), MiningOutcome::Idle);
        assert!(!state.is_mining());
        assert_eq!(state.progress(), None);
    }

    #[test]
    fn air_and_unbreakable_blocks_cannot_be_mined() {
        let mut state = MiningState::new();
        let air = target(IVec3::ZERO, Block::Air);
        assert_eq!(state.update(true, air, 1.0), MiningOutcome::Idle);
        assert!(!state.is_mining());
    }

    #[test]
    fn invalid_or_negative_delta_does_not_advance_progress() {
        let mut state = MiningState::new();
        let dirt = target(IVec3::ZERO, Block::Dirt);

        state.update(true, dirt, f32::NAN);
        assert_eq!(state.progress(), Some(0.0));
        state.update(true, dirt, -1.0);
        assert_eq!(state.progress(), Some(0.0));
    }

    #[test]
    fn target_set_to_air_after_progress_is_cancelled() {
        let mut state = MiningState::new();
        let dirt = target(IVec3::ONE, Block::Dirt);
        state.update(true, dirt, 0.2);

        let replaced = target(IVec3::ONE, Block::Air);
        assert_eq!(state.update(true, replaced, 0.2), MiningOutcome::Idle);
        assert_eq!(state.progress(), None);
    }
}

//! 玩家生存状态聚合：饥饿、体力、自然恢复与床复活点。
//!
//! 主循环只需要通过本结构推进生存状态；死亡、暂停与界面阻塞由调用方以显式
//! 参数传入，避免各子系统各自判断游戏状态。

use glam::IVec3;
use serde::{Deserialize, Serialize};

use super::hunger::{Activity, Hunger, SavedHunger};
use super::regeneration::Regenerator;
use super::stamina::{SavedStamina, Stamina};

/// 存档中的玩家生存数据。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedVitals {
    #[serde(default)]
    pub hunger: SavedHunger,
    #[serde(default)]
    pub stamina: SavedStamina,
    /// 床复活点；`None` 表示尚未设置或已失效。
    #[serde(default)]
    pub respawn: Option<[i32; 3]>,
}

impl SavedVitals {
    /// 清洗越界复活点；垂直坐标越界时视为未设置。
    pub fn sanitized(self) -> Self {
        let respawn = self
            .respawn
            .filter(|point| (0..crate::world::WORLD_HEIGHT).contains(&point[1]));
        Self { respawn, ..self }
    }
}

/// 玩家生存运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vitals {
    hunger: Hunger,
    stamina: Stamina,
    regeneration: Regenerator,
    respawn: Option<IVec3>,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            hunger: Hunger::new(),
            stamina: Stamina::new(),
            regeneration: Regenerator::new(),
            respawn: None,
        }
    }
}

impl Vitals {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_saved(saved: SavedVitals) -> Self {
        let saved = saved.sanitized();
        Self {
            hunger: Hunger::from_saved(saved.hunger),
            stamina: Stamina::from_saved(saved.stamina),
            regeneration: Regenerator::new(),
            respawn: saved
                .respawn
                .map(|point| IVec3::new(point[0], point[1], point[2])),
        }
    }

    pub fn to_saved(self) -> SavedVitals {
        SavedVitals {
            hunger: self.hunger.to_saved(),
            stamina: self.stamina.to_saved(),
            respawn: self.respawn.map(|point| [point.x, point.y, point.z]),
        }
    }

    pub fn hunger(&self) -> &Hunger {
        &self.hunger
    }

    pub fn hunger_mut(&mut self) -> &mut Hunger {
        &mut self.hunger
    }

    pub fn stamina(&self) -> &Stamina {
        &self.stamina
    }

    pub fn stamina_mut(&mut self) -> &mut Stamina {
        &mut self.stamina
    }

    pub fn respawn(&self) -> Option<IVec3> {
        self.respawn
    }

    pub fn set_respawn(&mut self, point: IVec3) {
        self.respawn = Some(point);
    }

    pub fn clear_respawn(&mut self) {
        self.respawn = None;
    }

    /// 是否允许开始冲刺：存活、未阻塞、有移动输入、体力与饥饿均允许。
    pub fn can_sprint(&self, alive: bool, blocked: bool, moving: bool) -> bool {
        alive && !blocked && moving && self.stamina.can_sprint() && self.hunger.can_sprint()
    }

    /// 推进生存状态，返回本帧应恢复的生命值（0 或 1）。
    ///
    /// - `blocked`：界面阻塞期间体力保持不变，但饥饿仍按游戏时间消耗。
    /// - `alive` 为假时不推进任何数值。
    /// - `needs_heal`：当前生命值未满。
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        delta_seconds: f32,
        activity: Activity,
        sprinting: bool,
        moving: bool,
        blocked: bool,
        alive: bool,
        needs_heal: bool,
    ) -> u32 {
        if !alive || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return 0;
        }
        if !blocked {
            self.stamina.update(
                delta_seconds,
                sprinting,
                moving,
                self.hunger.regeneration_scale(),
            );
        }
        self.hunger.update(delta_seconds, activity);
        self.hunger.tick_effect(delta_seconds);

        let eligible = needs_heal && self.hunger.can_regenerate();
        if self.regeneration.tick(delta_seconds, eligible) {
            self.hunger.drain(self.regeneration.hunger_cost());
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::survival::regeneration::REGEN_INTERVAL;

    #[test]
    fn default_vitals_are_full() {
        let vitals = Vitals::new();
        assert_eq!(vitals.hunger().value(), 20.0);
        assert_eq!(vitals.stamina().value(), 100.0);
        assert!(vitals.respawn().is_none());
    }

    #[test]
    fn dead_players_do_not_change() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(5.0);
        let before = *vitals.hunger();
        vitals.update(10.0, Activity::Sprinting, true, true, false, false, true);
        assert_eq!(*vitals.hunger(), before);
    }

    #[test]
    fn blocked_ui_pauses_stamina_but_not_hunger() {
        let mut vitals = Vitals::new();
        vitals.stamina_mut().drain_sprint(1.0);
        let stamina_before = vitals.stamina().value();
        let saturation_before = vitals.hunger().saturation();
        vitals.update(1.0, Activity::Walking, true, true, true, true, false);
        assert_eq!(vitals.stamina().value(), stamina_before);
        assert!(vitals.hunger().saturation() < saturation_before);
    }

    #[test]
    fn regeneration_heals_and_costs_hunger() {
        let mut vitals = Vitals::new();
        let saturation_before = vitals.hunger().saturation();
        let healed = vitals.update(
            REGEN_INTERVAL,
            Activity::Idle,
            false,
            false,
            false,
            true,
            true,
        );
        assert_eq!(healed, 1);
        assert!(vitals.hunger().saturation() < saturation_before);
    }

    #[test]
    fn regeneration_requires_enough_hunger() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(20.0);
        assert!(!vitals.hunger().can_regenerate());
        let healed = vitals.update(
            REGEN_INTERVAL,
            Activity::Idle,
            false,
            false,
            false,
            true,
            true,
        );
        assert_eq!(healed, 0);
    }

    #[test]
    fn sprint_requires_stamina_and_hunger() {
        let mut vitals = Vitals::new();
        assert!(vitals.can_sprint(true, false, true));
        vitals.stamina_mut().drain_sprint(10.0);
        assert!(!vitals.can_sprint(true, false, true));
        assert!(!vitals.can_sprint(true, false, false));
        assert!(!vitals.can_sprint(false, false, true));
    }

    #[test]
    fn saved_vitals_round_trip() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(5.0);
        vitals.stamina_mut().drain_sprint(2.0);
        vitals.set_respawn(IVec3::new(3, 4, 5));
        let restored = Vitals::from_saved(vitals.to_saved());
        assert_eq!(restored.hunger().value(), vitals.hunger().value());
        assert_eq!(restored.stamina().value(), vitals.stamina().value());
        assert_eq!(restored.respawn(), Some(IVec3::new(3, 4, 5)));
    }

    #[test]
    fn out_of_range_respawn_is_dropped() {
        let saved = SavedVitals {
            respawn: Some([0, -4, 0]),
            ..SavedVitals::default()
        };
        assert!(Vitals::from_saved(saved).respawn().is_none());
    }
}

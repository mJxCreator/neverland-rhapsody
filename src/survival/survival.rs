//! 玩家生存状态聚合：饥饿、体力、自然恢复与床复活点。
//!
//! 主循环只需要通过本结构推进生存状态；死亡、暂停与界面阻塞由调用方以显式
//! 参数传入，避免各子系统各自判断游戏状态。
//!
//! 体力上限始终等于当前饥饿值。体力耗尽一轮后扣除 1 点饥饿并以新上限补满；
//! 饥饿不低于 50% 时按 `1000 / hunger` 秒恢复 1 点生命，每次恢复消耗 20 点体力。

use glam::IVec3;
use serde::{Deserialize, Serialize};

use super::hunger::{Hunger, SavedHunger};
use super::regeneration::{Regenerator, HEAL_STAMINA_COST, NAT_HEART_RECOVER};
use super::stamina::{SavedStamina, Stamina, JUMP_COST, MINING_DRAIN_PER_SECOND};

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
    /// 清洗越界数值：饥饿裁剪到上限，体力不得超过当前饥饿，复活点垂直越界时视为未设置。
    pub fn sanitized(self) -> Self {
        let hunger = self.hunger.sanitized();
        let stamina = self.stamina.sanitized(hunger.hunger);
        let respawn = self
            .respawn
            .filter(|point| (0..crate::world::WORLD_HEIGHT).contains(&point[1]));
        Self {
            hunger,
            stamina,
            respawn,
        }
    }

    /// 把 v9 及更早存档的 0–20 饥饿换算为 0–100，并同步裁剪体力。
    pub fn migrated_to_v10(self) -> Self {
        let hunger = SavedHunger {
            hunger: (self.hunger.hunger * 5.0).clamp(0.0, super::hunger::MAX_HUNGER),
        };
        let stamina = self.stamina.sanitized(hunger.hunger);
        Self {
            hunger,
            stamina,
            respawn: self.respawn,
        }
    }
}

/// 一帧内影响生存结算的动作状态。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VitalsInput {
    /// 是否存在移动输入。
    pub moving: bool,
    /// 是否正在奔跑。
    pub sprinting: bool,
    /// 本帧真实发生的跳跃次数。
    pub jumps: u32,
    /// 界面是否阻塞世界输入；阻塞期间体力保持不变。
    pub blocked: bool,
}

/// 玩家生存运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vitals {
    hunger: Hunger,
    stamina: Stamina,
    regeneration: Regenerator,
    /// 破坏方块的累计秒数，满 1 秒结算 1 点体力消耗。
    mining_seconds: f32,
    respawn: Option<IVec3>,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            hunger: Hunger::new(),
            stamina: Stamina::new(),
            regeneration: Regenerator::new(),
            mining_seconds: 0.0,
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
        let hunger = Hunger::from_saved(saved.hunger);
        let mut stamina = Stamina::from_saved(saved.stamina);
        stamina.set_max(hunger.value());
        Self {
            hunger,
            stamina,
            regeneration: Regenerator::new(),
            mining_seconds: 0.0,
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

    /// 是否允许开始冲刺：存活、未阻塞、有移动输入、有体力且未完全饥饿。
    pub fn can_sprint(&self, alive: bool, blocked: bool, moving: bool) -> bool {
        alive && !blocked && moving && self.stamina.can_sprint() && self.hunger.value() > 0.0
    }

    /// 推进生存状态，返回本帧应恢复的生命值（0 或 1）。
    ///
    /// `alive` 为假时不推进任何数值；`needs_heal` 表示当前生命值未满。
    pub fn update(
        &mut self,
        delta_seconds: f32,
        input: VitalsInput,
        alive: bool,
        needs_heal: bool,
    ) -> u32 {
        if !alive || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return 0;
        }

        // 体力上限始终跟随饥饿值；饥饿恢复后允许从零重新补满。
        self.stamina.set_max(self.hunger.value());
        if !input.blocked && !self.stamina.can_sprint() && self.stamina.max() > 0.0 {
            self.stamina.refill();
        }

        if !input.blocked {
            let multiplier = if self.hunger.hunger_effect_active() {
                2.0
            } else {
                1.0
            };
            let mut depleted = self.stamina.update(
                delta_seconds,
                input.moving && !input.sprinting,
                input.sprinting,
                multiplier,
            );
            for _ in 0..input.jumps {
                depleted |= self.stamina.drain(JUMP_COST);
            }
            if depleted {
                self.consume_hunger_for_stamina();
            }
        }
        self.hunger.tick_effect(delta_seconds);

        let eligible = needs_heal && self.hunger.can_regenerate();
        if self
            .regeneration
            .tick(delta_seconds, eligible, self.hunger.value())
        {
            // 每恢复 1 点生命额外消耗 20 点体力。
            self.spend_stamina(HEAL_STAMINA_COST);
            NAT_HEART_RECOVER
        } else {
            0
        }
    }

    /// 立即结算一次事件消耗（攻击命中、破坏方块等）；耗尽时同样扣 1 点饥饿。
    pub fn spend_stamina(&mut self, amount: f32) {
        if self.stamina.drain(amount) {
            self.consume_hunger_for_stamina();
        }
    }

    /// 破坏方块按累计挖掘时间消耗体力：每累计 1 秒扣 1 点。
    pub fn drain_mining(&mut self, delta_seconds: f32) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        self.mining_seconds += delta_seconds;
        while self.mining_seconds >= 1.0 {
            self.mining_seconds -= 1.0;
            self.spend_stamina(MINING_DRAIN_PER_SECOND);
        }
    }

    /// 体力耗尽：扣 1 点饥饿并以新的上限补满体力。
    fn consume_hunger_for_stamina(&mut self) {
        self.hunger.drain(1.0);
        self.stamina.set_max(self.hunger.value());
        self.stamina.refill();
    }
}

#[cfg(test)]
mod tests {
    use super::super::stamina::ATTACK_COST;
    use super::*;

    #[test]
    fn default_vitals_are_full() {
        let vitals = Vitals::new();
        assert_eq!(vitals.hunger().value(), 100.0);
        assert_eq!(vitals.stamina().value(), 100.0);
        assert_eq!(vitals.stamina().max(), 100.0);
        assert!(vitals.respawn().is_none());
    }

    #[test]
    fn dead_players_do_not_change() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(5.0);
        let before = *vitals.hunger();
        vitals.update(
            10.0,
            VitalsInput {
                moving: true,
                sprinting: true,
                jumps: 3,
                blocked: false,
            },
            false,
            true,
        );
        assert_eq!(*vitals.hunger(), before);
    }

    #[test]
    fn blocked_ui_pauses_stamina() {
        let mut vitals = Vitals::new();
        let stamina_before = vitals.stamina().value();
        vitals.update(
            1.0,
            VitalsInput {
                moving: true,
                blocked: true,
                ..VitalsInput::default()
            },
            true,
            false,
        );
        assert_eq!(vitals.stamina().value(), stamina_before);
    }

    #[test]
    fn walking_depletes_stamina_and_consumes_one_hunger() {
        let mut vitals = Vitals::new();
        // 100 点体力在行走 1 点/秒下正好 100 秒耗尽。
        vitals.update(
            100.0,
            VitalsInput {
                moving: true,
                ..VitalsInput::default()
            },
            true,
            false,
        );
        assert!((vitals.hunger().value() - 99.0).abs() < 1e-3);
        assert!((vitals.stamina().value() - 99.0).abs() < 1e-3);
        assert!((vitals.stamina().max() - 99.0).abs() < 1e-3);
    }

    #[test]
    fn jumping_costs_half_stamina_each() {
        let mut vitals = Vitals::new();
        vitals.update(0.0, VitalsInput::default(), true, false);
        // 用一次极短但有效的更新结算跳跃消耗。
        vitals.stamina_mut().set_max(100.0);
        let before = vitals.stamina().value();
        vitals.update(
            0.001,
            VitalsInput {
                jumps: 3,
                ..VitalsInput::default()
            },
            true,
            false,
        );
        assert!((vitals.stamina().value() - (before - 1.5)).abs() < 1e-3);
    }

    #[test]
    fn stamina_max_follows_hunger() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(60.0);
        vitals.update(0.001, VitalsInput::default(), true, false);
        assert!((vitals.stamina().max() - 40.0).abs() < 1e-6);
        assert!(vitals.stamina().value() <= 40.0);
    }

    #[test]
    fn regeneration_heals_above_half_hunger_without_hunger_cost() {
        let mut vitals = Vitals::new();
        let hunger_before = vitals.hunger().value();
        let stamina_before = vitals.stamina().value();
        let healed = vitals.update(10.0, VitalsInput::default(), true, true);
        assert_eq!(healed, 1);
        assert!((vitals.hunger().value() - hunger_before).abs() < 1e-6);
        assert!(
            (vitals.stamina().value() - (stamina_before - HEAL_STAMINA_COST)).abs() < 0.2,
            "自然恢复应消耗 20 点体力"
        );
    }

    #[test]
    fn regeneration_requires_at_least_half_hunger() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(50.0);
        let healed = vitals.update(20.0, VitalsInput::default(), true, true);
        assert_eq!(healed, 1, "恰好 50% 饥饿应允许恢复");
        vitals.hunger_mut().drain(0.5);
        let healed = vitals.update(100.0, VitalsInput::default(), true, true);
        assert_eq!(healed, 0);
    }

    #[test]
    fn attack_hit_spends_stamina() {
        let mut vitals = Vitals::new();
        vitals.spend_stamina(ATTACK_COST);
        assert!((vitals.stamina().value() - 99.0).abs() < 1e-6);
    }

    #[test]
    fn mining_drains_one_stamina_per_accumulated_second() {
        let mut vitals = Vitals::new();
        vitals.drain_mining(0.5);
        assert!((vitals.stamina().value() - 100.0).abs() < 1e-6);
        vitals.drain_mining(0.5);
        assert!((vitals.stamina().value() - 99.0).abs() < 1e-6);
        vitals.drain_mining(2.5);
        assert!((vitals.stamina().value() - 97.0).abs() < 1e-6);
    }

    #[test]
    fn event_spending_depletes_into_hunger() {
        let mut vitals = Vitals::new();
        vitals.stamina_mut().set_max(2.0);
        vitals.stamina_mut().drain(1.0);
        vitals.spend_stamina(2.0);
        assert_eq!(vitals.hunger().value(), 99.0);
        assert!((vitals.stamina().max() - 99.0).abs() < 1e-6);
        assert!((vitals.stamina().value() - 99.0).abs() < 1e-6);
    }

    #[test]
    fn sprint_requires_stamina_and_hunger() {
        let mut vitals = Vitals::new();
        assert!(vitals.can_sprint(true, false, true));
        vitals.stamina_mut().set_max(100.0);
        vitals.stamina_mut().drain(100.0);
        assert!(!vitals.can_sprint(true, false, true));
        assert!(!vitals.can_sprint(true, false, false));
        assert!(!vitals.can_sprint(false, false, true));
    }

    #[test]
    fn eating_after_starvation_refills_stamina() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(100.0);
        vitals.update(
            0.001,
            VitalsInput {
                moving: true,
                ..VitalsInput::default()
            },
            true,
            false,
        );
        assert_eq!(vitals.stamina().max(), 0.0);
        assert_eq!(vitals.stamina().value(), 0.0);
        vitals.hunger_mut().eat(40.0);
        vitals.update(0.001, VitalsInput::default(), true, false);
        assert!((vitals.stamina().value() - 40.0).abs() < 1e-4);
    }

    #[test]
    fn saved_vitals_round_trip() {
        let mut vitals = Vitals::new();
        vitals.hunger_mut().drain(5.0);
        vitals.stamina_mut().set_max(95.0);
        vitals.stamina_mut().drain(2.0);
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

    #[test]
    fn legacy_hunger_is_scaled_to_the_new_range() {
        let saved = SavedVitals {
            hunger: SavedHunger { hunger: 12.0 },
            stamina: SavedStamina { stamina: 42.0 },
            respawn: None,
        }
        .migrated_to_v10();
        assert_eq!(saved.hunger.hunger, 60.0);
        assert_eq!(saved.stamina.stamina, 42.0);
    }
}

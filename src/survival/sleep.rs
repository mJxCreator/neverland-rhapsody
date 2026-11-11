//! 睡眠规则：夜间、存活、周围无敌对生物时才能入睡。
//!
//! 睡眠成功后推进到次日清晨并恢复全部体力；复活点由调用方在成功时设置。

use super::stamina::Stamina;
use super::world_time::WorldTime;

/// 睡眠失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepError {
    /// 玩家已死亡。
    Dead,
    /// 当前不是夜间。
    NotNight,
    /// 附近存在敌对生物。
    HostileNearby,
}

impl SleepError {
    /// 面向玩家的英文提示；字体仅覆盖 ASCII。
    pub const fn message(self) -> &'static str {
        match self {
            Self::Dead => "YOU CANNOT SLEEP NOW",
            Self::NotNight => "YOU CAN ONLY SLEEP AT NIGHT",
            Self::HostileNearby => "MONSTERS ARE NEARBY",
        }
    }
}

/// 检查是否允许睡眠；检查顺序为存活、夜间、周围环境。
pub fn check(time: WorldTime, alive: bool, hostile_nearby: bool) -> Result<(), SleepError> {
    if !alive {
        return Err(SleepError::Dead);
    }
    if !time.is_night() {
        return Err(SleepError::NotNight);
    }
    if hostile_nearby {
        return Err(SleepError::HostileNearby);
    }
    Ok(())
}

/// 尝试睡眠；成功时推进到清晨并恢复体力。
pub fn try_sleep(
    time: &mut WorldTime,
    stamina: &mut Stamina,
    alive: bool,
    hostile_nearby: bool,
) -> Result<(), SleepError> {
    check(*time, alive, hostile_nearby)?;
    time.advance_to_morning();
    stamina.refill();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn night() -> WorldTime {
        let mut time = WorldTime::new();
        time.advance(crate::survival::world_time::DAY_LENGTH_SECONDS * 0.6);
        time
    }

    #[test]
    fn rejects_dead_players() {
        assert_eq!(check(night(), false, false), Err(SleepError::Dead));
    }

    #[test]
    fn rejects_daytime() {
        assert_eq!(
            check(WorldTime::new(), true, false),
            Err(SleepError::NotNight)
        );
    }

    #[test]
    fn rejects_nearby_monsters() {
        assert_eq!(check(night(), true, true), Err(SleepError::HostileNearby));
    }

    #[test]
    fn sleep_advances_to_morning_and_refills_stamina() {
        let mut time = night();
        let mut stamina = Stamina::new();
        stamina.drain_sprint(2.0);
        assert!(stamina.value() < stamina.max());
        assert_eq!(try_sleep(&mut time, &mut stamina, true, false), Ok(()));
        assert_eq!(time.fraction(), 0.0);
        assert_eq!(stamina.value(), stamina.max());
    }

    #[test]
    fn failed_sleep_changes_nothing() {
        let mut time = night();
        let before = time.fraction();
        let mut stamina = Stamina::new();
        stamina.drain_sprint(2.0);
        let stamina_before = stamina.value();
        assert!(try_sleep(&mut time, &mut stamina, true, true).is_err());
        assert_eq!(time.fraction(), before);
        assert_eq!(stamina.value(), stamina_before);
    }

    #[test]
    fn messages_are_ascii() {
        for error in [
            SleepError::Dead,
            SleepError::NotNight,
            SleepError::HostileNearby,
        ] {
            assert!(error.message().is_ascii());
        }
    }
}

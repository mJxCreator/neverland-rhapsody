//! 护甲减伤公式。
//!
//! 采用可解释的线性减伤：每点防御提供 4% 减免，防御上限 20 点（80% 减免），
//! 结算后至少造成 1 点伤害。坠落与窒息等环境伤害不经过护甲。

/// 根据防御力把原始伤害折算为最终伤害。
pub fn reduce_damage(amount: u32, defense: u32) -> u32 {
    if amount == 0 {
        return 0;
    }
    let defense = defense.min(20);
    let numerator = amount.saturating_mul(25 - defense);
    // 向上取整，保证少量伤害不会被护甲完全抹平。
    let reduced = numerator.div_ceil(25);
    reduced.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defense_reduces_damage_but_never_below_one() {
        assert_eq!(reduce_damage(10, 0), 10);
        assert_eq!(reduce_damage(10, 5), 8);
        assert_eq!(reduce_damage(10, 20), 2);
        assert_eq!(reduce_damage(1, 20), 1);
        assert_eq!(reduce_damage(0, 20), 0);
    }

    #[test]
    fn defense_is_capped() {
        assert_eq!(reduce_damage(100, 25), reduce_damage(100, 20));
    }

    #[test]
    fn full_diamond_set_reduces_most_damage() {
        let damage = reduce_damage(40, 20);
        assert!(damage <= 8, "20 点防御至少减免 80%");
    }
}

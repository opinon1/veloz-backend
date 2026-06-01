//! Per-user dynamic pricing.
//!
//! Spec: every price is unique per (user, item), grows linearly with
//! the user's progression, and oscillates around the linear trend so
//! it periodically dips before climbing higher. Same user state →
//! same prices (no time component, no randomness).
//!
//! Formula:
//!
//!   x = total_xp / XP_DIVISOR
//!   φ = stable hash of (user_id, item_id) mapped into [0, 2π)
//!   m(x) = 1 + SLOPE·x + AMPLITUDE·sin(OMEGA·x + φ)
//!
//! The sine term is NOT anchored, so even a brand-new user
//! (total_xp = 0) already sees per-(user, item) divergence in the
//! `[1 − AMPLITUDE, 1 + AMPLITUDE]` band. As they grind XP, the
//! linear term pulls prices upward forever while the sine continues
//! to wobble around the trend.
//!
//! Final cost: `round(base_cost · m(x) · profiles.price_multiplier)`.
//! The two multipliers stack so an admin discount on the profile is
//! still honored on top of the dynamic curve.

use uuid::Uuid;

/// Linear slope per 100 XP. Default 0.05 → +5% per level.
pub const LINEAR_SLOPE: f64 = 0.05;

/// Sine amplitude. ±15% from the linear trend at the extremes.
pub const SINE_AMPLITUDE: f64 = 0.15;

/// Sine frequency. One full cycle per ~6.28 levels.
pub const SINE_OMEGA: f64 = 1.0;

/// Converts total_xp into the abstract "progression" axis. 1 unit = 100 XP.
pub const XP_DIVISOR: f64 = 100.0;

/// Two odd 64-bit constants — golden-ratio prime and the splitmix64
/// mixer — so the phase hash mixes both UUIDs evenly without dragging
/// in a crate-level hasher dependency.
const PHASE_MIX_A: u128 = 0x9E37_79B9_7F4A_7C15;
const PHASE_MIX_B: u128 = 0xBF58_476D_1CE4_E5B9;

/// Stable phase in [0, 2π) derived from `(user_id, item_id)`. Two
/// different UUIDs → two different phases; same pair → identical
/// phase forever. No `DefaultHasher` involved (which is unstable
/// across compiler versions).
fn phase(user_id: Uuid, item_id: Uuid) -> f64 {
    let a = user_id.as_u128();
    let b = item_id.as_u128();
    let mixed = a.wrapping_mul(PHASE_MIX_A) ^ b.wrapping_mul(PHASE_MIX_B);
    // Pull the low 53 bits and normalize so the result fits exactly in
    // an f64 without precision loss.
    let bits = (mixed & ((1u128 << 53) - 1)) as u64;
    let normalized = (bits as f64) / ((1u64 << 53) as f64);
    normalized * std::f64::consts::TAU
}

/// The (user, item) multiplier on top of base_cost. Always ≥ 0.
pub fn dynamic_multiplier(user_id: Uuid, item_id: Uuid, total_xp: i64) -> f64 {
    let x = (total_xp.max(0) as f64) / XP_DIVISOR;
    let phi = phase(user_id, item_id);
    let m = 1.0 + LINEAR_SLOPE * x + SINE_AMPLITUDE * (SINE_OMEGA * x + phi).sin();
    m.max(0.0)
}

/// Final per-user price for one item. Stacks the dynamic multiplier
/// with the admin-controlled `profiles.price_multiplier` (clamped to
/// ≥ 0 here so a misconfigured negative multiplier can't be exploited).
pub fn apply_dynamic_price(
    base_cost: i64,
    user_id: Uuid,
    item_id: Uuid,
    total_xp: i64,
    account_multiplier: f64,
) -> i64 {
    if base_cost <= 0 {
        return 0;
    }
    let m = dynamic_multiplier(user_id, item_id, total_xp);
    let acct = if account_multiplier.is_finite() {
        account_multiplier.max(0.0)
    } else {
        0.0
    };
    let raw = (base_cost as f64) * m * acct;
    if raw.is_finite() {
        raw.round().max(0.0) as i64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn zero_xp_already_diverges_within_band() {
        // x = 0 → m = 1 + AMPLITUDE·sin(φ), bounded by [1 - A, 1 + A].
        // Every (user, item) pair lands in that band, and the
        // distribution is wide enough that not all pairs share a value.
        let mut seen = std::collections::HashSet::new();
        for u in 1..50u128 {
            for i in 1..50u128 {
                let cost = apply_dynamic_price(1000, uid(u), uid(i), 0, 1.0);
                let lo = (1000.0 * (1.0 - SINE_AMPLITUDE)).round() as i64;
                let hi = (1000.0 * (1.0 + SINE_AMPLITUDE)).round() as i64;
                assert!(
                    cost >= lo && cost <= hi,
                    "user={u} item={i} cost={cost} band=[{lo},{hi}]"
                );
                seen.insert(cost);
            }
        }
        assert!(seen.len() > 50, "expected diverse prices, got {} distinct", seen.len());
    }

    #[test]
    fn deterministic_same_inputs_same_output() {
        let u = uid(7);
        let i = uid(11);
        let a = apply_dynamic_price(500, u, i, 1234, 1.0);
        let b = apply_dynamic_price(500, u, i, 1234, 1.0);
        assert_eq!(a, b);
    }

    #[test]
    fn different_users_get_different_prices_when_leveled() {
        // At higher XP the sine wave should fan two different users out.
        let i = uid(42);
        let prices: Vec<i64> = (1..20u128)
            .map(|u| apply_dynamic_price(1000, uid(u), i, 5000, 1.0))
            .collect();
        let distinct = prices.iter().collect::<std::collections::HashSet<_>>().len();
        assert!(distinct > 1, "all users got identical prices: {prices:?}");
    }

    #[test]
    fn stacks_with_account_multiplier() {
        // With anchor removed the dynamic factor at x=0 lives in
        // [1 - A, 1 + A]. account_multiplier = 0.5 ⇒ result lives in
        // [(1 - A)·50, (1 + A)·50] = [42.5, 57.5] → rounded [43, 58].
        let cost = apply_dynamic_price(100, uid(1), uid(2), 0, 0.5);
        let lo = (50.0 * (1.0 - SINE_AMPLITUDE)).round() as i64;
        let hi = (50.0 * (1.0 + SINE_AMPLITUDE)).round() as i64;
        assert!(cost >= lo && cost <= hi, "got {cost}, band [{lo},{hi}]");
    }

    #[test]
    fn linear_growth_dominates_at_high_xp() {
        // At very high XP the linear term should swamp the sine and
        // produce prices way above base.
        let u = uid(3);
        let i = uid(5);
        let high = apply_dynamic_price(100, u, i, 100_000, 1.0);
        // x = 1000, slope·x = 50 → multiplier ≈ 51 ± 0.15. Expect ≥ 5000.
        assert!(high >= 5000, "got {high}");
    }
}

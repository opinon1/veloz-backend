//! Leveling and XP formulas.
//!
//! Edit the constants / functions below to tune the game economy.
//! All formulas are pure Rust — no DB config lookup — so they recompile in place.

/// Per-run XP formula. Linear: 1 XP per point of score.
/// Change the divisor (or swap in a different shape) to tune how much XP a run grants.
pub fn xp_from_run(score: i64) -> i64 {
    score.max(0) / 1
}

/// Per-run battlepass XP. Currently same as account XP; decouple here if you want
/// BP progression to feel different from account progression.
pub fn bp_xp_from_run(score: i64) -> i64 {
    xp_from_run(score)
}

/// Level from total XP. Curved — level N requires `LEVEL_BASE * N^LEVEL_EXPONENT` total XP.
///
/// Inverse of `total_xp_for_level`.  Default: `sqrt(total_xp / 100)` + 1.
const LEVEL_BASE: f64 = 100.0;
const LEVEL_EXPONENT: f64 = 2.0;

pub fn level_from_total_xp(total_xp: i64) -> i32 {
    if total_xp <= 0 {
        return 1;
    }
    // Invert total_xp_for_level: level = floor((total_xp / base)^(1/exp)) + 1
    let level = ((total_xp as f64) / LEVEL_BASE).powf(1.0 / LEVEL_EXPONENT).floor() as i32 + 1;
    level.max(1)
}

/// Total XP required to *reach* the start of the given level.
/// Useful for progress bars and tier gating.
pub fn total_xp_for_level(level: i32) -> i64 {
    if level <= 1 {
        return 0;
    }
    let l = (level - 1) as f64;
    (LEVEL_BASE * l.powf(LEVEL_EXPONENT)).round() as i64
}

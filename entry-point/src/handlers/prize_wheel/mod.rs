pub mod cooldown;
pub mod spin;
pub mod wheel;
pub mod router;

/// Redis key for the per-user cooldown. TTL = 86400s.
pub fn cooldown_key(user_id: uuid::Uuid) -> String {
    format!("prize_wheel:cooldown:{}", user_id)
}

pub const COOLDOWN_SECS: i64 = 86_400;

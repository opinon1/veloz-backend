-- Single global Prize Wheel.
--
-- Items: ordered by `position` (the visual order on the wheel). Each item has
--   a Grant array reward (same shape as battlepass tier rewards / store
--   payloads) and an integer weight ≥ 1 used for weighted-random selection.
-- Spins: history of wins, one row per spin. The reward column is a JSON
--   snapshot of the won item's reward at spin time so changes to the wheel
--   don't rewrite history.
--
-- Cooldown lives in Redis (key: prize_wheel:cooldown:{user_id}, TTL 86400s)
-- so it doesn't need a DB column.

CREATE TABLE IF NOT EXISTS prize_wheel_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    position INTEGER NOT NULL,
    reward JSONB NOT NULL,
    weight INTEGER NOT NULL CHECK (weight >= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (position)
);

CREATE TABLE IF NOT EXISTS prize_wheel_spins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reward JSONB NOT NULL,
    won_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_prize_wheel_spins_user
    ON prize_wheel_spins(user_id, won_at DESC);

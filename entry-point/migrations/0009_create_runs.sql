CREATE TABLE IF NOT EXISTS runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    score BIGINT NOT NULL CHECK (score >= 0),
    distance BIGINT NOT NULL DEFAULT 0 CHECK (distance >= 0),
    coins_collected BIGINT NOT NULL DEFAULT 0 CHECK (coins_collected >= 0),
    duration_ms BIGINT NOT NULL DEFAULT 0 CHECK (duration_ms >= 0),
    xp_awarded BIGINT NOT NULL DEFAULT 0,
    bp_xp_awarded BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_runs_user ON runs(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_runs_score ON runs(score DESC);

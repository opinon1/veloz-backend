CREATE TABLE IF NOT EXISTS bp_seasons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    premium_cost BIGINT NOT NULL DEFAULT 0,
    premium_currency TEXT NOT NULL DEFAULT 'high',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (ends_at > starts_at)
);
CREATE INDEX IF NOT EXISTS idx_bp_seasons_window ON bp_seasons(starts_at, ends_at);

CREATE TABLE IF NOT EXISTS bp_tiers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID NOT NULL REFERENCES bp_seasons(id) ON DELETE CASCADE,
    tier INTEGER NOT NULL,
    xp_required BIGINT NOT NULL,
    free_reward JSONB NOT NULL DEFAULT '{}'::jsonb,     -- { type, payload, amount }
    premium_reward JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (season_id, tier)
);

CREATE TABLE IF NOT EXISTS bp_progress (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES bp_seasons(id) ON DELETE CASCADE,
    bp_xp BIGINT NOT NULL DEFAULT 0,
    premium_unlocked BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, season_id)
);

CREATE TABLE IF NOT EXISTS bp_claims (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    season_id UUID NOT NULL REFERENCES bp_seasons(id) ON DELETE CASCADE,
    tier INTEGER NOT NULL,
    track TEXT NOT NULL,  -- 'free' | 'premium'
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, season_id, tier, track)
);

CREATE TABLE IF NOT EXISTS skins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    outfit_url TEXT NOT NULL,
    cost BIGINT NOT NULL DEFAULT 0 CHECK (cost >= 0),
    currency TEXT NOT NULL DEFAULT 'soft',  -- 'high' | 'soft' | 'energy'
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS user_skins (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    skin_id UUID NOT NULL REFERENCES skins(id) ON DELETE CASCADE,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, skin_id)
);

CREATE INDEX IF NOT EXISTS idx_user_skins_user ON user_skins(user_id);

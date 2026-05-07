CREATE TABLE IF NOT EXISTS characters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    default_unlocked BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS user_characters (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    character_id UUID NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    unlocked BOOLEAN NOT NULL DEFAULT FALSE,
    equipped_skin_id UUID,
    PRIMARY KEY (user_id, character_id)
);

CREATE INDEX IF NOT EXISTS idx_user_characters_user ON user_characters(user_id);

-- Cosmetic items: avatars (profile picture) and frames (border around it).
-- Each is purchasable and the user can have at most one selected at a time
-- (the selection lives on profiles.avatar_url / profiles.frame_url).

CREATE TABLE IF NOT EXISTS avatars (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    price BIGINT NOT NULL DEFAULT 0 CHECK (price >= 0),
    currency TEXT NOT NULL DEFAULT 'soft',  -- 'high' | 'soft' | 'energy'
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS frames (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    price BIGINT NOT NULL DEFAULT 0 CHECK (price >= 0),
    currency TEXT NOT NULL DEFAULT 'soft',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS user_avatars (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    avatar_id UUID NOT NULL REFERENCES avatars(id) ON DELETE CASCADE,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, avatar_id)
);

CREATE TABLE IF NOT EXISTS user_frames (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    frame_id UUID NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, frame_id)
);

CREATE INDEX IF NOT EXISTS idx_user_avatars_user ON user_avatars(user_id);
CREATE INDEX IF NOT EXISTS idx_user_frames_user ON user_frames(user_id);

-- profiles.avatar_url / frame_url were free-form text. Repurpose them as
-- typed UUIDs that point at the catalog. Any existing values are stale
-- (skin ids from the old per-profile equip flow) and aren't valid avatar
-- ids, so wipe before changing the type.
UPDATE profiles SET avatar_url = NULL, frame_url = NULL;

ALTER TABLE profiles
    ALTER COLUMN avatar_url TYPE UUID USING avatar_url::uuid,
    ALTER COLUMN frame_url  TYPE UUID USING frame_url::uuid;

ALTER TABLE profiles
    ADD CONSTRAINT profiles_avatar_url_fkey
        FOREIGN KEY (avatar_url) REFERENCES avatars(id) ON DELETE SET NULL,
    ADD CONSTRAINT profiles_frame_url_fkey
        FOREIGN KEY (frame_url)  REFERENCES frames(id)  ON DELETE SET NULL;

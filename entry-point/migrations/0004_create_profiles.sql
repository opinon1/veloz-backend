CREATE TABLE IF NOT EXISTS profiles (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    account_level INTEGER NOT NULL DEFAULT 1,
    total_xp BIGINT NOT NULL DEFAULT 0,
    price_multiplier DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    main_highscore BIGINT NOT NULL DEFAULT 0,
    avatar_url TEXT,
    frame_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Auto-create a profile row whenever a new user is inserted.
CREATE OR REPLACE FUNCTION create_profile_for_user()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO profiles (user_id) VALUES (NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_create_profile_for_user
AFTER INSERT ON users
FOR EACH ROW EXECUTE FUNCTION create_profile_for_user();

-- Backfill profiles for existing users (should be empty at this point but keeps migration idempotent).
INSERT INTO profiles (user_id)
SELECT id FROM users
ON CONFLICT (user_id) DO NOTHING;

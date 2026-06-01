-- Per-user opaque metadata blob. Frontend stuffs whatever it wants here
-- (UI preferences, onboarding flags, tutorial state, …) without needing a
-- backend schema change. Backend never inspects the JSON.

CREATE TABLE IF NOT EXISTS user_metadata (
    user_id    UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    data       JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Auto-create a metadata row whenever a new user is inserted, same pattern
-- as wallets / profiles. Keeps handlers from having to UPSERT.
CREATE OR REPLACE FUNCTION create_user_metadata_for_user()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO user_metadata (user_id) VALUES (NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_create_user_metadata_for_user ON users;
CREATE TRIGGER trg_create_user_metadata_for_user
AFTER INSERT ON users
FOR EACH ROW EXECUTE FUNCTION create_user_metadata_for_user();

-- Backfill existing users.
INSERT INTO user_metadata (user_id)
SELECT id FROM users
ON CONFLICT (user_id) DO NOTHING;

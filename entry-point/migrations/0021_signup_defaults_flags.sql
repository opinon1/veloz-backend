-- Signup default flags.
--
-- Admin marks catalog rows as "give this to every new user on signup."
-- Existing systems:
--   characters.default_unlocked  → drives /characters "unlocked" view
--   skins.is_default             → existed but unwired; now used at signup
-- New systems (this migration):
--   avatars.is_default
--   frames.is_default
--   store_items.is_default       → fulfilled by applying payload Grants

ALTER TABLE avatars
    ADD COLUMN IF NOT EXISTS is_default BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE frames
    ADD COLUMN IF NOT EXISTS is_default BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE store_items
    ADD COLUMN IF NOT EXISTS is_default BOOLEAN NOT NULL DEFAULT FALSE;

-- Tracks per-(user, store_item) idempotency for default store-item
-- payloads. ON CONFLICT skips re-application on backfill so currency
-- isn't credited twice, skin payloads aren't re-inserted with stale
-- references, etc. Skins / avatars / frames don't need a separate
-- tracking table — their PK on (user_id, *_id) already handles it.
CREATE TABLE IF NOT EXISTS default_grants_applied (
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('store_item')),
    ref_id      UUID NOT NULL,
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, kind, ref_id)
);
CREATE INDEX IF NOT EXISTS idx_default_grants_applied_user
    ON default_grants_applied(user_id);

-- Generic mission system.
--
-- Admin defines a mission with a trigger_event + target JSON. When the
-- backend records a matching event for a user (run finished, currency
-- credited, store item purchased, character leveled up), the user's
-- progress row for the current cycle is bumped. Hitting target auto-
-- grants XP (no claim endpoint — keeps UX simple).
--
-- Reward is XP only by design. Currency / item rewards belong to the
-- store and prize wheel.

CREATE TABLE IF NOT EXISTS missions (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name           TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    -- daily, weekly, one_shot. The cycle_key on user_missions encodes
    -- which calendar bucket a given progress row belongs to.
    cycle          TEXT NOT NULL CHECK (cycle IN ('daily','weekly','one_shot')),
    -- Which app event drives progress. New variants land in
    -- models/mission_types.rs and the CHECK below.
    trigger_event  TEXT NOT NULL CHECK (trigger_event IN (
        'run_completed',
        'currency_collected',
        'store_purchase',
        'character_level_up'
    )),
    -- JSON shape depends on trigger_event:
    --   run_completed       => {"amount": N}                    -- N runs
    --   currency_collected  => {"currency":"soft","amount":500} -- total amount
    --   store_purchase      => {"item_type":"currency_bundle","amount":1}
    --   character_level_up  => {"character_id":"<uuid>","level":10}
    target         JSONB NOT NULL,
    xp_reward      BIGINT NOT NULL CHECK (xp_reward > 0),
    is_active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_missions_active_trigger
    ON missions(trigger_event) WHERE is_active = TRUE;

-- One progress row per (user, mission, cycle_key).
-- cycle_key encodes which calendar bucket this progress belongs to:
--   one_shot -> 'one_shot'
--   daily    -> 'YYYY-MM-DD' (UTC)
--   weekly   -> 'YYYY-Www'   (ISO week, UTC; '2026-W22')
-- Server derives the cycle_key on every read/write.
CREATE TABLE IF NOT EXISTS user_missions (
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mission_id   UUID NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    cycle_key    TEXT NOT NULL,
    progress     BIGINT NOT NULL DEFAULT 0 CHECK (progress >= 0),
    completed_at TIMESTAMPTZ,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, mission_id, cycle_key)
);

CREATE INDEX IF NOT EXISTS idx_user_missions_user
    ON user_missions(user_id);

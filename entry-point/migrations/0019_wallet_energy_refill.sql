-- Energy regen clock.
--
-- Rules (per spec):
--   - 1 energy / minute, up to a cap of 50.
--   - Regen only runs while stored energy < 50. Above 50 (from store
--     purchases), the wallet is frozen until energy is spent back down.
--
-- This column tracks "what moment does the next +1 tick land at". NULL
-- means no clock is running (stored energy is >= 50 right now). Whenever
-- energy crosses below 50 the clock is set to now(); whenever it climbs
-- back to >= 50 the clock is cleared.
--
-- Backfill: any user already at < 50 starts ticking from now.

ALTER TABLE wallets
    ADD COLUMN IF NOT EXISTS energy_refill_started_at TIMESTAMPTZ;

UPDATE wallets
SET energy_refill_started_at = CURRENT_TIMESTAMP
WHERE energy < 50 AND energy_refill_started_at IS NULL;

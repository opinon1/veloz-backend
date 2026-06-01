-- Per-character rarity. Drives the cards-required-to-level-up formula
-- (cards system itself is deferred). Values map to numeric multipliers
-- in models/rarity.rs (common=1, uncommon=1.25, rare=1.5, epic=1.75,
-- legendary=2). Backfill via DEFAULT — every existing row becomes
-- 'common' until an admin updates it.

ALTER TABLE characters
    ADD COLUMN IF NOT EXISTS rarity TEXT NOT NULL DEFAULT 'common'
        CHECK (rarity IN ('common','uncommon','rare','epic','legendary'));

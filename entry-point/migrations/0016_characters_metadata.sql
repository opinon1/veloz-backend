-- Frontend-only metadata blob on characters. Opaque JSON the client can
-- stuff with whatever it needs (animation hints, sort_order, lore text,
-- VFX refs, …) without requiring a schema change.

ALTER TABLE characters
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

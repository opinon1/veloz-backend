-- Skin assets now live in the public S3 bucket and are looked up by skin id;
-- name, description, and outfit_url are no longer needed on the row.
-- Each skin belongs to a Character; the character_id FK is required.

-- Existing rows can't be migrated because they have no character_id. This
-- migration assumes the skins table is empty (dev/integ environments).
DELETE FROM skins;

ALTER TABLE skins DROP COLUMN IF EXISTS name;
ALTER TABLE skins DROP COLUMN IF EXISTS description;
ALTER TABLE skins DROP COLUMN IF EXISTS outfit_url;

ALTER TABLE skins
    ADD COLUMN character_id UUID NOT NULL REFERENCES characters(id) ON DELETE CASCADE;

-- Wire the FK on user_characters.equipped_skin_id now that skins is in its
-- final shape.
ALTER TABLE user_characters
    ADD CONSTRAINT user_characters_equipped_skin_fkey
    FOREIGN KEY (equipped_skin_id) REFERENCES skins(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_skins_character ON skins(character_id);

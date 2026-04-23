CREATE TABLE IF NOT EXISTS store_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    item_type TEXT NOT NULL,         -- 'skin' | 'frame' | 'currency_bundle' | 'bp_unlock' | 'energy_refill' | 'custom'
    cost BIGINT NOT NULL CHECK (cost >= 0),
    currency TEXT NOT NULL,          -- 'high' | 'soft' | 'energy' | 'iap'
    iap_product_id TEXT,             -- set when currency = 'iap'
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,   -- e.g. { skin_id } | { high: 100 } | { energy: 50 } — opaque to backend
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,  -- frontend-only metadata
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_store_items_active ON store_items(is_active);

CREATE TABLE IF NOT EXISTS store_purchases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES store_items(id),
    cost_paid BIGINT NOT NULL,
    currency_paid TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_store_purchases_user ON store_purchases(user_id, created_at DESC);

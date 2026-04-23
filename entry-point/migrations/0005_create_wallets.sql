CREATE TABLE IF NOT EXISTS wallets (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    high BIGINT NOT NULL DEFAULT 0 CHECK (high >= 0),
    soft BIGINT NOT NULL DEFAULT 0 CHECK (soft >= 0),
    energy BIGINT NOT NULL DEFAULT 0 CHECK (energy >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Ledger for audit + placeholder spend/grant/IAP sources.
CREATE TABLE IF NOT EXISTS wallet_ledger (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    currency TEXT NOT NULL,          -- 'high' | 'soft' | 'energy'
    delta BIGINT NOT NULL,           -- + grant, - spend
    reason TEXT NOT NULL,            -- 'run' | 'store' | 'bp_claim' | 'iap' | 'admin_grant' | 'spend'
    reference_id TEXT,               -- store_item id, run id, iap product, etc.
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_wallet_ledger_user ON wallet_ledger(user_id, created_at DESC);

-- Auto-create a wallet row whenever a new user is inserted.
CREATE OR REPLACE FUNCTION create_wallet_for_user()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO wallets (user_id) VALUES (NEW.id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_create_wallet_for_user
AFTER INSERT ON users
FOR EACH ROW EXECUTE FUNCTION create_wallet_for_user();

INSERT INTO wallets (user_id)
SELECT id FROM users
ON CONFLICT (user_id) DO NOTHING;

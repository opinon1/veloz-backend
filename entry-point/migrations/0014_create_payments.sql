-- Real-money payments processed by Etomin (Mexico).
--
-- The flow:
--   1. User picks a store item with currency='iap'. Backend inserts a row
--      here in status='PENDING' before calling Etomin.
--   2. Backend POSTs /sale to Etomin with `id` as the `reference` (idempotent).
--   3. Etomin responds APPROVED/DECLINED/PENDING. We update `status` and
--      `etomin_response` accordingly. APPROVED triggers grant fulfillment
--      from the store item's payload.
--
-- `etomin_response` is the raw JSON Etomin returned; kept for audit + debugging.
-- `redirect_to` is non-null when status='PENDING' (3DS challenge URL).

CREATE TABLE IF NOT EXISTS payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES store_items(id) ON DELETE RESTRICT,
    amount BIGINT NOT NULL CHECK (amount >= 0),
    currency TEXT NOT NULL,                       -- ISO 4217 numeric, e.g. '484' for MXN
    status TEXT NOT NULL CHECK (status IN ('PENDING', 'APPROVED', 'DECLINED', 'EXPIRED')),
    redirect_to TEXT,
    etomin_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_payments_user
    ON payments(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payments_status
    ON payments(status);

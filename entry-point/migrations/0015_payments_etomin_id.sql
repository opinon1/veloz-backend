-- Etomin's internal transaction id (returned in the /sale response under
-- `id`). Required for GET /api/v1/transaction/{id} to look up later state
-- (3DS reconciliation).
--
-- Nullable because rows that fail to even reach Etomin (e.g. transport
-- error before the response) won't have one.

ALTER TABLE payments
    ADD COLUMN IF NOT EXISTS etomin_id TEXT;

CREATE INDEX IF NOT EXISTS idx_payments_pending_etomin
    ON payments(status, created_at)
    WHERE status = 'PENDING';

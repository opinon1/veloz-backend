-- Operational stats dashboard.
--
-- `dashboard_charts` holds every chart shown on /admin/stats: built-in ones
-- (seeded below, is_builtin = TRUE, not deletable) and ad-hoc ones admins save
-- at runtime. Each row is a named SELECT plus a rendering hint (chart_type +
-- config). The SQL is executed against the READ-ONLY stats pool, never the app
-- pool — see handlers/admin/stats.rs.

CREATE TABLE dashboard_charts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sql         TEXT NOT NULL,
    -- table | line | bar | pie | stat
    chart_type  TEXT NOT NULL DEFAULT 'table',
    -- rendering hints, e.g. {"x":"day","y":"revenue","series":"currency"}
    config      JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- TRUE for the seeded charts below; blocks deletion via the API.
    is_builtin  BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_dashboard_charts_sort ON dashboard_charts (sort_order, created_at);

-- ─────────── Seeded built-in charts ───────────
-- Money columns: payments.amount / store_purchases.cost_paid are BIGINT minor
-- units (cents); divide by 100.0 for display. Dollar-quoting ($q$…$q$) lets the
-- inner SQL use single quotes freely.

INSERT INTO dashboard_charts (title, description, sql, chart_type, config, is_builtin, sort_order) VALUES
(
  'Key totals', 'Lifetime headline numbers',
  $q$
  SELECT
    (SELECT count(*) FROM users)                                                          AS users,
    (SELECT count(*) FROM payments WHERE status = 'APPROVED')                             AS approved_payments,
    (SELECT round(coalesce(sum(amount), 0) / 100.0, 2) FROM payments WHERE status = 'APPROVED') AS gross_revenue,
    (SELECT count(DISTINCT user_id) FROM payments WHERE status = 'APPROVED')              AS paying_users
  $q$,
  'stat', '{}'::jsonb, TRUE, 10
),
(
  'Revenue per day', 'Approved payment revenue by day and currency',
  $q$
  SELECT date_trunc('day', created_at)::date AS day,
         currency,
         round(sum(amount) / 100.0, 2)        AS revenue
  FROM payments
  WHERE status = 'APPROVED'
  GROUP BY 1, 2
  ORDER BY 1
  $q$,
  'line', '{"x":"day","y":"revenue","series":"currency"}'::jsonb, TRUE, 20
),
(
  'Transactions by hour (7d)', 'Payment attempts per hour by status, last 7 days',
  $q$
  SELECT date_trunc('hour', created_at) AS hour,
         status,
         count(*)                        AS txns
  FROM payments
  WHERE created_at > now() - interval '7 days'
  GROUP BY 1, 2
  ORDER BY 1
  $q$,
  'line', '{"x":"hour","y":"txns","series":"status"}'::jsonb, TRUE, 30
),
(
  'Payments by status', 'Count and total amount per payment status',
  $q$
  SELECT status,
         count(*)                      AS count,
         round(sum(amount) / 100.0, 2) AS total
  FROM payments
  GROUP BY 1
  ORDER BY 2 DESC
  $q$,
  'bar', '{"x":"status","y":"count"}'::jsonb, TRUE, 40
),
(
  'Signups per day', 'New user registrations by day',
  $q$
  SELECT date_trunc('day', created_at)::date AS day,
         count(*)                            AS signups
  FROM users
  GROUP BY 1
  ORDER BY 1
  $q$,
  'line', '{"x":"day","y":"signups"}'::jsonb, TRUE, 50
),
(
  'Active users per day', 'Distinct users with a run, by day',
  $q$
  SELECT date_trunc('day', created_at)::date AS day,
         count(DISTINCT user_id)             AS active_users
  FROM runs
  GROUP BY 1
  ORDER BY 1
  $q$,
  'line', '{"x":"day","y":"active_users"}'::jsonb, TRUE, 60
),
(
  'Revenue by item type', 'Approved revenue grouped by store item type',
  $q$
  SELECT si.item_type,
         count(*)                         AS purchases,
         round(sum(p.amount) / 100.0, 2)  AS revenue
  FROM payments p
  JOIN store_items si ON si.id = p.item_id
  WHERE p.status = 'APPROVED'
  GROUP BY 1
  ORDER BY 3 DESC
  $q$,
  'bar', '{"x":"item_type","y":"revenue"}'::jsonb, TRUE, 70
),
(
  'Top items by revenue', 'Best-selling store items (approved)',
  $q$
  SELECT si.name,
         count(*)                         AS sales,
         round(sum(p.amount) / 100.0, 2)  AS revenue
  FROM payments p
  JOIN store_items si ON si.id = p.item_id
  WHERE p.status = 'APPROVED'
  GROUP BY 1
  ORDER BY 3 DESC
  LIMIT 20
  $q$,
  'table', '{}'::jsonb, TRUE, 80
),
(
  'Currency flow per day', 'In-game currency minted vs burned, by day and currency',
  $q$
  SELECT date_trunc('day', created_at)::date         AS day,
         currency,
         coalesce(sum(delta) FILTER (WHERE delta > 0), 0)  AS minted,
         coalesce(-sum(delta) FILTER (WHERE delta < 0), 0) AS burned
  FROM wallet_ledger
  GROUP BY 1, 2
  ORDER BY 1
  $q$,
  'table', '{}'::jsonb, TRUE, 90
),
(
  'In-game purchases per day', 'Store purchases (soft/hard currency) by day',
  $q$
  SELECT date_trunc('day', created_at)::date AS day,
         currency_paid                       AS currency,
         count(*)                            AS purchases,
         sum(cost_paid)                      AS spent
  FROM store_purchases
  GROUP BY 1, 2
  ORDER BY 1
  $q$,
  'line', '{"x":"day","y":"purchases","series":"currency"}'::jsonb, TRUE, 100
);

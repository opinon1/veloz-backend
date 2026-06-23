-- Make the time-series built-in charts true to the timescale.
--
-- Problem: the original seeded queries GROUP BY a bucket and only emit rows for
-- buckets that have data. Missing days/hours simply vanish, and the dashboard's
-- category x-axis then spaces the surviving points evenly — so a gap of days
-- looks like one step. Also buckets were cut on the UTC day, not local.
--
-- Fix: generate a continuous bucket series with generate_series and LEFT JOIN
-- the data onto it, so every day/hour is present (0 when empty), over a bounded
-- window. Day buckets are cut in America/Mexico_City (the game's locale, MXN),
-- so "day" matches the local calendar.

-- Revenue per day — last 90 local days, zero-filled, by currency.
UPDATE dashboard_charts SET sql = $q$
  SELECT d::date AS day, c.currency,
         round(coalesce(sum(p.amount), 0) / 100.0, 2) AS revenue
  FROM generate_series(
         (now() AT TIME ZONE 'America/Mexico_City')::date - 89,
         (now() AT TIME ZONE 'America/Mexico_City')::date,
         interval '1 day') d
  CROSS JOIN (SELECT DISTINCT currency FROM payments WHERE status = 'APPROVED') c
  LEFT JOIN payments p
    ON p.status = 'APPROVED'
   AND p.currency = c.currency
   AND (p.created_at AT TIME ZONE 'America/Mexico_City')::date = d::date
  GROUP BY d, c.currency
  ORDER BY d
$q$ WHERE title = 'Revenue per day' AND is_builtin;

-- Transactions by hour — last 7 days, every hour present, by status.
UPDATE dashboard_charts SET sql = $q$
  SELECT h AS hour, s.status, count(p.id) AS txns
  FROM generate_series(
         date_trunc('hour', now()) - interval '167 hours',
         date_trunc('hour', now()),
         interval '1 hour') h
  CROSS JOIN (SELECT unnest(ARRAY['APPROVED','PENDING','DECLINED','EXPIRED']) AS status) s
  LEFT JOIN payments p
    ON date_trunc('hour', p.created_at) = h
   AND p.status = s.status
  GROUP BY h, s.status
  ORDER BY h
$q$ WHERE title = 'Transactions by hour (7d)' AND is_builtin;

-- Signups per day — last 90 local days, zero-filled.
UPDATE dashboard_charts SET sql = $q$
  SELECT d::date AS day, count(u.id) AS signups
  FROM generate_series(
         (now() AT TIME ZONE 'America/Mexico_City')::date - 89,
         (now() AT TIME ZONE 'America/Mexico_City')::date,
         interval '1 day') d
  LEFT JOIN users u
    ON (u.created_at AT TIME ZONE 'America/Mexico_City')::date = d::date
  GROUP BY d
  ORDER BY d
$q$ WHERE title = 'Signups per day' AND is_builtin;

-- Active users per day — last 90 local days, zero-filled.
UPDATE dashboard_charts SET sql = $q$
  SELECT d::date AS day, count(DISTINCT r.user_id) AS active_users
  FROM generate_series(
         (now() AT TIME ZONE 'America/Mexico_City')::date - 89,
         (now() AT TIME ZONE 'America/Mexico_City')::date,
         interval '1 day') d
  LEFT JOIN runs r
    ON (r.created_at AT TIME ZONE 'America/Mexico_City')::date = d::date
  GROUP BY d
  ORDER BY d
$q$ WHERE title = 'Active users per day' AND is_builtin;

-- In-game purchases per day — last 90 local days, zero-filled, by currency.
UPDATE dashboard_charts SET sql = $q$
  SELECT d::date AS day, c.currency_paid AS currency,
         count(sp.id)                   AS purchases,
         coalesce(sum(sp.cost_paid), 0) AS spent
  FROM generate_series(
         (now() AT TIME ZONE 'America/Mexico_City')::date - 89,
         (now() AT TIME ZONE 'America/Mexico_City')::date,
         interval '1 day') d
  CROSS JOIN (SELECT DISTINCT currency_paid FROM store_purchases) c
  LEFT JOIN store_purchases sp
    ON sp.currency_paid = c.currency_paid
   AND (sp.created_at AT TIME ZONE 'America/Mexico_City')::date = d::date
  GROUP BY d, c.currency_paid
  ORDER BY d
$q$ WHERE title = 'In-game purchases per day' AND is_builtin;

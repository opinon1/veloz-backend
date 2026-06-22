-- Read-only role for the operational stats dashboard (/admin/stats).
--
-- The dashboard runs admin-typed, ad-hoc SELECTs. We isolate that from write
-- access with a dedicated NOLOGIN role that holds ONLY SELECT. The app does not
-- log in as this role (no password, no extra secret): the stats pool connects
-- with the normal app user and `SET ROLE veloz_stats` on every connection,
-- dropping the session to this role's privileges. See main.rs.
--
-- Privilege checks use the current role, so once SET ROLE veloz_stats is active
-- the session can only read — it is not the table owner and has no write grants.

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'veloz_stats') THEN
    CREATE ROLE veloz_stats NOLOGIN;
  END IF;
END
$$;

-- Read path: schema usage + SELECT on every existing table.
GRANT USAGE  ON SCHEMA public TO veloz_stats;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO veloz_stats;

-- Future tables created by the app role auto-grant SELECT to veloz_stats, so
-- later migrations need no follow-up.
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO veloz_stats;

-- Belt-and-suspenders: ensure no write privilege ever lands on this role.
REVOKE INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER
  ON ALL TABLES IN SCHEMA public FROM veloz_stats;

-- Let the app's login role assume veloz_stats via SET ROLE.
GRANT veloz_stats TO CURRENT_USER;

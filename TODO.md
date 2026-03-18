# TODO

## Done

- **Fixed broken access token cleanup in `refresh.rs`** — Added `associated_access_token` to `SessionData`; refresh now deletes the correct Redis key.
- **Fixed `signup.rs` swallowing all DB errors as 409** — Now matches on PG error code `23505` for unique violations; all other errors return `500`.
- **`POST /auth/signout`** — Deletes both the access and refresh token from Redis and removes them from the `user_sessions` set.
- **`POST /auth/signout-all`** — Reads `user_sessions:{user_id}` set from Redis, deletes all token keys, then deletes the set.
- **`PATCH /auth/password`** — Verifies current password, validates new password, updates hash in Postgres, then calls signout-all to invalidate all sessions.
- **`DELETE /auth/account`** — Verifies password, calls signout-all, then deletes the user row from Postgres.
- **Refresh token theft detection** — On rotation, a `revoked_refresh:{token}` tombstone (TTL 5 min) is written. If an already-rotated token is presented, all sessions for that user are immediately invalidated.
- **`is_active` flag** — Migration `0002_add_is_active.sql` adds the column; `signin` returns `403` for inactive accounts.
- **Rate limiting** — `RateLimitLayer` in `src/middleware/rate_limit.rs`. Tower `Layer` + `Service` backed by Redis fixed-window counters. IP resolved from `X-Forwarded-For` (proxy/production) with fallback to TCP peer address (localhost). Applied: signup (5 req / 1 hr), signin (10 req / 1 min). Add to any route with `.layer(RateLimitLayer::new(redis, max, window_secs, "key"))`.
- **Session metadata** — `SessionData` now carries `created_at`, `user_agent`, `ip`. Populated on signin, preserved across token refreshes.
- **`GET /auth/sessions`** — Returns all active sessions for the authenticated user with metadata (created_at, user_agent, ip). No internal token fields exposed.
- **Verify endpoint no longer leaks token fields** — `GET /auth/verify` now returns a `VerifyResponse` with only `user_id`, `username`, `email`, `created_at`.

---

## Remaining

### Email verification
Emails are never verified. Users can sign up with arbitrary or fake emails, making password recovery and account ownership unverifiable.

Requires an email-sending dependency (e.g. `lettre`) and SMTP credentials in `.env`.

- On signup, generate a short-lived token: `email_verify:{token}` → `user_id` in Redis (TTL 24h)
- Add `email_verified BOOLEAN NOT NULL DEFAULT FALSE` column to `users` table
- Add `GET /auth/verify-email?token=...` endpoint to mark the email as verified
- Optionally block signin for unverified accounts

---

### Stale access token entries in `user_sessions` set
When an access token expires naturally (15 min TTL), Redis removes the `access_token:{uuid}` key but the entry remains in the `user_sessions:{user_id}` set indefinitely. Over time, active accounts accumulate dead entries.

Session listing already handles this correctly (iterates refresh token keys, not access token keys). The dead entries are harmless for `signout-all` (DEL on a missing key is a no-op). But for high-traffic accounts the set grows unbounded.

Fix options:
- Add a `session_id` field (= refresh token UUID) to `SessionData` so `signout` can `SREM` the exact access token key even after it has expired
- Or run a periodic cleanup job that intersects `user_sessions` set members with actually-existing Redis keys

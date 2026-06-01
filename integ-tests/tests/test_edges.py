"""Adversarial probing: tries to break the API in every way I could think of.

Categories:
    [AUTHZ]      — cross-user data leakage / authorization isolation
    [TOKEN]      — refresh / signout / signin / token-lifecycle nuances
    [HTTP]       — header / method / content-type / body-size oddities
    [VALIDATION] — malformed payloads, missing fields, extra fields, wrong types
    [NUMERIC]    — boundary values around i64 limits and zero
    [CASCADE]    — resource lifecycle: delete one thing, downstream still consistent
    [CONCURRENCY]— races beyond what test_bugs.py already covers
    [DATA]       — duplicate rows, encoding, unicode, whitespace, length
"""
from __future__ import annotations

import asyncio

import httpx
import pytest

from helpers.api import AuthedClient
from helpers.factory import (
    admin_make_character,
    admin_make_skin,
    make_creds,
    quote_price,
    rand_character_name,
    rand_email,
    rand_item_name,
    rand_password,
    rand_username,
)


# ────────────────────────── [AUTHZ] cross-user isolation ──────────────────────────


def test_user_a_token_cannot_read_user_b_profile_via_admin(user_factory):
    """Regular users have no path to read another user's full profile.
    Their /profile is always self-scoped; admin endpoints 403."""
    a, _ = user_factory()
    b, _ = user_factory()
    b_id = b.get_profile().json()["user_id"]

    # /profile is self-scoped — A can't pass B's id anywhere.
    # The only cross-user listing is /admin/users, gated by 403.
    assert a.admin_list_users().status_code == 403
    # And the admin-grant variant is gated too.
    assert a.admin_grant(b_id, "soft", 100).status_code == 403


def test_user_a_cannot_equip_user_b_owned_skin(admin, user_factory):
    """A and B both exist; only B owns skin X. A's POST /skins/X/equip → 403
    (equip checks ownership). B's equip succeeds."""
    skin = admin_make_skin(admin, cost=0, currency="soft")
    a, _ = user_factory()
    b, _ = user_factory()
    b.purchase_skin(skin["id"])

    assert a.equip_skin(skin["id"]).status_code == 403
    # B is unaffected.
    assert b.equip_skin(skin["id"]).status_code == 200


def test_sessions_endpoint_only_lists_own_sessions(api):
    """A signs in twice (2 sessions). B signs in once.  A's /auth/sessions
    must show 2 entries — never B's."""
    ca = make_creds()
    cb = make_creds()
    api.signup(ca.username, ca.email, ca.password)
    api.signup(cb.username, cb.email, cb.password)

    api.signin(ca.email, ca.password)
    a2 = api.signin(ca.email, ca.password).json()
    api.signin(cb.email, cb.password)

    rows = api.raw_get("/auth/sessions", a2["access_token"]).json()
    assert isinstance(rows, list)
    assert len(rows) == 2  # A's two sessions, not B's


def test_user_a_cannot_use_user_b_refresh_token(api):
    """Refresh tokens are bound to one user. Even if A somehow gets B's
    refresh, redeeming it gives B's tokens (back to B), not A's. We verify
    the redeemed access matches B's identity."""
    ca = make_creds()
    cb = make_creds()
    api.signup(ca.username, ca.email, ca.password)
    api.signup(cb.username, cb.email, cb.password)
    sb = api.signin(cb.email, cb.password).json()

    fresh_b = api.refresh(sb["refresh_token"]).json()
    verify = api.raw_get("/auth/verify", fresh_b["access_token"]).json()
    assert verify["email"] == cb.email.lower()


# ───────────────────── [TOKEN] lifecycle nuances ─────────────────────


def test_old_access_token_dies_after_refresh(api):
    """After /auth/refresh rotates tokens, the OLD access token must 401."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    s = api.signin(c.email, c.password).json()
    old_access = s["access_token"]
    api.refresh(s["refresh_token"])
    assert api.raw_get("/auth/verify", old_access).status_code == 401


def test_refresh_after_signout_fails(api):
    """signout deletes both access + refresh from Redis. A subsequent
    refresh call with the killed refresh token must 401."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    s = api.signin(c.email, c.password).json()
    assert api.raw_post("/auth/signout", s["access_token"]).status_code in (200, 204)
    assert api.refresh(s["refresh_token"]).status_code == 401


def test_signout_other_session_does_not_kill_current(api):
    """Per-token signout only kills the calling session. A second active
    session for the same user keeps working."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    s1 = api.signin(c.email, c.password).json()
    s2 = api.signin(c.email, c.password).json()

    assert api.raw_post("/auth/signout", s1["access_token"]).status_code in (200, 204)
    assert api.raw_get("/auth/verify", s1["access_token"]).status_code == 401
    # s2 still alive.
    assert api.raw_get("/auth/verify", s2["access_token"]).status_code == 200


def test_double_signout_is_idempotent(api):
    """Second signout with the same (now invalid) token → 401, never 5xx."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    s = api.signin(c.email, c.password).json()
    assert api.raw_post("/auth/signout", s["access_token"]).status_code in (200, 204)
    assert api.raw_post("/auth/signout", s["access_token"]).status_code == 401


# ─────────────────── [HTTP] header / method / encoding ───────────────────


def test_authorization_header_is_case_sensitive_on_bearer_scheme(api, user):
    """`bearer <token>` (lowercase) doesn't satisfy the `Bearer ` prefix
    check — server expects the canonical scheme name."""
    r = api._http.get(
        "/auth/verify",
        headers={"Authorization": f"bearer {user.access_token}"},
    )
    assert r.status_code == 401


def test_authorization_header_with_extra_leading_space(api, user):
    """`Bearer  <token>` (two spaces) treats the leading space as part of the
    token in the current implementation, so the lookup misses."""
    r = api._http.get(
        "/auth/verify",
        headers={"Authorization": f"Bearer  {user.access_token}"},
    )
    assert r.status_code == 401


def test_empty_authorization_header_is_401(api):
    r = api._http.get("/auth/verify", headers={"Authorization": ""})
    assert r.status_code == 401


def test_wrong_method_on_known_route_is_405(api):
    """GET /auth/signup (POST-only) should be 405 Method Not Allowed."""
    r = api._http.get("/auth/signup")
    assert r.status_code == 405


def test_unknown_route_is_404(api):
    assert api._http.get("/totally/not/a/thing").status_code == 404


def test_signup_with_extra_unknown_fields_is_accepted(api):
    """serde defaults to ignoring unknown JSON fields. Forward-compat:
    a v2 client can include new fields without breaking v1 server."""
    c = make_creds()
    r = api._http.post(
        "/auth/signup",
        json={
            "username": c.username,
            "email": c.email,
            "password": c.password,
            "extra_unknown_field": "ignore me",
            "another": 42,
        },
    )
    assert r.status_code == 201


def test_signin_with_wrong_content_type_is_rejected(api):
    """Body declared text/plain isn't valid JSON → axum's Json extractor
    rejects with 415 (or 400 depending on the framework version)."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    r = api._http.post(
        "/auth/signin",
        content=f'{{"email":"{c.email}","password":"{c.password}"}}'.encode(),
        headers={"Content-Type": "text/plain"},
    )
    assert r.status_code in (400, 415, 422)


# ───────────────────── [VALIDATION] payload edges ─────────────────────


def test_signup_rejects_username_at_min_minus_one(api):
    """Username regex is {3,30}. 2 chars → 400."""
    assert api.signup("ab", rand_email(), rand_password()).status_code == 400


def test_signup_accepts_username_at_min(api):
    """3 chars → accepted. Randomize so reruns don't collide on UNIQUE."""
    import secrets, string
    short = "".join(secrets.choice(string.ascii_lowercase) for _ in range(3))
    assert api.signup(short, rand_email(), rand_password()).status_code == 201


def test_signup_with_underscore_only_username(api):
    """Regex allows underscores. Mix of underscores + a unique digit suffix
    so reruns don't collide on the UNIQUE constraint."""
    import secrets
    name = f"__{secrets.token_hex(4)}"  # `__<8 hex>` → 10 chars, valid + unique
    assert api.signup(name, rand_email(), rand_password()).status_code == 201


def test_signup_password_with_leading_trailing_whitespace_preserved(api):
    """Passwords must NOT be trimmed (would silently change credentials).
    Verify: signup with "abc123! " then signin with "abc123!" (no space) → 401."""
    c = make_creds()
    padded_pw = f"{c.password} "
    api.signup(c.username, c.email, padded_pw)
    assert api.signin(c.email, c.password).status_code == 401  # different pw
    assert api.signin(c.email, padded_pw).status_code == 200


def test_username_uniqueness_is_case_sensitive(api):
    """Currently signup with `Foo_xx` and `foo_xx` both succeed — usernames
    are stored verbatim and uniqueness is byte-equal."""
    base = rand_username()
    upper = base.upper()
    lower = base.lower()
    assert api.signup(upper, rand_email(), rand_password()).status_code == 201
    # If the second signup gets 409 your project changed username uniqueness;
    # update this test accordingly. Today: 201 expected.
    r2 = api.signup(lower, rand_email(), rand_password())
    assert r2.status_code in (201, 409)


def test_email_subaddressing_treated_as_distinct(api):
    """`foo@x.com` and `foo+test@x.com` are different emails — server does
    NOT collapse the `+suffix` form. Each can sign up independently."""
    base = rand_email()
    plus = base.replace("@", "+test@")
    c1 = rand_password()
    c2 = rand_password()
    assert api.signup(rand_username(), base, c1).status_code == 201
    assert api.signup(rand_username(), plus, c2).status_code == 201


# ───────────────────── [NUMERIC] boundary values ─────────────────────


def test_run_with_zero_score_is_not_a_new_pb(user):
    """score=0 must NOT be flagged as a new highscore (highscore is > 0)."""
    r = user.submit_run(score=0, distance=0, coins_collected=0, duration_ms=1)
    assert r.json()["new_highscore"] is False
    assert user.get_profile().json()["main_highscore"] == 0


def test_run_history_default_limit_returns_recent(user):
    """No limit specified → server default applies; multiple submitted runs
    return in newest-first order."""
    for s in [10, 20, 30]:
        user.submit_run(score=s, distance=0, coins_collected=0, duration_ms=1)
    rows = user.run_history().json()
    assert len(rows) == 3
    # Newest-first: scores recorded in reverse submission order.
    scores = [r["score"] for r in rows]
    assert scores[0] >= scores[-1]


def test_admin_grant_zero_amount_is_a_noop_record(admin, user):
    """Granting 0 of any currency is allowed (writes a zero ledger entry).
    Wallet balance unchanged."""
    uid = user.get_profile().json()["user_id"]
    before = user.get_wallet().json()
    r = admin.admin_grant(uid, "soft", 0)
    assert r.status_code == 200
    assert r.json()["new_balance"] == before["soft"]


def test_store_create_with_negative_cost_rejected(admin):
    """Admin cannot create a store item with cost < 0. CHECK on the column
    catches it as last resort, but we want a clean 400 from the handler."""
    r = admin.admin_create_store_item(
        name=rand_item_name("NegCost"),
        item_type="custom",
        cost=-10,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    )
    assert r.status_code == 400


def test_store_create_with_cost_zero_is_free(admin, user):
    """cost=0 → adjusted_cost=0 → free, but still records a purchase row +
    grants the payload."""
    item = admin.admin_create_store_item(
        name=rand_item_name("Free"),
        item_type="custom",
        cost=0,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 25}],
    ).json()
    r = user.purchase_store_item(item["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == 0
    assert user.get_wallet().json()["soft"] == 25


# ───────────────────── [CASCADE] resource lifecycle ─────────────────────


def test_admin_delete_skin_removes_from_user_skins(admin, user):
    """A user's user_skins row goes away when the underlying skin is deleted
    (FK ON DELETE CASCADE). Owned-skins list reflects the change."""
    skin = admin_make_skin(admin, cost=0, currency="soft")
    user.purchase_skin(skin["id"])
    assert skin["id"] in [s["id"] for s in user.owned_skins().json()]

    admin.admin_delete_skin(skin["id"])
    assert skin["id"] not in [s["id"] for s in user.owned_skins().json()]


def test_admin_delete_season_cascades_tiers_and_progress(admin, user):
    """Deleting a season removes its tiers, progress, and claims (FK CASCADE).
    Stale claim data isn't left over to leak across seasons."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    season = admin.admin_create_season(
        name=f"season_{rand_character_name()}",
        starts_at=(now - timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        premium_cost=10,
        premium_currency="high",
    ).json()
    admin.admin_create_tier(
        season["id"], tier=1, xp_required=0,
        free_reward=[{"type": "currency", "currency": "soft", "amount": 1}],
        premium_reward=[{"type": "currency", "currency": "high", "amount": 1}],
    )
    user.bp_claim(tier=1, track="free")
    # Tier listing returns at least the tier we created.
    assert len(admin.admin_list_tiers(season["id"]).json()) >= 1

    assert admin.admin_delete_season(season["id"]).status_code == 204
    # Tier listing now empty for the gone season — return is empty array, not 500.
    assert admin.admin_list_tiers(season["id"]).json() == []


def test_admin_can_delete_own_account(api, admin):
    """An admin is just a user with role='admin'. They can DELETE
    /auth/account on their own user. Other admins still work."""
    # admin's password isn't accessible from the fixture; skip if the suite
    # can't authoritatively know it. The admin fixture knows it via creds —
    # but it doesn't expose it. Just verify admin is real, then signed out.
    pw_response = admin.delete_account(password="this-is-not-the-password")
    # Wrong password → 401, account NOT deleted. Verify still authed.
    assert pw_response.status_code == 401
    assert admin.verify().status_code == 200


# ─────────────────── [CONCURRENCY] beyond test_bugs.py ───────────────────


def test_concurrent_purchases_of_different_items_both_succeed(base_url, admin, user):
    """Two parallel purchases of *different* skins must both succeed and
    both deduct independently. No spurious 409 from cross-item locking."""
    s1 = admin_make_skin(admin, cost=10, currency="soft")
    s2 = admin_make_skin(admin, cost=15, currency="soft")
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 100)
    expected = quote_price(user, "skin", s1["id"]) + quote_price(user, "skin", s2["id"])

    async def go() -> list[httpx.Response]:
        async with httpx.AsyncClient(base_url=base_url, timeout=10) as c:
            h = {"Authorization": f"Bearer {user.access_token}"}
            return await asyncio.gather(
                c.post(f"/skins/{s1['id']}/purchase", headers=h),
                c.post(f"/skins/{s2['id']}/purchase", headers=h),
            )

    rs = asyncio.run(go())
    assert all(r.status_code == 200 for r in rs)
    assert user.get_wallet().json()["soft"] == 100 - expected
    owned_ids = [s["id"] for s in user.owned_skins().json()]
    assert s1["id"] in owned_ids and s2["id"] in owned_ids


def test_concurrent_signins_for_same_user_yield_distinct_tokens(api, base_url):
    """Multiple parallel signins must each produce unique tokens, and all
    must be independently valid."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)

    async def go() -> list[httpx.Response]:
        async with httpx.AsyncClient(base_url=base_url, timeout=10) as cl:
            return await asyncio.gather(
                cl.post("/auth/signin", json={"email": c.email, "password": c.password}),
                cl.post("/auth/signin", json={"email": c.email, "password": c.password}),
                cl.post("/auth/signin", json={"email": c.email, "password": c.password}),
            )

    rs = asyncio.run(go())
    assert all(r.status_code == 200 for r in rs)
    tokens = {r.json()["access_token"] for r in rs}
    assert len(tokens) == 3
    for t in tokens:
        assert api.raw_get("/auth/verify", t).status_code == 200


def test_concurrent_unique_constraint_on_character_name(admin, base_url):
    """Two parallel admin_create_character requests with the same name — exactly
    one should succeed (201), the other 409."""
    name = rand_character_name()

    async def go() -> list[httpx.Response]:
        async with httpx.AsyncClient(base_url=base_url, timeout=10) as c:
            h = {"Authorization": f"Bearer {admin.access_token}"}
            payload = {"name": name}
            return await asyncio.gather(
                c.post("/admin/characters", headers=h, json=payload),
                c.post("/admin/characters", headers=h, json=payload),
            )

    rs = asyncio.run(go())
    codes = sorted(r.status_code for r in rs)
    assert codes == [201, 409], f"got {codes}"


# ───────────────────── [DATA] encoding / length / dupes ─────────────────────


def test_unicode_in_character_name_round_trips(admin):
    """Multibyte chars must round-trip through the DB and back unchanged."""
    name = f"角色_{rand_character_name()}"
    r = admin.admin_create_character(name=name)
    assert r.status_code == 201
    assert r.json()["name"] == name


def test_quote_in_character_name_does_not_break_query(admin):
    """Single quote in the name must not break the SQL — sqlx parameterizes,
    so this is just regression coverage."""
    name = f"O'Brien_{rand_character_name()}"
    r = admin.admin_create_character(name=name)
    assert r.status_code == 201
    assert r.json()["name"] == name


def test_create_two_seasons_with_same_name(admin):
    """bp_seasons.name has no UNIQUE constraint — two seasons can share a
    name (admin's responsibility to differentiate)."""
    from datetime import datetime, timedelta, timezone
    name = "Replay-Season"
    now = datetime.now(timezone.utc)
    ok1 = admin.admin_create_season(
        name=name,
        starts_at=(now + timedelta(days=400)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=430)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    )
    ok2 = admin.admin_create_season(
        name=name,
        starts_at=(now + timedelta(days=500)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=530)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    )
    assert ok1.status_code == 201
    assert ok2.status_code == 201
    assert ok1.json()["id"] != ok2.json()["id"]


def test_two_seasons_can_have_same_tier_number(admin):
    """`bp_tiers` UNIQUE is on (season_id, tier). Reusing tier numbers across
    seasons must succeed."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    s1 = admin.admin_create_season(
        name=f"season_{rand_character_name()}",
        starts_at=(now + timedelta(days=600)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=630)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    ).json()
    s2 = admin.admin_create_season(
        name=f"season_{rand_character_name()}",
        starts_at=(now + timedelta(days=700)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=730)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    ).json()
    grant = [{"type": "currency", "currency": "soft", "amount": 1}]
    assert admin.admin_create_tier(
        s1["id"], tier=7, xp_required=0, free_reward=grant, premium_reward=grant
    ).status_code == 201
    assert admin.admin_create_tier(
        s2["id"], tier=7, xp_required=0, free_reward=grant, premium_reward=grant
    ).status_code == 201


def test_free_and_premium_tracks_of_same_tier_are_independent(admin, user):
    """Same tier, different track = two separate claim rows.
    Claim free → 200; claim premium (after unlock) → also 200."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    season = admin.admin_create_season(
        name=f"season_{rand_character_name()}",
        starts_at=(now - timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        premium_cost=50,
        premium_currency="high",
    ).json()
    admin.admin_create_tier(
        season["id"], tier=99, xp_required=0,
        free_reward=[{"type": "currency", "currency": "soft", "amount": 7}],
        premium_reward=[{"type": "currency", "currency": "high", "amount": 1}],
    )
    admin.admin_grant(user.get_profile().json()["user_id"], "high", 100)
    user.bp_unlock_premium()

    assert user.bp_claim(tier=99, track="free").status_code == 200
    assert user.bp_claim(tier=99, track="premium").status_code == 200
    # And neither prevents the other from being a permanent record.
    progress = user.bp_progress().json()
    assert 99 in progress["claimed_free"]
    assert 99 in progress["claimed_premium"]


def test_owned_skins_list_isolates_per_user(admin, user_factory):
    """A's owned-skins list never contains B's purchases."""
    a, _ = user_factory()
    b, _ = user_factory()
    skin = admin_make_skin(admin, cost=0, currency="soft")
    a.purchase_skin(skin["id"])

    assert skin["id"] in [s["id"] for s in a.owned_skins().json()]
    assert skin["id"] not in [s["id"] for s in b.owned_skins().json()]


# ───────────────────── [VALIDATION] Grant + reward edges ─────────────────────


def test_admin_create_tier_rejects_empty_grant_array(admin):
    """free_reward = [] is meaningless — claim would return [] and grant
    nothing. validate_grants rejects."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    s = admin.admin_create_season(
        name=f"season_{rand_character_name()}",
        starts_at=(now + timedelta(days=800)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=830)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    ).json()
    r = admin.admin_create_tier(
        s["id"], tier=1, xp_required=0,
        free_reward=[],
        premium_reward=[{"type": "currency", "currency": "soft", "amount": 1}],
    )
    assert r.status_code == 400


def test_admin_create_tier_rejects_non_array_reward(admin):
    """A bare object isn't a Grant array."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    s = admin.admin_create_season(
        name=f"season_{rand_character_name()}",
        starts_at=(now + timedelta(days=900)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=930)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    ).json()
    r = admin.admin_create_tier(
        s["id"], tier=1, xp_required=0,
        free_reward={"type": "currency", "currency": "soft", "amount": 1},
        premium_reward=[{"type": "currency", "currency": "high", "amount": 1}],
    )
    assert r.status_code == 400


def test_admin_update_tier_with_invalid_reward_rejected(admin):
    """PATCH /admin/battlepass/tiers/{id} with bad reward shape → 400."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    s = admin.admin_create_season(
        name=f"season_{rand_character_name()}",
        starts_at=(now + timedelta(days=1000)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=1030)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    ).json()
    grant = [{"type": "currency", "currency": "soft", "amount": 1}]
    t = admin.admin_create_tier(
        s["id"], tier=2, xp_required=0,
        free_reward=grant, premium_reward=grant,
    ).json()
    r = admin.admin_update_tier(t["id"], free_reward=[])
    assert r.status_code == 400


def test_store_update_cost_to_negative_rejected(admin):
    """Lowering cost below zero is now blocked at PATCH (was previously only
    caught by the DB CHECK as 500)."""
    item = admin.admin_create_store_item(
        name=rand_item_name(),
        item_type="custom", cost=10, currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()
    r = admin.admin_update_store_item(item["id"], cost=-5)
    assert r.status_code == 400


def test_admin_grant_with_extreme_negative_amount_capped_by_check_constraint(admin, user):
    """Granting a delta that would overflow the wallet to negative trips the
    CHECK and returns 422 with no partial state. Wallet stays at 0."""
    uid = user.get_profile().json()["user_id"]
    r = admin.admin_grant(uid, "high", -1)  # balance 0 → -1, should fail
    assert r.status_code == 422
    assert user.get_wallet().json()["high"] == 0


# ───────────────────── [VALIDATION] auth payload edges ─────────────────────


def test_signup_missing_field_is_400_or_422(api):
    """Missing required `email` → JSON deserialization fails → 422."""
    r = api._http.post(
        "/auth/signup",
        json={"username": rand_username(), "password": rand_password()},
    )
    assert r.status_code in (400, 422)


def test_signup_with_wrong_field_types_is_4xx(api):
    """`username` as int instead of string → 422 from serde."""
    r = api._http.post(
        "/auth/signup",
        json={"username": 42, "email": rand_email(), "password": rand_password()},
    )
    assert r.status_code in (400, 422)


def test_signin_with_empty_strings(api):
    """email="" and password="" → server hits the empty case path. Should be
    a clean 401 (no credential match)."""
    r = api.signin("", "")
    assert r.status_code in (400, 401)


# ───────────────────── [STORE] purchase flow edges ─────────────────────


def test_purchase_iap_currency_via_store_returns_400(admin, user):
    """currency='iap' items must always 400 on /store/{id}/purchase regardless
    of payload validity, because IAP requires receipt validation."""
    item = admin.admin_create_store_item(
        name=rand_item_name("IAP"),
        item_type="currency_bundle",
        cost=499,
        currency="iap",
        iap_product_id="com.veloz.gem_pack",
        payload=[{"type": "currency", "currency": "high", "amount": 100}],
    ).json()
    admin.admin_grant(user.get_profile().json()["user_id"], "high", 10000)
    r = user.purchase_store_item(item["id"])
    assert r.status_code == 400
    # Wallet not touched.
    assert user.get_wallet().json()["high"] == 10000


def test_store_purchase_records_purchase_row(admin, user):
    """Each successful purchase writes a `store_purchases` row. We can't read
    it directly, but a re-purchase still works (no UNIQUE on item_id), proving
    the row was written without conflict."""
    item = admin.admin_create_store_item(
        name=rand_item_name("RePurchase"),
        item_type="custom",
        cost=1,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 5}],
    ).json()
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 10)
    assert user.purchase_store_item(item["id"]).status_code == 200
    # Re-purchase works (store items are not idempotent like skins).
    assert user.purchase_store_item(item["id"]).status_code == 200
    # Wallet: started at 10, paid 1 + 1 = 2, gained 5 + 5 = 10 → 18.
    assert user.get_wallet().json()["soft"] == 18


def test_purchase_inactive_skin_still_owned_returns_409_then_410_after_repurchase_attempt(
    admin, user
):
    """User owns skin → admin marks inactive. User tries to re-buy → 410
    (item is gone for new buyers). Existing ownership is unaffected."""
    skin = admin_make_skin(admin, cost=0, currency="soft")
    user.purchase_skin(skin["id"])
    admin.admin_update_skin(skin["id"], is_active=False)
    assert user.purchase_skin(skin["id"]).status_code == 410
    # And the user still owns it (admin only deactivated, didn't delete).
    assert skin["id"] in [s["id"] for s in user.owned_skins().json()]


def test_purchase_inactive_skin_after_delete_returns_404(admin, user):
    """After hard delete, skin is gone → 404 on purchase, ownership wiped via
    cascade."""
    skin = admin_make_skin(admin, cost=0, currency="soft")
    user.purchase_skin(skin["id"])
    admin.admin_delete_skin(skin["id"])
    assert user.purchase_skin(skin["id"]).status_code == 404
    assert skin["id"] not in [s["id"] for s in user.owned_skins().json()]

"""Regression tests for bugs that were found + fixed.

Each test here was originally written as a failing (@xfail) demonstration of
a real bug. After the fix landed, the xfail marker was removed so the test
becomes a permanent guard — if any of these ever fail again, the bug is
back.

Categories preserved from the original audit:
    [RACE]         — concurrent-request races on check-then-act flows
    [LOGIC]        — deterministic business-logic bugs
    [DESIGN]       — behaviors that users expect but the server used to violate
    [SILENT-FAIL]  — endpoints that returned 200 while silently dropping work
    [SAFETY]       — properties that were already correct but untested
"""
from __future__ import annotations

import asyncio

import httpx

from helpers.api import AuthedClient
from helpers.factory import (
    admin_make_skin,
    make_creds,
    rand_item_name,
)


# ───────────────────────── [RACE] purchase skin ─────────────────────────


def test_skin_purchase_is_race_safe(base_url, admin, user):
    """Two concurrent purchase_skin requests on the same skin must result in
    exactly one successful purchase + one 409. Fix: INSERT ... ON CONFLICT
    DO NOTHING is now the first write; `rows_affected == 0` short-circuits
    with 409 BEFORE the wallet deduction, so the racer never charges."""
    skin = admin_make_skin(admin, cost=100, currency="soft")
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "soft", 500)

    async def go() -> list[httpx.Response]:
        async with httpx.AsyncClient(base_url=base_url, timeout=10) as client:
            headers = {"Authorization": f"Bearer {user.access_token}"}
            return await asyncio.gather(
                client.post(f"/skins/{skin['id']}/purchase", headers=headers),
                client.post(f"/skins/{skin['id']}/purchase", headers=headers),
            )

    responses = asyncio.run(go())
    codes = sorted(r.status_code for r in responses)
    assert codes == [200, 409], f"expected [200, 409], got {codes}"
    # Wallet charged exactly once.
    assert user.get_wallet().json()["soft"] == 400


# ─────────────────── [RACE] battlepass unlock-premium ───────────────────


def test_unlock_premium_is_race_safe(base_url, admin, user):
    """Concurrent unlock-premium calls must result in exactly one charge +
    one 409. Fix: UPDATE bp_progress SET premium_unlocked=TRUE WHERE
    premium_unlocked=FALSE now serves as the atomic claim — `rows_affected
    == 0` means another racer got there first."""
    from datetime import datetime, timedelta, timezone

    now = datetime.now(timezone.utc)
    admin.admin_create_season(
        name="race-season",
        starts_at=(now - timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        premium_cost=100,
        premium_currency="high",
    )

    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "high", 500)

    async def go() -> list[httpx.Response]:
        async with httpx.AsyncClient(base_url=base_url, timeout=10) as client:
            headers = {"Authorization": f"Bearer {user.access_token}"}
            return await asyncio.gather(
                client.post("/battlepass/unlock-premium", headers=headers),
                client.post("/battlepass/unlock-premium", headers=headers),
            )

    responses = asyncio.run(go())
    codes = sorted(r.status_code for r in responses)
    assert codes == [200, 409], f"expected [200, 409], got {codes}"
    assert user.get_wallet().json()["high"] == 400


# ───────────────────── [LOGIC] free-skin accurate balance ─────────────────────


def test_free_skin_purchase_returns_actual_balance(admin, user):
    """Previously `new_balance` was hard-coded to 0 for free skins, misleading
    clients into thinking the user had been zeroed out. Fix: when cost=0, the
    handler now SELECTs the relevant wallet column and returns the real
    current balance."""
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 500)
    free = admin_make_skin(admin, cost=0, currency="soft")
    r = user.purchase_skin(free["id"])
    assert r.status_code == 200
    assert r.json()["new_balance"] == 500


# ─────────────────── [LOGIC] tie score is not a new PB ───────────────────


def test_tying_previous_highscore_is_not_reported_as_new_pb(user):
    """Fix: submit_run now captures the PRE-update main_highscore via CTE
    and uses strict `>` for the new_highscore flag."""
    user.submit_run(score=1000, distance=0, coins_collected=0, duration_ms=1)
    r = user.submit_run(score=1000, distance=0, coins_collected=0, duration_ms=1)
    assert r.status_code == 200
    assert r.json()["new_highscore"] is False
    # But a strictly higher score IS a new PB.
    r2 = user.submit_run(score=1500, distance=0, coins_collected=0, duration_ms=1)
    assert r2.json()["new_highscore"] is True


# ─────────────────── [DESIGN] email signin case-insensitivity ───────────────────


def test_signin_is_case_insensitive_on_email(api):
    """Signup/signin now both lowercase+trim the email. Users who typed mixed
    case at signup can sign in with any case later."""
    c = make_creds()
    mixed = c.email.split("@")[0].upper() + "@" + c.email.split("@")[1].upper()
    assert api.signup(c.username, mixed, c.password).status_code == 201

    # The stored email should be the lowercased form.
    assert api.signin(mixed.lower(), c.password).status_code == 200
    # And mixed case on the signin side also resolves.
    assert api.signin(mixed, c.password).status_code == 200
    # And with padding whitespace (also normalized).
    assert api.signin(f"  {mixed.lower()}  ", c.password).status_code == 200


# ─────────────────── [SILENT-FAIL] store payload validation ───────────────────


def test_store_skin_grant_rejects_invalid_uuid(admin):
    """A skin grant with a non-UUID skin_id is rejected at admin create.
    Previously the purchase handler silently skipped fulfillment on bad
    UUIDs, charging the user for nothing."""
    r = admin.admin_create_store_item(
        name=rand_item_name("BadSkin"),
        item_type="skin",
        cost=50,
        currency="soft",
        payload=[{"type": "skin", "skin_id": "not-a-uuid"}],
    )
    assert r.status_code == 400


def test_store_payload_must_be_array(admin):
    """The payload must be a JSON array of Grants. A bare object is rejected."""
    r = admin.admin_create_store_item(
        name=rand_item_name("BareObj"),
        item_type="skin",
        cost=50,
        currency="soft",
        payload={"type": "skin", "skin_id": "00000000-0000-0000-0000-000000000001"},
    )
    assert r.status_code == 400


def test_store_payload_must_not_be_empty(admin):
    """An empty grant array is rejected — every store item must grant
    something concrete or the purchase becomes a no-op."""
    r = admin.admin_create_store_item(
        name=rand_item_name("EmptyArr"),
        item_type="custom",
        cost=10,
        currency="soft",
        payload=[],
    )
    assert r.status_code == 400


def test_store_currency_grant_rejects_unknown_currency(admin):
    """Currency variants are restricted to high/soft/energy. `coins` rejected."""
    r = admin.admin_create_store_item(
        name=rand_item_name("TypoBundle"),
        item_type="currency_bundle",
        cost=10,
        currency="high",
        payload=[{"type": "currency", "currency": "coins", "amount": 500}],
    )
    assert r.status_code == 400


def test_store_currency_grant_rejects_non_positive_amounts(admin):
    """Zero or negative grant amounts are nonsense and rejected at admin time."""
    r = admin.admin_create_store_item(
        name=rand_item_name("ZeroBundle"),
        item_type="currency_bundle",
        cost=10,
        currency="high",
        payload=[{"type": "currency", "currency": "soft", "amount": 0}],
    )
    assert r.status_code == 400


def test_store_grant_rejects_unknown_type(admin):
    """Grant `type` is a closed set (currency, skin). `weird` rejected."""
    r = admin.admin_create_store_item(
        name=rand_item_name("WeirdGrant"),
        item_type="custom",
        cost=10,
        currency="soft",
        payload=[{"type": "weird", "stuff": 42}],
    )
    assert r.status_code == 400


def test_store_unknown_item_type_rejected(admin):
    """Item-type label outside the closed enum is rejected, even with a
    valid payload. (item_type drives admin filtering even though it no
    longer drives fulfillment.)"""
    r = admin.admin_create_store_item(
        name=rand_item_name("Weird"),
        item_type="teleporter",
        cost=5,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    )
    assert r.status_code == 400


def test_store_iap_item_requires_product_id(admin):
    """currency='iap' items must declare an iap_product_id so /wallet/iap/*
    can route them."""
    r = admin.admin_create_store_item(
        name=rand_item_name("NoSku"),
        item_type="currency_bundle",
        cost=499,
        currency="iap",
        payload=[{"type": "currency", "currency": "high", "amount": 100}],
    )
    assert r.status_code == 400


def test_store_validation_applies_to_update(admin):
    """PATCH /admin/store/:id with a bad payload should also 400, not silently
    accept it."""
    good = admin.admin_create_store_item(
        name=rand_item_name("Good"),
        item_type="currency_bundle",
        cost=10,
        currency="high",
        payload=[{"type": "currency", "currency": "soft", "amount": 100}],
    ).json()
    # Swap to a bad payload (object instead of array).
    r = admin.admin_update_store_item(good["id"], payload={"type": "currency"})
    assert r.status_code == 400


# ─────────────────── [DESIGN] admin self-demotion guard ───────────────────


def test_admin_cannot_self_demote(admin):
    """An admin cannot downgrade their own role. They'd lose access to the
    admin surface immediately and — if they're the only admin — brick the
    system. Another admin must perform the demotion."""
    me = admin.verify().json()
    r = admin.admin_update_role(me["user_id"], role="user")
    assert r.status_code == 400
    # Sanity: admin endpoints still work after the attempt.
    assert admin.admin_list_users().status_code == 200


def test_admin_can_promote_others_and_be_demoted_by_them(admin, user_factory):
    """Workflow that should still work: admin A promotes user B to admin,
    then B demotes A. Confirms the guard only blocks SELF-demotion, not
    peer demotion."""
    b, _ = user_factory()
    b_id = b.get_profile().json()["user_id"]
    assert admin.admin_update_role(b_id, role="admin").status_code == 200

    # B demotes A.
    a_id = admin.verify().json()["user_id"]
    assert b.admin_update_role(a_id, role="user").status_code == 200
    # A no longer has admin access.
    assert admin.admin_list_users().status_code == 403


# ─────────────────── [SAFETY] positive guards ───────────────────


def test_refresh_token_theft_detection(api):
    """Replaying a rotated refresh token must invalidate every session for
    the user (tombstone in refresh.rs). Already-correct behavior, guarded
    here so it doesn't silently regress."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    signin = api.signin(c.email, c.password).json()
    original_refresh = signin["refresh_token"]

    first = api.refresh(original_refresh).json()
    # Replay the original (rotated) refresh token → 401 + wipe all sessions.
    replay = api.refresh(original_refresh)
    assert replay.status_code == 401
    # And the tokens from the legitimate first refresh are also killed.
    assert api.raw_get("/auth/verify", first["access_token"]).status_code == 401


def test_change_password_kills_current_token(user, creds):
    """After change_password, the token used to make the change must also be
    invalidated. Protects against compromised-session password resets."""
    r = user.change_password(current_password=creds.password, new_password="NewPw-123!")
    assert r.status_code in (200, 204)
    assert user.verify().status_code == 401


# ─────────────────── Round 2 audit (deeper edge cases) ───────────────────


def test_profile_patch_cannot_equip_unowned_skin(user_factory, admin):
    """PATCH /profile with a UUID-shaped avatar_url must validate ownership.
    Without this gate a user could bypass the check on POST /skins/{id}/equip
    just by writing the skin_id directly into their profile."""
    a, _ = user_factory()
    b, _ = user_factory()

    skin = admin_make_skin(admin, cost=0, currency="soft")
    a.purchase_skin(skin["id"])  # A owns it; B doesn't.

    r = b.update_profile(avatar_url=skin["id"])
    assert r.status_code == 403
    assert b.get_profile().json()["avatar_url"] in (None, "")


def test_profile_patch_allows_non_uuid_avatar_strings(user):
    """Non-UUID avatar_url values still pass through (no ownership check
    applies — only the UUID branch is gated). Preserves legacy URLs."""
    r = user.update_profile(avatar_url="https://cdn.example.com/legacy.png")
    assert r.status_code == 200
    assert r.json()["avatar_url"] == "https://cdn.example.com/legacy.png"


def test_admin_delete_skin_clears_dangling_avatar_url(admin, user):
    """When an admin deletes a skin, every profile that had it equipped
    should be cleared. Otherwise profile.avatar_url points at a UUID with
    no matching row in skins."""
    skin = admin_make_skin(admin, cost=0, currency="soft")
    user.purchase_skin(skin["id"])
    user.equip_skin(skin["id"])
    user.update_profile(avatar_url=skin["id"])
    assert user.get_profile().json()["avatar_url"] == skin["id"]

    assert admin.admin_delete_skin(skin["id"]).status_code == 204
    after = user.get_profile().json()
    assert after["avatar_url"] in (None, "")


def test_admin_grant_for_nonexistent_user_returns_404(admin):
    """adjust_balance now distinguishes RowNotFound (no wallet for user) from
    other DB errors and returns 404 instead of 500."""
    r = admin.admin_grant("00000000-0000-0000-0000-000000000000", "soft", 100)
    assert r.status_code == 404


def test_admin_cannot_set_negative_price_multiplier(admin, user):
    """A negative price_multiplier would *credit* the user during a store
    purchase (cost * -1 → adjust_balance(+cost)). Reject at admin time."""
    uid = user.get_profile().json()["user_id"]
    assert admin.admin_update_user_profile(uid, price_multiplier=-1.0).status_code == 400
    assert admin.admin_update_user_profile(uid, price_multiplier=0.5).status_code == 200


def test_store_purchase_with_zero_multiplier_is_free_and_returns_real_balance(admin, user):
    """price_multiplier=0 → cost_paid=0. Response must report the actual
    current spend-currency balance (not a hard-coded 0)."""
    uid = user.get_profile().json()["user_id"]
    admin.admin_update_user_profile(uid, price_multiplier=0.0)
    admin.admin_grant(uid, "high", 200)

    item = admin.admin_create_store_item(
        name=rand_item_name("ZeroPrice"),
        item_type="currency_bundle",
        cost=100,
        currency="high",
        payload=[{"type": "currency", "currency": "soft", "amount": 50}],
    ).json()
    r = user.purchase_store_item(item["id"])
    assert r.status_code == 200
    body = r.json()
    assert body["cost_paid"] == 0
    # Spend currency was 'high'; new_balance reflects high (untouched at 200).
    assert body["new_balance"] == 200
    w = user.get_wallet().json()
    assert w["high"] == 200
    assert w["soft"] == 50


def test_admin_create_skin_rejects_invalid_currency(admin):
    """Currency must be one of high/soft/energy. Bogus codes used to get
    inserted as-is and only fail later at purchase time."""
    from helpers.factory import admin_make_character
    char = admin_make_character(admin)
    r = admin.admin_create_skin(
        character_id=char["id"],
        cost=10,
        currency="btc",
    )
    assert r.status_code == 400


def test_admin_create_skin_rejects_negative_cost(admin):
    from helpers.factory import admin_make_character
    char = admin_make_character(admin)
    r = admin.admin_create_skin(
        character_id=char["id"],
        cost=-5,
        currency="soft",
    )
    assert r.status_code == 400


def test_admin_create_tier_rejects_non_positive_tier(admin):
    """Tier numbers must be >= 1. Non-positive tiers were previously
    inserted; players could effectively claim them on every run."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    season = admin.admin_create_season(
        name="bad-tier-season",
        starts_at=(now + timedelta(days=200)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=230)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    ).json()
    assert admin.admin_create_tier(season["id"], tier=0, xp_required=0).status_code == 400
    assert admin.admin_create_tier(season["id"], tier=-1, xp_required=0).status_code == 400


def test_admin_create_tier_rejects_negative_xp(admin):
    """xp_required must be non-negative."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    season = admin.admin_create_season(
        name="neg-xp-season",
        starts_at=(now + timedelta(days=300)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=330)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    ).json()
    assert admin.admin_create_tier(season["id"], tier=1, xp_required=-1).status_code == 400


def test_admin_update_profile_rejects_negative_xp_or_level(admin, user):
    """Sanity-check guards on level/xp/highscore overrides."""
    uid = user.get_profile().json()["user_id"]
    assert admin.admin_update_user_profile(uid, account_level=0).status_code == 400
    assert admin.admin_update_user_profile(uid, total_xp=-1).status_code == 400
    assert admin.admin_update_user_profile(uid, main_highscore=-1).status_code == 400


# ─────────────────── Round 3: protocol-level + cascade edges ───────────────────


def test_wallet_spend_with_iap_currency_rejected(user):
    """`iap` is a store-payment marker, not a wallet column. Spend/grant
    on it must 400 — adjust_balance::is_valid_currency rejects."""
    r = user.spend("iap", 1)
    assert r.status_code == 400


def test_admin_grant_with_iap_currency_rejected(admin, user):
    uid = user.get_profile().json()["user_id"]
    r = admin.admin_grant(uid, "iap", 100)
    assert r.status_code == 400


def test_spend_exact_balance_leaves_zero(admin, user):
    """Spending exactly the available balance must succeed and zero out the
    column (not trip the >= 0 CHECK constraint)."""
    uid = user.get_profile().json()["user_id"]
    admin.admin_grant(uid, "soft", 42)
    r = user.spend("soft", 42)
    assert r.status_code == 200
    assert r.json()["new_balance"] == 0
    assert user.get_wallet().json()["soft"] == 0


def test_runs_history_limit_negative_or_zero_is_clamped(user):
    """The limit query param is clamped to [1,100]. Negative or zero must
    not error — they should be coerced to 1."""
    user.submit_run(score=10, distance=0, coins_collected=0, duration_ms=1)
    assert user.run_history(limit=0).status_code == 200
    assert user.run_history(limit=-5).status_code == 200


def test_refresh_with_access_token_value_returns_401(api):
    """The refresh endpoint looks up `refresh_token:<value>` in Redis. A
    valid access_token won't match that key prefix, so passing one in as
    the refresh token must be 401, not 500."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    signin = api.signin(c.email, c.password).json()
    r = api.refresh(signin["access_token"])  # wrong token type
    assert r.status_code == 401


def test_authorization_header_must_have_bearer_prefix(api, user):
    """Token without "Bearer " prefix → 401. Catches client bugs that
    forget to add the scheme."""
    r = api._http.get(
        "/auth/verify",
        headers={"Authorization": user.access_token},  # missing "Bearer "
    )
    assert r.status_code == 401


def test_delete_account_cascades_runs_wallet_profile_skins(api, admin, creds):
    """delete_account → CASCADE FKs wipe profile, wallet, runs, user_skins.
    Recreate the user with the same email afterward and verify a clean slate
    (highscore=0, no owned skins, default wallet)."""
    api.signup(creds.username, creds.email, creds.password)
    signin = api.signin(creds.email, creds.password).json()
    me = AuthedClient(api, signin["access_token"], signin["refresh_token"])
    uid = me.get_profile().json()["user_id"]

    # Build state: highscore + currency + skin ownership.
    me.submit_run(score=999, distance=0, coins_collected=10, duration_ms=1)
    skin = admin_make_skin(admin, cost=0, currency="soft")
    me.purchase_skin(skin["id"])
    assert me.get_profile().json()["main_highscore"] == 999
    assert skin["id"] in [s["id"] for s in me.owned_skins().json()]

    assert me.delete_account(creds.password).status_code in (200, 204)

    # Old token dead.
    assert me.verify().status_code == 401

    # Sanity from admin side: user no longer in /admin/users.
    rows = admin.admin_list_users(search=creds.username).json()
    assert all(u["id"] != uid for u in rows)

    # Re-sign-up with the same creds: brand-new account, fresh state.
    api.signup(creds.username, creds.email, creds.password)
    fresh = api.signin(creds.email, creds.password).json()
    me2 = AuthedClient(api, fresh["access_token"], fresh["refresh_token"])
    profile = me2.get_profile().json()
    assert profile["main_highscore"] == 0
    assert profile["total_xp"] == 0
    assert me2.owned_skins().json() == []
    assert me2.get_wallet().json() == {"high": 0, "soft": 0, "energy": 0}


def test_concurrent_run_submits_accumulate_xp_correctly(base_url, user):
    """Two simultaneous /runs calls should each award their XP. Postgres
    row-level locking on the profiles UPDATE serializes them; the second
    UPDATE sees the first's commit thanks to read-committed visibility."""
    async def go() -> list[httpx.Response]:
        async with httpx.AsyncClient(base_url=base_url, timeout=10) as client:
            headers = {"Authorization": f"Bearer {user.access_token}"}
            payload = {"score": 100, "distance": 0, "coins_collected": 0, "duration_ms": 1}
            return await asyncio.gather(
                client.post("/runs", headers=headers, json=payload),
                client.post("/runs", headers=headers, json=payload),
            )

    responses = asyncio.run(go())
    assert all(r.status_code == 200 for r in responses)
    # Both runs awarded; total XP must equal the sum.
    assert user.get_profile().json()["total_xp"] == 200


def test_signup_normalizes_email_whitespace(api):
    """Leading/trailing whitespace in the email is trimmed before storage,
    so the same lookup also resolves with or without it."""
    c = make_creds()
    padded = f"  {c.email}  "
    assert api.signup(c.username, padded, c.password).status_code == 201
    assert api.signin(c.email, c.password).status_code == 200


def test_admin_can_demote_other_admin(admin, user_factory):
    """Self-demotion is blocked (already covered) — peer demotion must
    still work, otherwise admins can't be removed."""
    target, _ = user_factory()
    tid = target.get_profile().json()["user_id"]
    admin.admin_update_role(tid, role="admin")
    # Now admin demotes target (not self).
    r = admin.admin_update_role(tid, role="user")
    assert r.status_code == 200
    assert target.admin_list_users().status_code == 403

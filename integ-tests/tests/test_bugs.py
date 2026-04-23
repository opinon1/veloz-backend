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

from helpers.factory import (
    make_creds,
    rand_item_name,
    rand_skin_name,
    rand_url,
)


# ───────────────────────── [RACE] purchase skin ─────────────────────────


def test_skin_purchase_is_race_safe(base_url, admin, user):
    """Two concurrent purchase_skin requests on the same skin must result in
    exactly one successful purchase + one 409. Fix: INSERT ... ON CONFLICT
    DO NOTHING is now the first write; `rows_affected == 0` short-circuits
    with 409 BEFORE the wallet deduction, so the racer never charges."""
    skin = admin.admin_create_skin(
        name=rand_skin_name(), outfit_url=rand_url(), cost=100, currency="soft"
    ).json()
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
    free = admin.admin_create_skin(
        name=rand_skin_name(), outfit_url=rand_url(), cost=0, currency="soft"
    ).json()
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


def test_store_skin_item_rejects_invalid_uuid_at_create(admin):
    """Admin cannot create a `skin`-type store item whose payload.skin_id is
    not a valid UUID. Previously the purchase handler silently skipped
    fulfillment on bad UUIDs, charging the user for nothing."""
    r = admin.admin_create_store_item(
        name=rand_item_name("BadSkin"),
        item_type="skin",
        cost=50,
        currency="soft",
        payload={"skin_id": "not-a-uuid"},
    )
    assert r.status_code == 400


def test_store_skin_item_rejects_missing_skin_id(admin):
    """Missing skin_id in payload also blocked at create time."""
    r = admin.admin_create_store_item(
        name=rand_item_name("NoSkin"),
        item_type="skin",
        cost=50,
        currency="soft",
        payload={},
    )
    assert r.status_code == 400


def test_store_currency_bundle_rejects_unknown_keys(admin):
    """currency_bundle payloads must ONLY contain high/soft/energy.
    `coins` (a typo) used to silently grant nothing."""
    r = admin.admin_create_store_item(
        name=rand_item_name("TypoBundle"),
        item_type="currency_bundle",
        cost=10,
        currency="high",
        payload={"coins": 500},
    )
    assert r.status_code == 400


def test_store_currency_bundle_rejects_non_positive_amounts(admin):
    """Zero or negative grant amounts are nonsense and now rejected."""
    r = admin.admin_create_store_item(
        name=rand_item_name("ZeroBundle"),
        item_type="currency_bundle",
        cost=10,
        currency="high",
        payload={"soft": 0},
    )
    assert r.status_code == 400


def test_store_energy_refill_requires_amount(admin):
    """energy_refill must declare a positive energy amount."""
    r = admin.admin_create_store_item(
        name=rand_item_name("BadEnergy"),
        item_type="energy_refill",
        cost=5,
        currency="soft",
        payload={},
    )
    assert r.status_code == 400


def test_store_unknown_item_type_rejected(admin):
    """Item types outside the known set (skin/frame/currency_bundle/bp_unlock/
    energy_refill/custom) are rejected."""
    r = admin.admin_create_store_item(
        name=rand_item_name("Weird"),
        item_type="teleporter",
        cost=5,
        currency="soft",
        payload={},
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
        payload={"high": 100},
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
        payload={"soft": 100},
    ).json()
    # Swap to a bad payload for the same item_type.
    r = admin.admin_update_store_item(good["id"], payload={"coins": 1})
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

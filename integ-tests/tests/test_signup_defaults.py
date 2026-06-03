"""Signup defaults.

Admin flags catalog rows as `is_default` and:
  - newly-signed-up users automatically own / unlock them
  - the admin backfill endpoint applies the current default set to
    every existing user (idempotently)

Resources covered (each via its own `is_default` column):
    skins        → user_skins insert
    avatars      → user_avatars insert
    frames       → user_frames insert
    characters   → user_characters insert (unlocked=true) +
                   legacy default_unlocked column
    store_items  → payload Grants applied; tracked in
                   default_grants_applied so re-running doesn't
                   re-credit currency or re-grant skins.
"""
from __future__ import annotations

import os

import pytest

from helpers.api import AuthedClient
from helpers.compose import exec_sql
from helpers.factory import (
    admin_make_avatar,
    admin_make_character,
    admin_make_frame,
    admin_make_skin,
    make_creds,
    rand_item_name,
)


def _db_env() -> dict[str, str]:
    return dict(
        db_name=os.environ["DB_NAME"],
        db_user=os.environ["DB_USER"],
        pg_port=os.environ["POSTGRES_PORT"],
    )


def _wipe_defaults():
    """Reset every is_default flag + the per-user tracking table so
    leftover defaults from prior tests don't bleed into this file's
    expectations."""
    exec_sql("UPDATE skins SET is_default = FALSE", **_db_env())
    exec_sql("UPDATE avatars SET is_default = FALSE", **_db_env())
    exec_sql("UPDATE frames SET is_default = FALSE", **_db_env())
    exec_sql("UPDATE store_items SET is_default = FALSE", **_db_env())
    exec_sql("UPDATE characters SET default_unlocked = FALSE", **_db_env())
    exec_sql("DELETE FROM default_grants_applied", **_db_env())


@pytest.fixture(autouse=True)
def _cleanup_defaults():
    """Module-wide guard: clear flags before AND after each test so
    other test files (which assume zero-state signup) stay green."""
    _wipe_defaults()
    yield
    _wipe_defaults()


def _signup_fresh(api) -> tuple[AuthedClient, str]:
    creds = make_creds()
    r = api.signup(creds.username, creds.email, creds.password)
    assert r.status_code == 201, r.text
    user_id = r.json()["id"]
    body = api.signin(creds.email, creds.password).json()
    return AuthedClient(api, body["access_token"], body["refresh_token"]), user_id


# ────────────────────── Admin DTOs expose is_default ──────────────────────


@pytest.mark.admin
def test_admin_avatar_create_with_is_default(admin):
    a = admin.admin_create_avatar(
        name=f"Default-{os.urandom(3).hex()}",
        price=0,
        currency="soft",
        is_default=True,
    ).json()
    assert a["is_default"] is True
    listed = next(r for r in admin.admin_list_avatars().json() if r["id"] == a["id"])
    assert listed["is_default"] is True


@pytest.mark.admin
def test_admin_frame_create_with_is_default(admin):
    f = admin.admin_create_frame(
        name=f"Default-{os.urandom(3).hex()}",
        price=0,
        currency="soft",
        is_default=True,
    ).json()
    assert f["is_default"] is True


@pytest.mark.admin
def test_admin_store_create_with_is_default(admin):
    item = admin.admin_create_store_item(
        name=rand_item_name("Default"),
        item_type="custom",
        currency="soft",
        cost=0,
        payload=[{"type": "currency", "currency": "soft", "amount": 10}],
        is_default=True,
    ).json()
    assert item["is_default"] is True


@pytest.mark.admin
def test_admin_can_toggle_is_default_off(admin):
    a = admin.admin_create_avatar(
        name=f"Toggle-{os.urandom(3).hex()}",
        price=0,
        currency="soft",
        is_default=True,
    ).json()
    upd = admin.admin_update_avatar(a["id"], is_default=False).json()
    assert upd["is_default"] is False


# ────────────────────── Auto-apply on signup ──────────────────────


@pytest.mark.admin
def test_signup_grants_default_avatar(admin, api):
    a = admin.admin_create_avatar(
        name=f"Welcome-{os.urandom(3).hex()}",
        price=0,
        currency="soft",
        is_default=True,
    ).json()
    user, _ = _signup_fresh(api)
    owned = user.list_avatars().json()
    assert a["id"] in [row["id"] for row in owned]


@pytest.mark.admin
def test_signup_grants_default_frame(admin, api):
    f = admin.admin_create_frame(
        name=f"Welcome-{os.urandom(3).hex()}",
        price=0,
        currency="soft",
        is_default=True,
    ).json()
    user, _ = _signup_fresh(api)
    owned = user.list_frames().json()
    assert f["id"] in [row["id"] for row in owned]


@pytest.mark.admin
def test_signup_grants_default_skin(admin, api):
    char = admin_make_character(admin)
    s = admin_make_skin(admin, char["id"], cost=0, currency="soft", is_default=True)
    user, _ = _signup_fresh(api)
    owned = user.owned_skins().json()
    assert s["id"] in [row["id"] for row in owned]


@pytest.mark.admin
def test_signup_unlocks_default_character(admin, api):
    """Defaulting a character creates a concrete user_characters row +
    surfaces unlocked=true in /characters."""
    char = admin_make_character(admin, default_unlocked=True)
    user, _ = _signup_fresh(api)
    rows = user.list_characters().json()
    row = next(r for r in rows if r["id"] == char["id"])
    assert row["unlocked"] is True


@pytest.mark.admin
def test_signup_applies_default_store_item_payload(admin, api):
    """A default store item with `[{currency soft 500}]` payload credits
    soft on signup. The payment cost is NOT charged — defaults are an
    admin gift, not a fake purchase."""
    item = admin.admin_create_store_item(
        name=rand_item_name("Welcome"),
        item_type="custom",
        currency="soft",
        cost=999,
        payload=[
            {"type": "currency", "currency": "soft", "amount": 500},
            {"type": "currency", "currency": "high", "amount": 25},
        ],
        is_default=True,
    ).json()
    user, _ = _signup_fresh(api)
    w = user.get_wallet().json()
    assert w["soft"] == 500
    assert w["high"] == 25
    # And the item is marked applied for the user, idempotently.
    assert item["is_default"] is True


@pytest.mark.admin
def test_inactive_default_is_skipped(admin, api):
    """`is_default = true` + `is_active = false` → row is ignored on
    signup. Toggling an item off must not retroactively grant it."""
    a = admin.admin_create_avatar(
        name=f"Hidden-{os.urandom(3).hex()}",
        price=0,
        currency="soft",
        is_default=True,
    ).json()
    admin.admin_update_avatar(a["id"], is_active=False)
    user, _ = _signup_fresh(api)
    assert all(row["id"] != a["id"] for row in user.list_avatars().json())


# ────────────────────── Backfill endpoint ──────────────────────


@pytest.mark.admin
def test_backfill_requires_admin(user):
    assert user.admin_backfill_signup_defaults().status_code == 403


@pytest.mark.admin
def test_backfill_grants_default_to_existing_user(admin, user):
    """User created BEFORE defaults are flagged. Backfill grants the
    defaults retroactively."""
    a = admin.admin_create_avatar(
        name=f"Retro-{os.urandom(3).hex()}",
        price=0,
        currency="soft",
        is_default=True,
    ).json()
    # User created BEFORE we created the avatar — but admin_make_avatar
    # was called after the `user` fixture, so the user existed first.
    # Sanity-check the user doesn't own it yet.
    assert all(row["id"] != a["id"] for row in user.list_avatars().json())

    r = admin.admin_backfill_signup_defaults()
    assert r.status_code == 200
    body = r.json()
    assert body["users_processed"] >= 1
    assert body["totals"]["avatars"] >= 1

    assert a["id"] in [row["id"] for row in user.list_avatars().json()]


@pytest.mark.admin
def test_backfill_is_idempotent_on_owned_items(admin, user):
    """Running backfill twice for the same user doesn't 500 and doesn't
    duplicate ownership."""
    char = admin_make_character(admin)
    s = admin_make_skin(admin, char["id"], cost=0, currency="soft", is_default=True)

    admin.admin_backfill_signup_defaults()
    admin.admin_backfill_signup_defaults()
    second = admin.admin_backfill_signup_defaults()
    assert second.status_code == 200
    owned = [row["id"] for row in user.owned_skins().json()]
    # Skin owned exactly once.
    assert owned.count(s["id"]) == 1


@pytest.mark.admin
def test_backfill_does_not_double_credit_store_payload(admin, user):
    """Default store item credits currency once; subsequent backfills
    are no-ops thanks to default_grants_applied."""
    item = admin.admin_create_store_item(
        name=rand_item_name("ReBackfill"),
        item_type="custom",
        currency="soft",
        cost=0,
        payload=[{"type": "currency", "currency": "soft", "amount": 200}],
        is_default=True,
    ).json()

    admin.admin_backfill_signup_defaults()
    after_first = user.get_wallet().json()["soft"]
    assert after_first >= 200
    admin.admin_backfill_signup_defaults()
    admin.admin_backfill_signup_defaults()
    after_more = user.get_wallet().json()["soft"]
    assert after_more == after_first
    assert item["is_default"] is True


@pytest.mark.admin
def test_backfill_reports_totals(admin, user):
    """Totals reflect what got newly granted on this run."""
    admin_make_avatar(admin, price=0, currency="soft", is_default=True)
    admin_make_frame(admin, price=0, currency="soft", is_default=True)

    r = admin.admin_backfill_signup_defaults().json()
    assert r["totals"]["avatars"] >= 1
    assert r["totals"]["frames"] >= 1


# ────────────────────── No-op when no defaults ──────────────────────


@pytest.mark.admin
def test_signup_with_no_defaults_yields_empty_state(admin, api):
    """Spec / safety: if the admin hasn't flagged anything, a new user
    starts with zero inventory and zero balances (same as before this
    feature)."""
    user, _ = _signup_fresh(api)
    w = user.get_wallet().json()
    assert (w["high"], w["soft"], w["energy"]) == (0, 0, 0)
    assert user.list_avatars().json() == []
    assert user.list_frames().json() == []
    assert user.owned_skins().json() == []


@pytest.mark.admin
def test_backfill_with_no_defaults_is_noop(admin):
    r = admin.admin_backfill_signup_defaults().json()
    for k in ("skins", "avatars", "frames", "characters", "store_items"):
        assert r["totals"][k] == 0

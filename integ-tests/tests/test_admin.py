"""Admin gating + user-management flows.

Admin CRUD for skins/store/battlepass is covered in their respective test files.
This file focuses on: role gating, user listing/search, role promotion, currency
grants, profile overrides.
"""
from __future__ import annotations

import pytest


# ───────────────────── Role gating ─────────────────────


def test_admin_endpoint_without_auth(api):
    """No Bearer → 401 (not 403, because we can't check role yet)."""
    assert api.raw_get("/admin/users").status_code == 401


def test_admin_endpoint_as_regular_user(user):
    """Regular user with valid token, no admin role → 403."""
    assert user.admin_list_users().status_code == 403
    assert user.admin_grant("00000000-0000-0000-0000-000000000000", "soft", 1).status_code == 403


# ───────────────────── User list ─────────────────────


@pytest.mark.admin
def test_admin_list_users_returns_rows(admin, user):
    """Admin sees at least themselves and the other test user."""
    r = admin.admin_list_users()
    assert r.status_code == 200
    rows = r.json()
    assert isinstance(rows, list)
    assert len(rows) >= 2

    user_id = user.get_profile().json()["user_id"]
    assert user_id in [u["id"] for u in rows]

    # Every row exposes role + wallet joined in.
    for u in rows:
        assert "role" in u
        assert "high" in u and "soft" in u and "energy" in u


@pytest.mark.admin
def test_admin_list_users_search(admin, user_factory):
    """?search= filters by username or email (ILIKE)."""
    new_user, creds = user_factory()
    r = admin.admin_list_users(search=creds.username)
    assert r.status_code == 200
    rows = r.json()
    assert len(rows) == 1
    assert rows[0]["username"] == creds.username


# ───────────────────── Role promotion ─────────────────────


@pytest.mark.admin
def test_admin_promote_user_to_admin(admin, user_factory):
    """After PATCH /admin/users/:id/role=admin, that user can call admin endpoints."""
    target, _ = user_factory()
    target_id = target.get_profile().json()["user_id"]

    # Before: not admin.
    assert target.admin_list_users().status_code == 403

    assert admin.admin_update_role(target_id, role="admin").status_code == 200

    # After: can hit admin endpoints.
    assert target.admin_list_users().status_code == 200


@pytest.mark.admin
def test_admin_demote_admin_to_user(admin, user_factory):
    """Role PATCH 'user' revokes admin access."""
    target, _ = user_factory()
    tid = target.get_profile().json()["user_id"]
    admin.admin_update_role(tid, role="admin")
    assert target.admin_list_users().status_code == 200

    admin.admin_update_role(tid, role="user")
    assert target.admin_list_users().status_code == 403


@pytest.mark.admin
def test_admin_update_role_invalid_value(admin, user):
    """Only 'user' and 'admin' allowed → anything else 400."""
    uid = user.get_profile().json()["user_id"]
    assert admin.admin_update_role(uid, role="superadmin").status_code == 400


@pytest.mark.admin
def test_admin_update_role_unknown_user(admin):
    """Nonexistent user → 404."""
    r = admin.admin_update_role("00000000-0000-0000-0000-000000000000", role="admin")
    assert r.status_code == 404


# ───────────────────── Profile override ─────────────────────


@pytest.mark.admin
def test_admin_can_override_profile_fields(admin, user):
    """Admin PATCH sets price_multiplier + main_highscore on a target user."""
    uid = user.get_profile().json()["user_id"]
    r = admin.admin_update_user_profile(uid, price_multiplier=0.25, main_highscore=50_000)
    assert r.status_code == 200
    body = r.json()
    assert body["price_multiplier"] == 0.25
    assert body["main_highscore"] == 50_000

    # User sees the override from their own /profile.
    mine = user.get_profile().json()
    assert mine["price_multiplier"] == 0.25
    assert mine["main_highscore"] == 50_000


@pytest.mark.admin
def test_admin_profile_override_unknown_user(admin):
    """UUID not in profiles → 404."""
    r = admin.admin_update_user_profile(
        "00000000-0000-0000-0000-000000000000", price_multiplier=1.5
    )
    assert r.status_code == 404

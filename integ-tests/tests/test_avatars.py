"""Avatar resource: admin CRUD + user catalog/owned/purchase/select.

Per spec, GET /avatars returns ONLY unlocked avatars, each as
`{ id, is_selected }`. The selection is persisted on profiles.avatar_url
which is the UUID rendered back by /runs/leaderboard.
"""
from __future__ import annotations

import pytest

from helpers.factory import (
    admin_make_avatar,
    rand_avatar_name,
)


# ───────────────────── Admin CRUD ─────────────────────


@pytest.mark.admin
def test_admin_avatar_crud_roundtrip(admin):
    name = rand_avatar_name()
    created = admin.admin_create_avatar(
        name=name, price=100, currency="soft"
    ).json()
    aid = created["id"]
    assert created["name"] == name
    assert created["price"] == 100
    assert created["currency"] == "soft"
    assert created["is_active"] is True

    upd = admin.admin_update_avatar(aid, price=250, is_active=False).json()
    assert upd["price"] == 250
    assert upd["is_active"] is False

    assert admin.admin_delete_avatar(aid).status_code == 204
    assert admin.admin_update_avatar(aid, price=1).status_code == 404


@pytest.mark.admin
def test_admin_avatar_name_must_be_unique(admin):
    name = rand_avatar_name()
    assert admin.admin_create_avatar(name=name).status_code == 201
    assert admin.admin_create_avatar(name=name).status_code == 409


@pytest.mark.admin
def test_admin_avatar_validates_inputs(admin):
    assert admin.admin_create_avatar(name="").status_code == 400
    assert admin.admin_create_avatar(name=rand_avatar_name(), price=-1).status_code == 400
    assert admin.admin_create_avatar(name=rand_avatar_name(), currency="btc").status_code == 400


def test_non_admin_cannot_manage_avatars(user):
    assert user.admin_create_avatar(name=rand_avatar_name()).status_code == 403
    assert user.admin_list_avatars().status_code == 403


# ───────────────────── Catalog (public) ─────────────────────


def test_catalog_lists_active_avatars(api, admin):
    a = admin_make_avatar(admin, price=10, currency="soft")
    rows = api.raw_get("/avatars/catalog").json()
    ids = [r["id"] for r in rows]
    assert a["id"] in ids
    row = next(r for r in rows if r["id"] == a["id"])
    assert row["price"] == 10
    assert row["currency"] == "soft"


def test_catalog_hides_inactive_avatars(api, admin):
    a = admin_make_avatar(admin)
    admin.admin_update_avatar(a["id"], is_active=False)
    rows = api.raw_get("/avatars/catalog").json()
    assert a["id"] not in [r["id"] for r in rows]


# ───────────────────── Owned listing ─────────────────────


def test_avatars_list_requires_auth(api):
    assert api.raw_get("/avatars").status_code == 401


def test_avatars_owned_returns_unlocked_only(admin, user):
    """Catalog is full list; /avatars is unlocked only."""
    a = admin_make_avatar(admin)
    # Before purchase, /avatars is empty.
    assert user.list_avatars().json() == []
    user.purchase_avatar(a["id"])
    rows = user.list_avatars().json()
    assert [r["id"] for r in rows] == [a["id"]]
    assert rows[0]["is_selected"] is False


def test_select_marks_is_selected(admin, user):
    a = admin_make_avatar(admin)
    user.purchase_avatar(a["id"])
    user.select_avatar(a["id"])

    rows = user.list_avatars().json()
    assert rows[0]["is_selected"] is True


def test_select_reflects_in_profile_and_leaderboard(admin, user):
    a = admin_make_avatar(admin)
    user.purchase_avatar(a["id"])
    user.select_avatar(a["id"])

    profile = user.get_profile().json()
    assert profile["avatar_url"] == a["id"]

    user.submit_run(score=42, distance=0, coins_collected=0, duration_ms=1)
    rows = user.leaderboard().json()
    me = next(r for r in rows if r["user_id"] == profile["user_id"])
    assert me["avatar_url"] == a["id"]


def test_select_only_one_avatar_at_a_time(admin, user):
    a = admin_make_avatar(admin)
    b = admin_make_avatar(admin)
    user.purchase_avatar(a["id"])
    user.purchase_avatar(b["id"])

    user.select_avatar(a["id"])
    user.select_avatar(b["id"])

    rows = {r["id"]: r for r in user.list_avatars().json()}
    assert rows[a["id"]]["is_selected"] is False
    assert rows[b["id"]]["is_selected"] is True


def test_deselect_clears_selection(admin, user):
    a = admin_make_avatar(admin)
    user.purchase_avatar(a["id"])
    user.select_avatar(a["id"])

    r = user.deselect_avatar()
    assert r.status_code == 200
    assert r.json()["avatar_id"] is None
    assert user.get_profile().json()["avatar_url"] is None


# ───────────────────── Purchase ─────────────────────


def test_purchase_with_insufficient_funds(user, admin):
    a = admin_make_avatar(admin, price=999, currency="soft")
    r = user.purchase_avatar(a["id"])
    assert r.status_code == 422


def test_purchase_deducts_currency(user, admin):
    a = admin_make_avatar(admin, price=50, currency="soft")
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 200)
    r = user.purchase_avatar(a["id"])
    assert r.status_code == 200
    body = r.json()
    assert body["avatar_id"] == a["id"]
    assert body["cost_paid"] == 50
    assert body["new_balance"] == 150


def test_purchase_free_avatar_returns_real_balance(user, admin):
    a = admin_make_avatar(admin, price=0, currency="soft")
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 75)
    r = user.purchase_avatar(a["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == 0
    assert r.json()["new_balance"] == 75


def test_double_purchase_rejected(user, admin):
    a = admin_make_avatar(admin)
    assert user.purchase_avatar(a["id"]).status_code == 200
    assert user.purchase_avatar(a["id"]).status_code == 409


def test_purchase_inactive_avatar_returns_410(admin, user):
    a = admin_make_avatar(admin)
    admin.admin_update_avatar(a["id"], is_active=False)
    assert user.purchase_avatar(a["id"]).status_code == 410


def test_purchase_nonexistent_returns_404(user):
    r = user.purchase_avatar("00000000-0000-0000-0000-000000000000")
    assert r.status_code == 404


# ───────────────────── Select authz ─────────────────────


def test_select_requires_ownership(admin, user_factory):
    a = admin_make_avatar(admin)
    x, _ = user_factory()
    y, _ = user_factory()
    x.purchase_avatar(a["id"])

    assert y.select_avatar(a["id"]).status_code == 403


def test_admin_delete_avatar_clears_selection(admin, user):
    """profiles.avatar_url FK ON DELETE SET NULL: removing the catalog row
    nulls out anyone who had it selected."""
    a = admin_make_avatar(admin)
    user.purchase_avatar(a["id"])
    user.select_avatar(a["id"])
    assert user.get_profile().json()["avatar_url"] == a["id"]

    admin.admin_delete_avatar(a["id"])
    assert user.get_profile().json()["avatar_url"] is None

"""Frame resource: admin CRUD + user catalog/owned/purchase/select.

Mirrors test_avatars exactly: same shape, different table. profiles.frame_url
holds the selected frame UUID and is what /runs/leaderboard returns.
"""
from __future__ import annotations

import pytest

from helpers.factory import (
    admin_make_frame,
    quote_price,
    rand_frame_name,
)


# ───────────────────── Admin CRUD ─────────────────────


@pytest.mark.admin
def test_admin_frame_crud_roundtrip(admin):
    name = rand_frame_name()
    created = admin.admin_create_frame(
        name=name, price=100, currency="soft"
    ).json()
    fid = created["id"]
    assert created["name"] == name
    assert created["price"] == 100

    upd = admin.admin_update_frame(fid, price=250, is_active=False).json()
    assert upd["price"] == 250
    assert upd["is_active"] is False

    assert admin.admin_delete_frame(fid).status_code == 204
    assert admin.admin_update_frame(fid, price=1).status_code == 404


@pytest.mark.admin
def test_admin_frame_name_must_be_unique(admin):
    name = rand_frame_name()
    assert admin.admin_create_frame(name=name).status_code == 201
    assert admin.admin_create_frame(name=name).status_code == 409


@pytest.mark.admin
def test_admin_frame_validates_inputs(admin):
    assert admin.admin_create_frame(name="").status_code == 400
    assert admin.admin_create_frame(name=rand_frame_name(), price=-1).status_code == 400
    assert admin.admin_create_frame(name=rand_frame_name(), currency="btc").status_code == 400


def test_non_admin_cannot_manage_frames(user):
    assert user.admin_create_frame(name=rand_frame_name()).status_code == 403


# ───────────────────── Catalog (public) ─────────────────────


def test_frames_catalog_lists_active(api, admin):
    f = admin_make_frame(admin, price=10, currency="soft")
    rows = api.raw_get("/frames/catalog").json()
    assert f["id"] in [r["id"] for r in rows]


def test_frames_catalog_hides_inactive(api, admin):
    f = admin_make_frame(admin)
    admin.admin_update_frame(f["id"], is_active=False)
    rows = api.raw_get("/frames/catalog").json()
    assert f["id"] not in [r["id"] for r in rows]


# ───────────────────── Owned listing ─────────────────────


def test_frames_list_requires_auth(api):
    assert api.raw_get("/frames").status_code == 401


def test_frames_owned_returns_unlocked_only(admin, user):
    f = admin_make_frame(admin)
    assert user.list_frames().json() == []
    user.purchase_frame(f["id"])
    rows = user.list_frames().json()
    assert [r["id"] for r in rows] == [f["id"]]
    assert rows[0]["is_selected"] is False


def test_select_marks_is_selected(admin, user):
    f = admin_make_frame(admin)
    user.purchase_frame(f["id"])
    user.select_frame(f["id"])
    rows = user.list_frames().json()
    assert rows[0]["is_selected"] is True


def test_select_reflects_in_profile_and_leaderboard(admin, user):
    f = admin_make_frame(admin)
    user.purchase_frame(f["id"])
    user.select_frame(f["id"])

    profile = user.get_profile().json()
    assert profile["frame_url"] == f["id"]

    user.submit_run(score=42, distance=0, coins_collected=0, duration_ms=1)
    rows = user.leaderboard(limit=500).json()
    me = next(r for r in rows if r["user_id"] == profile["user_id"])
    assert me["frame_url"] == f["id"]


def test_select_only_one_frame_at_a_time(admin, user):
    a = admin_make_frame(admin)
    b = admin_make_frame(admin)
    user.purchase_frame(a["id"])
    user.purchase_frame(b["id"])

    user.select_frame(a["id"])
    user.select_frame(b["id"])

    rows = {r["id"]: r for r in user.list_frames().json()}
    assert rows[a["id"]]["is_selected"] is False
    assert rows[b["id"]]["is_selected"] is True


def test_deselect_clears_selection(admin, user):
    f = admin_make_frame(admin)
    user.purchase_frame(f["id"])
    user.select_frame(f["id"])

    r = user.deselect_frame()
    assert r.status_code == 200
    assert r.json()["frame_id"] is None
    assert user.get_profile().json()["frame_url"] is None


# ───────────────────── Purchase ─────────────────────


def test_purchase_insufficient_funds(user, admin):
    f = admin_make_frame(admin, price=999, currency="soft")
    assert user.purchase_frame(f["id"]).status_code == 422


def test_purchase_deducts_currency(user, admin):
    f = admin_make_frame(admin, price=50, currency="soft")
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 200)
    expected = quote_price(user, "frame", f["id"])
    body = user.purchase_frame(f["id"]).json()
    assert body["frame_id"] == f["id"]
    assert body["cost_paid"] == expected
    assert body["new_balance"] == 200 - expected


def test_double_purchase_rejected(user, admin):
    f = admin_make_frame(admin)
    assert user.purchase_frame(f["id"]).status_code == 200
    assert user.purchase_frame(f["id"]).status_code == 409


def test_purchase_inactive_frame_returns_410(admin, user):
    f = admin_make_frame(admin)
    admin.admin_update_frame(f["id"], is_active=False)
    assert user.purchase_frame(f["id"]).status_code == 410


def test_purchase_nonexistent_returns_404(user):
    r = user.purchase_frame("00000000-0000-0000-0000-000000000000")
    assert r.status_code == 404


# ───────────────────── Select authz ─────────────────────


def test_select_requires_ownership(admin, user_factory):
    f = admin_make_frame(admin)
    x, _ = user_factory()
    y, _ = user_factory()
    x.purchase_frame(f["id"])

    assert y.select_frame(f["id"]).status_code == 403


def test_admin_delete_frame_clears_selection(admin, user):
    f = admin_make_frame(admin)
    user.purchase_frame(f["id"])
    user.select_frame(f["id"])
    assert user.get_profile().json()["frame_url"] == f["id"]

    admin.admin_delete_frame(f["id"])
    assert user.get_profile().json()["frame_url"] is None

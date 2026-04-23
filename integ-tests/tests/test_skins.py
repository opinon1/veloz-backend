"""Skin flows: admin CRUD catalog, user purchase/equip, ownership, edge cases."""
from __future__ import annotations

import pytest

from helpers.factory import rand_skin_name, rand_url


@pytest.fixture
def paid_skin(admin):
    """A freshly-created skin that costs 100 soft."""
    r = admin.admin_create_skin(
        name=rand_skin_name(),
        description="Paid test skin",
        outfit_url=rand_url(),
        cost=100,
        currency="soft",
    )
    assert r.status_code == 201
    return r.json()


@pytest.fixture
def free_skin(admin):
    """A free (cost=0) skin — useful for testing the no-ledger code path."""
    r = admin.admin_create_skin(
        name=rand_skin_name(),
        outfit_url=rand_url(),
        cost=0,
        currency="soft",
        is_default=True,
    )
    assert r.status_code == 201
    return r.json()


# ───────────────────── Catalog listing ─────────────────────


def test_list_skins_is_public(api, paid_skin):
    """The catalog is publicly listable (no auth)."""
    r = api.raw_get("/skins")
    assert r.status_code == 200
    names = [s["name"] for s in r.json()]
    assert paid_skin["name"] in names


def test_inactive_skins_hidden_from_public_list(admin, api, paid_skin):
    """Skins toggled is_active=false must NOT appear in public /skins."""
    admin.admin_update_skin(paid_skin["id"], is_active=False)
    public = api.raw_get("/skins").json()
    ids = [s["id"] for s in public]
    assert paid_skin["id"] not in ids

    # But still listed in the admin view.
    admin_ids = [s["id"] for s in admin.admin_list_skins().json()]
    assert paid_skin["id"] in admin_ids


# ───────────────────── Purchase ─────────────────────


def test_purchase_with_insufficient_funds(user, paid_skin):
    """User has 0 soft; purchase costs 100 → 422 (CHECK constraint)."""
    r = user.purchase_skin(paid_skin["id"])
    assert r.status_code == 422


def test_purchase_deducts_and_records_ownership(user, admin, paid_skin):
    """Grant soft, purchase, verify wallet decreased + user_skins row exists."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "soft", 500)

    r = user.purchase_skin(paid_skin["id"])
    assert r.status_code == 200
    body = r.json()
    assert body["skin_id"] == paid_skin["id"]
    assert body["cost_paid"] == 100
    assert body["currency"] == "soft"
    assert body["new_balance"] == 400

    owned = user.owned_skins().json()
    assert paid_skin["id"] in [s["id"] for s in owned]


def test_double_purchase_rejected(user, admin, paid_skin):
    """Purchasing an already-owned skin → 409 Conflict (no double-charge)."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "soft", 500)
    assert user.purchase_skin(paid_skin["id"]).status_code == 200
    assert user.purchase_skin(paid_skin["id"]).status_code == 409
    # Balance only charged once.
    assert user.get_wallet().json()["soft"] == 400


def test_purchase_free_skin(user, free_skin):
    """Free skin: no wallet mutation, still adds to owned list."""
    r = user.purchase_skin(free_skin["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == 0
    assert free_skin["id"] in [s["id"] for s in user.owned_skins().json()]
    assert user.get_wallet().json()["soft"] == 0


def test_purchase_nonexistent_skin(user):
    """UUID that does not exist → 404."""
    r = user.purchase_skin("00000000-0000-0000-0000-000000000000")
    assert r.status_code == 404


def test_purchase_inactive_skin(admin, user, paid_skin):
    """Disabling a skin blocks new purchases → 410 Gone."""
    admin.admin_update_skin(paid_skin["id"], is_active=False)
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 500)
    r = user.purchase_skin(paid_skin["id"])
    assert r.status_code == 410


# ───────────────────── Equip ─────────────────────


def test_equip_requires_ownership(user, paid_skin):
    """Equipping a skin you don't own → 403."""
    r = user.equip_skin(paid_skin["id"])
    assert r.status_code == 403


def test_equip_sets_profile_avatar(user, admin, paid_skin):
    """After equip, profile.avatar_url = skin_id."""
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 500)
    user.purchase_skin(paid_skin["id"])
    r = user.equip_skin(paid_skin["id"])
    assert r.status_code == 200
    assert r.json()["avatar_url"] == paid_skin["id"]
    assert user.get_profile().json()["avatar_url"] == paid_skin["id"]


# ───────────────────── Admin ─────────────────────


@pytest.mark.admin
def test_admin_skin_crud_roundtrip(admin):
    """Create → update → delete; verify at each step."""
    created = admin.admin_create_skin(
        name=rand_skin_name(),
        outfit_url=rand_url(),
        cost=50,
        currency="high",
    ).json()
    sid = created["id"]

    upd = admin.admin_update_skin(sid, cost=75, description="updated").json()
    assert upd["cost"] == 75
    assert upd["description"] == "updated"

    assert admin.admin_delete_skin(sid).status_code == 204
    assert admin.admin_update_skin(sid, cost=1).status_code == 404


@pytest.mark.admin
def test_admin_skin_name_must_be_unique(admin):
    """Duplicate skin name → 409."""
    name = rand_skin_name()
    assert admin.admin_create_skin(name=name, outfit_url=rand_url()).status_code == 201
    assert admin.admin_create_skin(name=name, outfit_url=rand_url()).status_code == 409


def test_non_admin_cannot_create_skin(user):
    """Regular user hitting admin endpoint → 403."""
    r = user.admin_create_skin(name=rand_skin_name(), outfit_url=rand_url())
    assert r.status_code == 403

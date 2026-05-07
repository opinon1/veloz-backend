"""Skin flows: admin CRUD catalog, user purchase/equip, ownership, edge cases."""
from __future__ import annotations

import pytest

from helpers.factory import admin_make_character, admin_make_skin


@pytest.fixture
def character(admin):
    return admin_make_character(admin)


@pytest.fixture
def paid_skin(admin, character):
    """A freshly-created skin that costs 100 soft."""
    return admin_make_skin(admin, character["id"], cost=100, currency="soft")


@pytest.fixture
def free_skin(admin, character):
    """A free (cost=0) skin — useful for testing the no-ledger code path."""
    return admin_make_skin(
        admin, character["id"], cost=0, currency="soft", is_default=True
    )


# ───────────────────── Catalog listing ─────────────────────


def test_list_skins_is_public(api, paid_skin):
    """The catalog is publicly listable (no auth)."""
    r = api.raw_get("/skins")
    assert r.status_code == 200
    ids = [s["id"] for s in r.json()]
    assert paid_skin["id"] in ids


def test_skins_list_exposes_character_id(api, paid_skin, character):
    """Public listing must include character_id (no name/description/url)."""
    rows = api.raw_get("/skins").json()
    row = next(s for s in rows if s["id"] == paid_skin["id"])
    assert row["character_id"] == character["id"]
    assert "name" not in row
    assert "description" not in row
    assert "outfit_url" not in row


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


def test_equip_sets_character_equipped_skin(user, admin, paid_skin, character):
    """After equip, the character's equipped_skin matches the skin id."""
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 500)
    user.purchase_skin(paid_skin["id"])
    r = user.equip_skin(paid_skin["id"])
    assert r.status_code == 200
    body = r.json()
    assert body["character_id"] == character["id"]
    assert body["equipped_skin_id"] == paid_skin["id"]

    chars = user.list_characters().json()
    char = next(c for c in chars if c["id"] == character["id"])
    assert char["equipped_skin"] == paid_skin["id"]
    assert char["unlocked"] is True


# ───────────────────── Admin ─────────────────────


@pytest.mark.admin
def test_admin_skin_crud_roundtrip(admin, character):
    """Create → update → delete; verify at each step."""
    created = admin.admin_create_skin(
        character_id=character["id"],
        cost=50,
        currency="high",
    ).json()
    sid = created["id"]
    assert created["character_id"] == character["id"]

    upd = admin.admin_update_skin(sid, cost=75).json()
    assert upd["cost"] == 75

    assert admin.admin_delete_skin(sid).status_code == 204
    assert admin.admin_update_skin(sid, cost=1).status_code == 404


@pytest.mark.admin
def test_admin_create_skin_requires_character_id(admin):
    """character_id is mandatory."""
    r = admin.admin_create_skin(cost=0, currency="soft")
    assert r.status_code in (400, 422)


@pytest.mark.admin
def test_admin_create_skin_unknown_character_id_rejected(admin):
    """character_id must reference an existing character."""
    r = admin.admin_create_skin(
        character_id="00000000-0000-0000-0000-000000000000",
        cost=0,
        currency="soft",
    )
    assert r.status_code == 400


def test_non_admin_cannot_create_skin(user, character):
    """Regular user hitting admin endpoint → 403."""
    r = user.admin_create_skin(character_id=character["id"], cost=0, currency="soft")
    assert r.status_code == 403

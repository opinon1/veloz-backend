"""Character endpoints: admin CRUD + user-facing /characters listing.

Each character row exposed to the user must include:
    id              — UUID
    unlocked        — bool, per-user
    equipped_skin   — UUID or null, per-user
    related_skins   — list of skin UUIDs that belong to this character
"""
from __future__ import annotations

import pytest

from helpers.factory import (
    admin_make_character,
    admin_make_skin,
    rand_character_name,
)


# ───────────────────── Admin CRUD ─────────────────────


@pytest.mark.admin
def test_admin_character_crud_roundtrip(admin):
    name = rand_character_name()
    created = admin.admin_create_character(name=name).json()
    cid = created["id"]
    assert created["name"] == name
    assert created["is_active"] is True
    assert created["default_unlocked"] is False

    upd = admin.admin_update_character(cid, default_unlocked=True, is_active=False).json()
    assert upd["default_unlocked"] is True
    assert upd["is_active"] is False

    assert admin.admin_delete_character(cid).status_code == 204
    assert admin.admin_update_character(cid, name="x").status_code == 404


@pytest.mark.admin
def test_admin_character_name_must_be_unique(admin):
    name = rand_character_name()
    assert admin.admin_create_character(name=name).status_code == 201
    assert admin.admin_create_character(name=name).status_code == 409


@pytest.mark.admin
def test_admin_character_name_must_be_non_empty(admin):
    assert admin.admin_create_character(name="   ").status_code == 400
    assert admin.admin_create_character(name="").status_code == 400


def test_non_admin_cannot_manage_characters(user):
    assert user.admin_create_character(name=rand_character_name()).status_code == 403
    assert user.admin_list_characters().status_code == 403


@pytest.mark.admin
def test_character_metadata_roundtrip(admin, user):
    """Admin attaches frontend-only metadata; it round-trips through the
    user-facing GET /characters."""
    meta = {"sort_order": 3, "lore": "ancient warrior", "vfx": ["glow", "trail"]}
    char = admin.admin_create_character(name=rand_character_name(), metadata=meta).json()
    assert char["metadata"] == meta

    # User-facing list exposes the same blob.
    rows = user.list_characters().json()
    row = next(c for c in rows if c["id"] == char["id"])
    assert row["metadata"] == meta

    # Update replaces metadata wholesale (no merge).
    new_meta = {"sort_order": 9}
    upd = admin.admin_update_character(char["id"], metadata=new_meta).json()
    assert upd["metadata"] == new_meta
    rows = user.list_characters().json()
    row = next(c for c in rows if c["id"] == char["id"])
    assert row["metadata"] == new_meta


@pytest.mark.admin
def test_character_metadata_defaults_to_empty_object(admin, user):
    char = admin.admin_create_character(name=rand_character_name()).json()
    assert char["metadata"] == {}
    rows = user.list_characters().json()
    row = next(c for c in rows if c["id"] == char["id"])
    assert row["metadata"] == {}


# ───────────────────── User listing ─────────────────────


def test_characters_list_requires_auth(api):
    assert api.raw_get("/characters").status_code == 401


def test_characters_list_returns_only_active(admin, user):
    """Inactive characters are hidden from /characters."""
    visible = admin_make_character(admin)
    hidden = admin_make_character(admin)
    admin.admin_update_character(hidden["id"], is_active=False)

    rows = user.list_characters().json()
    ids = [c["id"] for c in rows]
    assert visible["id"] in ids
    assert hidden["id"] not in ids


def test_characters_default_unlocked_true_for_new_user(admin, user):
    """A character created with default_unlocked=true is unlocked for any user
    who hasn't yet interacted with it."""
    char = admin_make_character(admin, default_unlocked=True)
    rows = user.list_characters().json()
    row = next(c for c in rows if c["id"] == char["id"])
    assert row["unlocked"] is True
    assert row["equipped_skin"] is None
    assert row["related_skins"] == []


def test_characters_default_locked_until_equip(admin, user):
    """default_unlocked=false → unlocked=false until the user equips one of
    its skins (which sets unlocked=true on user_characters)."""
    char = admin_make_character(admin, default_unlocked=False)

    rows = user.list_characters().json()
    row = next(c for c in rows if c["id"] == char["id"])
    assert row["unlocked"] is False
    assert row["equipped_skin"] is None


def test_characters_related_skins_lists_active_skins_for_character(admin, user):
    char = admin_make_character(admin)
    s1 = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    s2 = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    # An inactive skin is excluded from related_skins.
    s3 = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    admin.admin_update_skin(s3["id"], is_active=False)
    # A skin on a different character should NOT leak into this character's
    # related_skins.
    other = admin_make_character(admin)
    admin_make_skin(admin, other["id"], cost=0, currency="soft")

    rows = user.list_characters().json()
    row = next(c for c in rows if c["id"] == char["id"])
    related = set(row["related_skins"])
    assert {s1["id"], s2["id"]} <= related
    assert s3["id"] not in related


def test_equip_skin_unlocks_character_and_sets_equipped(admin, user):
    """Equipping a skin (a) unlocks the character it belongs to and (b) makes
    that skin the character's equipped skin."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    user.purchase_skin(skin["id"])

    # Pre-equip: locked, no equipped skin.
    rows = user.list_characters().json()
    pre = next(c for c in rows if c["id"] == char["id"])
    assert pre["unlocked"] is False
    assert pre["equipped_skin"] is None

    user.equip_skin(skin["id"])

    rows = user.list_characters().json()
    post = next(c for c in rows if c["id"] == char["id"])
    assert post["unlocked"] is True
    assert post["equipped_skin"] == skin["id"]


def test_equip_swaps_equipped_skin_within_same_character(admin, user):
    """User equips skin A, then skin B (same character). equipped_skin
    follows the latest equip, never accumulates."""
    char = admin_make_character(admin)
    a = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    b = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    user.purchase_skin(a["id"])
    user.purchase_skin(b["id"])

    user.equip_skin(a["id"])
    user.equip_skin(b["id"])

    rows = user.list_characters().json()
    row = next(c for c in rows if c["id"] == char["id"])
    assert row["equipped_skin"] == b["id"]


def test_equip_per_character_is_independent(admin, user):
    """User equipping a skin on character A must not affect character B's
    equipped_skin."""
    char_a = admin_make_character(admin)
    char_b = admin_make_character(admin)
    skin_a = admin_make_skin(admin, char_a["id"], cost=0, currency="soft")
    skin_b = admin_make_skin(admin, char_b["id"], cost=0, currency="soft")
    user.purchase_skin(skin_a["id"])
    user.purchase_skin(skin_b["id"])

    user.equip_skin(skin_a["id"])
    user.equip_skin(skin_b["id"])

    rows = {c["id"]: c for c in user.list_characters().json()}
    assert rows[char_a["id"]]["equipped_skin"] == skin_a["id"]
    assert rows[char_b["id"]]["equipped_skin"] == skin_b["id"]


def test_characters_list_isolates_per_user(admin, user_factory):
    """User A's character state must not leak into user B's view."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=0, currency="soft")

    a, _ = user_factory()
    b, _ = user_factory()
    a.purchase_skin(skin["id"])
    a.equip_skin(skin["id"])

    a_row = next(c for c in a.list_characters().json() if c["id"] == char["id"])
    b_row = next(c for c in b.list_characters().json() if c["id"] == char["id"])
    assert a_row["unlocked"] is True
    assert a_row["equipped_skin"] == skin["id"]
    assert b_row["unlocked"] is False
    assert b_row["equipped_skin"] is None


def test_admin_delete_character_clears_skins_via_cascade(admin, user):
    """ON DELETE CASCADE on skins.character_id wipes the character's skins
    too. Owned-skin rows are also cleaned up via FK cascade chain."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    user.purchase_skin(skin["id"])
    assert skin["id"] in [s["id"] for s in user.owned_skins().json()]

    admin.admin_delete_character(char["id"])

    # Skin gone from public listing.
    public = next(
        (s for s in user.list_skins().json() if s["id"] == skin["id"]),
        None,
    )
    assert public is None
    # Owned list no longer includes it (cascade).
    assert skin["id"] not in [s["id"] for s in user.owned_skins().json()]
    # Character disappears from /characters.
    assert all(c["id"] != char["id"] for c in user.list_characters().json())

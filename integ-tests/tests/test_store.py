"""Store flows: listing, purchases, multi-grant fulfillment, price_multiplier, IAP gating."""
from __future__ import annotations

import pytest

from helpers.factory import rand_item_name, rand_skin_name, rand_url


@pytest.fixture
def soft_bundle(admin):
    """100 high buys a payload of 500 soft. Tests Currency-grant fulfillment."""
    r = admin.admin_create_store_item(
        name=rand_item_name("Bundle"),
        description="500 soft coins",
        item_type="currency_bundle",
        cost=100,
        currency="high",
        payload=[{"type": "currency", "currency": "soft", "amount": 500}],
    )
    assert r.status_code == 201
    return r.json()


@pytest.fixture
def skin_item(admin):
    """Store item that grants ownership of a specific skin when purchased."""
    skin = admin.admin_create_skin(
        name=rand_skin_name(), outfit_url=rand_url(), cost=0, currency="soft"
    ).json()
    r = admin.admin_create_store_item(
        name=rand_item_name("SkinPack"),
        item_type="skin",
        cost=50,
        currency="soft",
        payload=[{"type": "skin", "skin_id": skin["id"]}],
    )
    return {"item": r.json(), "skin": skin}


@pytest.fixture
def multi_grant_bundle(admin):
    """A "starter pack" bundle: a skin + 100 soft + 5 energy in a single item."""
    skin = admin.admin_create_skin(
        name=rand_skin_name(), outfit_url=rand_url(), cost=0, currency="soft"
    ).json()
    r = admin.admin_create_store_item(
        name=rand_item_name("Starter"),
        item_type="custom",
        cost=10,
        currency="high",
        payload=[
            {"type": "skin", "skin_id": skin["id"]},
            {"type": "currency", "currency": "soft", "amount": 100},
            {"type": "currency", "currency": "energy", "amount": 5},
        ],
    )
    return {"item": r.json(), "skin": skin}


@pytest.fixture
def iap_item(admin):
    """currency='iap' items must not be purchasable via /store/:id/purchase."""
    r = admin.admin_create_store_item(
        name=rand_item_name("IAP"),
        item_type="currency_bundle",
        cost=499,
        currency="iap",
        iap_product_id="com.veloz.gems_500",
        payload=[{"type": "currency", "currency": "high", "amount": 500}],
    )
    return r.json()


# ───────────────────── Listing ─────────────────────


def test_list_store_is_public(api, soft_bundle):
    """/store is public, active items only."""
    r = api.raw_get("/store")
    assert r.status_code == 200
    ids = [i["id"] for i in r.json()]
    assert soft_bundle["id"] in ids


def test_inactive_item_hidden_from_store(admin, api, soft_bundle):
    """Setting is_active=false removes an item from the public /store listing."""
    admin.admin_update_store_item(soft_bundle["id"], is_active=False)
    assert soft_bundle["id"] not in [i["id"] for i in api.raw_get("/store").json()]


# ───────────────────── Purchase ─────────────────────


def test_purchase_insufficient_funds(user, soft_bundle):
    """User has 0 high currency → 422."""
    r = user.purchase_store_item(soft_bundle["id"])
    assert r.status_code == 422


def test_currency_bundle_fulfillment(user, admin, soft_bundle):
    """Buying a soft bundle grants the soft currency in the payload."""
    admin.admin_grant(user.get_profile().json()["user_id"], "high", 200)

    r = user.purchase_store_item(soft_bundle["id"])
    assert r.status_code == 200
    body = r.json()
    assert body["cost_paid"] == 100
    assert body["currency_paid"] == "high"
    # After: high decreased by 100, soft increased by 500.
    w = user.get_wallet().json()
    assert w["high"] == 100
    assert w["soft"] == 500


def test_skin_fulfillment(user, admin, skin_item):
    """Buying a skin-grant item adds to user's owned skins."""
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 200)
    r = user.purchase_store_item(skin_item["item"]["id"])
    assert r.status_code == 200
    assert skin_item["skin"]["id"] in [s["id"] for s in user.owned_skins().json()]


def test_multi_grant_bundle_fulfills_everything(user, admin, multi_grant_bundle):
    """A single store item with a multi-element payload (skin + currency +
    currency) must apply ALL grants in the same transaction."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "high", 50)

    r = user.purchase_store_item(multi_grant_bundle["item"]["id"])
    assert r.status_code == 200

    # Skin owned.
    assert multi_grant_bundle["skin"]["id"] in [
        s["id"] for s in user.owned_skins().json()
    ]
    # Both currency grants applied; high paid the cost.
    w = user.get_wallet().json()
    assert w["high"] == 40   # 50 starting - 10 cost
    assert w["soft"] == 100
    assert w["energy"] == 5


def test_iap_item_not_purchasable_via_store(user, iap_item):
    """currency='iap' requires /wallet/iap/purchase, not /store/:id/purchase → 400."""
    r = user.purchase_store_item(iap_item["id"])
    assert r.status_code == 400


def test_price_multiplier_discount(user, admin, soft_bundle):
    """profile.price_multiplier < 1.0 → buyer charged less than listed cost."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_update_user_profile(user_id, price_multiplier=0.5)
    admin.admin_grant(user_id, "high", 200)

    r = user.purchase_store_item(soft_bundle["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == 50
    assert user.get_wallet().json()["high"] == 150


def test_purchase_nonexistent_item(user):
    """UUID not in store_items → 404."""
    r = user.purchase_store_item("00000000-0000-0000-0000-000000000000")
    assert r.status_code == 404


def test_purchase_inactive_item(user, admin, soft_bundle):
    """Item toggled is_active=false → 410 Gone on purchase."""
    admin.admin_update_store_item(soft_bundle["id"], is_active=False)
    admin.admin_grant(user.get_profile().json()["user_id"], "high", 200)
    r = user.purchase_store_item(soft_bundle["id"])
    assert r.status_code == 410


# ───────────────────── Admin CRUD ─────────────────────


@pytest.mark.admin
def test_admin_store_crud(admin):
    """Round-trip: create with non-empty grant array, update, delete."""
    item = admin.admin_create_store_item(
        name=rand_item_name(),
        item_type="custom",
        cost=1,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()
    assert admin.admin_update_store_item(item["id"], cost=2).json()["cost"] == 2
    assert admin.admin_delete_store_item(item["id"]).status_code == 204


@pytest.mark.admin
def test_admin_list_store_items_shows_inactive(admin):
    """GET /admin/store returns both active + inactive items (unlike public /store)."""
    active = admin.admin_create_store_item(
        name=rand_item_name("Active"),
        item_type="custom",
        cost=1,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()
    inactive = admin.admin_create_store_item(
        name=rand_item_name("Hidden"),
        item_type="custom",
        cost=1,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()
    admin.admin_update_store_item(inactive["id"], is_active=False)

    r = admin.admin_list_store_items()
    assert r.status_code == 200
    ids = [i["id"] for i in r.json()]
    assert active["id"] in ids
    assert inactive["id"] in ids


def test_non_admin_cannot_list_store(user):
    """Regular user → 403 on admin list."""
    assert user.admin_list_store_items().status_code == 403

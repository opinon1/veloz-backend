"""Store flows: listing, purchases, fulfillment per item_type, price_multiplier, IAP gating."""
from __future__ import annotations

import pytest

from helpers.factory import rand_item_name, rand_skin_name, rand_url


@pytest.fixture
def soft_bundle(admin):
    """100 high buys 500 soft. Tests currency_bundle fulfillment."""
    r = admin.admin_create_store_item(
        name=rand_item_name("Bundle"),
        description="500 soft coins",
        item_type="currency_bundle",
        cost=100,
        currency="high",
        payload={"soft": 500},
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
        payload={"skin_id": skin["id"]},
    )
    return {"item": r.json(), "skin": skin}


@pytest.fixture
def iap_item(admin):
    """currency='iap' items must not be purchasable via /store/:id/purchase."""
    r = admin.admin_create_store_item(
        name=rand_item_name("IAP"),
        item_type="currency_bundle",
        cost=499,  # cents or whatever frontend normalizes
        currency="iap",
        iap_product_id="com.veloz.gems_500",
        payload={"high": 500},
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
    """Buying a soft bundle should grant the payload currency."""
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
    """Buying a skin-type item should add to user's owned skins."""
    admin.admin_grant(user.get_profile().json()["user_id"], "soft", 200)
    r = user.purchase_store_item(skin_item["item"]["id"])
    assert r.status_code == 200
    assert skin_item["skin"]["id"] in [s["id"] for s in user.owned_skins().json()]


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
    # cost 100 * 0.5 = 50
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
    item = admin.admin_create_store_item(
        name=rand_item_name(),
        item_type="custom",
        cost=1,
        currency="soft",
        payload={},
    ).json()
    assert admin.admin_update_store_item(item["id"], cost=2).json()["cost"] == 2
    assert admin.admin_delete_store_item(item["id"]).status_code == 204


@pytest.mark.admin
def test_admin_list_store_items_shows_inactive(admin):
    """GET /admin/store returns both active + inactive items (unlike public /store)."""
    active = admin.admin_create_store_item(
        name=rand_item_name("Active"), item_type="custom", cost=1, currency="soft"
    ).json()
    inactive = admin.admin_create_store_item(
        name=rand_item_name("Hidden"), item_type="custom", cost=1, currency="soft"
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

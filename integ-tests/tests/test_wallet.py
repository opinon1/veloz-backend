"""Wallet flows: default balances, spend validation, admin grant, IAP placeholders."""
from __future__ import annotations

import pytest


def test_wallet_defaults_zero(user):
    """All three currencies start at 0 for a new user (per spec)."""
    r = user.get_wallet()
    assert r.status_code == 200
    body = r.json()
    assert body == {"high": 0, "soft": 0, "energy": 0}


def test_spend_with_zero_balance_fails(user):
    """Spending from a 0 balance trips the CHECK constraint → 422."""
    r = user.spend("soft", 10)
    assert r.status_code == 422


@pytest.mark.parametrize("amount", [0, -5])
def test_spend_non_positive_amount(user, amount):
    """Spend amount must be > 0 → 400."""
    r = user.spend("soft", amount)
    assert r.status_code == 400


def test_spend_invalid_currency(user):
    """Unknown currency code → 400 (validated before touching DB)."""
    r = user.spend("bitcoin", 5)
    assert r.status_code == 400


@pytest.mark.admin
def test_admin_grant_then_spend(admin, user, api):
    """Admin grants 100 soft to the user, user spends 40, balance = 60."""
    me = user.get_profile().json()
    user_id = me["user_id"]

    grant = admin.admin_grant(user_id, "soft", 100, reason="test_grant")
    assert grant.status_code == 200
    assert grant.json()["new_balance"] == 100

    spend = user.spend("soft", 40, reason="test_spend")
    assert spend.status_code == 200
    assert spend.json()["new_balance"] == 60

    assert user.get_wallet().json()["soft"] == 60


@pytest.mark.admin
def test_admin_grant_negative_amount_deducts(admin, user):
    """A negative delta on /grant deducts funds (after they exist)."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "high", 50)
    r = admin.admin_grant(user_id, "high", -30)
    assert r.status_code == 200
    assert r.json()["new_balance"] == 20


@pytest.mark.admin
def test_admin_grant_overspend_blocked(admin, user):
    """Admin can't deduct past zero — CHECK constraint rejects → 422."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "energy", 5)
    r = admin.admin_grant(user_id, "energy", -100)
    assert r.status_code == 422


def test_iap_purchase_placeholder(user):
    """Placeholder IAP purchase returns pending_verification. No real fulfillment."""
    r = user.iap_purchase("com.veloz.gems_100", "ios", "BASE64_FAKE_RECEIPT")
    assert r.status_code == 200
    body = r.json()
    assert body["status"] == "pending_verification"
    assert body["product_id"] == "com.veloz.gems_100"
    # Wallet unchanged — placeholder doesn't credit.
    assert user.get_wallet().json()["high"] == 0


def test_iap_validate_placeholder(user):
    """Placeholder validate always returns valid=true (no real receipt check)."""
    r = user.iap_validate("com.veloz.any", "android", "fake")
    assert r.status_code == 200
    assert r.json()["valid"] is True


def test_wallet_requires_auth(api):
    """All wallet endpoints gated by auth."""
    assert api.raw_get("/wallet").status_code == 401
    assert api.raw_post("/wallet/spend", json={"currency": "soft", "amount": 1}).status_code == 401

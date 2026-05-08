"""Payments via Etomin.

Without ETOMIN_EMAIL/PASSWORD set, /payments/charge returns 503 (the client
isn't constructed). These tests cover handler-level validation that runs
*before* hitting Etomin so they pass even without sandbox creds. Tests that
need the live sandbox are marked `pytest.mark.etomin` and skipped when the
client isn't configured.
"""
from __future__ import annotations

import os
import uuid

import pytest

from helpers.factory import rand_item_name


def _customer():
    return {
        "firstName": "John",
        "lastName": "Due",
        "middleName": "",
        "email": "john.due@mail.com",
        "phone1": "5555555555",
        "city": "Mexico",
        "address1": "Test 123",
        "postalCode": "11000",
        "state": "Mexico",
        "country": "MX",
        "ip": "0.0.0.0",
    }


def _card():
    # Etomin sandbox-approved Visa.
    return {
        "cardNumber": "4111111111111111",
        "cvv": "120",
        "cardholderName": "John Due",
        "expirationYear": "27",
        "expirationMonth": "12",
    }


def _make_iap_item(admin, cost: int = 10):
    r = admin.admin_create_store_item(
        name=rand_item_name("IAP"),
        description="IAP test item",
        item_type="currency_bundle",
        cost=cost,
        currency="iap",
        iap_product_id="com.veloz.iap.test",
        payload=[{"type": "currency", "currency": "soft", "amount": 500}],
    )
    assert r.status_code == 201, r.text
    return r.json()


# ───────────── Validation (no Etomin call) ─────────────


def test_charge_requires_auth(api):
    assert api.raw_post("/payments/charge", json={}).status_code == 401


def test_charge_unknown_item_returns_404(user, admin):
    r = user.charge_payment(
        item_id=str(uuid.uuid4()),
        customer_information=_customer(),
        card_data=_card(),
    )
    # 404 (item not found) takes precedence over the 503 from a missing
    # Etomin client because the lookup runs first.
    assert r.status_code == 404


def test_charge_non_iap_item_returns_400(admin, user):
    item = admin.admin_create_store_item(
        name=rand_item_name("Soft"),
        item_type="currency_bundle",
        cost=10,
        currency="soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()
    r = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(),
    )
    assert r.status_code == 400


def test_charge_inactive_iap_item_returns_410(admin, user):
    item = _make_iap_item(admin)
    admin.admin_update_store_item(item["id"], is_active=False)
    r = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(),
    )
    assert r.status_code == 410


def test_charge_returns_503_when_etomin_not_configured(admin, user):
    """Without ETOMIN_EMAIL/PASSWORD set in the running stack, the client is
    `None` and /payments/charge short-circuits with 503."""
    if os.environ.get("ETOMIN_EMAIL") and os.environ.get("ETOMIN_PASSWORD"):
        pytest.skip("Etomin client is configured — happy-path test instead")
    item = _make_iap_item(admin)
    r = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(),
    )
    assert r.status_code == 503


# ───────────── Live Etomin sandbox (requires creds) ─────────────


@pytest.mark.etomin
def test_charge_approved_grants_payload(admin, user):
    """End-to-end against Etomin sandbox: APPROVED → grants applied."""
    if not (os.environ.get("ETOMIN_EMAIL") and os.environ.get("ETOMIN_PASSWORD")):
        pytest.skip("ETOMIN_EMAIL / ETOMIN_PASSWORD not set")

    item = _make_iap_item(admin, cost=10)
    pre_soft = user.get_wallet().json()["soft"]
    r = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(),
    )
    body = r.json()
    if body.get("status") == "APPROVED":
        assert r.status_code == 200
        assert user.get_wallet().json()["soft"] == pre_soft + 500
    elif body.get("status") == "PENDING":
        assert r.status_code == 202
        assert body["redirect_to"]
    else:
        assert r.status_code == 402
        assert body["status"] == "DECLINED"


# ───────────── Get payment status ─────────────


def test_get_payment_404_for_other_user(admin, user_factory):
    """A user can't read someone else's payment row by id."""
    a, _ = user_factory()
    b, _ = user_factory()
    item = _make_iap_item(admin)
    # Trigger A's payment row insert via the 503 path (Etomin not configured)
    # OR skip the row creation if Etomin is configured & APPROVED. Either
    # way the row may exist for A, never for B.
    a_charge = a.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(),
    )
    if a_charge.status_code in (200, 202, 402):
        pid = a_charge.json()["payment_id"]
        assert b.get_payment(pid).status_code == 404


def test_get_payment_unknown_returns_404(user):
    assert user.get_payment(str(uuid.uuid4())).status_code == 404

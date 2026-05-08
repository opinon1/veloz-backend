"""Payments via Etomin.

Sandbox test cards we exercise (provided by Etomin):

  2D Cards:
    4111111111111111  → APPROVED   (Visa)
    4111111111111112  → DECLINED   (Visa)
    5111111111111111  → APPROVED   (Mastercard)
    5111111111111112  → DECLINED   (Mastercard)

  3DS Cards (driven through to APPROVED via the sandbox 3DS simulator):
    4111111111111111   Frictionless (sandbox routes through 2D)
    4000000000002503   Challenge 3DS
    4000000000002511   Frictionless 3DS
    376701078252003    Challenge 3DS

ETOMIN_EMAIL + ETOMIN_PASSWORD must be set on the running stack.
There are no skips — if Etomin isn't configured, tests fail loudly.
"""
from __future__ import annotations

import uuid

import pytest

from helpers.etomin_browser import complete_3ds_in_sandbox
from helpers.factory import rand_item_name


def _customer(email: str | None = None):
    return {
        "firstName": "John",
        "lastName": "Due",
        "middleName": "",
        "email": email or "john.due@mail.com",
        "phone1": "5555555555",
        "city": "Mexico",
        "address1": "Test 123",
        "postalCode": "11000",
        "state": "Mexico",
        "country": "MX",
        "ip": "0.0.0.0",
    }


def _card(number: str = "4111111111111111", cvv: str = "120"):
    return {
        "cardNumber": number,
        "cvv": cvv,
        "cardholderName": "John Due",
        "expirationYear": "27",
        "expirationMonth": "12",
    }


def _make_iap_item(admin, cost: int = 10, grant_amount: int = 500):
    r = admin.admin_create_store_item(
        name=rand_item_name("IAP"),
        description="IAP test item",
        item_type="currency_bundle",
        cost=cost,
        currency="iap",
        iap_product_id="com.veloz.iap.test",
        payload=[{"type": "currency", "currency": "soft", "amount": grant_amount}],
    )
    assert r.status_code == 201, r.text
    return r.json()


# ───────────────────── Validation (no Etomin call) ─────────────────────


def test_charge_requires_auth(api):
    assert api.raw_post("/payments/charge", json={}).status_code == 401


def test_charge_unknown_item_returns_404(user):
    r = user.charge_payment(
        item_id=str(uuid.uuid4()),
        customer_information=_customer(),
        card_data=_card(),
    )
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


# ───────────────────── 2DS card matrix (live Etomin) ─────────────────────


@pytest.mark.etomin
@pytest.mark.parametrize(
    "card_number,brand,expected",
    [
        ("4111111111111111", "Visa",       "APPROVED"),
        ("4111111111111112", "Visa",       "DECLINED"),
        ("5111111111111111", "Mastercard", "APPROVED"),
        ("5111111111111112", "Mastercard", "DECLINED"),
    ],
)
def test_2ds_card_outcomes(admin, user_factory, card_number, brand, expected):
    """Each Etomin sandbox 2DS card has a deterministic outcome. Verify the
    HTTP code, persisted status, and grant fulfillment per case.

    Uses a fresh user per case so the wallet delta is isolated."""
    item = _make_iap_item(admin, cost=10, grant_amount=500)
    u, _ = user_factory()
    pre = u.get_wallet().json()["soft"]

    r = u.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(card_number),
    )
    body = r.json()
    assert body["status"] == expected, f"{brand} {card_number}: got {body}"

    if expected == "APPROVED":
        assert r.status_code == 200
        assert body["redirect_to"] is None
        assert u.get_wallet().json()["soft"] == pre + 500
        row = u.get_payment(body["payment_id"]).json()
        assert row["status"] == "APPROVED"
        assert row["amount"] == 10
        assert row["currency"] == "484"
    else:
        assert r.status_code == 402
        assert u.get_wallet().json()["soft"] == pre
        row = u.get_payment(body["payment_id"]).json()
        assert row["status"] == "DECLINED"


@pytest.mark.etomin
def test_approved_records_etomin_response(admin, user):
    item = _make_iap_item(admin)
    r = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111111"),
    )
    body = r.json()
    assert body["status"] == "APPROVED"
    row = user.get_payment(body["payment_id"]).json()
    assert row["etomin_response"]["status"] == "APPROVED"
    assert "orderId" in row["etomin_response"]
    assert "4111111111111111" not in str(row["etomin_response"])


@pytest.mark.etomin
def test_declined_records_etomin_response(admin, user):
    item = _make_iap_item(admin)
    r = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111112"),
    )
    body = r.json()
    assert body["status"] == "DECLINED"
    assert r.status_code == 402
    row = user.get_payment(body["payment_id"]).json()
    assert row["status"] == "DECLINED"
    assert row["etomin_response"]["status"] == "DECLINED"


@pytest.mark.etomin
def test_each_charge_uses_unique_reference(admin, user):
    item = _make_iap_item(admin)
    a = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111111"),
    ).json()
    b = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111111"),
    ).json()
    assert a["payment_id"] != b["payment_id"]
    assert a["status"] == "APPROVED"
    assert b["status"] == "APPROVED"


@pytest.mark.etomin
def test_decline_does_not_apply_grants(admin, user):
    item = _make_iap_item(admin, grant_amount=999)
    pre_soft = user.get_wallet().json()["soft"]
    r = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111112"),
    )
    assert r.json()["status"] == "DECLINED"
    assert user.get_wallet().json()["soft"] == pre_soft


@pytest.mark.etomin
def test_approved_then_inactive_item_blocks_subsequent_charge(admin, user):
    item = _make_iap_item(admin)
    first = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111111"),
    ).json()
    assert first["status"] == "APPROVED"

    admin.admin_update_store_item(item["id"], is_active=False)
    second = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111111"),
    )
    assert second.status_code == 410


# ───────────────────── 3DS card matrix (live Etomin + simulator) ─────────────────────


@pytest.mark.etomin
@pytest.mark.parametrize(
    "card_number,brand,flow",
    [
        ("4000000000002511", "Visa", "Frictionless"),
        ("4000000000002503", "Visa", "Challenge"),
        ("376701078252003",  "Amex", "Challenge"),
    ],
)
def test_3ds_completion_flow(admin, user_factory, card_number, brand, flow):
    """End-to-end 3DS: charge → PENDING + redirectTo → simulator finishes
    the 3DS dance against Etomin sandbox → we poll GET /payments/{id} →
    lazy reconcile pulls APPROVED status from Etomin → grants applied.

    Mirrors what a real frontend would do: redirect the user, wait, then
    poll until terminal."""
    item = _make_iap_item(admin, cost=10, grant_amount=500)
    u, _ = user_factory()
    pre_soft = u.get_wallet().json()["soft"]

    r = u.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(card_number),
    )
    body = r.json()
    pid = body["payment_id"]
    assert r.status_code == 202, f"{brand} {flow}: expected 202 got {r.status_code} body={body}"
    assert body["status"] == "PENDING"
    assert body["redirect_to"] is not None
    # Wallet untouched while 3DS is mid-flight.
    assert u.get_wallet().json()["soft"] == pre_soft

    # Drive Etomin's 3DS through to completion (sandbox accepts empty
    # deviceInfo and frictionlessly approves the test cards).
    complete_3ds_in_sandbox(body["redirect_to"])

    # Poll our backend — lazy reconcile should pick up the APPROVED state.
    polled = u.get_payment(pid).json()
    assert polled["status"] == "APPROVED", f"{brand} {flow}: still {polled['status']}"
    assert u.get_wallet().json()["soft"] == pre_soft + 500


@pytest.mark.etomin
def test_3ds_no_completion_stays_pending_then_lazy_reconcile_keeps_pending(
    admin, user_factory
):
    """Without running the 3DS simulator, the row stays PENDING and
    polling keeps returning PENDING — never falsely credits the wallet."""
    item = _make_iap_item(admin, grant_amount=500)
    u, _ = user_factory()
    r = u.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4000000000002503"),
    )
    pid = r.json()["payment_id"]
    for _ in range(3):
        polled = u.get_payment(pid).json()
        assert polled["status"] == "PENDING"
    assert u.get_wallet().json()["soft"] == 0


@pytest.mark.etomin
def test_3ds_completion_then_status_query_persists(admin, user_factory):
    """After 3DS completes and we've reconciled once, the row is
    APPROVED + frozen. Subsequent polls don't re-process or re-grant."""
    item = _make_iap_item(admin, grant_amount=500)
    u, _ = user_factory()
    r = u.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4000000000002511"),
    )
    pid = r.json()["payment_id"]
    complete_3ds_in_sandbox(r.json()["redirect_to"])

    first = u.get_payment(pid).json()
    assert first["status"] == "APPROVED"
    bal_after_first = u.get_wallet().json()["soft"]

    # Two more polls — must not re-grant.
    for _ in range(2):
        again = u.get_payment(pid).json()
        assert again["status"] == "APPROVED"
    assert u.get_wallet().json()["soft"] == bal_after_first


# ───────────────────── Get payment status ─────────────────────


def test_get_payment_404_for_other_user(admin, user_factory):
    """A user can't read someone else's payment row by id."""
    a, _ = user_factory()
    b, _ = user_factory()
    item = _make_iap_item(admin)
    a_charge = a.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card(),
    )
    pid = a_charge.json()["payment_id"]
    assert b.get_payment(pid).status_code == 404


def test_get_payment_unknown_returns_404(user):
    assert user.get_payment(str(uuid.uuid4())).status_code == 404

"""Prize wheel: admin sets the wheel; users spin once per 24h via Redis cooldown.

Cooldown lifecycle test deletes the Redis key directly (via the admin
DELETE /admin/prize-wheel/cooldown endpoint that clears the admin's own
cooldown) so we don't have to wait 86400 seconds in CI.
"""
from __future__ import annotations

import pytest

from helpers.factory import admin_make_skin


def _wheel_payload(skin_id: str | None = None):
    items = [
        {"reward": [{"type": "currency", "currency": "soft", "amount": 50}], "weight": 1},
        {"reward": [{"type": "currency", "currency": "high", "amount": 5}], "weight": 1},
    ]
    if skin_id is not None:
        items.append(
            {"reward": [{"type": "skin", "skin_id": skin_id}], "weight": 1}
        )
    return items


# ───────────────────── Admin PUT/GET wheel ─────────────────────


@pytest.mark.admin
def test_admin_put_wheel_replaces_items(admin):
    a = _wheel_payload()
    r1 = admin.admin_put_prize_wheel(a)
    assert r1.status_code == 200
    rows = r1.json()
    assert len(rows) == 2
    assert [r["position"] for r in rows] == [0, 1]
    assert all(r["weight"] == 1 for r in rows)

    # PUT again with a single item — wheel is fully replaced.
    r2 = admin.admin_put_prize_wheel(
        [{"reward": [{"type": "currency", "currency": "soft", "amount": 1}], "weight": 7}]
    )
    assert r2.status_code == 200
    rows = r2.json()
    assert len(rows) == 1
    assert rows[0]["weight"] == 7

    # GET reflects the latest set.
    rows = admin.admin_get_prize_wheel().json()
    assert len(rows) == 1


@pytest.mark.admin
def test_admin_put_wheel_rejects_empty(admin):
    assert admin.admin_put_prize_wheel([]).status_code == 400


@pytest.mark.admin
def test_admin_put_wheel_rejects_zero_weight(admin):
    assert admin.admin_put_prize_wheel(
        [{"reward": [{"type": "currency", "currency": "soft", "amount": 1}], "weight": 0}]
    ).status_code == 400


@pytest.mark.admin
def test_admin_put_wheel_rejects_invalid_grant(admin):
    # bad currency
    assert admin.admin_put_prize_wheel(
        [{"reward": [{"type": "currency", "currency": "btc", "amount": 1}], "weight": 1}]
    ).status_code == 400
    # empty grant array
    assert admin.admin_put_prize_wheel(
        [{"reward": [], "weight": 1}]
    ).status_code == 400


def test_non_admin_cannot_put_wheel(user):
    assert user.admin_put_prize_wheel(_wheel_payload()).status_code == 403
    assert user.admin_get_prize_wheel().status_code == 403


# ───────────────────── User GET wheel ─────────────────────


def test_get_wheel_requires_auth(api):
    assert api.raw_get("/prize-wheel").status_code == 401


def test_get_wheel_returns_items_and_cooldown(admin, user):
    admin.admin_put_prize_wheel(_wheel_payload())
    body = user.get_prize_wheel().json()
    assert "items" in body and "cooldown" in body
    assert len(body["items"]) == 2
    assert body["cooldown"]["ready"] is True
    assert body["cooldown"]["retry_after_seconds"] == 0


# ───────────────────── Spin ─────────────────────


def test_spin_empty_wheel_returns_503(admin, user_factory):
    """Wheel with no items → spin returns 503 + cooldown is NOT set
    (otherwise the user would be punished for an unconfigured wheel).

    Uses DELETE /admin/prize-wheel to reach empty state (PUT requires a
    non-empty array). Fresh user so the cooldown assertion is clean."""
    admin.admin_delete_prize_wheel()
    u, _ = user_factory()
    r = u.spin_prize_wheel()
    assert r.status_code == 503
    # Cooldown NOT set — user can spin again immediately once admin
    # populates the wheel.
    assert u.prize_wheel_cooldown().json()["ready"] is True


def test_spin_grants_currency_and_records_history(admin, user):
    admin.admin_put_prize_wheel([
        {"reward": [{"type": "currency", "currency": "soft", "amount": 100}], "weight": 1}
    ])
    pre_balance = user.get_wallet().json()["soft"]
    r = user.spin_prize_wheel()
    assert r.status_code == 200
    body = r.json()
    assert body["won_index"] == 0
    assert body["reward"] == [{"type": "currency", "currency": "soft", "amount": 100}]
    assert body["new_balances"]["soft"] == pre_balance + 100


def test_spin_sets_cooldown_then_429_on_reattempt(admin, user):
    admin.admin_put_prize_wheel([
        {"reward": [{"type": "currency", "currency": "soft", "amount": 1}], "weight": 1}
    ])
    assert user.spin_prize_wheel().status_code == 200

    cd = user.prize_wheel_cooldown().json()
    assert cd["ready"] is False
    # Should be very close to 86400s.
    assert 86000 <= cd["retry_after_seconds"] <= 86400

    second = user.spin_prize_wheel()
    assert second.status_code == 429
    body = second.json()
    assert body["error"] == "cooldown"
    assert body["retry_after_seconds"] > 0


def test_spin_grants_skin(admin, user):
    skin = admin_make_skin(admin, cost=0, currency="soft")
    admin.admin_put_prize_wheel([
        {"reward": [{"type": "skin", "skin_id": skin["id"]}], "weight": 1}
    ])

    user.spin_prize_wheel()
    owned = [s["id"] for s in user.owned_skins().json()]
    assert skin["id"] in owned


# ───────────────────── Admin clears own cooldown ─────────────────────


@pytest.mark.admin
def test_admin_can_clear_own_cooldown_and_spin_again(admin):
    admin.admin_put_prize_wheel([
        {"reward": [{"type": "currency", "currency": "soft", "amount": 1}], "weight": 1}
    ])
    # Admin spins (same 24h cooldown).
    assert admin.spin_prize_wheel().status_code == 200
    assert admin.spin_prize_wheel().status_code == 429

    # Clear own cooldown → spin succeeds again.
    assert admin.admin_clear_prize_wheel_cooldown().status_code == 204
    assert admin.spin_prize_wheel().status_code == 200


def test_non_admin_cannot_clear_cooldown(user):
    assert user.admin_clear_prize_wheel_cooldown().status_code == 403


# ───────────────────── Cooldown query ─────────────────────


def test_cooldown_query_requires_auth(api):
    assert api.raw_get("/prize-wheel/cooldown").status_code == 401


def test_cooldown_query_initially_ready(user):
    body = user.prize_wheel_cooldown().json()
    assert body["ready"] is True
    assert body["retry_after_seconds"] == 0

"""Energy regen (lazy on read).

Spec:
    - 1 energy / minute while stored energy < 50.
    - Cap at 50 — refilling never pushes above.
    - User can hold >50 from store / admin grants; while above 50 the
      clock is paused and stays paused until energy drops back below 50.

These tests poke the wallet's `energy_refill_started_at` column directly
via psql to simulate elapsed minutes without waiting around for real
time to pass.
"""
from __future__ import annotations

import os
from datetime import datetime, timedelta, timezone

import pytest

from helpers.compose import exec_sql


def _db_env() -> dict[str, str]:
    return dict(
        db_name=os.environ["DB_NAME"],
        db_user=os.environ["DB_USER"],
        pg_port=os.environ["POSTGRES_PORT"],
    )


def _set_energy_state(user_id: str, energy: int, started_at: datetime | None) -> None:
    """Force the wallet's energy + refill anchor. `started_at` may be None
    to clear the clock."""
    ts = "NULL" if started_at is None else f"'{started_at.isoformat()}'"
    exec_sql(
        f"UPDATE wallets SET energy={energy}, energy_refill_started_at={ts} "
        f"WHERE user_id='{user_id}'",
        **_db_env(),
    )


# ──────────────────────────── Tests ────────────────────────────


def test_new_user_starts_with_clock_running(user):
    """Brand-new user: energy=0 < cap, so the GET-side lazy init sets
    the clock on first read."""
    body = user.get_wallet().json()
    assert body["energy"] == 0
    assert body["energy_refill_started_at"] is not None


def test_lazy_refill_grants_one_per_minute(user):
    """Set the anchor 5 minutes ago → next read finds 5 energy."""
    user_id = user.get_profile().json()["user_id"]
    five_min_ago = datetime.now(timezone.utc) - timedelta(minutes=5)
    _set_energy_state(user_id, 0, five_min_ago)

    body = user.get_wallet().json()
    assert body["energy"] == 5


def test_lazy_refill_caps_at_50(user):
    """Anchor an hour ago should top out at 50, not 60."""
    user_id = user.get_profile().json()["user_id"]
    long_ago = datetime.now(timezone.utc) - timedelta(minutes=60)
    _set_energy_state(user_id, 0, long_ago)

    body = user.get_wallet().json()
    assert body["energy"] == 50
    # Cap reached → clock cleared.
    assert body["energy_refill_started_at"] is None


def test_refill_anchor_advances_by_full_minutes(user):
    """Refill must consume only whole minutes; leftover seconds carry
    over so the user doesn't lose progress."""
    user_id = user.get_profile().json()["user_id"]
    # 2 minutes 30 seconds ago → grant 2 energy, anchor moves forward 2 minutes.
    anchor = datetime.now(timezone.utc) - timedelta(minutes=2, seconds=30)
    _set_energy_state(user_id, 0, anchor)

    body = user.get_wallet().json()
    assert body["energy"] == 2
    new_anchor = datetime.fromisoformat(body["energy_refill_started_at"])
    expected = anchor + timedelta(minutes=2)
    assert abs((new_anchor - expected).total_seconds()) < 2


@pytest.mark.admin
def test_above_cap_stays_above_cap(admin, user):
    """Energy granted past 50 (e.g. refill pack) doesn't tick down; the
    regen clock is paused while >= cap."""
    user_id = user.get_profile().json()["user_id"]
    # Top up to 80 via admin grant; reconciler clears the clock.
    assert admin.admin_grant(user_id, "energy", 80).status_code == 200

    body = user.get_wallet().json()
    assert body["energy"] == 80
    assert body["energy_refill_started_at"] is None


@pytest.mark.admin
def test_spending_back_below_cap_restarts_clock(admin, user):
    """User holds 60 from a grant; spends 20 down to 40 (< cap) → clock
    restarts from the spend moment."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "energy", 60)
    # Clock paused while at 60.
    assert user.get_wallet().json()["energy_refill_started_at"] is None

    assert user.spend("energy", 20).status_code == 200
    body = user.get_wallet().json()
    assert body["energy"] == 40
    assert body["energy_refill_started_at"] is not None

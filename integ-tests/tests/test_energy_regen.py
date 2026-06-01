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


def _ledger_sum(user_id: str, reason: str) -> int:
    """Direct DB read for SUM(delta) on energy ledger rows. exec_sql is
    write-only in the INTEG_NO_DOCKER fallback path so we go through
    psycopg ourselves."""
    import psycopg

    with psycopg.connect(
        host="localhost",
        port=int(os.environ["POSTGRES_PORT"]),
        dbname=os.environ["DB_NAME"],
        user=os.environ["DB_USER"],
        password=os.environ["DB_PASSWORD"],
    ) as conn, conn.cursor() as cur:
        cur.execute(
            "SELECT COALESCE(SUM(delta),0) FROM wallet_ledger "
            "WHERE user_id=%s AND currency='energy' AND reason=%s",
            (user_id, reason),
        )
        return int(cur.fetchone()[0])


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


# ────────────────────────── Edge cases ──────────────────────────


@pytest.mark.admin
def test_spend_to_exactly_50_clears_clock(admin, user):
    """50 is the cap — spending down to exactly 50 must clear the
    clock (not start it). Clock only runs when stored < 50."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "energy", 70)
    assert user.spend("energy", 20).status_code == 200
    body = user.get_wallet().json()
    assert body["energy"] == 50
    assert body["energy_refill_started_at"] is None


@pytest.mark.admin
def test_grant_exactly_to_50_clears_clock(admin, user):
    """Granting energy that lands exactly on the cap stops the timer."""
    # New user has clock running.
    user_id = user.get_profile().json()["user_id"]
    user.get_wallet()  # ensures clock initialized
    admin.admin_grant(user_id, "energy", 50)
    body = user.get_wallet().json()
    assert body["energy"] == 50
    assert body["energy_refill_started_at"] is None


def test_lazy_refill_with_anchor_far_in_the_past_caps_at_50(user):
    """An anchor from days ago is no different from an anchor an hour
    ago — saturation behavior is the same."""
    user_id = user.get_profile().json()["user_id"]
    days_ago = datetime.now(timezone.utc) - timedelta(days=3)
    _set_energy_state(user_id, 0, days_ago)
    body = user.get_wallet().json()
    assert body["energy"] == 50
    assert body["energy_refill_started_at"] is None


def test_lazy_refill_with_partial_minute_grants_zero(user):
    """30 seconds elapsed = 0 full minutes = no grant; anchor unchanged."""
    user_id = user.get_profile().json()["user_id"]
    anchor = datetime.now(timezone.utc) - timedelta(seconds=30)
    _set_energy_state(user_id, 0, anchor)
    body = user.get_wallet().json()
    assert body["energy"] == 0
    new_anchor = datetime.fromisoformat(body["energy_refill_started_at"])
    assert abs((new_anchor - anchor).total_seconds()) < 2


def test_lazy_refill_writes_regen_ledger_entry(user):
    """Every regen burst lands a single `regen` row in wallet_ledger
    so the ledger explains where any non-spend balance came from."""
    user_id = user.get_profile().json()["user_id"]
    # 5 min elapsed → 5 energy granted.
    _set_energy_state(user_id, 0, datetime.now(timezone.utc) - timedelta(minutes=5))
    user.get_wallet()
    assert _ledger_sum(user_id, "regen") >= 5


def test_repeated_reads_dont_double_credit(user):
    """Calling /wallet twice in the same second doesn't grant twice."""
    user_id = user.get_profile().json()["user_id"]
    _set_energy_state(user_id, 0, datetime.now(timezone.utc) - timedelta(minutes=3))
    user.get_wallet()
    e1 = user.get_wallet().json()["energy"]
    e2 = user.get_wallet().json()["energy"]
    e3 = user.get_wallet().json()["energy"]
    assert e1 == e2 == e3 == 3


@pytest.mark.admin
def test_grant_below_cap_keeps_clock_running(admin, user):
    """Granting energy that stays below cap shouldn't reset the clock
    — only spending past the cap (downward) starts it. If already
    running, leave the existing anchor in place (no progress reset)."""
    user_id = user.get_profile().json()["user_id"]
    # New user → clock initialized on first read.
    initial = user.get_wallet().json()
    assert initial["energy_refill_started_at"] is not None
    initial_clock = datetime.fromisoformat(initial["energy_refill_started_at"])

    admin.admin_grant(user_id, "energy", 5)
    body = user.get_wallet().json()
    assert body["energy"] == 5
    # Clock still set (energy < cap). Anchor preserved (Postgres rounds
    # the in-memory `Utc::now()` to microsecond precision so we compare
    # within 1s tolerance rather than exact string equality).
    after = datetime.fromisoformat(body["energy_refill_started_at"])
    assert abs((after - initial_clock).total_seconds()) < 1


@pytest.mark.admin
def test_spend_records_ledger(admin, user):
    """Sanity: spending energy still writes a ledger row, regardless
    of the regen wiring above it."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "energy", 30)
    user.spend("energy", 10)
    assert _ledger_sum(user_id, "spend") == -10


def test_anchor_in_the_future_grants_nothing(user):
    """Defensive: clock somehow set in the future (clock drift, bad
    fixture) must not produce negative grants or rollover bugs."""
    user_id = user.get_profile().json()["user_id"]
    future = datetime.now(timezone.utc) + timedelta(hours=1)
    _set_energy_state(user_id, 0, future)
    body = user.get_wallet().json()
    assert body["energy"] == 0


@pytest.mark.admin
def test_refill_after_partial_spend_continues_from_below_cap(admin, user):
    """User at 60 (no clock) spends 30 → at 30 with clock. Set anchor
    10 minutes back → reads as 40."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "energy", 60)
    user.spend("energy", 30)
    # Now energy=30 with clock running.
    _set_energy_state(user_id, 30, datetime.now(timezone.utc) - timedelta(minutes=10))
    body = user.get_wallet().json()
    assert body["energy"] == 40


def test_wallet_response_shape_includes_refill_field(user):
    """Contract: every wallet GET returns these four keys, regardless
    of whether the clock is running."""
    body = user.get_wallet().json()
    assert set(body.keys()) == {"high", "soft", "energy", "energy_refill_started_at"}

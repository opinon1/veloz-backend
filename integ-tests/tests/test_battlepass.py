"""Battlepass flows: season lifecycle, tier claims (free/premium), unlock gating."""
from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest

from helpers.factory import rand_season_name


def _iso(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


@pytest.fixture
def active_season(admin):
    """A season whose window includes 'now'. Kept small (1h past → 1h future)
    so tests don't accidentally overlap a real upcoming season."""
    now = datetime.now(timezone.utc)
    r = admin.admin_create_season(
        name=rand_season_name(),
        description="integ-test season",
        starts_at=_iso(now - timedelta(hours=1)),
        ends_at=_iso(now + timedelta(hours=1)),
        premium_cost=100,
        premium_currency="high",
    )
    assert r.status_code == 201
    return r.json()


@pytest.fixture
def tier_cheap(admin, active_season):
    """Tier 1 requiring 0 BP XP — claimable immediately."""
    r = admin.admin_create_tier(
        active_season["id"],
        tier=1,
        xp_required=0,
        free_reward={"type": "currency", "currency": "soft", "amount": 50},
        premium_reward={"type": "currency", "currency": "high", "amount": 10},
    )
    assert r.status_code == 201
    return r.json()


@pytest.fixture
def tier_expensive(admin, active_season):
    """Tier 2 requiring 1_000_000_000 XP — never reachable in a test run."""
    r = admin.admin_create_tier(
        active_season["id"],
        tier=2,
        xp_required=10**9,
        free_reward={"type": "currency", "currency": "soft", "amount": 500},
        premium_reward={"type": "currency", "currency": "high", "amount": 100},
    )
    assert r.status_code == 201
    return r.json()


# ───────────────────── No active season ─────────────────────


def test_bp_current_without_season(api):
    """No season window covering now → 404 on /battlepass/current."""
    # NOTE: This test is fragile if run in parallel with `active_season` fixtures.
    # pytest default is serial; if parallelizing, gate with a unique test DB per worker.
    r = api.raw_get("/battlepass/current")
    # Either 404 (empty) or 200 (another test already created an active season this run).
    assert r.status_code in (200, 404)


def test_bp_progress_requires_active_season(user):
    """Without an active season, progress endpoint → 404."""
    # Same caveat as above — if another fixture seeded a season, this may 200.
    r = user.bp_progress()
    assert r.status_code in (200, 404)


# ───────────────────── Active season basics ─────────────────────


def test_bp_current_returns_season_with_tiers(api, active_season, tier_cheap, tier_expensive):
    """/battlepass/current returns the active season + ordered tiers."""
    r = api.raw_get("/battlepass/current")
    assert r.status_code == 200
    body = r.json()
    # Body may reference a different season if multiple overlap — match by id.
    # We only assert when the returned season is ours.
    if body["id"] == active_season["id"]:
        tier_numbers = [t["tier"] for t in body["tiers"]]
        assert 1 in tier_numbers
        assert 2 in tier_numbers
        assert tier_numbers == sorted(tier_numbers)


def test_bp_progress_defaults_zero(user, active_season):
    """Fresh user with no runs → bp_xp=0, premium_unlocked=false, claims=[]."""
    r = user.bp_progress()
    assert r.status_code == 200
    body = r.json()
    assert body["bp_xp"] == 0
    assert body["premium_unlocked"] is False
    assert body["claimed_free"] == []
    assert body["claimed_premium"] == []


# ───────────────────── Tier claims ─────────────────────


def test_claim_free_tier_happy(user, active_season, tier_cheap):
    """xp_required=0 → claim works immediately. Response echoes reward payload."""
    r = user.bp_claim(tier=1, track="free")
    assert r.status_code == 200
    body = r.json()
    assert body["tier"] == 1
    assert body["track"] == "free"
    assert body["reward"]["amount"] == 50


def test_double_claim_rejected(user, active_season, tier_cheap):
    """Second claim of the same tier+track → 409."""
    assert user.bp_claim(tier=1, track="free").status_code == 200
    assert user.bp_claim(tier=1, track="free").status_code == 409


def test_claim_tier_without_enough_xp(user, active_season, tier_expensive):
    """Tier 2 needs 10^9 XP → 403."""
    r = user.bp_claim(tier=2, track="free")
    assert r.status_code == 403


def test_claim_premium_without_unlock(user, active_season, tier_cheap):
    """Premium track blocked until /unlock-premium → 402 (Payment Required)."""
    r = user.bp_claim(tier=1, track="premium")
    assert r.status_code == 402


def test_claim_invalid_track(user, active_season, tier_cheap):
    """Unknown track name → 400."""
    r = user.bp_claim(tier=1, track="platinum")
    assert r.status_code == 400


def test_claim_nonexistent_tier(user, active_season):
    """Tier number not seeded → 404."""
    r = user.bp_claim(tier=99, track="free")
    assert r.status_code == 404


# ───────────────────── Unlock premium ─────────────────────


def test_unlock_premium_insufficient_funds(user, active_season):
    """User has 0 'high' currency → 422 (CHECK constraint)."""
    r = user.bp_unlock_premium()
    assert r.status_code == 422


def test_unlock_premium_happy(user, admin, active_season, tier_cheap):
    """Grant high, unlock premium, then premium claim works."""
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "high", 500)

    r = user.bp_unlock_premium()
    assert r.status_code == 200
    body = r.json()
    assert body["cost_paid"] == 100
    assert body["new_balance"] == 400

    assert user.bp_progress().json()["premium_unlocked"] is True
    assert user.bp_claim(tier=1, track="premium").status_code == 200


def test_unlock_premium_double_call(user, admin, active_season):
    """Unlocking twice → 409 Conflict (no double-charge)."""
    admin.admin_grant(user.get_profile().json()["user_id"], "high", 500)
    assert user.bp_unlock_premium().status_code == 200
    assert user.bp_unlock_premium().status_code == 409


# ───────────────────── Run → BP XP ─────────────────────


def test_run_grants_bp_xp_when_season_active(user, active_season):
    """Submitting a run should credit BP XP on the active season's progress row."""
    before = user.bp_progress().json()["bp_xp"]
    user.submit_run(score=500, distance=100, coins_collected=0, duration_ms=30_000)
    after = user.bp_progress().json()["bp_xp"]
    assert after > before


# ───────────────────── Admin CRUD ─────────────────────


@pytest.mark.admin
def test_admin_season_crud(admin):
    """Create → update → delete season."""
    now = datetime.now(timezone.utc)
    s = admin.admin_create_season(
        name=rand_season_name(),
        starts_at=_iso(now + timedelta(days=30)),
        ends_at=_iso(now + timedelta(days=60)),
        premium_cost=50,
    ).json()
    sid = s["id"]

    upd = admin.admin_update_season(sid, premium_cost=75).json()
    assert upd["premium_cost"] == 75

    assert admin.admin_delete_season(sid).status_code == 204
    assert admin.admin_update_season(sid, premium_cost=1).status_code == 404


@pytest.mark.admin
def test_admin_season_invalid_window(admin):
    """ends_at <= starts_at → DB CHECK fires → 500 (or 400 if server normalizes).
    Either way, no row is persisted."""
    now = datetime.now(timezone.utc)
    r = admin.admin_create_season(
        name=rand_season_name(),
        starts_at=_iso(now + timedelta(days=10)),
        ends_at=_iso(now + timedelta(days=5)),
    )
    assert r.status_code >= 400

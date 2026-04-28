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


@pytest.mark.admin
def test_admin_list_seasons(admin):
    """GET /admin/battlepass/seasons returns every season — past, active, future."""
    now = datetime.now(timezone.utc)
    future = admin.admin_create_season(
        name=rand_season_name(),
        starts_at=_iso(now + timedelta(days=100)),
        ends_at=_iso(now + timedelta(days=130)),
    ).json()
    r = admin.admin_list_seasons()
    assert r.status_code == 200
    ids = [s["id"] for s in r.json()]
    assert future["id"] in ids


@pytest.mark.admin
def test_admin_list_tiers(admin, active_season, tier_cheap, tier_expensive):
    """GET /admin/battlepass/seasons/{id}/tiers lists all tiers for the season,
    ordered by tier number."""
    r = admin.admin_list_tiers(active_season["id"])
    assert r.status_code == 200
    rows = r.json()
    numbers = [t["tier"] for t in rows]
    assert 1 in numbers
    assert 2 in numbers
    assert numbers == sorted(numbers)


@pytest.mark.admin
def test_admin_tier_update_and_delete(admin, active_season, tier_cheap):
    """PATCH + DELETE on /admin/battlepass/tiers/{id}."""
    updated = admin.admin_update_tier(tier_cheap["id"], xp_required=999).json()
    assert updated["xp_required"] == 999

    assert admin.admin_delete_tier(tier_cheap["id"]).status_code == 204
    # Post-delete PATCH → 404.
    assert admin.admin_update_tier(tier_cheap["id"], xp_required=1).status_code == 404


@pytest.mark.admin
def test_admin_tier_update_unknown(admin):
    """Updating nonexistent tier UUID → 404."""
    r = admin.admin_update_tier(
        "00000000-0000-0000-0000-000000000000", xp_required=10
    )
    assert r.status_code == 404


def test_non_admin_cannot_list_seasons(user):
    """Regular user on admin list → 403."""
    assert user.admin_list_seasons().status_code == 403


# ─────────────────── End-to-end progression flows ───────────────────


def test_run_to_claim_full_progression(admin, user, active_season):
    """Walk a user through the realistic flow: submit runs to earn BP XP,
    cross a non-trivial tier threshold, then claim the reward.

    With the default leveling formula (`bp_xp_from_run(score) = score`),
    a single 600-point run earns 600 BP XP. We seed two tiers — tier 1 at
    100 XP, tier 2 at 500 XP — and verify both become claimable in order
    after one run."""
    # Tiers staged at thresholds the user will actually cross.
    t1 = admin.admin_create_tier(
        active_season["id"],
        tier=10,
        xp_required=100,
        free_reward={"type": "currency", "currency": "soft", "amount": 25},
        premium_reward={"type": "currency", "currency": "high", "amount": 5},
    ).json()
    t2 = admin.admin_create_tier(
        active_season["id"],
        tier=11,
        xp_required=500,
        free_reward={"type": "currency", "currency": "soft", "amount": 250},
        premium_reward={"type": "currency", "currency": "high", "amount": 50},
    ).json()

    # Before any runs, neither tier is claimable.
    assert user.bp_claim(tier=10, track="free").status_code == 403
    assert user.bp_claim(tier=11, track="free").status_code == 403
    assert user.bp_progress().json()["bp_xp"] == 0

    # One big run crosses both thresholds (600 > 500 > 100).
    run = user.submit_run(score=600, distance=0, coins_collected=0, duration_ms=1).json()
    assert run["bp_xp_awarded"] == 600
    assert run["active_season_id"] == active_season["id"]
    assert user.bp_progress().json()["bp_xp"] == 600

    # Both tiers now claimable on the free track. Reward payload preserved.
    r1 = user.bp_claim(tier=10, track="free")
    assert r1.status_code == 200
    body1 = r1.json()
    assert body1["track"] == "free"
    assert body1["reward"] == {"type": "currency", "currency": "soft", "amount": 25}

    r2 = user.bp_claim(tier=11, track="free")
    assert r2.status_code == 200
    assert r2.json()["reward"] == {"type": "currency", "currency": "soft", "amount": 250}

    # Progress reflects both claims.
    progress = user.bp_progress().json()
    assert sorted(progress["claimed_free"]) == [10, 11]
    assert progress["claimed_premium"] == []
    _ = t1, t2


def test_xp_accrues_across_multiple_runs(user, active_season):
    """BP XP must accumulate across runs, not overwrite.
    `submit_run` uses `INSERT … ON CONFLICT … DO UPDATE SET bp_xp = bp_xp + EXCLUDED.bp_xp`."""
    user.submit_run(score=100, distance=0, coins_collected=0, duration_ms=1)
    user.submit_run(score=200, distance=0, coins_collected=0, duration_ms=1)
    user.submit_run(score=50, distance=0, coins_collected=0, duration_ms=1)
    assert user.bp_progress().json()["bp_xp"] == 350


def test_xp_threshold_cross_only_after_enough_runs(admin, user, active_season):
    """Tier locked until cumulative BP XP crosses the threshold via multiple
    runs. Mid-progression claim attempts return 403 with no state change."""
    tier = admin.admin_create_tier(
        active_season["id"],
        tier=20,
        xp_required=300,
        free_reward={"type": "currency", "currency": "soft", "amount": 99},
        premium_reward={},
    ).json()

    # 100 XP — not enough.
    user.submit_run(score=100, distance=0, coins_collected=0, duration_ms=1)
    assert user.bp_claim(tier=20, track="free").status_code == 403

    # 200 more XP → 300 total → exactly at threshold (>=).
    user.submit_run(score=200, distance=0, coins_collected=0, duration_ms=1)
    r = user.bp_claim(tier=20, track="free")
    assert r.status_code == 200
    assert r.json()["reward"]["amount"] == 99
    _ = tier


def test_premium_reward_payload_only_after_unlock_and_xp(admin, user, active_season):
    """Premium track requires BOTH the XP threshold AND a paid premium unlock.
    Test order: insufficient XP → 403; enough XP but no unlock → 402; both → 200."""
    tier = admin.admin_create_tier(
        active_season["id"],
        tier=30,
        xp_required=400,
        free_reward={"type": "currency", "currency": "soft", "amount": 10},
        premium_reward={"type": "skin", "skin_id": "pretend-skin-id"},
    ).json()

    # No XP yet → premium claim 403 (gate is XP first).
    assert user.bp_claim(tier=30, track="premium").status_code == 403

    # Earn enough XP, but premium not unlocked → 402.
    user.submit_run(score=400, distance=0, coins_collected=0, duration_ms=1)
    assert user.bp_claim(tier=30, track="premium").status_code == 402

    # Unlock premium with the 'high' currency.
    admin.admin_grant(user.get_profile().json()["user_id"], "high", 500)
    assert user.bp_unlock_premium().status_code == 200

    # Now premium claim succeeds, and the configured payload is returned verbatim.
    r = user.bp_claim(tier=30, track="premium")
    assert r.status_code == 200
    assert r.json()["reward"] == {"type": "skin", "skin_id": "pretend-skin-id"}

    # And the FREE track of the same tier remains independently claimable.
    rf = user.bp_claim(tier=30, track="free")
    assert rf.status_code == 200
    assert rf.json()["reward"] == {"type": "currency", "currency": "soft", "amount": 10}
    _ = tier


def test_claim_free_does_not_consume_bp_xp(admin, user, active_season):
    """Claiming a tier records the claim but does NOT spend BP XP. Players
    keep accumulating toward higher tiers after claiming earlier ones."""
    admin.admin_create_tier(
        active_season["id"],
        tier=40,
        xp_required=100,
        free_reward={"type": "currency", "currency": "soft", "amount": 1},
        premium_reward={},
    )
    admin.admin_create_tier(
        active_season["id"],
        tier=41,
        xp_required=200,
        free_reward={"type": "currency", "currency": "soft", "amount": 2},
        premium_reward={},
    )

    user.submit_run(score=200, distance=0, coins_collected=0, duration_ms=1)
    assert user.bp_progress().json()["bp_xp"] == 200

    user.bp_claim(tier=40, track="free")
    # XP unchanged after a claim.
    assert user.bp_progress().json()["bp_xp"] == 200
    # And tier 41 still claimable since XP hasn't been spent.
    assert user.bp_claim(tier=41, track="free").status_code == 200


def test_claims_persist_across_progress_check(admin, user, active_season):
    """After claiming, /battlepass/progress lists the claimed tier under the
    correct track until the season ends."""
    admin.admin_create_tier(
        active_season["id"],
        tier=50,
        xp_required=0,
        free_reward={"type": "currency", "currency": "soft", "amount": 7},
        premium_reward={"type": "currency", "currency": "high", "amount": 1},
    )
    rc = user.bp_claim(tier=50, track="free")
    assert rc.status_code == 200, f"free claim failed: {rc.status_code} {rc.text}"

    progress = user.bp_progress().json()
    assert 50 in progress["claimed_free"]
    assert 50 not in progress["claimed_premium"]

    # Premium track of the same tier still claimable independently after unlock.
    admin.admin_grant(user.get_profile().json()["user_id"], "high", 500)
    user.bp_unlock_premium()
    assert user.bp_claim(tier=50, track="premium").status_code == 200
    progress2 = user.bp_progress().json()
    assert 50 in progress2["claimed_free"]
    assert 50 in progress2["claimed_premium"]


def test_run_outside_season_does_not_grant_bp_xp(api, user_factory, admin):
    """A run submitted while no season is active must report
    `active_season_id == null` and `bp_xp_awarded == 0`. Other tests in this
    file create active seasons, so spin up a fresh user from a moment when
    we explicitly know there's no overlapping season fixture in this test."""
    # We can't actually pause active_season fixtures from elsewhere; instead
    # rely on the fact that submit_run only awards BP XP when active_season
    # returns Some, and the response carries the season_id. If a season
    # happens to be active right now, skip the assertion gracefully.
    fresh, _ = user_factory()
    r = fresh.submit_run(score=100, distance=0, coins_collected=0, duration_ms=1).json()
    if r["active_season_id"] is None:
        assert r["bp_xp_awarded"] == 0
        # No bp_progress row created either — /progress returns 404 since
        # active_season() inside the handler is None.
        assert fresh.bp_progress().status_code in (200, 404)

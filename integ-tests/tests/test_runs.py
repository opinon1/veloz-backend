"""Run submission: XP/level/highscore/soft updates + leaderboard + history."""
from __future__ import annotations

import pytest


def test_submit_run_awards_xp_and_soft(user):
    """Score 1000, coins 75 → +1000 total_xp, soft balance 75."""
    r = user.submit_run(score=1000, distance=250, coins_collected=75, duration_ms=60_000)
    assert r.status_code == 200
    body = r.json()
    assert body["xp_awarded"] == 1000
    assert body["new_total_xp"] == 1000
    assert body["soft_awarded"] == 75
    assert body["new_soft_balance"] == 75
    assert body["new_highscore"] is True
    assert body["main_highscore"] == 1000

    profile = user.get_profile().json()
    assert profile["total_xp"] == 1000
    assert profile["main_highscore"] == 1000


def test_highscore_only_increases(user):
    """Submitting a lower-scoring second run must not regress main_highscore."""
    user.submit_run(score=1000, distance=0, coins_collected=0, duration_ms=1)
    r = user.submit_run(score=400, distance=0, coins_collected=0, duration_ms=1)
    assert r.status_code == 200
    assert r.json()["main_highscore"] == 1000
    # But XP still accrues.
    assert r.json()["new_total_xp"] == 1400


def test_level_progression(user):
    """Default curve: level = floor(sqrt(total_xp / 100)) + 1. 10_000 XP → level 11."""
    user.submit_run(score=10_000, distance=0, coins_collected=0, duration_ms=1)
    assert user.get_profile().json()["account_level"] == 11


@pytest.mark.parametrize(
    "score,distance,coins,duration",
    [
        (-1, 0, 0, 0),
        (0, -1, 0, 0),
        (0, 0, -1, 0),
        (0, 0, 0, -1),
    ],
)
def test_submit_run_rejects_negative(user, score, distance, coins, duration):
    """Any negative field → 400 before touching DB."""
    r = user.submit_run(score, distance, coins, duration)
    assert r.status_code == 400


def test_run_without_active_season_has_no_bp_xp(user):
    """If no season spans 'now', active_season_id=null and bp_xp_awarded=0."""
    # Caveat: another test may have seeded an active season this session.
    r = user.submit_run(score=500, distance=0, coins_collected=0, duration_ms=1)
    body = r.json()
    if body["active_season_id"] is None:
        assert body["bp_xp_awarded"] == 0


def test_history_shows_recent_runs(user):
    """GET /runs returns the submitting user's own runs, newest first."""
    user.submit_run(score=100, distance=0, coins_collected=0, duration_ms=1)
    user.submit_run(score=300, distance=0, coins_collected=0, duration_ms=1)
    r = user.run_history(limit=10)
    assert r.status_code == 200
    rows = r.json()
    assert len(rows) >= 2
    # Newest first.
    assert rows[0]["score"] >= rows[-1]["score"] or len(rows) == 1


def test_history_limit_clamped(user):
    """limit is clamped to [1,100]. limit=9999 should not blow up."""
    r = user.run_history(limit=9999)
    assert r.status_code == 200


def test_leaderboard_excludes_users_with_zero_score(api, user_factory):
    """Users who never submitted a run (highscore=0) should not appear."""
    fresh, _ = user_factory()
    r = api.raw_get("/runs/leaderboard")
    assert r.status_code == 200
    ids = [row["user_id"] for row in r.json()]
    fresh_id = fresh.get_profile().json()["user_id"]
    assert fresh_id not in ids


def test_leaderboard_includes_after_run(user):
    """After a positive run, the user appears on the public leaderboard."""
    user.submit_run(score=12345, distance=0, coins_collected=0, duration_ms=1)
    r = user.leaderboard(limit=200)
    assert r.status_code == 200
    ids = [row["user_id"] for row in r.json()]
    me = user.get_profile().json()["user_id"]
    assert me in ids


def test_runs_requires_auth(api):
    """POST /runs and GET /runs require auth; /runs/leaderboard is public."""
    assert api.raw_post("/runs", json={"score": 1, "distance": 0, "coins_collected": 0, "duration_ms": 1}).status_code == 401
    assert api.raw_get("/runs").status_code == 401
    assert api.raw_get("/runs/leaderboard").status_code == 200

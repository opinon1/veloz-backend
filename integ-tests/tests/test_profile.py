"""Profile flows: read defaults + auth.

Avatar/frame selection now lives behind dedicated `/avatars/{id}/select` and
`/frames/{id}/select` endpoints (see test_avatars.py / test_frames.py). PATCH
/profile is a no-op echo today.
"""
from __future__ import annotations


def test_profile_defaults_on_signup(user):
    """Freshly-signed-up user gets a profile row auto-created with sane defaults.
    Starting balances of 0 are covered in test_wallet — this test focuses on the
    profile fields (level, xp, multiplier, highscore, avatar, frame)."""
    r = user.get_profile()
    assert r.status_code == 200
    body = r.json()
    assert body["account_level"] == 1
    assert body["total_xp"] == 0
    assert body["price_multiplier"] == 1.0
    assert body["main_highscore"] == 0
    assert body["avatar_url"] is None
    assert body["frame_url"] is None


def test_profile_patch_echoes_current_selections(user):
    """PATCH /profile with empty body returns the current selections."""
    r = user.update_profile()
    assert r.status_code == 200
    assert r.json() == {"avatar_url": None, "frame_url": None}


def test_profile_requires_auth(api):
    """Unauthenticated request → 401."""
    assert api.raw_get("/profile").status_code == 401
    assert api.raw_patch("/profile", json={}).status_code == 401

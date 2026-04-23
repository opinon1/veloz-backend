"""Profile flows: read defaults, partial updates (avatar_url / frame_url)."""
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
    assert body["avatar_url"] in (None, "")
    assert body["frame_url"] in (None, "")


def test_profile_update_avatar_only(user):
    """PATCH with only avatar_url leaves frame_url untouched."""
    resp = user.update_profile(avatar_url="avatar-abc")
    assert resp.status_code == 200
    assert resp.json()["avatar_url"] == "avatar-abc"
    assert resp.json()["frame_url"] in (None, "")

    profile = user.get_profile().json()
    assert profile["avatar_url"] == "avatar-abc"


def test_profile_update_frame_only(user):
    """PATCH with only frame_url leaves avatar_url untouched."""
    user.update_profile(avatar_url="keep-me")
    r = user.update_profile(frame_url="https://cdn.example.com/frames/gold.png")
    assert r.status_code == 200
    body = r.json()
    assert body["avatar_url"] == "keep-me"
    assert body["frame_url"] == "https://cdn.example.com/frames/gold.png"


def test_profile_update_both(user):
    """PATCH with both fields writes both."""
    r = user.update_profile(avatar_url="a", frame_url="f")
    assert r.status_code == 200
    assert r.json() == {"avatar_url": "a", "frame_url": "f"}


def test_profile_requires_auth(api):
    """Unauthenticated request → 401."""
    assert api.raw_get("/profile").status_code == 401
    assert api.raw_patch("/profile", json={"avatar_url": "x"}).status_code == 401

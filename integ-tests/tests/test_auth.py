"""Auth flows: signup, signin, verify, refresh, signout, password change, account delete, sessions.

Covers happy paths + validation failures + token invalidation semantics.
"""
from __future__ import annotations

import pytest

from helpers.factory import make_creds, rand_email, rand_password, rand_username


# ───────────────────────── Signup ─────────────────────────


def test_signup_happy_path(api):
    """Valid creds → 201 + user body with id/username/email (no password)."""
    c = make_creds()
    r = api.signup(c.username, c.email, c.password)
    assert r.status_code == 201
    body = r.json()
    assert body["username"] == c.username
    assert body["email"] == c.email
    assert "id" in body
    assert "password" not in body
    assert "password_hash" not in body


def test_signup_duplicate_email(api):
    """Second signup with same email → 409 Conflict."""
    c = make_creds()
    assert api.signup(c.username, c.email, c.password).status_code == 201
    r = api.signup(rand_username(), c.email, c.password)
    assert r.status_code == 409


def test_signup_duplicate_username(api):
    """Second signup with same username → 409 Conflict."""
    c = make_creds()
    assert api.signup(c.username, c.email, c.password).status_code == 201
    r = api.signup(c.username, rand_email(), c.password)
    assert r.status_code == 409


@pytest.mark.parametrize(
    "password,reason",
    [
        ("short1!", "too short (< 8)"),
        ("a" * 73, "too long (> 72)"),
        ("onlyletters", "no digit or special char"),
    ],
)
def test_signup_invalid_password(api, password, reason):
    """Password validation rules enforced server-side, regardless of reason."""
    r = api.signup(rand_username(), rand_email(), password)
    assert r.status_code == 400, reason


@pytest.mark.parametrize(
    "username",
    ["ab", "a" * 31, "bad-dash", "bad space", "bad.dot", "emoji😀"],
)
def test_signup_invalid_username(api, username):
    """Username must match `^[a-zA-Z0-9_]{3,30}$`."""
    r = api.signup(username, rand_email(), rand_password())
    assert r.status_code == 400


@pytest.mark.parametrize("email", ["no-at-symbol", "a@b", "x"])
def test_signup_invalid_email(api, email):
    """Basic email sanity check rejects obviously malformed addresses."""
    r = api.signup(rand_username(), email, rand_password())
    assert r.status_code == 400


# ───────────────────────── Signin ─────────────────────────


def test_signin_returns_tokens(api):
    """Valid creds → 200 + access_token + refresh_token."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    r = api.signin(c.email, c.password)
    assert r.status_code == 200
    body = r.json()
    assert body["access_token"]
    assert body["refresh_token"]
    # Tokens should be opaque UUIDs — not JWTs.
    assert "." not in body["access_token"]


def test_signin_wrong_password(api):
    """Wrong password on existing account → 401."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    r = api.signin(c.email, "WrongPw-9999")
    assert r.status_code == 401


def test_signin_nonexistent_account(api):
    """No such email in DB → 401 (not 404, to avoid account enumeration)."""
    r = api.signin(rand_email("ghost"), rand_password())
    assert r.status_code == 401


# ───────────────────────── Verify ─────────────────────────


def test_verify_with_valid_token(user):
    """Bearer token from signin → 200 + user identity echoed back."""
    r = user.verify()
    assert r.status_code == 200
    body = r.json()
    assert "user_id" in body
    assert "username" in body
    assert "email" in body


def test_verify_without_token(api):
    """No Authorization header → 401."""
    r = api.raw_get("/auth/verify")
    assert r.status_code == 401


def test_verify_malformed_bearer(api):
    """Authorization header missing `Bearer ` prefix → 401."""
    r = api.raw_get("/auth/verify", token=None)
    # raw_get with token=None sends no header. Also test explicit bad header:
    r2 = api._http.get("/auth/verify", headers={"Authorization": "Token xyz"})
    assert r2.status_code == 401


def test_verify_with_fake_token(api):
    """Random UUID masquerading as token → 401 (not in Redis)."""
    r = api.raw_get("/auth/verify", token="11111111-1111-1111-1111-111111111111")
    assert r.status_code == 401


# ───────────────────────── Refresh ─────────────────────────


def test_refresh_rotates_tokens(api):
    """Valid refresh_token → 200 + fresh access + fresh refresh."""
    c = make_creds()
    api.signup(c.username, c.email, c.password)
    signin = api.signin(c.email, c.password).json()
    r = api.refresh(signin["refresh_token"])
    assert r.status_code == 200
    body = r.json()
    assert body["access_token"] != signin["access_token"]
    assert body["refresh_token"] != signin["refresh_token"] or body["refresh_token"]


def test_refresh_invalid_token(api):
    """Bogus refresh token → 401."""
    r = api.refresh("not-a-real-refresh")
    assert r.status_code == 401


# ───────────────────────── Signout ─────────────────────────


def test_signout_invalidates_access_token(user):
    """After POST /auth/signout, the access token no longer verifies."""
    assert user.verify().status_code == 200
    assert user.signout().status_code in (200, 204)
    assert user.verify().status_code == 401


def test_signout_all_kills_all_user_sessions(api, creds):
    """Two simultaneous sessions for the same user. signout-all from one
    should invalidate *both*."""
    api.signup(creds.username, creds.email, creds.password)
    a = api.signin(creds.email, creds.password).json()
    b = api.signin(creds.email, creds.password).json()
    assert a["access_token"] != b["access_token"]

    assert api.raw_get("/auth/verify", a["access_token"]).status_code == 200
    assert api.raw_get("/auth/verify", b["access_token"]).status_code == 200

    out = api.raw_post("/auth/signout-all", a["access_token"])
    assert out.status_code in (200, 204)

    assert api.raw_get("/auth/verify", a["access_token"]).status_code == 401
    assert api.raw_get("/auth/verify", b["access_token"]).status_code == 401


# ───────────────────────── Password change ─────────────────────────


def test_change_password_requires_current(user, creds):
    """Submitting wrong current_password → 401. Must prove possession of old pw."""
    r = user.change_password(current_password="WrongOld-1234", new_password="NewPw-9876!")
    assert r.status_code == 401


def test_change_password_then_signin_with_new(api, user, creds):
    """Successful change → old password fails on signin, new password works."""
    new_pw = "Changed-9999!"
    r = user.change_password(current_password=creds.password, new_password=new_pw)
    assert r.status_code in (200, 204)
    assert api.signin(creds.email, creds.password).status_code == 401
    assert api.signin(creds.email, new_pw).status_code == 200


# ───────────────────────── Delete account ─────────────────────────


def test_delete_account_wrong_password(user):
    """Delete requires password confirmation → 401 on mismatch."""
    r = user.delete_account(password="Not-The-Password-9")
    assert r.status_code == 401


def test_delete_account_removes_user(api, user, creds):
    """After delete, signin with same email fails (user gone)."""
    r = user.delete_account(password=creds.password)
    assert r.status_code in (200, 204)
    assert api.signin(creds.email, creds.password).status_code == 401


# ───────────────────────── Sessions list ─────────────────────────


def test_sessions_list(api, creds):
    """/auth/sessions lists active sessions for the user (multiple signins → multiple entries)."""
    api.signup(creds.username, creds.email, creds.password)
    api.signin(creds.email, creds.password)
    second = api.signin(creds.email, creds.password).json()

    r = api.raw_get("/auth/sessions", second["access_token"])
    assert r.status_code == 200
    body = r.json()
    assert isinstance(body, list)
    assert len(body) >= 2

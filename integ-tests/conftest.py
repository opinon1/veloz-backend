"""Session-wide fixtures.

Lifecycle:
    1. Load repo-root `.env` so APP_PORT / DB_* / REDIS_* are available.
    2. `docker compose up -d` (unless INTEG_NO_DOCKER=1).
    3. Poll the API for readiness.
    4. Yield to tests.
    5. `docker compose down` on session end (add -v via INTEG_RESET=1).

Per-test convenience fixtures:
    - `api`: session-scoped VelozClient bound to base_url
    - `user`: function-scoped fresh (signed-up + signed-in) AuthedClient
    - `admin`: function-scoped AuthedClient promoted to role='admin' via psql
"""
from __future__ import annotations

import os
from pathlib import Path

import pytest
from dotenv import load_dotenv

from helpers.api import AuthedClient, VelozClient
from helpers.compose import down, exec_sql, up, wait_for_api
from helpers.factory import UserCreds, make_creds

REPO_ROOT = Path(__file__).resolve().parent.parent


def pytest_configure(config: pytest.Config) -> None:
    """Load `.env` before any fixture runs so env vars are visible everywhere."""
    env_path = REPO_ROOT / ".env"
    if env_path.exists():
        load_dotenv(env_path)


# ────────────────────── Session fixtures ──────────────────────


@pytest.fixture(scope="session")
def base_url() -> str:
    port = os.environ.get("APP_PORT", "81")
    return f"http://localhost:{port}"


@pytest.fixture(scope="session", autouse=True)
def _docker_lifecycle(base_url: str):
    """Stand the stack up once per test session, tear down at the end."""
    up()
    wait_for_api(base_url)
    yield
    down()


@pytest.fixture(scope="session")
def api(base_url: str) -> VelozClient:
    """Unauthed HTTP client shared across all tests."""
    client = VelozClient(base_url)
    yield client
    client.close()


# ────────────────────── User factories ──────────────────────


def _signup_and_signin(api: VelozClient, creds: UserCreds) -> AuthedClient:
    r = api.signup(creds.username, creds.email, creds.password)
    assert r.status_code == 201, f"signup failed: {r.status_code} {r.text}"
    r = api.signin(creds.email, creds.password)
    assert r.status_code == 200, f"signin failed: {r.status_code} {r.text}"
    body = r.json()
    return AuthedClient(api, body["access_token"], body["refresh_token"])


@pytest.fixture
def creds() -> UserCreds:
    """A fresh UserCreds triple — unique per test to avoid UNIQUE collisions."""
    return make_creds()


@pytest.fixture
def user(api: VelozClient, creds: UserCreds) -> AuthedClient:
    """Function-scoped authed client. Every test that uses this gets a brand
    new account signed up + signed in."""
    return _signup_and_signin(api, creds)


@pytest.fixture
def user_factory(api: VelozClient):
    """Returns a callable producing additional authed users on demand.
    Useful for tests that need two users (e.g. leaderboard, role gating)."""
    def make() -> tuple[AuthedClient, UserCreds]:
        c = make_creds()
        return _signup_and_signin(api, c), c
    return make


# ────────────────────── Admin ──────────────────────


@pytest.fixture
def admin(api: VelozClient) -> AuthedClient:
    """Fresh authed user, promoted to role='admin' via in-container psql.
    Must re-signin to pick up the new role — no, actually: role is read fresh
    from DB per admin-gated request (see AdminClaims extractor), so the existing
    access token works immediately after the UPDATE."""
    creds = make_creds(prefix="admin")
    authed = _signup_and_signin(api, creds)
    # Escape single quotes in the email before interpolating into SQL.
    safe_email = creds.email.replace("'", "''")
    exec_sql(
        f"UPDATE users SET role='admin' WHERE email='{safe_email}'",
        db_name=os.environ["DB_NAME"],
        db_user=os.environ["DB_USER"],
        pg_port=os.environ["POSTGRES_PORT"],
    )
    return authed

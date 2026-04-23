"""Docker Compose lifecycle for the integration test session.

Environment variables consumed (from repo-root `.env` + shell):
    INTEG_NO_DOCKER=1   — skip docker up/down; assume stack already running
    INTEG_REBUILD=1     — pass --build to `docker compose up`
    INTEG_RESET=1       — pass -v to `docker compose down` (wipes volumes)

All other behavior uses repo-root `docker-compose.yml` unchanged.
"""
from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

import httpx

REPO_ROOT = Path(__file__).resolve().parent.parent.parent


def _compose(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    """Run `docker compose` rooted at the repo, inheriting the .env file."""
    return subprocess.run(
        ["docker", "compose", *args],
        cwd=REPO_ROOT,
        check=check,
        capture_output=True,
        text=True,
    )


def up() -> None:
    """Bring the stack up (idempotent). Respects INTEG_NO_DOCKER / INTEG_REBUILD."""
    if os.environ.get("INTEG_NO_DOCKER"):
        return
    args = ["up", "-d"]
    if os.environ.get("INTEG_REBUILD"):
        args.append("--build")
    _compose(*args)


def down() -> None:
    """Tear the stack down. Respects INTEG_NO_DOCKER / INTEG_RESET."""
    if os.environ.get("INTEG_NO_DOCKER"):
        return
    args = ["down"]
    if os.environ.get("INTEG_RESET"):
        args.append("-v")
    _compose(*args, check=False)


def wait_for_api(base_url: str, timeout_s: int = 90) -> None:
    """Poll the API until it responds. /auth/verify returning 401 = up + healthy."""
    deadline = time.time() + timeout_s
    last_err: Exception | None = None
    while time.time() < deadline:
        try:
            r = httpx.get(f"{base_url}/auth/verify", timeout=3)
            if r.status_code in (200, 401):
                return
        except httpx.RequestError as e:
            last_err = e
        time.sleep(1.5)
    raise TimeoutError(f"API at {base_url} not ready in {timeout_s}s (last: {last_err})")


def exec_sql(sql: str, db_name: str, db_user: str, pg_port: str) -> str:
    """Run a SQL statement inside the postgres container. Use for admin bootstrap."""
    if os.environ.get("INTEG_NO_DOCKER"):
        # Fallback: connect from host. Requires psycopg + exposed port.
        import psycopg

        with psycopg.connect(
            host="localhost",
            port=int(pg_port),
            dbname=db_name,
            user=db_user,
            password=os.environ["DB_PASSWORD"],
            autocommit=True,
        ) as conn, conn.cursor() as cur:
            cur.execute(sql)
            return ""

    result = _compose(
        "exec", "-T", "postgres",
        "psql", "-U", db_user, "-d", db_name, "-p", pg_port,
        "-v", "ON_ERROR_STOP=1",
        "-c", sql,
    )
    return result.stdout

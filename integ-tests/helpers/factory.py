"""Random-data factories. Every name/email/payload is unique-per-call so
tests can run repeatedly against the same DB without UNIQUE collisions."""
from __future__ import annotations

import secrets
import string
from dataclasses import dataclass


def rand_suffix(n: int = 8) -> str:
    """Short lowercase-alphanumeric suffix for names/emails."""
    alphabet = string.ascii_lowercase + string.digits
    return "".join(secrets.choice(alphabet) for _ in range(n))


def rand_username(prefix: str = "user") -> str:
    # Username regex allows [a-zA-Z0-9_] 3-30. Prefix + suffix fits comfortably.
    return f"{prefix}_{rand_suffix(10)}"


def rand_email(prefix: str = "user") -> str:
    return f"{prefix}_{rand_suffix(10)}@integ.test"


def rand_password() -> str:
    # Server requires: len 8-72, must contain a digit or special char.
    return f"Pw-{rand_suffix(12)}-9"


@dataclass
class UserCreds:
    username: str
    email: str
    password: str


def make_creds(prefix: str = "user") -> UserCreds:
    return UserCreds(rand_username(prefix), rand_email(prefix), rand_password())


def rand_character_name() -> str:
    return f"Char_{rand_suffix(8)}"


def admin_make_character(admin, **opts):
    """Create a character via the admin API, asserting success. Returns the row."""
    payload = {"name": rand_character_name(), **opts}
    r = admin.admin_create_character(**payload)
    assert r.status_code == 201, f"create_character failed: {r.status_code} {r.text}"
    return r.json()


def admin_make_skin(admin, character_id: str | None = None, **opts):
    """Create a skin via the admin API. Auto-creates a character if `character_id`
    is omitted so callers that don't care about the character can stay terse."""
    if character_id is None:
        character_id = admin_make_character(admin)["id"]
    payload = {"character_id": character_id, **opts}
    r = admin.admin_create_skin(**payload)
    assert r.status_code == 201, f"create_skin failed: {r.status_code} {r.text}"
    return r.json()


def rand_item_name(prefix: str = "Item") -> str:
    return f"{prefix}_{rand_suffix(6)}"


def rand_season_name() -> str:
    return f"Season_{rand_suffix(6)}"

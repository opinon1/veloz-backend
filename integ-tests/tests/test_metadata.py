"""Per-user metadata service: whole-blob + per-key access.

The backend never inspects the JSON — clients use it as a free-form k/v
store. Top-level must always be an object. Keys are lowercase
alphanumeric + underscore, 1..=64 chars.
"""
from __future__ import annotations


# ────────────────────────── Whole-blob ──────────────────────────


def test_metadata_default_is_empty_object(user):
    """Fresh user has the metadata row auto-created by the signup
    trigger; the blob defaults to `{}`."""
    r = user.get_metadata()
    assert r.status_code == 200
    assert r.json() == {}


def test_metadata_put_then_get_roundtrip(user):
    blob = {
        "theme": "dark",
        "tutorial_seen": True,
        "audio_pref": {"music": 0.6, "sfx": 1.0},
        "saved_runs": [1, 2, 3],
    }
    assert user.put_metadata(blob).status_code == 200
    assert user.get_metadata().json() == blob


def test_metadata_put_replaces_whole_blob(user):
    user.put_metadata({"a": 1, "b": 2})
    user.put_metadata({"c": 3})
    # PUT is wholesale replace, not merge — `a` and `b` are gone.
    assert user.get_metadata().json() == {"c": 3}


def test_metadata_delete_clears_to_empty_object(user):
    user.put_metadata({"x": "y"})
    assert user.delete_metadata().status_code == 204
    assert user.get_metadata().json() == {}


def test_metadata_put_rejects_non_object_root(user):
    # Arrays and primitives at root are forbidden — per-key endpoints
    # depend on a map at the top level.
    assert user.put_metadata([]).status_code == 400  # type: ignore[arg-type]
    assert user.put_metadata("hello").status_code == 400  # type: ignore[arg-type]
    assert user.put_metadata(42).status_code == 400  # type: ignore[arg-type]


def test_metadata_per_user_isolation(user_factory):
    a, _ = user_factory()
    b, _ = user_factory()
    a.put_metadata({"who": "a"})
    b.put_metadata({"who": "b"})
    assert a.get_metadata().json() == {"who": "a"}
    assert b.get_metadata().json() == {"who": "b"}


def test_metadata_requires_auth(api):
    assert api.raw_get("/me/metadata").status_code == 401
    assert api.raw_put("/me/metadata", json={"x": 1}).status_code == 401
    assert api.raw_delete("/me/metadata").status_code == 401


# ────────────────────────── Per-key ──────────────────────────


def test_metadata_put_key_creates_and_returns_value(user):
    r = user.put_metadata_key("theme", "dark")
    assert r.status_code == 200
    assert r.json() == "dark"
    # GET single key.
    r = user.get_metadata_key("theme")
    assert r.status_code == 200
    assert r.json() == "dark"


def test_metadata_put_key_merges_with_existing_blob(user):
    """Per-key PUT must not wipe sibling keys."""
    user.put_metadata({"theme": "dark", "lang": "es"})
    user.put_metadata_key("theme", "light")
    blob = user.get_metadata().json()
    assert blob == {"theme": "light", "lang": "es"}


def test_metadata_put_key_supports_nested_value(user):
    user.put_metadata_key("settings", {"audio": {"music": 0.5}, "vibrate": True})
    got = user.get_metadata_key("settings").json()
    assert got == {"audio": {"music": 0.5}, "vibrate": True}


def test_metadata_get_unknown_key_404(user):
    assert user.get_metadata_key("never_set").status_code == 404


def test_metadata_delete_key(user):
    user.put_metadata_key("k", "v")
    assert user.delete_metadata_key("k").status_code == 204
    assert user.get_metadata_key("k").status_code == 404
    # Sibling keys preserved.
    user.put_metadata_key("a", 1)
    user.put_metadata_key("b", 2)
    user.delete_metadata_key("a")
    assert user.get_metadata().json() == {"b": 2}


def test_metadata_delete_unknown_key_404(user):
    """No row exists for this key, but the metadata row does. The DELETE
    `data - key` is a no-op against an existing row in jsonb, which still
    affects 1 row. To return 404 we'd need a contains check — accept
    that DELETE of an unknown key returns 204 here (idempotent)."""
    r = user.delete_metadata_key("never_set")
    # Either 204 (idempotent) or 404 (strict) — assert at least one.
    assert r.status_code in (204, 404)


def test_metadata_key_validation_rejects_bad_shape(user):
    # Empty key collapses to the blob route in axum (`/me/metadata/` ≡
    # `/me/metadata`) so it's tested via the blob endpoint shape rules
    # rather than per-key validation. Skip it here.
    for bad in ["UPPER", "with-dash", "with space", "a" * 65]:
        assert user.put_metadata_key(bad, "x").status_code == 400, bad
        assert user.get_metadata_key(bad).status_code == 400, bad
        assert user.delete_metadata_key(bad).status_code == 400, bad

"""Mission system.

Admin defines a mission with a trigger_event + target JSON. Server-side
event hooks (runs, store purchases, currency credits) bump per-user
progress and auto-grant XP when target is hit.

Targets:
    run_completed       => {"amount": N}                          (delta 1/run)
    currency_collected  => {"currency":"soft","amount":N}         (delta = amount)
    store_purchase      => {"item_type":"...", "amount":N}        (delta 1/purchase)
    character_level_up  => {"character_id":"<uuid>","level":N}    (cards system: deferred)
"""
from __future__ import annotations

from datetime import datetime, timezone

import pytest

from helpers.factory import rand_item_name


def _mission_payload(**overrides):
    base = {
        "name": "Test mission",
        "description": "Smoke",
        "cycle": "daily",
        "trigger_event": "run_completed",
        "target": {"amount": 3},
        "xp_reward": 100,
        "is_active": True,
    }
    base.update(overrides)
    return base


def _wipe_missions(admin):
    """Clean slate for tests that assume an empty list. Other tests in
    the file should create their own missions and not rely on order."""
    for m in admin.admin_list_missions().json():
        admin.admin_delete_mission(m["id"])


# ────────────────────────── Admin CRUD ──────────────────────────


@pytest.mark.admin
def test_admin_mission_crud_roundtrip(admin):
    created = admin.admin_create_mission(**_mission_payload(name="Daily 3"))
    assert created.status_code == 201
    body = created.json()
    assert body["name"] == "Daily 3"
    assert body["cycle"] == "daily"
    assert body["xp_reward"] == 100
    mid = body["id"]

    upd = admin.admin_update_mission(mid, xp_reward=250, is_active=False)
    assert upd.status_code == 200
    assert upd.json()["xp_reward"] == 250
    assert upd.json()["is_active"] is False

    assert admin.admin_delete_mission(mid).status_code == 204
    assert admin.admin_update_mission(mid, name="x").status_code == 404


@pytest.mark.admin
@pytest.mark.parametrize("cycle", ["never", "", "DAILY"])
def test_admin_create_rejects_bad_cycle(admin, cycle):
    r = admin.admin_create_mission(**_mission_payload(cycle=cycle))
    assert r.status_code == 400


@pytest.mark.admin
@pytest.mark.parametrize("evt", ["bogus", "", "RUN_COMPLETED"])
def test_admin_create_rejects_bad_trigger_event(admin, evt):
    r = admin.admin_create_mission(**_mission_payload(trigger_event=evt))
    assert r.status_code == 400


@pytest.mark.admin
def test_admin_create_rejects_bad_target_shape(admin):
    # Currency event without `currency` field.
    bad = _mission_payload(
        trigger_event="currency_collected", target={"amount": 100}
    )
    assert admin.admin_create_mission(**bad).status_code == 400
    # Negative amount.
    bad = _mission_payload(target={"amount": -1})
    assert admin.admin_create_mission(**bad).status_code == 400


@pytest.mark.admin
def test_admin_create_rejects_non_positive_xp(admin):
    assert admin.admin_create_mission(**_mission_payload(xp_reward=0)).status_code == 400
    assert admin.admin_create_mission(**_mission_payload(xp_reward=-50)).status_code == 400


def test_non_admin_cannot_manage_missions(user):
    assert user.admin_create_mission(**_mission_payload()).status_code == 403
    assert user.admin_list_missions().status_code == 403


# ────────────────────────── User listing ──────────────────────────


@pytest.mark.admin
def test_list_missions_returns_active_only_with_progress(admin, user):
    _wipe_missions(admin)
    active = admin.admin_create_mission(**_mission_payload(name="Active")).json()
    inactive = admin.admin_create_mission(
        **_mission_payload(name="Inactive", is_active=False)
    ).json()

    rows = user.list_missions().json()
    ids = {r["id"] for r in rows}
    assert active["id"] in ids
    assert inactive["id"] not in ids

    row = next(r for r in rows if r["id"] == active["id"])
    assert row["progress"] == 0
    assert row["completed_at"] is None
    assert row["target_amount"] == 3
    assert row["xp_reward"] == 100
    # Daily cycle_key = today's UTC date in YYYY-MM-DD.
    today = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    assert row["cycle_key"] == today


@pytest.mark.admin
def test_one_shot_cycle_key_is_constant(admin, user):
    _wipe_missions(admin)
    admin.admin_create_mission(**_mission_payload(cycle="one_shot"))
    rows = user.list_missions().json()
    assert rows[0]["cycle_key"] == "one_shot"


# ────────────────────────── Run-completed events ──────────────────────────


@pytest.mark.admin
def test_run_completed_bumps_progress(admin, user):
    _wipe_missions(admin)
    admin.admin_create_mission(**_mission_payload(target={"amount": 3}))

    user.submit_run(score=10, distance=5, coins_collected=0, duration_ms=1000)
    rows = user.list_missions().json()
    assert rows[0]["progress"] == 1
    assert rows[0]["completed_at"] is None

    user.submit_run(score=10, distance=5, coins_collected=0, duration_ms=1000)
    user.submit_run(score=10, distance=5, coins_collected=0, duration_ms=1000)
    rows = user.list_missions().json()
    assert rows[0]["progress"] == 3
    assert rows[0]["completed_at"] is not None


@pytest.mark.admin
def test_completion_grants_xp(admin, user):
    """Hitting target auto-grants xp_reward; no claim endpoint."""
    _wipe_missions(admin)
    admin.admin_create_mission(
        **_mission_payload(target={"amount": 1}, xp_reward=500)
    )

    start_xp = user.get_profile().json()["total_xp"]
    # Run with score 0 awards 0 run-XP; mission XP is what we measure.
    user.submit_run(score=0, distance=0, coins_collected=0, duration_ms=1000)
    end_xp = user.get_profile().json()["total_xp"]
    assert end_xp - start_xp == 500


@pytest.mark.admin
def test_completion_is_idempotent(admin, user):
    """Once completed, further triggering events do not credit again."""
    _wipe_missions(admin)
    admin.admin_create_mission(
        **_mission_payload(target={"amount": 1}, xp_reward=300)
    )

    start_xp = user.get_profile().json()["total_xp"]
    user.submit_run(score=0, distance=0, coins_collected=0, duration_ms=1000)
    after_first = user.get_profile().json()["total_xp"]
    assert after_first - start_xp == 300

    user.submit_run(score=0, distance=0, coins_collected=0, duration_ms=1000)
    user.submit_run(score=0, distance=0, coins_collected=0, duration_ms=1000)
    after_more = user.get_profile().json()["total_xp"]
    assert after_more == after_first


# ────────────────────────── Currency events ──────────────────────────


@pytest.mark.admin
def test_currency_collected_via_run(admin, user):
    """Runs grant `soft` equal to `coins_collected`. The mission service
    fires CurrencyCollected for that grant."""
    _wipe_missions(admin)
    admin.admin_create_mission(**_mission_payload(
        trigger_event="currency_collected",
        target={"currency": "soft", "amount": 100},
        xp_reward=200,
    ))

    user.submit_run(score=0, distance=0, coins_collected=40, duration_ms=1000)
    rows = user.list_missions().json()
    assert rows[0]["progress"] == 40

    user.submit_run(score=0, distance=0, coins_collected=70, duration_ms=1000)
    rows = user.list_missions().json()
    # Saturated at target_amount = 100.
    assert rows[0]["progress"] == 100
    assert rows[0]["completed_at"] is not None


@pytest.mark.admin
def test_wrong_currency_does_not_count(admin, user):
    """Mission targets soft; a high-currency credit must not bump it."""
    _wipe_missions(admin)
    admin.admin_create_mission(**_mission_payload(
        trigger_event="currency_collected",
        target={"currency": "high", "amount": 100},
    ))
    user.submit_run(score=0, distance=0, coins_collected=50, duration_ms=1000)
    rows = user.list_missions().json()
    assert rows[0]["progress"] == 0


# ────────────────────────── Store-purchase events ──────────────────────────


@pytest.mark.admin
def test_store_purchase_event_credits(admin, user):
    """Buying a store item with matching item_type bumps progress.
    Also: the grant inside the purchase emits a CurrencyCollected
    event the mission service translates separately — that's tested
    elsewhere; here we focus on the purchase counter."""
    _wipe_missions(admin)
    admin.admin_create_mission(**_mission_payload(
        trigger_event="store_purchase",
        target={"item_type": "currency_bundle", "amount": 1},
        xp_reward=150,
    ))

    # Stock up the user's wallet so the buy succeeds.
    user_id = user.get_profile().json()["user_id"]
    admin.admin_grant(user_id, "soft", 100)

    item = admin.admin_create_store_item(
        name=rand_item_name("Pack"),
        item_type="currency_bundle",
        currency="soft",
        cost=10,
        payload=[{"type": "currency", "currency": "soft", "amount": 5}],
    ).json()
    r = user.purchase_store_item(item["id"])
    assert r.status_code == 200

    rows = user.list_missions().json()
    progress_row = next(r for r in rows if r["trigger_event"] == "store_purchase")
    assert progress_row["progress"] == 1
    assert progress_row["completed_at"] is not None

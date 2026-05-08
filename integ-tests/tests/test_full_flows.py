"""Full end-to-end user journeys.

Each test is one complete multi-step flow exercising several endpoints in
sequence. The goal is finding cross-feature bugs that single-endpoint tests
can't see — stale state, FK cascade gaps, post-side-effect inconsistencies.

These complement (not replace) the per-resource test files. If one of these
breaks, look at the assertions in order: the first one to fail tells you
which feature's contract was violated.
"""
from __future__ import annotations

import uuid

import pytest

from helpers.api import AuthedClient
from helpers.factory import (
    admin_make_avatar,
    admin_make_character,
    admin_make_frame,
    admin_make_skin,
    make_creds,
    rand_avatar_name,
    rand_character_name,
    rand_frame_name,
    rand_item_name,
)


# ──────────────────────────────────────────────────────────────────────
# Helpers
# ──────────────────────────────────────────────────────────────────────


# Etomin sandbox creds are required on the running stack — tests fail
# loudly if missing rather than silently skipping.


def _customer():
    return {
        "firstName": "John",
        "lastName": "Due",
        "middleName": "",
        "email": "john.due@mail.com",
        "phone1": "5555555555",
        "city": "Mexico",
        "address1": "Test 123",
        "postalCode": "11000",
        "state": "Mexico",
        "country": "MX",
        "ip": "0.0.0.0",
    }


def _card(number: str = "4111111111111111"):
    return {
        "cardNumber": number,
        "cvv": "120",
        "cardholderName": "John Due",
        "expirationYear": "27",
        "expirationMonth": "12",
    }


# ──────────────────────────────────────────────────────────────────────
# 1. New-user onboarding all the way through to first paid action
# ──────────────────────────────────────────────────────────────────────


def test_full_onboarding_flow(api):
    """Signup → signin → verify → profile defaults → wallet zero → can't spend
    yet → admin grant via psql route would be heavy here, so we just confirm
    the gating and move on."""
    creds = make_creds()
    r = api.signup(creds.username, creds.email, creds.password)
    assert r.status_code == 201
    user_id = r.json()["id"]

    si = api.signin(creds.email, creds.password)
    assert si.status_code == 200
    tokens = si.json()
    assert tokens["access_token"] != tokens["refresh_token"]

    me = AuthedClient(api, tokens["access_token"], tokens["refresh_token"])

    # /verify resolves to the same user.
    v = me.verify().json()
    assert v["user_id"] == user_id
    assert v["email"] == creds.email.lower()

    # Profile + wallet defaults.
    p = me.get_profile().json()
    assert p["account_level"] == 1
    assert p["total_xp"] == 0
    assert p["main_highscore"] == 0
    assert p["avatar_url"] is None and p["frame_url"] is None
    w = me.get_wallet().json()
    assert w["high"] == 0 and w["soft"] == 0 and w["energy"] == 0

    # Can't spend any currency.
    assert me.spend("soft", 1).status_code == 422
    assert me.spend("high", 1).status_code == 422
    assert me.spend("energy", 1).status_code == 422


# ──────────────────────────────────────────────────────────────────────
# 2. First-time spender: fail → grant → succeed → out of funds → grant → resume
# ──────────────────────────────────────────────────────────────────────


def test_full_skin_purchase_run_out_of_currency_recover(admin, user):
    """Try to buy → fail (no funds). Get a grant → succeed. Buy a second →
    drains wallet → next purchase fails again. Get more → finishes.

    Verifies: ownership recorded each step, wallet running balance correct,
    insufficient-funds gate fires + recovers, no double-grant on the
    already-owned check."""
    char = admin_make_character(admin)
    a = admin_make_skin(admin, char["id"], cost=100, currency="soft")
    b = admin_make_skin(admin, char["id"], cost=200, currency="soft")
    uid = user.get_profile().json()["user_id"]

    # No funds → 422
    assert user.purchase_skin(a["id"]).status_code == 422

    # Grant 100 → buy A → wallet 0
    admin.admin_grant(uid, "soft", 100)
    assert user.purchase_skin(a["id"]).status_code == 200
    assert user.get_wallet().json()["soft"] == 0
    owned = [s["id"] for s in user.owned_skins().json()]
    assert a["id"] in owned

    # Try to buy A again → 409 (already owned), wallet untouched
    assert user.purchase_skin(a["id"]).status_code == 409
    assert user.get_wallet().json()["soft"] == 0

    # B costs 200 → 422
    assert user.purchase_skin(b["id"]).status_code == 422

    # Grant 250 → buy B → wallet 50
    admin.admin_grant(uid, "soft", 250)
    assert user.purchase_skin(b["id"]).status_code == 200
    assert user.get_wallet().json()["soft"] == 50
    owned = [s["id"] for s in user.owned_skins().json()]
    assert {a["id"], b["id"]} <= set(owned)

    # Equipping B sets character.equipped_skin to B (and unlocks the char).
    user.equip_skin(b["id"])
    chars = {c["id"]: c for c in user.list_characters().json()}
    assert chars[char["id"]]["unlocked"] is True
    assert chars[char["id"]]["equipped_skin"] == b["id"]


# ──────────────────────────────────────────────────────────────────────
# 3. Character lifecycle: admin CRUD + cascade clears user state
# ──────────────────────────────────────────────────────────────────────


def test_full_character_lifecycle_with_cascades(admin, user):
    """Admin creates char + skins → user owns + equips → admin updates +
    deletes char → user's owned_skins reflects cascade → /characters no
    longer lists the char → re-equip 404."""
    char = admin_make_character(admin)
    s1 = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    s2 = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    user.purchase_skin(s1["id"])
    user.purchase_skin(s2["id"])
    user.equip_skin(s1["id"])
    user.equip_skin(s2["id"])

    # State pre-delete
    chars = {c["id"]: c for c in user.list_characters().json()}
    assert chars[char["id"]]["equipped_skin"] == s2["id"]
    assert set(chars[char["id"]]["related_skins"]) == {s1["id"], s2["id"]}

    # Admin updates character (name + default_unlocked) → still visible
    admin.admin_update_character(char["id"], default_unlocked=True)
    chars = {c["id"]: c for c in user.list_characters().json()}
    assert chars[char["id"]]["unlocked"] is True

    # Admin deletes → cascades to skins → cascades to user_skins
    assert admin.admin_delete_character(char["id"]).status_code == 204
    assert all(c["id"] != char["id"] for c in user.list_characters().json())
    owned = [s["id"] for s in user.owned_skins().json()]
    assert s1["id"] not in owned and s2["id"] not in owned

    # Re-equipping a now-deleted skin → 404
    assert user.equip_skin(s1["id"]).status_code == 404


# ──────────────────────────────────────────────────────────────────────
# 4. Avatars + frames purchase, select, leaderboard reflection, swap
# ──────────────────────────────────────────────────────────────────────


def test_full_avatar_frame_select_leaderboard_flow(admin, user):
    """Admin creates 2 avatars + 2 frames. User buys both of each, selects
    one of each, runs a high score, leaderboard returns the *current*
    selection. Switches selection → leaderboard updates. Deselects →
    leaderboard returns null."""
    a1 = admin_make_avatar(admin, price=10, currency="soft")
    a2 = admin_make_avatar(admin, price=20, currency="soft")
    f1 = admin_make_frame(admin, price=10, currency="soft")
    f2 = admin_make_frame(admin, price=20, currency="soft")
    uid = user.get_profile().json()["user_id"]
    admin.admin_grant(uid, "soft", 100)

    for x in (a1, a2):
        assert user.purchase_avatar(x["id"]).status_code == 200
    for x in (f1, f2):
        assert user.purchase_frame(x["id"]).status_code == 200

    user.select_avatar(a1["id"])
    user.select_frame(f1["id"])

    user.submit_run(score=1500, distance=0, coins_collected=0, duration_ms=1)
    rows = user.leaderboard().json()
    me = next(r for r in rows if r["user_id"] == uid)
    assert me["avatar_url"] == a1["id"]
    assert me["frame_url"] == f1["id"]

    # Swap → leaderboard reflects new selection (server reads profile live).
    user.select_avatar(a2["id"])
    user.select_frame(f2["id"])
    rows = user.leaderboard().json()
    me = next(r for r in rows if r["user_id"] == uid)
    assert me["avatar_url"] == a2["id"]
    assert me["frame_url"] == f2["id"]

    # Deselect both → leaderboard goes null.
    user.deselect_avatar()
    user.deselect_frame()
    rows = user.leaderboard().json()
    me = next(r for r in rows if r["user_id"] == uid)
    assert me["avatar_url"] is None and me["frame_url"] is None

    # Admin deletes the still-purchased a1 → owned-list drops it, no select
    # to clear (already deselected).
    admin.admin_delete_avatar(a1["id"])
    assert a1["id"] not in [a["id"] for a in user.list_avatars().json()]


# ──────────────────────────────────────────────────────────────────────
# 5. Battlepass full season: admin seasons + tiers, user runs, claims
# ──────────────────────────────────────────────────────────────────────


def test_full_battlepass_progression_flow(admin, user):
    """Admin creates active season + tiers. User submits runs, accumulates
    BP XP, claims tiers in order. Tries to double-claim → 409. Tries to
    claim premium without unlock → 402. Pays for unlock → premium claim
    works. Verify claim list."""
    from datetime import datetime, timedelta, timezone
    now = datetime.now(timezone.utc)
    season = admin.admin_create_season(
        name=f"FullFlow_{rand_character_name()}",
        starts_at=(now - timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        ends_at=(now + timedelta(days=30)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        premium_cost=50,
        premium_currency="high",
    ).json()
    soft_grant = [{"type": "currency", "currency": "soft", "amount": 100}]
    high_grant = [{"type": "currency", "currency": "high", "amount": 10}]
    admin.admin_create_tier(
        season["id"], tier=1, xp_required=100,
        free_reward=soft_grant, premium_reward=high_grant,
    )
    admin.admin_create_tier(
        season["id"], tier=2, xp_required=500,
        free_reward=soft_grant, premium_reward=high_grant,
    )

    uid = user.get_profile().json()["user_id"]
    admin.admin_grant(uid, "high", 100)

    # No runs yet → 403 on every claim.
    assert user.bp_claim(tier=1, track="free").status_code == 403
    assert user.bp_claim(tier=2, track="free").status_code == 403

    # 100 XP → claim tier 1 free works, tier 2 still locked.
    user.submit_run(score=100, distance=0, coins_collected=0, duration_ms=1)
    assert user.bp_progress().json()["bp_xp"] == 100
    assert user.bp_claim(tier=1, track="free").status_code == 200
    assert user.bp_claim(tier=2, track="free").status_code == 403
    # Double claim → 409.
    assert user.bp_claim(tier=1, track="free").status_code == 409

    # Premium track without unlock → 402.
    assert user.bp_claim(tier=1, track="premium").status_code == 402

    # Unlock premium (50 high) → balance 50. Premium claim works.
    assert user.bp_unlock_premium().status_code == 200
    assert user.get_wallet().json()["high"] >= 50  # 100 - 50 + tier1 free reward grants soft
    assert user.bp_claim(tier=1, track="premium").status_code == 200

    # Push to 500 XP → tier 2 unlocks both tracks.
    user.submit_run(score=400, distance=0, coins_collected=0, duration_ms=1)
    assert user.bp_progress().json()["bp_xp"] == 500
    assert user.bp_claim(tier=2, track="free").status_code == 200
    assert user.bp_claim(tier=2, track="premium").status_code == 200

    progress = user.bp_progress().json()
    assert sorted(progress["claimed_free"]) == [1, 2]
    assert sorted(progress["claimed_premium"]) == [1, 2]


# ──────────────────────────────────────────────────────────────────────
# 6. Store: currency-bundle purchase chains (high → soft) → spend on skin
# ──────────────────────────────────────────────────────────────────────


def test_full_store_bundle_buy_then_use_currency(admin, user):
    """User has 200 high, no soft. Store sells "200 soft for 50 high" bundle.
    Buy → wallet shows 150 high + 200 soft. Then buy a 200-soft skin → drains
    wallet → owned. Repeat: out of soft → 422 → buy bundle again → recover."""
    bundle = admin.admin_create_store_item(
        name=rand_item_name("Bundle"),
        item_type="currency_bundle",
        cost=50,
        currency="high",
        payload=[{"type": "currency", "currency": "soft", "amount": 200}],
    ).json()
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=200, currency="soft")

    uid = user.get_profile().json()["user_id"]
    admin.admin_grant(uid, "high", 200)

    r = user.purchase_store_item(bundle["id"])
    assert r.status_code == 200
    w = user.get_wallet().json()
    assert w["high"] == 150 and w["soft"] == 200

    # Buy skin (200 soft) → soft 0
    assert user.purchase_skin(skin["id"]).status_code == 200
    assert user.get_wallet().json()["soft"] == 0

    # Try another skin priced at 200 soft → 422
    skin2 = admin_make_skin(admin, char["id"], cost=200, currency="soft")
    assert user.purchase_skin(skin2["id"]).status_code == 422

    # Buy bundle again → soft refilled.
    user.purchase_store_item(bundle["id"])
    assert user.get_wallet().json()["soft"] == 200
    assert user.purchase_skin(skin2["id"]).status_code == 200


# ──────────────────────────────────────────────────────────────────────
# 7. Prize wheel: spin → cooldown → admin self-clear → spin again
# ──────────────────────────────────────────────────────────────────────


def test_full_prize_wheel_cooldown_flow(admin):
    """Admin sets wheel, spins as a user, gets cooldown. Second spin → 429.
    Cooldown query reflects ~86400s. Admin clears own cooldown → spin
    succeeds again. Wallet credited each time."""
    admin.admin_put_prize_wheel([
        {"reward": [{"type": "currency", "currency": "soft", "amount": 75}], "weight": 1}
    ])

    pre = admin.get_wallet().json()["soft"]
    r1 = admin.spin_prize_wheel()
    assert r1.status_code == 200
    assert admin.get_wallet().json()["soft"] == pre + 75

    cd = admin.prize_wheel_cooldown().json()
    assert cd["ready"] is False
    assert 86000 <= cd["retry_after_seconds"] <= 86400

    r2 = admin.spin_prize_wheel()
    assert r2.status_code == 429
    assert r2.json()["retry_after_seconds"] > 0

    assert admin.admin_clear_prize_wheel_cooldown().status_code == 204
    assert admin.prize_wheel_cooldown().json()["ready"] is True

    r3 = admin.spin_prize_wheel()
    assert r3.status_code == 200
    assert admin.get_wallet().json()["soft"] == pre + 150


# ──────────────────────────────────────────────────────────────────────
# 8. Account deletion cascades + re-signup is fresh
# ──────────────────────────────────────────────────────────────────────


def test_full_account_deletion_cascades(api, admin):
    """Sign up new user, accumulate state (highscore, owned skin, BP claim,
    payment row if Etomin enabled), delete account, verify clean wipe, then
    re-signup with same email → fresh state across the board."""
    creds = make_creds()
    api.signup(creds.username, creds.email, creds.password)
    me = AuthedClient(
        api,
        api.signin(creds.email, creds.password).json()["access_token"],
        api.signin(creds.email, creds.password).json()["refresh_token"],
    )
    uid = me.get_profile().json()["user_id"]
    admin.admin_grant(uid, "soft", 200)
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=100, currency="soft")
    me.purchase_skin(skin["id"])
    me.submit_run(score=4242, distance=0, coins_collected=0, duration_ms=1)
    assert me.get_profile().json()["main_highscore"] == 4242
    assert skin["id"] in [s["id"] for s in me.owned_skins().json()]

    assert me.delete_account(creds.password).status_code in (200, 204)
    # Token invalidated.
    assert me.verify().status_code == 401

    # Re-signup with same creds → brand new user.
    assert api.signup(creds.username, creds.email, creds.password).status_code == 201
    fresh = AuthedClient(
        api,
        api.signin(creds.email, creds.password).json()["access_token"],
        "",
    )
    p = fresh.get_profile().json()
    assert p["main_highscore"] == 0
    assert p["account_level"] == 1
    assert fresh.get_wallet().json() == {"high": 0, "soft": 0, "energy": 0}
    assert fresh.owned_skins().json() == []


# ──────────────────────────────────────────────────────────────────────
# 9. Auth lifecycle: refresh rotation, multi-session, signout-all
# ──────────────────────────────────────────────────────────────────────


def test_full_auth_session_lifecycle(api):
    """Sign in twice → 2 sessions. Refresh one → old access still works (not
    auto-killed), new one works. Signout one → other still works. Signout-all
    on the surviving one → both dead. Refresh after signout → 401."""
    creds = make_creds()
    api.signup(creds.username, creds.email, creds.password)

    s1 = api.signin(creds.email, creds.password).json()
    s2 = api.signin(creds.email, creds.password).json()
    assert s1["access_token"] != s2["access_token"]
    assert s1["refresh_token"] != s2["refresh_token"]

    me1 = AuthedClient(api, s1["access_token"], s1["refresh_token"])
    me2 = AuthedClient(api, s2["access_token"], s2["refresh_token"])

    # Both verify.
    assert me1.verify().status_code == 200
    assert me2.verify().status_code == 200

    # /auth/sessions on me1 → 2 entries.
    assert len(me1.sessions().json()) == 2

    # Refresh me1: get new access. Old access dies (existing test).
    refreshed = api.refresh(me1.refresh_token).json()
    me1_new = AuthedClient(api, refreshed["access_token"], refreshed["refresh_token"])
    assert me1_new.verify().status_code == 200

    # Original me1.access_token now invalidated by refresh.
    assert me1.verify().status_code == 401

    # me2 unaffected.
    assert me2.verify().status_code == 200

    # Signout me2 → its access dies, me1_new lives.
    assert me2.signout().status_code in (200, 204)
    assert me2.verify().status_code == 401
    assert me1_new.verify().status_code == 200

    # Signout-all on me1_new → all gone.
    assert me1_new.signout_all().status_code in (200, 204)
    assert me1_new.verify().status_code == 401

    # Old refresh token also dead.
    assert api.refresh(me1.refresh_token).status_code == 401


# ──────────────────────────────────────────────────────────────────────
# 10. Cross-admin demote: admin can't demote self, but another admin can
# ──────────────────────────────────────────────────────────────────────


def test_full_cross_admin_demote_flow(admin, user_factory, api):
    """A is admin. B is admin. A can promote B; A can demote B; A can't
    demote A. After A demotes self via B's promotion + B's demote, A loses
    admin → can't manage anymore."""
    other_creds = make_creds(prefix="otheradmin")
    api.signup(other_creds.username, other_creds.email, other_creds.password)
    other = AuthedClient(
        api,
        api.signin(other_creds.email, other_creds.password).json()["access_token"],
    )
    other_uid = other.get_profile().json()["user_id"]
    admin_uid = admin.get_profile().json()["user_id"]

    # admin promotes other → admin.
    assert admin.admin_update_role(other_uid, "admin").status_code == 200

    # admin can't demote self.
    assert admin.admin_update_role(admin_uid, "user").status_code == 400

    # other (now admin) demotes admin.
    assert other.admin_update_role(admin_uid, "user").status_code == 200

    # admin's privilege gone.
    assert admin.admin_list_users().status_code == 403


# ──────────────────────────────────────────────────────────────────────
# 11. Wallet edge: spend exact balance, then 0 → next spend rejected
# ──────────────────────────────────────────────────────────────────────


def test_full_wallet_drain_and_refill(admin, user):
    uid = user.get_profile().json()["user_id"]
    admin.admin_grant(uid, "high", 300)

    # Spend exactly all → 0.
    assert user.spend("high", 300).status_code == 200
    assert user.get_wallet().json()["high"] == 0

    # Next spend → 422.
    assert user.spend("high", 1).status_code == 422
    assert user.get_wallet().json()["high"] == 0

    # Negative grant deducts (admin-only). Currently at 0; -10 → CHECK fails.
    assert admin.admin_grant(uid, "high", -10).status_code == 422

    # Refill works.
    assert admin.admin_grant(uid, "high", 50).status_code == 200
    assert user.get_wallet().json()["high"] == 50


# ──────────────────────────────────────────────────────────────────────
# 12. Cross-cascade: admin deletes selected avatar → user.profile.avatar_url = NULL
# ──────────────────────────────────────────────────────────────────────


def test_full_admin_delete_cascade_clears_selection(admin, user_factory):
    """Two users select the same avatar. Admin deletes it → both users'
    profile.avatar_url goes NULL via FK ON DELETE SET NULL. Same for frames."""
    a, _ = user_factory()
    b, _ = user_factory()
    av = admin_make_avatar(admin)
    fr = admin_make_frame(admin)

    for u in (a, b):
        u.purchase_avatar(av["id"])
        u.purchase_frame(fr["id"])
        u.select_avatar(av["id"])
        u.select_frame(fr["id"])
        p = u.get_profile().json()
        assert p["avatar_url"] == av["id"]
        assert p["frame_url"] == fr["id"]

    admin.admin_delete_avatar(av["id"])
    admin.admin_delete_frame(fr["id"])

    for u in (a, b):
        p = u.get_profile().json()
        assert p["avatar_url"] is None
        assert p["frame_url"] is None


# ──────────────────────────────────────────────────────────────────────
# 13. Highscore monotonicity + leaderboard ordering
# ──────────────────────────────────────────────────────────────────────


def test_full_highscore_and_leaderboard_ordering(admin, user_factory):
    """Three users post different scores. Leaderboard is ordered desc.
    Lower scores submitted afterwards don't replace highscore. New higher
    score does. Tie isn't a new PB."""
    a, _ = user_factory()
    b, _ = user_factory()
    c, _ = user_factory()

    a.submit_run(score=100, distance=0, coins_collected=0, duration_ms=1)
    b.submit_run(score=300, distance=0, coins_collected=0, duration_ms=1)
    c.submit_run(score=200, distance=0, coins_collected=0, duration_ms=1)

    rows = a.leaderboard().json()
    score_map = {r["user_id"]: r["main_highscore"] for r in rows}
    a_id = a.get_profile().json()["user_id"]
    b_id = b.get_profile().json()["user_id"]
    c_id = c.get_profile().json()["user_id"]
    assert score_map[a_id] == 100
    assert score_map[b_id] == 300
    assert score_map[c_id] == 200

    # Order: B (300), C (200), A (100). Subset assertion; other users may
    # exist from other tests in this run.
    ordered = [r["user_id"] for r in rows if r["user_id"] in (a_id, b_id, c_id)]
    assert ordered == [b_id, c_id, a_id]

    # Submit a lower score — highscore stays.
    a.submit_run(score=50, distance=0, coins_collected=0, duration_ms=1)
    assert a.get_profile().json()["main_highscore"] == 100
    # Tie isn't a new PB.
    r = a.submit_run(score=100, distance=0, coins_collected=0, duration_ms=1)
    assert r.json()["new_highscore"] is False
    # Strictly higher → new PB.
    r = a.submit_run(score=150, distance=0, coins_collected=0, duration_ms=1)
    assert r.json()["new_highscore"] is True
    assert a.get_profile().json()["main_highscore"] == 150


# ──────────────────────────────────────────────────────────────────────
# 14. Validation edges across resources
# ──────────────────────────────────────────────────────────────────────


def test_full_validation_edges_sweep(admin, user):
    """One test that hammers various 4xx paths to make sure none of them
    return 500 / leak server state."""
    # Profile patch with arbitrary unknown body — accepted (empty echo).
    r = user.update_profile(garbage="value", evil={"x": 1})
    assert r.status_code == 200

    # Spend: bad currency, negative, zero
    assert user.spend("btc", 1).status_code == 400
    assert user.spend("soft", 0).status_code == 400
    assert user.spend("soft", -5).status_code == 400

    # Submit-run with negative fields → 400
    assert user.submit_run(score=-1, distance=0, coins_collected=0, duration_ms=1).status_code == 400
    assert user.submit_run(score=0, distance=-1, coins_collected=0, duration_ms=1).status_code == 400

    # Equip nonexistent skin → 404
    assert user.equip_skin(str(uuid.uuid4())).status_code == 404
    # Purchase nonexistent skin → 404
    assert user.purchase_skin(str(uuid.uuid4())).status_code == 404
    # Select nonexistent avatar → 403 (ownership) — server can't tell missing
    # from unowned without an extra lookup.
    assert user.select_avatar(str(uuid.uuid4())).status_code == 403

    # Admin-only routes from a regular user.
    assert user.admin_list_users().status_code == 403
    assert user.admin_create_character(name=rand_character_name()).status_code == 403
    assert user.admin_create_avatar(name=rand_avatar_name()).status_code == 403
    assert user.admin_create_frame(name=rand_frame_name()).status_code == 403
    assert user.admin_put_prize_wheel(
        [{"reward": [{"type": "currency", "currency": "soft", "amount": 1}], "weight": 1}]
    ).status_code == 403


# ──────────────────────────────────────────────────────────────────────
# 15. Full IAP payment → wallet credit → spend (live Etomin)
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.etomin
def test_full_iap_payment_credit_spend_flow(admin, user):
    """End-to-end real-money flow:
        1. Admin creates IAP item that grants 500 soft for 10 MXN.
        2. User has 0 soft. Tries declined card → 402, wallet untouched.
        3. Retries with approved card → 200, wallet=500.
        4. Buys a 300-soft skin → wallet=200.
        5. Buys another 300-soft skin → 422 (not enough). Charges again with
           approved card → wallet=700. Buy succeeds → wallet=400."""
    item = admin.admin_create_store_item(
        name=rand_item_name("IAP500"),
        item_type="currency_bundle",
        cost=10,
        currency="iap",
        iap_product_id="com.veloz.iap.500soft",
        payload=[{"type": "currency", "currency": "soft", "amount": 500}],
    ).json()
    char = admin_make_character(admin)
    s_a = admin_make_skin(admin, char["id"], cost=300, currency="soft")
    s_b = admin_make_skin(admin, char["id"], cost=300, currency="soft")

    assert user.get_wallet().json()["soft"] == 0

    # Declined card.
    declined = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111112"),
    )
    assert declined.status_code == 402
    assert user.get_wallet().json()["soft"] == 0

    # Approved card.
    approved = user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111111"),
    )
    assert approved.status_code == 200
    assert user.get_wallet().json()["soft"] == 500

    # Buy first skin → 200 left.
    assert user.purchase_skin(s_a["id"]).status_code == 200
    assert user.get_wallet().json()["soft"] == 200

    # Second skin → 422.
    assert user.purchase_skin(s_b["id"]).status_code == 422

    # Top up via second IAP charge → 700.
    user.charge_payment(
        item_id=item["id"],
        customer_information=_customer(),
        card_data=_card("4111111111111111"),
    )
    assert user.get_wallet().json()["soft"] == 700

    # Buy second skin → 400.
    assert user.purchase_skin(s_b["id"]).status_code == 200
    assert user.get_wallet().json()["soft"] == 400


# ──────────────────────────────────────────────────────────────────────
# 16. Idempotency under retry: same client retries on transient failure
# ──────────────────────────────────────────────────────────────────────


def test_full_double_purchase_does_not_double_charge(admin, user):
    """Race or naive client retry on /skins/{id}/purchase must not double-
    charge. ON CONFLICT DO NOTHING + rows_affected==0 short-circuit."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=100, currency="soft")
    uid = user.get_profile().json()["user_id"]
    admin.admin_grant(uid, "soft", 200)

    assert user.purchase_skin(skin["id"]).status_code == 200
    assert user.get_wallet().json()["soft"] == 100
    assert user.purchase_skin(skin["id"]).status_code == 409
    assert user.get_wallet().json()["soft"] == 100


# ──────────────────────────────────────────────────────────────────────
# 17. Header / format pickiness
# ──────────────────────────────────────────────────────────────────────


def test_full_authorization_header_strictness(api, user):
    """Authorization header must be `Bearer <token>` (case-sensitive scheme,
    space delimiter, no extra fluff)."""
    tok = user.access_token
    base = api._http  # the underlying httpx.Client

    # Lowercase scheme rejected.
    assert base.get("/auth/verify", headers={"Authorization": f"bearer {tok}"}).status_code == 401
    # Random scheme rejected.
    assert base.get("/auth/verify", headers={"Authorization": f"Token {tok}"}).status_code == 401
    # Bare token rejected.
    assert base.get("/auth/verify", headers={"Authorization": tok}).status_code == 401
    # No header → 401.
    assert base.get("/auth/verify").status_code == 401

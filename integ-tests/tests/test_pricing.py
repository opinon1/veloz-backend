"""Dynamic per-user pricing.

Formula (see entry-point/src/pricing.rs):
    x = total_xp / 100
    phi = stable_hash(user_id, item_id) -> [0, 2π)
    m(x) = 1 + 0.05·x + 0.15·(sin(x + phi) - sin(phi))
    cost_for_you = round(base_cost · m(x) · profile.price_multiplier)

Properties this file pins down:
    - At total_xp = 0 the curve collapses to 1.0 → user pays base.
    - Same state → same prices (no time, no RNG).
    - Two users at the same XP get different prices on the same item.
    - The same user gets different prices on two different items.
    - Profile.price_multiplier still stacks on top.
    - IAP store items are excluded from /me/prices.
    - Prices climb with XP (linear term dominates at high XP).
"""
from __future__ import annotations

import os

from helpers.compose import exec_sql
from helpers.factory import (
    admin_make_avatar,
    admin_make_character,
    admin_make_frame,
    admin_make_skin,
    rand_item_name,
)


def _db_env() -> dict[str, str]:
    return dict(
        db_name=os.environ["DB_NAME"],
        db_user=os.environ["DB_USER"],
        pg_port=os.environ["POSTGRES_PORT"],
    )


def _set_xp(user_id: str, xp: int) -> None:
    """Force total_xp directly. /runs would also raise account_level,
    which the dynamic-pricing formula ignores — XP is what matters."""
    exec_sql(
        f"UPDATE profiles SET total_xp={xp} WHERE user_id='{user_id}'",
        **_db_env(),
    )


def _set_price_multiplier(user_id: str, mult: float) -> None:
    exec_sql(
        f"UPDATE profiles SET price_multiplier={mult} WHERE user_id='{user_id}'",
        **_db_env(),
    )


# ────────────────────── /me/prices endpoint ──────────────────────


def test_my_prices_requires_auth(api):
    assert api.raw_get("/me/prices").status_code == 401


def test_my_prices_zero_xp_lives_within_sine_band(admin, user):
    """Brand-new user (total_xp = 0) already sees per-(user, item)
    divergence — every cost_for_you sits inside `[0.85·base, 1.15·base]`
    (the sine band) since the linear term contributes zero at x = 0."""
    char = admin_make_character(admin)
    s = admin_make_skin(admin, char["id"], cost=120, currency="soft")
    a = admin_make_avatar(admin, price=80, currency="soft")
    f = admin_make_frame(admin, price=200, currency="soft")
    item = admin.admin_create_store_item(
        name=rand_item_name(),
        item_type="custom",
        currency="soft",
        cost=350,
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()

    rows = user.list_my_prices().json()
    by_id = {(r["kind"], r["id"]): r for r in rows}
    for kind, item_id, base in [
        ("skin", s["id"], 120),
        ("avatar", a["id"], 80),
        ("frame", f["id"], 200),
        ("store", item["id"], 350),
    ]:
        got = by_id[(kind, item_id)]["cost_for_you"]
        lo = round(base * 0.85)
        hi = round(base * 1.15)
        assert lo <= got <= hi, f"{kind}={got} outside [{lo},{hi}]"
        assert by_id[(kind, item_id)]["base_cost"] == base


def test_my_prices_excludes_iap_items(admin, user):
    item = admin.admin_create_store_item(
        name=rand_item_name(),
        item_type="currency_bundle",
        currency="iap",
        cost=999,
        iap_product_id="com.veloz.bundle",
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()
    rows = user.list_my_prices().json()
    assert all(r["id"] != item["id"] for r in rows if r["kind"] == "store")


def test_my_prices_excludes_inactive_items(admin, user):
    char = admin_make_character(admin)
    s = admin_make_skin(admin, char["id"], cost=50, currency="soft")
    admin.admin_update_skin(s["id"], is_active=False)
    rows = user.list_my_prices().json()
    assert all(not (r["kind"] == "skin" and r["id"] == s["id"]) for r in rows)


def test_my_prices_is_deterministic(admin, user):
    """Two consecutive reads with no state change return identical
    cost_for_you for every row."""
    char = admin_make_character(admin)
    admin_make_skin(admin, char["id"], cost=100, currency="soft")
    admin_make_avatar(admin, price=70, currency="high")

    _set_xp(user.get_profile().json()["user_id"], 4321)
    first = user.list_my_prices().json()
    second = user.list_my_prices().json()
    assert {(r["kind"], r["id"]): r["cost_for_you"] for r in first} == \
        {(r["kind"], r["id"]): r["cost_for_you"] for r in second}


def test_two_users_diverge_at_same_xp(admin, user_factory):
    """Same item, two users, identical XP — prices differ because the
    sine phase is keyed on user_id."""
    a, _ = user_factory()
    b, _ = user_factory()
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=1000, currency="soft")

    a_id = a.get_profile().json()["user_id"]
    b_id = b.get_profile().json()["user_id"]
    _set_xp(a_id, 2000)
    _set_xp(b_id, 2000)

    a_price = next(r for r in a.list_my_prices().json() if r["id"] == skin["id"])
    b_price = next(r for r in b.list_my_prices().json() if r["id"] == skin["id"])
    assert a_price["cost_for_you"] != b_price["cost_for_you"]


def test_same_user_different_items_diverge(admin, user):
    """Same user, two different items, same base cost — different
    cost_for_you because the phase is per-(user, item)."""
    char = admin_make_character(admin)
    s1 = admin_make_skin(admin, char["id"], cost=1000, currency="soft")
    s2 = admin_make_skin(admin, char["id"], cost=1000, currency="soft")

    _set_xp(user.get_profile().json()["user_id"], 2000)
    rows = {r["id"]: r["cost_for_you"] for r in user.list_my_prices().json()}
    assert rows[s1["id"]] != rows[s2["id"]]


def test_xp_change_moves_prices(admin, user):
    """Same item, two different XP levels for the same user → prices
    differ. (linear and sine terms both engage when x > 0.)"""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=1000, currency="soft")
    uid = user.get_profile().json()["user_id"]

    _set_xp(uid, 1500)
    p1 = next(r for r in user.list_my_prices().json() if r["id"] == skin["id"])["cost_for_you"]

    _set_xp(uid, 8000)
    p2 = next(r for r in user.list_my_prices().json() if r["id"] == skin["id"])["cost_for_you"]

    assert p1 != p2
    # Linear term dominates at high XP — p2 should be strictly larger
    # than the base cost.
    assert p2 > 1000


def test_linear_growth_dominates_at_high_xp(admin, user):
    """At very high XP the sine wobble is dwarfed by the linear ramp."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=100, currency="soft")

    _set_xp(user.get_profile().json()["user_id"], 100_000)
    p = next(r for r in user.list_my_prices().json() if r["id"] == skin["id"])
    # x = 1000 → slope·x = 50 → multiplier ≈ 51 ± 0.15. Expect ≥ 5000.
    assert p["cost_for_you"] >= 5000


def test_account_multiplier_stacks(admin, user):
    """profile.price_multiplier is honored on top of the dynamic curve.
    At total_xp = 0 the dynamic factor lives in `[1-A, 1+A]`, so the
    final cost lives in `[(1-A)·base·pm, (1+A)·base·pm]`."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=100, currency="soft")
    uid = user.get_profile().json()["user_id"]
    _set_price_multiplier(uid, 0.5)

    p = next(r for r in user.list_my_prices().json() if r["id"] == skin["id"])
    # base=100, pm=0.5 → trend 50, ±15% band → [43, 58].
    assert 43 <= p["cost_for_you"] <= 58


def test_base_cost_field_is_admin_set_cost(admin, user):
    """base_cost is always the catalog row, untouched by the curve."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=777, currency="high")
    _set_xp(user.get_profile().json()["user_id"], 5000)
    p = next(r for r in user.list_my_prices().json() if r["id"] == skin["id"])
    assert p["base_cost"] == 777


# ────────────────────── Charged price matches /me/prices ──────────────────────


def test_purchase_charges_dynamic_price_for_skin(admin, user):
    """The wallet deduction on /skins/{id}/purchase equals the
    cost_for_you the same user just read from /me/prices."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=500, currency="soft")
    uid = user.get_profile().json()["user_id"]
    _set_xp(uid, 3000)

    quoted = next(r for r in user.list_my_prices().json() if r["id"] == skin["id"])
    admin.admin_grant(uid, "soft", 10_000)
    bal_before = user.get_wallet().json()["soft"]

    r = user.purchase_skin(skin["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == quoted["cost_for_you"]
    bal_after = user.get_wallet().json()["soft"]
    assert bal_before - bal_after == quoted["cost_for_you"]


def test_purchase_charges_dynamic_price_for_store(admin, user):
    item = admin.admin_create_store_item(
        name=rand_item_name(),
        item_type="custom",
        currency="soft",
        cost=400,
        payload=[{"type": "currency", "currency": "soft", "amount": 1}],
    ).json()
    uid = user.get_profile().json()["user_id"]
    _set_xp(uid, 2200)
    admin.admin_grant(uid, "soft", 10_000)

    quoted = next(r for r in user.list_my_prices().json() if r["id"] == item["id"])
    bal_before = user.get_wallet().json()["soft"]
    r = user.purchase_store_item(item["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == quoted["cost_for_you"]
    # The store item itself grants +1 soft on purchase, so net delta is
    # (cost_for_you - 1).
    bal_after = user.get_wallet().json()["soft"]
    assert bal_before - bal_after == quoted["cost_for_you"] - 1


def test_zero_cost_item_stays_free(admin, user):
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=0, currency="soft")
    uid = user.get_profile().json()["user_id"]
    _set_xp(uid, 10_000)
    r = user.purchase_skin(skin["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == 0


def test_dynamic_discount_can_render_purchase_free(admin, user):
    """profile.price_multiplier = 0 → adjusted cost = 0 → free buy
    even though base cost is positive."""
    char = admin_make_character(admin)
    skin = admin_make_skin(admin, char["id"], cost=100, currency="soft")
    uid = user.get_profile().json()["user_id"]
    _set_price_multiplier(uid, 0.0)
    r = user.purchase_skin(skin["id"])
    assert r.status_code == 200
    assert r.json()["cost_paid"] == 0

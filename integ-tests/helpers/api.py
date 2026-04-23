"""Thin HTTP client wrapper for the Veloz API.

Every method returns the raw `httpx.Response` so tests can assert both status
and body. A separate `AuthedClient` auto-injects the Bearer header.
"""
from __future__ import annotations

from typing import Any

import httpx


class VelozClient:
    def __init__(self, base_url: str) -> None:
        self._http = httpx.Client(base_url=base_url, timeout=10)

    # ── Auth ──
    def signup(self, username: str, email: str, password: str) -> httpx.Response:
        return self._http.post(
            "/auth/signup",
            json={"username": username, "email": email, "password": password},
        )

    def signin(self, email: str, password: str) -> httpx.Response:
        return self._http.post("/auth/signin", json={"email": email, "password": password})

    def refresh(self, refresh_token: str) -> httpx.Response:
        return self._http.post("/auth/refresh", json={"refresh_token": refresh_token})

    def raw_get(self, path: str, token: str | None = None, **kwargs: Any) -> httpx.Response:
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        return self._http.get(path, headers=headers, **kwargs)

    def raw_post(
        self, path: str, token: str | None = None, json: Any = None, **kwargs: Any
    ) -> httpx.Response:
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        return self._http.post(path, headers=headers, json=json, **kwargs)

    def raw_patch(
        self, path: str, token: str | None = None, json: Any = None, **kwargs: Any
    ) -> httpx.Response:
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        return self._http.patch(path, headers=headers, json=json, **kwargs)

    def raw_delete(
        self, path: str, token: str | None = None, json: Any = None, **kwargs: Any
    ) -> httpx.Response:
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        return self._http.request("DELETE", path, headers=headers, json=json, **kwargs)

    def close(self) -> None:
        self._http.close()


class AuthedClient:
    """Wraps a VelozClient + a token so tests don't thread the token manually.
    Exposes convenience methods for each feature group."""

    def __init__(self, client: VelozClient, access_token: str, refresh_token: str = "") -> None:
        self._c = client
        self.access_token = access_token
        self.refresh_token = refresh_token

    # ── Profile ──
    def get_profile(self) -> httpx.Response:
        return self._c.raw_get("/profile", self.access_token)

    def update_profile(self, **fields: Any) -> httpx.Response:
        return self._c.raw_patch("/profile", self.access_token, json=fields)

    # ── Wallet ──
    def get_wallet(self) -> httpx.Response:
        return self._c.raw_get("/wallet", self.access_token)

    def spend(self, currency: str, amount: int, reason: str | None = None) -> httpx.Response:
        body: dict[str, Any] = {"currency": currency, "amount": amount}
        if reason:
            body["reason"] = reason
        return self._c.raw_post("/wallet/spend", self.access_token, json=body)

    def iap_purchase(self, product_id: str, platform: str, receipt: str) -> httpx.Response:
        return self._c.raw_post(
            "/wallet/iap/purchase",
            self.access_token,
            json={"product_id": product_id, "platform": platform, "receipt": receipt},
        )

    def iap_validate(self, product_id: str, platform: str, receipt: str) -> httpx.Response:
        return self._c.raw_post(
            "/wallet/iap/validate",
            self.access_token,
            json={"product_id": product_id, "platform": platform, "receipt": receipt},
        )

    # ── Skins ──
    def list_skins(self) -> httpx.Response:
        return self._c.raw_get("/skins")

    def owned_skins(self) -> httpx.Response:
        return self._c.raw_get("/skins/owned", self.access_token)

    def purchase_skin(self, skin_id: str) -> httpx.Response:
        return self._c.raw_post(f"/skins/{skin_id}/purchase", self.access_token)

    def equip_skin(self, skin_id: str) -> httpx.Response:
        return self._c.raw_post(f"/skins/{skin_id}/equip", self.access_token)

    # ── Battlepass ──
    def bp_current(self) -> httpx.Response:
        return self._c.raw_get("/battlepass/current")

    def bp_progress(self) -> httpx.Response:
        return self._c.raw_get("/battlepass/progress", self.access_token)

    def bp_claim(self, tier: int, track: str) -> httpx.Response:
        return self._c.raw_post(
            f"/battlepass/claim/{tier}", self.access_token, json={"track": track}
        )

    def bp_unlock_premium(self) -> httpx.Response:
        return self._c.raw_post("/battlepass/unlock-premium", self.access_token)

    # ── Store ──
    def list_store(self) -> httpx.Response:
        return self._c.raw_get("/store")

    def purchase_store_item(self, item_id: str) -> httpx.Response:
        return self._c.raw_post(f"/store/{item_id}/purchase", self.access_token)

    # ── Runs ──
    def submit_run(
        self, score: int, distance: int, coins_collected: int, duration_ms: int
    ) -> httpx.Response:
        return self._c.raw_post(
            "/runs",
            self.access_token,
            json={
                "score": score,
                "distance": distance,
                "coins_collected": coins_collected,
                "duration_ms": duration_ms,
            },
        )

    def run_history(self, limit: int = 25) -> httpx.Response:
        return self._c.raw_get(f"/runs?limit={limit}", self.access_token)

    def leaderboard(self, limit: int = 50) -> httpx.Response:
        return self._c.raw_get(f"/runs/leaderboard?limit={limit}")

    # ── Admin ──
    def admin_create_skin(self, **fields: Any) -> httpx.Response:
        return self._c.raw_post("/admin/skins", self.access_token, json=fields)

    def admin_update_skin(self, skin_id: str, **fields: Any) -> httpx.Response:
        return self._c.raw_patch(f"/admin/skins/{skin_id}", self.access_token, json=fields)

    def admin_delete_skin(self, skin_id: str) -> httpx.Response:
        return self._c.raw_delete(f"/admin/skins/{skin_id}", self.access_token)

    def admin_list_skins(self) -> httpx.Response:
        return self._c.raw_get("/admin/skins", self.access_token)

    def admin_create_store_item(self, **fields: Any) -> httpx.Response:
        return self._c.raw_post("/admin/store", self.access_token, json=fields)

    def admin_update_store_item(self, item_id: str, **fields: Any) -> httpx.Response:
        return self._c.raw_patch(f"/admin/store/{item_id}", self.access_token, json=fields)

    def admin_delete_store_item(self, item_id: str) -> httpx.Response:
        return self._c.raw_delete(f"/admin/store/{item_id}", self.access_token)

    def admin_create_season(self, **fields: Any) -> httpx.Response:
        return self._c.raw_post("/admin/battlepass/seasons", self.access_token, json=fields)

    def admin_update_season(self, season_id: str, **fields: Any) -> httpx.Response:
        return self._c.raw_patch(
            f"/admin/battlepass/seasons/{season_id}", self.access_token, json=fields
        )

    def admin_delete_season(self, season_id: str) -> httpx.Response:
        return self._c.raw_delete(
            f"/admin/battlepass/seasons/{season_id}", self.access_token
        )

    def admin_create_tier(self, season_id: str, **fields: Any) -> httpx.Response:
        return self._c.raw_post(
            f"/admin/battlepass/seasons/{season_id}/tiers",
            self.access_token,
            json=fields,
        )

    def admin_update_tier(self, tier_id: str, **fields: Any) -> httpx.Response:
        return self._c.raw_patch(
            f"/admin/battlepass/tiers/{tier_id}", self.access_token, json=fields
        )

    def admin_delete_tier(self, tier_id: str) -> httpx.Response:
        return self._c.raw_delete(
            f"/admin/battlepass/tiers/{tier_id}", self.access_token
        )

    def admin_list_users(self, search: str | None = None) -> httpx.Response:
        q = f"?search={search}" if search else ""
        return self._c.raw_get(f"/admin/users{q}", self.access_token)

    def admin_update_role(self, user_id: str, role: str) -> httpx.Response:
        return self._c.raw_patch(
            f"/admin/users/{user_id}/role", self.access_token, json={"role": role}
        )

    def admin_grant(self, user_id: str, currency: str, amount: int, reason: str | None = None) -> httpx.Response:
        body: dict[str, Any] = {"currency": currency, "amount": amount}
        if reason:
            body["reason"] = reason
        return self._c.raw_post(
            f"/admin/users/{user_id}/grant", self.access_token, json=body
        )

    def admin_update_user_profile(self, user_id: str, **fields: Any) -> httpx.Response:
        return self._c.raw_patch(
            f"/admin/users/{user_id}/profile", self.access_token, json=fields
        )

    # ── Auth continued ──
    def verify(self) -> httpx.Response:
        return self._c.raw_get("/auth/verify", self.access_token)

    def signout(self) -> httpx.Response:
        return self._c.raw_post("/auth/signout", self.access_token)

    def signout_all(self) -> httpx.Response:
        return self._c.raw_post("/auth/signout-all", self.access_token)

    def change_password(self, current_password: str, new_password: str) -> httpx.Response:
        return self._c.raw_patch(
            "/auth/password",
            self.access_token,
            json={"current_password": current_password, "new_password": new_password},
        )

    def delete_account(self, password: str) -> httpx.Response:
        return self._c.raw_delete(
            "/auth/account", self.access_token, json={"password": password}
        )

    def sessions(self) -> httpx.Response:
        return self._c.raw_get("/auth/sessions", self.access_token)

use axum::routing::{get, patch, post};
use axum::Router;

use crate::state::AppState;
use super::{skins, store, battlepass, users};

pub fn router() -> Router<AppState> {
    Router::new()
        // Skins catalog
        .route("/skins", post(skins::create_skin).get(skins::list_all_skins))
        .route("/skins/{id}", patch(skins::update_skin).delete(skins::delete_skin))
        // Store catalog
        .route("/store", post(store::create_item).get(store::list_all_items))
        .route("/store/{id}", patch(store::update_item).delete(store::delete_item))
        // Battlepass seasons + tiers
        .route("/battlepass/seasons", post(battlepass::create_season).get(battlepass::list_seasons))
        .route("/battlepass/seasons/{id}", patch(battlepass::update_season).delete(battlepass::delete_season))
        .route("/battlepass/seasons/{id}/tiers", post(battlepass::create_tier).get(battlepass::list_tiers))
        .route("/battlepass/tiers/{id}", patch(battlepass::update_tier).delete(battlepass::delete_tier))
        // Users: role + currency grants
        .route("/users", get(users::list_users))
        .route("/users/{id}/role", patch(users::update_role))
        .route("/users/{id}/grant", post(users::grant_currency))
        .route("/users/{id}/profile", patch(users::update_profile))
}

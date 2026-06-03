use axum::Router;
use axum::routing::{delete, get, patch, post, put};

use super::{avatars, battlepass, characters, frames, missions, prize_wheel, skins, store, users};
use crate::handlers::signup_defaults;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Skins catalog
        .route(
            "/skins",
            post(skins::create_skin).get(skins::list_all_skins),
        )
        .route(
            "/skins/{id}",
            patch(skins::update_skin).delete(skins::delete_skin),
        )
        // Characters catalog
        .route(
            "/characters",
            post(characters::create_character).get(characters::list_all_characters),
        )
        .route(
            "/characters/{id}",
            patch(characters::update_character).delete(characters::delete_character),
        )
        // Avatars catalog
        .route(
            "/avatars",
            post(avatars::create_avatar).get(avatars::list_all_avatars),
        )
        .route(
            "/avatars/{id}",
            patch(avatars::update_avatar).delete(avatars::delete_avatar),
        )
        // Frames catalog
        .route(
            "/frames",
            post(frames::create_frame).get(frames::list_all_frames),
        )
        .route(
            "/frames/{id}",
            patch(frames::update_frame).delete(frames::delete_frame),
        )
        // Prize wheel
        .route(
            "/prize-wheel",
            put(prize_wheel::put_wheel)
                .get(prize_wheel::get_wheel)
                .delete(prize_wheel::delete_wheel),
        )
        .route(
            "/prize-wheel/cooldown",
            delete(prize_wheel::delete_self_cooldown),
        )
        // Store catalog
        .route(
            "/store",
            post(store::create_item).get(store::list_all_items),
        )
        .route(
            "/store/{id}",
            patch(store::update_item).delete(store::delete_item),
        )
        // Battlepass seasons + tiers
        .route(
            "/battlepass/seasons",
            post(battlepass::create_season).get(battlepass::list_seasons),
        )
        .route(
            "/battlepass/seasons/{id}",
            patch(battlepass::update_season).delete(battlepass::delete_season),
        )
        .route(
            "/battlepass/seasons/{id}/tiers",
            post(battlepass::create_tier).get(battlepass::list_tiers),
        )
        .route(
            "/battlepass/tiers/{id}",
            patch(battlepass::update_tier).delete(battlepass::delete_tier),
        )
        // Users: role + currency grants
        .route("/users", get(users::list_users))
        .route("/users/{id}/role", patch(users::update_role))
        .route("/users/{id}/grant", post(users::grant_currency))
        .route("/users/{id}/profile", patch(users::update_profile))
        // Missions
        .route(
            "/missions",
            post(missions::create_mission).get(missions::list_all_missions),
        )
        .route(
            "/missions/{id}",
            patch(missions::update_mission).delete(missions::delete_mission),
        )
        // Signup defaults backfill — reapplies every is_default catalog
        // row to every existing user (idempotent).
        .route(
            "/signup-defaults/backfill",
            post(signup_defaults::backfill::backfill),
        )
}

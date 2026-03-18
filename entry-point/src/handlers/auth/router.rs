use axum::routing::{delete, get, patch, post};
use axum::Router;

use crate::middleware::rate_limit::RateLimitLayer;
use crate::state::AppState;
use super::{signin, signup, verify, refresh, signout, signout_all, password, delete_account, sessions};

pub fn router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/signup", post(signup::signup)
            .layer(RateLimitLayer::new(state.redis.clone(), 5, 3600, "signup")))
        .route("/signin", post(signin::signin)
            .layer(RateLimitLayer::new(state.redis.clone(), 10, 60, "signin")))
        .route("/verify", get(verify::verify))
        .route("/refresh", post(refresh::refresh))
        .route("/signout", post(signout::signout))
        .route("/signout-all", post(signout_all::signout_all))
        .route("/password", patch(password::change_password))
        .route("/account", delete(delete_account::delete_account))
        .route("/sessions", get(sessions::sessions))
}

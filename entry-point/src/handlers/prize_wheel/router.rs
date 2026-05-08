use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{cooldown, spin, wheel};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(wheel::get_wheel))
        .route("/spin", post(spin::spin))
        .route("/cooldown", get(cooldown::get_cooldown))
}

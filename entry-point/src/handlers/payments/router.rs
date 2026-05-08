use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{charge, get as get_handler};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/charge", post(charge::charge))
        .route("/{id}", get(get_handler::get_payment))
}

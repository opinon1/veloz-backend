use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{get as get_handler, spend, iap};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_handler::get_wallet))
        .route("/spend", post(spend::spend))
        .route("/iap/purchase", post(iap::purchase))
        .route("/iap/validate", post(iap::validate))
}

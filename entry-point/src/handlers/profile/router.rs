use axum::routing::get;
use axum::Router;

use crate::state::AppState;
use super::{get as get_handler, update};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_handler::get_profile).patch(update::update_profile))
}

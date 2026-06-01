use axum::Router;
use axum::routing::get;

use super::list;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list::list_missions))
}

use axum::Router;
use axum::routing::get;

use crate::state::AppState;
use super::list;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list::list_my_prices))
}

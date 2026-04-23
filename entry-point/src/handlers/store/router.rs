use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{list, purchase};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list::list_items))
        .route("/{id}/purchase", post(purchase::purchase_item))
}

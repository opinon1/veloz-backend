use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{catalog, owned, purchase, select};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(owned::owned_frames))
        .route("/catalog", get(catalog::catalog_frames))
        .route("/deselect", post(select::deselect_frame))
        .route("/{id}/purchase", post(purchase::purchase_frame))
        .route("/{id}/select", post(select::select_frame))
}

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{catalog, owned, purchase, select};

pub fn router() -> Router<AppState> {
    Router::new()
        // GET /avatars            — owned list with is_selected (per spec)
        // GET /avatars/catalog    — every active avatar (browse-to-buy)
        // POST /avatars/{id}/purchase
        // POST /avatars/{id}/select
        // POST /avatars/deselect  — clear current selection
        .route("/", get(owned::owned_avatars))
        .route("/catalog", get(catalog::catalog_avatars))
        .route("/deselect", post(select::deselect_avatar))
        .route("/{id}/purchase", post(purchase::purchase_avatar))
        .route("/{id}/select", post(select::select_avatar))
}

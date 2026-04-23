use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;
use super::{list, owned, purchase, equip};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list::list_skins))
        .route("/owned", get(owned::owned_skins))
        .route("/{id}/purchase", post(purchase::purchase_skin))
        .route("/{id}/equip", post(equip::equip_skin))
}

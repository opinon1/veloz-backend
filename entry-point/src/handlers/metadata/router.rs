use axum::Router;
use axum::routing::get;

use super::{blob, by_key};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(blob::get_blob)
                .put(blob::put_blob)
                .delete(blob::delete_blob),
        )
        .route(
            "/{key}",
            get(by_key::get_key)
                .put(by_key::put_key)
                .delete(by_key::delete_key),
        )
}

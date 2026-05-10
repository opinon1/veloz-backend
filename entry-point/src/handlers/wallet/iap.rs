//! Placeholder IAP endpoints — currently disabled.
//!
//! Web card payments go through `/payments/charge` (Etomin). These two
//! endpoints exist for *future* native-mobile receipt validation
//! (Apple App Store / Google Play). Until that's implemented, both return
//! 501 so a misrouted frontend fails loud instead of getting a fake
//! "success" with no actual fulfillment.
use axum::{extract::State, http::StatusCode};
use crate::state::AppState;
use crate::extractors::Claims;

pub async fn purchase(
    State(_state): State<AppState>,
    Claims(_): Claims,
) -> Result<(), StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn validate(
    State(_state): State<AppState>,
    Claims(_): Claims,
) -> Result<(), StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

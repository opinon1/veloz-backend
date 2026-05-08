use redis::aio::ConnectionManager;
use sqlx::PgPool;

use crate::services::etomin::EtominClient;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    /// Optional: only present when ETOMIN_EMAIL/ETOMIN_PASSWORD are set.
    /// Endpoints that need it return 503 when missing.
    pub etomin: Option<EtominClient>,
}

use redis::aio::ConnectionManager;
use sqlx::PgPool;

use crate::services::etomin::EtominClient;
use crate::services::mailer::Mailer;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    /// Optional: only present when ETOMIN_EMAIL/ETOMIN_PASSWORD are set.
    /// Endpoints that need it return 503 when missing.
    pub etomin: Option<EtominClient>,
    /// Optional: only present when SQS_EMAIL_QUEUE_URL is set. When absent,
    /// email dispatch is a no-op (see `services::mailer::dispatch_to`).
    pub mailer: Option<Mailer>,
    /// Optional read-only pool for the stats dashboard. Reuses the app DB creds
    /// but runs each session as the NOLOGIN `veloz_stats` role (SET ROLE), forced
    /// `default_transaction_read_only = on` with a short statement_timeout.
    /// None (e.g. role missing) makes the chart/query endpoints return 503.
    pub stats_db: Option<PgPool>,
}

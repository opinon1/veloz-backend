mod extractors;
mod handlers;
mod leveling;
mod middleware;
mod models;
mod pricing;
mod services;
mod state;

use axum::{Router, routing::get};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use state::AppState;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Percent-encodes a string for safe use in a URL userinfo component.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "veloz=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_opts = PgConnectOptions::new()
        .host(&std::env::var("DB_HOST").expect("DB_HOST missing"))
        .port(
            std::env::var("DB_PORT")
                .expect("DB_PORT missing")
                .parse()
                .expect("DB_PORT must be a number"),
        )
        .username(&std::env::var("DB_USER").expect("DB_USER missing"))
        .password(&std::env::var("DB_PASSWORD").expect("DB_PASSWORD missing"))
        .database(&std::env::var("DB_NAME").expect("DB_NAME missing"));

    let redis_host = std::env::var("REDIS_HOST").expect("REDIS_HOST missing");
    let redis_port = std::env::var("REDIS_PORT").expect("REDIS_PORT missing");
    let redis_password =
        url_encode(&std::env::var("REDIS_PASSWORD").expect("REDIS_PASSWORD missing"));
    let redis_scheme = if std::env::var("REDIS_TLS").as_deref() == Ok("true") {
        "rediss"
    } else {
        "redis"
    };
    let redis_url = format!(
        "{}://:{}@{}:{}",
        redis_scheme, redis_password, redis_host, redis_port
    );

    tracing::info!("Connecting to Postgres...");
    let pool = PgPoolOptions::new().connect_with(db_opts).await?;
    tracing::info!("Connected to Postgres");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    tracing::info!("Connecting to Redis...");
    let redis_client = redis::Client::open(redis_url)?;
    let redis_manager = redis::aio::ConnectionManager::new(redis_client).await?;
    tracing::info!("Connected to Redis");

    let etomin = match services::etomin::EtominClient::from_env(redis_manager.clone()) {
        Ok(c) => {
            tracing::info!("Etomin client configured");
            Some(c)
        }
        Err(e) => {
            tracing::warn!("Etomin client not configured: {e}");
            None
        }
    };

    let mailer = match services::mailer::Mailer::from_env().await {
        Some(m) => {
            tracing::info!("Mailer configured (SQS + SES)");
            Some(m)
        }
        None => {
            tracing::warn!("Mailer not configured: SQS_EMAIL_QUEUE_URL unset");
            None
        }
    };

    let state = AppState {
        db: pool,
        redis: redis_manager,
        etomin,
        mailer,
    };

    // Email worker: long-polls the SQS queue and sends each job through SES.
    // poll_once blocks up to 20s waiting for messages, so this loop never
    // busy-spins.
    if let Some(mailer) = state.mailer.clone() {
        tokio::spawn(async move {
            loop {
                mailer.poll_once().await;
            }
        });
        tracing::info!("Email worker running (SQS long-poll)");
    }

    // Background payment reconciler. Etomin has no webhooks, so we poll
    // for status updates on PENDING rows. Catches "user closed the 3DS tab"
    // by converging to terminal state without any client involvement.
    if let Some(etomin_client) = state.etomin.clone() {
        let db = state.db.clone();
        let mailer = state.mailer.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            // Skip the immediate first tick.
            interval.tick().await;
            loop {
                interval.tick().await;
                handlers::payments::reconcile::sweep(&db, &etomin_client, mailer.as_ref(), 50).await;
            }
        });
        tracing::info!("Payment reconciler running (30s interval)");
    }

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/auth", handlers::auth::router::router(&state))
        .nest("/profile", handlers::profile::router::router())
        .nest("/wallet", handlers::wallet::router::router())
        .nest("/skins", handlers::skins::router::router())
        .nest("/characters", handlers::characters::router::router())
        .nest("/avatars", handlers::avatars::router::router())
        .nest("/frames", handlers::frames::router::router())
        .nest("/battlepass", handlers::battlepass::router::router())
        .nest("/store", handlers::store::router::router())
        .nest("/runs", handlers::runs::router::router())
        .nest("/prize-wheel", handlers::prize_wheel::router::router())
        .nest("/payments", handlers::payments::router::router())
        .nest("/missions", handlers::missions::router::router())
        .nest("/me/metadata", handlers::metadata::router::router())
        .nest("/me/prices", handlers::pricing::router::router())
        .nest("/admin", handlers::admin::router::router())
        // Permissive CORS: any origin, any method, any header. Auth tokens
        // travel as Bearer in the Authorization header (not cookies), so the
        // credentials-omitted policy is fine. Tighten allowed_origin to a
        // specific list before production hardening if needed.
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("APP_PORT").expect("APP_PORT missing");
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}

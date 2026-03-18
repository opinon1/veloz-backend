mod handlers;
mod middleware;
mod state;
mod extractors;
mod models;

use axum::Router;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use state::AppState;
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
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "veloz=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_opts = PgConnectOptions::new()
        .host(&std::env::var("DB_HOST").expect("DB_HOST missing"))
        .port(std::env::var("DB_PORT").expect("DB_PORT missing").parse().expect("DB_PORT must be a number"))
        .username(&std::env::var("DB_USER").expect("DB_USER missing"))
        .password(&std::env::var("DB_PASSWORD").expect("DB_PASSWORD missing"))
        .database(&std::env::var("DB_NAME").expect("DB_NAME missing"));

    let redis_host = std::env::var("REDIS_HOST").expect("REDIS_HOST missing");
    let redis_port = std::env::var("REDIS_PORT").expect("REDIS_PORT missing");
    let redis_password = url_encode(&std::env::var("REDIS_PASSWORD").expect("REDIS_PASSWORD missing"));
    let redis_url = format!("redis://:{}@{}:{}", redis_password, redis_host, redis_port);

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

    let state = AppState {
        db: pool,
        redis: redis_manager,
    };

    let app = Router::new()
        .nest("/auth", handlers::auth::router::router(&state))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("APP_PORT").expect("APP_PORT missing");
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await?;

    Ok(())
}

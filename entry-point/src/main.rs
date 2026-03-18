mod handlers;
mod middleware;
mod state;
mod extractors;
mod models;

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "veloz=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL missing");
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL missing");

    tracing::info!("Connecting to Postgres...");
    let pool = PgPoolOptions::new().connect(&db_url).await?;
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

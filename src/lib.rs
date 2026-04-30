mod config;
mod content_service;
mod database;
mod logging;

use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use config::DatabaseConfigResolver;
use content_service::ContentService;
use database::DatabaseBootstrapper;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::error::Error;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;

#[derive(Clone)]
pub struct AppState {
    content_service: ContentService,
}

impl AppState {
    fn new(pool: Arc<PgPool>) -> Self {
        Self {
            content_service: ContentService::new(pool),
        }
    }
}

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(handler_root))
        .route("/{*path}", get(handler_by_path))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(
                    tower_http::trace::DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(tower_http::LatencyUnit::Millis),
                ),
        )
        .with_state(state)
}

pub fn build_app_for_tests(database_url: &str) -> Router {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy(database_url)
        .expect("failed to create lazy pool for tests");

    build_app(AppState::new(Arc::new(pool)))
}

pub async fn run_server() -> Result<(), Box<dyn Error>> {
    let _log_guard = logging::RequestLogger::init()?;

    let connection_string = DatabaseConfigResolver::resolve_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await?;
    DatabaseBootstrapper::ensure_required_tables(&pool).await?;

    let app = build_app(AppState::new(Arc::new(pool)));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("🚀 Server running in http://{}", listener.local_addr()?);

    axum::serve(listener, app).await?;
    Ok(())
}

async fn handler_root(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    state.content_service.serve_path("/").await
}

async fn handler_by_path(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl axum::response::IntoResponse {
    let normalized = format!("/{}", path.trim_start_matches('/'));
    state.content_service.serve_path(&normalized).await
}
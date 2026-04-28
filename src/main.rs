use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Clone)]
struct AppState {
    pool: Arc<PgPool>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _log_guard = init_request_logging()?;

    let connection_string = resolve_database_url()?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await?;
    ensure_required_tables(&pool).await?;

    let state = AppState {
        pool: Arc::new(pool),
    };

    let app = Router::new()
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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        ?;

    println!("🚀 Server running in http://{}", listener.local_addr()?);

    axum::serve(listener, app).await?;
    Ok(())
}

fn init_request_logging() -> Result<WorkerGuard, Box<dyn Error>> {
    fs::create_dir_all("logs")?;

    let file_appender = tracing_appender::rolling::never("logs", "requests.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    Ok(guard)
}

async fn ensure_required_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
    for table_name in ["permissions", "paths", "files"] {
        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {} (id SERIAL PRIMARY KEY, content JSONB, created_at TIMESTAMP DEFAULT NOW())",
            table_name
        ))
        .execute(pool)
        .await?;
    }

    Ok(())
}

fn resolve_database_url() -> Result<String, Box<dyn Error>> {
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        if !database_url.trim().is_empty() {
            return Ok(database_url);
        }
    }

    let env_file_values = load_env_file("config/.env")?;

    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        if !database_url.trim().is_empty() {
            return Ok(database_url);
        }
    }

    let user = resolve_setting("DB_USER", &env_file_values, "postgres");
    let password = resolve_setting("DB_PASS", &env_file_values, "postgres");
    let host = resolve_setting("DB_HOST", &env_file_values, "localhost");
    let port = resolve_setting("DB_PORT", &env_file_values, "5432");
    let database = resolve_app_database_name(&env_file_values);

    Ok(format!(
        "postgres://{}:{}@{}:{}/{}",
        user, password, host, port, database
    ))
}

fn load_env_file(path: &str) -> Result<HashMap<String, String>, Box<dyn Error>> {
    let content = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(Box::new(error)),
    };

    let mut values = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(values)
}

fn resolve_setting(name: &str, file_values: &HashMap<String, String>, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            file_values
                .get(name)
                .cloned()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| fallback.to_string())
}

fn resolve_app_database_name(file_values: &HashMap<String, String>) -> String {
    let configured_database = resolve_setting("DB_NAME", file_values, "potagia");

    if configured_database == "postgres" {
        return "potagia".to_string();
    }

    configured_database
}

async fn handler_root(State(state): State<AppState>) -> impl IntoResponse {
    serve_path_from_db(&state.pool, "/").await
}

async fn handler_by_path(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let normalized = format!("/{}", path.trim_start_matches('/'));
    serve_path_from_db(&state.pool, &normalized).await
}

async fn serve_path_from_db(pool: &PgPool, requested_path: &str) -> Response {
    let mut record = Ok(None);

    for path_candidate in path_candidates(requested_path) {
        record = sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            SELECT
                f.content->>'content' AS file_content,
                f.content->>'filename' AS filename
            FROM files f
            JOIN paths p ON p.id = (f.content->>'path_id')::int
            WHERE p.content->>'path_string' = $1
            LIMIT 1
            "#,
        )
        .bind(path_candidate)
        .fetch_optional(pool)
        .await;

        if matches!(record, Ok(Some(_)) | Err(_)) {
            break;
        }
    }

    match record {
        Ok(Some((file_content, filename))) => {
            let mut headers = HeaderMap::new();
            let content_type = content_type_for_filename(filename.as_deref());
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
            (StatusCode::OK, headers, file_content).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "File not found for requested path").into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error while loading file: {error}"),
        )
            .into_response(),
    }
}

fn path_candidates(requested_path: &str) -> Vec<String> {
    let normalized = if requested_path.is_empty() {
        "/"
    } else {
        requested_path
    };

    if normalized == "/" || normalized == "/index.html" {
        return vec!["/index.html".to_string(), "/".to_string()];
    }

    let trimmed = normalized.trim_end_matches('/');
    let base = if trimmed.is_empty() { "/" } else { trimmed };

    let mut candidates = vec![format!("{base}/index.html"), base.to_string()];

    if normalized.ends_with('/') && base != "/" {
        candidates.push(format!("{base}/"));
    }

    candidates
}

fn content_type_for_filename(filename: Option<&str>) -> &'static str {
    match filename {
        Some(name) if name.ends_with(".html") => "text/html; charset=utf-8",
        Some(name) if name.ends_with(".css") => "text/css; charset=utf-8",
        Some(name) if name.ends_with(".js") => "application/javascript; charset=utf-8",
        Some(name) if name.ends_with(".json") => "application/json; charset=utf-8",
        Some(name) if name.ends_with(".svg") => "image/svg+xml",
        Some(name) if name.ends_with(".png") => "image/png",
        Some(name) if name.ends_with(".jpg") || name.ends_with(".jpeg") => "image/jpeg",
        _ => "text/plain; charset=utf-8",
    }
}

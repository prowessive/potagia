use axum::http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct ContentService {
    pool: Arc<PgPool>,
}

impl ContentService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn serve_path(&self, requested_path: &str) -> Response {
        let mut record = Ok(None);

        for path_candidate in Self::path_candidates(requested_path) {
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
            .fetch_optional(self.pool.as_ref())
            .await;

            if matches!(record, Ok(Some(_)) | Err(_)) {
                break;
            }
        }

        match record {
            Ok(Some((file_content, filename))) => {
                let mut headers = HeaderMap::new();
                let content_type = Self::content_type_for_filename(filename.as_deref());
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

    pub fn path_candidates(requested_path: &str) -> Vec<String> {
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

    pub fn content_type_for_filename(filename: Option<&str>) -> &'static str {
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
}

#[cfg(test)]
mod tests {
    use super::ContentService;

    #[test]
    fn path_candidates_for_root_and_index() {
        assert_eq!(ContentService::path_candidates("/"), vec!["/index.html", "/"]);
        assert_eq!(
            ContentService::path_candidates("/index.html"),
            vec!["/index.html", "/"]
        );
        assert_eq!(ContentService::path_candidates(""), vec!["/index.html", "/"]);
    }

    #[test]
    fn path_candidates_for_nested_paths() {
        assert_eq!(
            ContentService::path_candidates("/docs"),
            vec!["/docs/index.html", "/docs"]
        );
        assert_eq!(
            ContentService::path_candidates("/docs/"),
            vec!["/docs/index.html", "/docs", "/docs/"]
        );
    }

    #[test]
    fn content_type_resolution_for_known_and_unknown_extensions() {
        assert_eq!(
            ContentService::content_type_for_filename(Some("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            ContentService::content_type_for_filename(Some("styles.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            ContentService::content_type_for_filename(Some("script.js")),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(
            ContentService::content_type_for_filename(Some("icon.svg")),
            "image/svg+xml"
        );
        assert_eq!(
            ContentService::content_type_for_filename(Some("unknown.bin")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            ContentService::content_type_for_filename(None),
            "text/plain; charset=utf-8"
        );
    }
}
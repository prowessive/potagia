use crate::database::{DatabaseError, PostgresContentRepository, StoredFile};
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use std::future::Future;

/// Reads public files from application content storage.
pub trait FileRepository: Clone + Send + Sync + 'static {
    /// Finds the file associated with `path`.
    fn find_file(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<Option<StoredFile>, DatabaseError>> + Send;
}

impl FileRepository for PostgresContentRepository {
    fn find_file(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<Option<StoredFile>, DatabaseError>> + Send {
        self.find_file_by_path(path)
    }
}

/// Builds the application router with database-backed public files.
pub fn router<R>(repository: R) -> Router
where
    R: FileRepository,
{
    Router::new()
        .fallback(get(serve_file::<R>))
        .with_state(repository)
}

async fn serve_file<R>(State(repository): State<R>, uri: Uri) -> Response
where
    R: FileRepository,
{
    match repository.find_file(uri.path()).await {
        Ok(Some(file)) => file_response(file),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            eprintln!("Could not serve file content: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn file_response(file: StoredFile) -> Response {
    let content_type = media_type(file.name());
    let mut response = Body::from(file.into_content()).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(content_type),
    );
    response
}

fn media_type(file_name: &str) -> &'static str {
    if file_name.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if file_name.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if file_name.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use std::collections::HashMap;
    use tower::ServiceExt;

    #[derive(Clone, Default)]
    struct FakeRepository {
        files: HashMap<String, StoredFile>,
        fails: bool,
    }

    impl FileRepository for FakeRepository {
        fn find_file(
            &self,
            path: &str,
        ) -> impl Future<Output = Result<Option<StoredFile>, DatabaseError>> + Send {
            let result = if self.fails {
                Err(DatabaseError::InvalidMetadata(
                    "sensitive database details".to_owned(),
                ))
            } else {
                Ok(self.files.get(path).cloned())
            };
            async move { result }
        }
    }

    #[tokio::test]
    async fn root_returns_the_stored_index_html() {
        let response = request(initial_content_repository(), "/").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(
                "text/html; charset=utf-8"
            ))
        );
        let body = response_body(response).await;
        assert_eq!(body, include_bytes!("../magia/files/index.html").as_slice());
    }

    #[tokio::test]
    async fn direct_index_path_returns_the_stored_index_html() {
        let response = request(initial_content_repository(), "/index.html").await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_body(response).await;
        assert_eq!(body, include_bytes!("../magia/files/index.html").as_slice());
    }

    #[tokio::test]
    async fn stylesheet_is_served_with_its_media_type() {
        let response = request(initial_content_repository(), "/styles.css").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static("text/css; charset=utf-8"))
        );
        let body = response_body(response).await;
        assert_eq!(body, include_bytes!("../magia/files/styles.css").as_slice());
    }

    #[tokio::test]
    async fn favicon_is_served_with_its_media_type() {
        let response = request(initial_content_repository(), "/assets/favicon.svg").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static("image/svg+xml"))
        );
    }

    #[tokio::test]
    async fn unknown_paths_return_not_found() {
        let response = request(FakeRepository::default(), "/missing").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn repository_errors_return_a_generic_server_error() {
        let repository = FakeRepository {
            fails: true,
            ..FakeRepository::default()
        };

        let response = request(repository, "/").await;

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(response_body(response).await.is_empty());
    }

    async fn request(repository: FakeRepository, uri: &str) -> Response {
        let request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request should be valid");
        router(repository)
            .oneshot(request)
            .await
            .expect("route should respond")
    }

    async fn response_body(response: Response) -> Vec<u8> {
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable")
            .to_vec()
    }

    fn initial_content_repository() -> FakeRepository {
        let index = StoredFile::new(
            "index.html",
            include_bytes!("../magia/files/index.html").as_slice(),
        );
        let stylesheet = StoredFile::new(
            "styles.css",
            include_bytes!("../magia/files/styles.css").as_slice(),
        );
        let favicon = StoredFile::new(
            "favicon.svg",
            include_bytes!("../magia/files/assets/favicon.svg").as_slice(),
        );
        FakeRepository {
            files: HashMap::from([
                ("/".to_owned(), index.clone()),
                ("/index.html".to_owned(), index),
                ("/styles.css".to_owned(), stylesheet),
                ("/assets/favicon.svg".to_owned(), favicon),
            ]),
            fails: false,
        }
    }
}

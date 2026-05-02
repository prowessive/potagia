use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn admin_page_is_available() {
    let app = potagia::build_app_for_tests("postgres://invalid:invalid@127.0.0.1:1/invalid");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_crud_page_is_available() {
    let app = potagia::build_app_for_tests("postgres://invalid:invalid@127.0.0.1:1/invalid");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/crud/rbac/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn root_endpoint_returns_server_error_without_database() {
    let app = potagia::build_app_for_tests("postgres://invalid:invalid@127.0.0.1:1/invalid");

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn nested_endpoint_returns_server_error_without_database() {
    let app = potagia::build_app_for_tests("postgres://invalid:invalid@127.0.0.1:1/invalid");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/assets/styles.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn admin_databases_endpoint_returns_server_error_without_database() {
    let app = potagia::build_app_for_tests("postgres://invalid:invalid@127.0.0.1:1/invalid");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/admin/databases")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
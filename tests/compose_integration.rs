#![cfg(unix)]

mod support;

use anyhow::{Context, Result};
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, StatusCode};
use std::time::Duration;
use support::compose::ComposeStack;
use tokio::time::timeout;
use tokio_postgres::NoTls;

const DATABASE_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn compose_stack_serves_seeded_files_and_rejects_unknown_paths() -> Result<()> {
    let stack = ComposeStack::start()?;
    let client = http_client()?;

    assert_file_response(
        &client,
        &stack,
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../magia/files/index.html"),
    )
    .await?;
    assert_file_response(
        &client,
        &stack,
        "/index.html",
        "text/html; charset=utf-8",
        include_bytes!("../magia/files/index.html"),
    )
    .await?;
    assert_file_response(
        &client,
        &stack,
        "/styles.css",
        "text/css; charset=utf-8",
        include_bytes!("../magia/files/styles.css"),
    )
    .await?;
    assert_file_response(
        &client,
        &stack,
        "/assets/favicon.svg",
        "image/svg+xml",
        include_bytes!("../magia/files/assets/favicon.svg"),
    )
    .await?;

    let response = client
        .get(stack.application_url("/missing"))
        .send()
        .await
        .context("could not request an unknown path")?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.bytes().await?.is_empty());

    stack.tear_down()
}

#[tokio::test]
async fn application_restart_preserves_seed_rows_without_repeating_initialization() -> Result<()> {
    let mut stack = ComposeStack::start()?;

    let before_restart = initial_database_snapshot(&stack).await?;
    stack.restart_application().await?;
    let after_restart = initial_database_snapshot(&stack).await?;
    assert_eq!(after_restart, before_restart);

    assert_file_response(
        &http_client()?,
        &stack,
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../magia/files/index.html"),
    )
    .await?;

    stack.tear_down()
}

fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("could not create the HTTP test client")
}

async fn assert_file_response(
    client: &Client,
    stack: &ComposeStack,
    path: &str,
    expected_content_type: &str,
    expected_body: &[u8],
) -> Result<()> {
    let response = client
        .get(stack.application_url(path))
        .send()
        .await
        .with_context(|| format!("could not request '{path}'"))?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "unexpected status for '{path}'"
    );
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some(expected_content_type),
        "unexpected content type for '{path}'"
    );
    assert_eq!(
        response.bytes().await?.as_ref(),
        expected_body,
        "unexpected body for '{path}'"
    );
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct DatabaseSnapshot {
    schema_version: i32,
    schema_updated_at: String,
    files: Vec<FileRow>,
    paths: Vec<PathRow>,
}

#[derive(Debug, Eq, PartialEq)]
struct FileRow {
    id: i64,
    name: String,
    content: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct PathRow {
    id: i64,
    path: String,
    file_id: i64,
}

async fn initial_database_snapshot(stack: &ComposeStack) -> Result<DatabaseSnapshot> {
    let (client, connection) = timeout(DATABASE_TIMEOUT, stack.database_config().connect(NoTls))
        .await
        .context("timed out connecting to the disposable PostgreSQL database")?
        .context("could not connect to the disposable PostgreSQL database")?;
    let connection = tokio::spawn(connection);

    let metadata = timeout(
        DATABASE_TIMEOUT,
        client.query_one(
            "SELECT schema_version, updated_at::text
             FROM public._potagia_schema_metadata",
            &[],
        ),
    )
    .await
    .context("timed out reading schema metadata")?
    .context("could not read schema metadata")?;
    let file_rows = timeout(
        DATABASE_TIMEOUT,
        client.query(
            "SELECT id, name, content FROM public.files ORDER BY id",
            &[],
        ),
    )
    .await
    .context("timed out reading seeded files")?
    .context("could not read seeded files")?;
    let path_rows = timeout(
        DATABASE_TIMEOUT,
        client.query(
            "SELECT id, path, file_id FROM public.paths ORDER BY id",
            &[],
        ),
    )
    .await
    .context("timed out reading seeded paths")?
    .context("could not read seeded paths")?;

    let snapshot = DatabaseSnapshot {
        schema_version: metadata.get(0),
        schema_updated_at: metadata.get(1),
        files: file_rows
            .into_iter()
            .map(|row| FileRow {
                id: row.get(0),
                name: row.get(1),
                content: row.get(2),
            })
            .collect(),
        paths: path_rows
            .into_iter()
            .map(|row| PathRow {
                id: row.get(0),
                path: row.get(1),
                file_id: row.get(2),
            })
            .collect(),
    };
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.files.len(), 3);
    assert_eq!(snapshot.paths.len(), 4);

    drop(client);
    timeout(DATABASE_TIMEOUT, connection)
        .await
        .context("timed out closing the PostgreSQL connection")?
        .context("the PostgreSQL connection task panicked")?
        .context("the PostgreSQL connection failed")?;
    Ok(snapshot)
}

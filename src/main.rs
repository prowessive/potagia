use potagia::database::PostgresDatabase;
use std::error::Error;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Potagia failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let database = PostgresDatabase::from_env()?;
    let schema_version = database.ensure_schema().await?;
    let content = database.content_repository().await?;
    println!("Database schema version: {schema_version}");

    let address = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(address).await?;

    println!("Listening on http://{address}");
    axum::serve(listener, potagia::web::router(content)).await?;

    Ok(())
}

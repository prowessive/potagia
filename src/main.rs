use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    potagia::run_server().await
}

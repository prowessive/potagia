use std::error::Error;
use std::fs;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt};

pub struct RequestLogger;

impl RequestLogger {
    pub fn init() -> Result<WorkerGuard, Box<dyn Error>> {
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
}
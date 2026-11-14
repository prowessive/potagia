use anyhow::{Context, Result, anyhow, bail, ensure};
use reqwest::{Client, StatusCode};
use std::env;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::{NamedTempFile, TempDir};
use tokio::time::{interval, timeout};
use tokio_postgres::Config as PostgresConfig;
use wait_timeout::ChildExt;

const DATABASE_NAME: &str = "potagia_integration";
const DATABASE_USER: &str = "potagia_integration";
const DATABASE_PASSWORD: &str = "synthetic-integration-password";
const COMPOSE_STARTUP_TIMEOUT_SECONDS: &str = "120";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const BUILD_TIMEOUT: Duration = Duration::from_secs(300);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);
const REAP_TIMEOUT: Duration = Duration::from_secs(5);
const RESTART_TIMEOUT: Duration = Duration::from_secs(60);
const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const READINESS_REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

static NEXT_STACK_ID: AtomicU64 = AtomicU64::new(0);

/// A running, isolated Compose stack that is removed when the test finishes.
pub struct ComposeStack {
    project: ComposeProject,
    application_address: SocketAddr,
    database_address: SocketAddr,
}

impl ComposeStack {
    /// Builds and starts a clean stack, waiting for all service healthchecks.
    ///
    /// # Errors
    ///
    /// Returns an error when Docker Compose cannot build or start the stack,
    /// or when the loopback-published service addresses cannot be discovered.
    pub fn start() -> Result<Self> {
        let project = ComposeProject::new()?;
        if let Err(error) = project.start() {
            return Err(project.with_logs(error));
        }

        let application_address = project.published_address("app", 3000)?;
        let database_address = project.published_address("database", 5432)?;

        Ok(Self {
            project,
            application_address,
            database_address,
        })
    }

    /// Returns the loopback URL for an application path.
    pub fn application_url(&self, path: &str) -> String {
        format!("http://{}{path}", self.application_address)
    }

    /// Returns connection settings for this stack's disposable PostgreSQL database.
    pub fn database_config(&self) -> PostgresConfig {
        let host = self.database_address.ip().to_string();
        let mut config = PostgresConfig::new();
        config
            .host(&host)
            .port(self.database_address.port())
            .user(DATABASE_USER)
            .password(DATABASE_PASSWORD)
            .dbname(DATABASE_NAME);
        config
    }

    /// Restarts the application, rediscovers its ephemeral port, and polls readiness.
    ///
    /// # Errors
    ///
    /// Returns an error when the application cannot restart or return HTTP 200
    /// within the bounded readiness period.
    pub async fn restart_application(&mut self) -> Result<()> {
        self.project.run(&["restart", "app"])?;
        self.application_address = self.project.published_address("app", 3000)?;
        self.wait_until_application_ready().await
    }

    /// Removes all resources owned by this stack.
    ///
    /// Unlike the best-effort fallback during unwinding, an explicit teardown
    /// failure makes the calling test fail.
    ///
    /// # Errors
    ///
    /// Returns an error when project resources or the test image cannot be removed.
    pub fn tear_down(self) -> Result<()> {
        self.project.tear_down()
    }

    async fn wait_until_application_ready(&self) -> Result<()> {
        let client = Client::builder()
            .timeout(READINESS_REQUEST_TIMEOUT)
            .build()
            .context("could not create the readiness HTTP client")?;
        let url = self.application_url("/");
        let readiness = async {
            let mut interval = interval(READINESS_POLL_INTERVAL);
            loop {
                interval.tick().await;
                if let Ok(response) = client.get(&url).send().await
                    && response.status() == StatusCode::OK
                {
                    return;
                }
            }
        };

        if timeout(RESTART_TIMEOUT, readiness).await.is_err() {
            let error = anyhow!(
                "application at {url} did not return HTTP 200 within {} seconds after restart",
                RESTART_TIMEOUT.as_secs()
            );
            return Err(self.project.with_logs(error));
        }
        Ok(())
    }
}

struct ComposeProject {
    name: String,
    image_name: String,
    password_file_directory: TempDir,
    cleanup_state: CleanupState,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CleanupState {
    Active,
    Cleaned,
}

impl ComposeProject {
    fn new() -> Result<Self> {
        let password_file_directory =
            tempfile::tempdir().context("could not create a temporary Compose secret directory")?;
        let password_file = password_file_directory.path().join("postgres_password");
        fs::write(&password_file, DATABASE_PASSWORD)
            .context("could not write the synthetic PostgreSQL password")?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("the system clock is before the Unix epoch")?
            .as_nanos();
        let stack_id = NEXT_STACK_ID.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "potagia-integration-{}-{timestamp}-{stack_id}",
            std::process::id()
        );
        let image_name = format!("{name}:local");

        let project = Self {
            name,
            image_name,
            password_file_directory,
            cleanup_state: CleanupState::Active,
        };
        project.validate_configuration()?;
        Ok(project)
    }

    fn start(&self) -> Result<()> {
        self.run_with_timeout(
            &[
                "up",
                "--detach",
                "--build",
                "--wait",
                "--wait-timeout",
                COMPOSE_STARTUP_TIMEOUT_SECONDS,
            ],
            BUILD_TIMEOUT,
        )?;
        Ok(())
    }

    fn published_address(&self, service: &str, container_port: u16) -> Result<SocketAddr> {
        let container_port = container_port.to_string();
        let output = self.run(&["port", "--protocol", "tcp", service, &container_port])?;
        let output = String::from_utf8(output.stdout)
            .context("Docker Compose returned a non-UTF-8 published address")?;
        let address = output
            .lines()
            .next_back()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .context("Docker Compose did not return a published address")?;
        let address: SocketAddr = address
            .parse()
            .with_context(|| format!("invalid published address '{address}'"))?;
        ensure!(
            address.ip().is_loopback(),
            "published address {address} is not restricted to loopback"
        );
        Ok(address)
    }

    fn validate_configuration(&self) -> Result<()> {
        self.run(&["config", "--quiet"])?;
        Ok(())
    }

    fn command(&self) -> Command {
        let mut command = Command::new("docker");
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .arg("compose")
            .arg("--project-name")
            .arg(&self.name)
            .arg("--file")
            .arg("compose.yaml")
            .arg("--file")
            .arg("tests/compose.integration.yaml")
            .env("COMPOSE_DISABLE_ENV_FILE", "1")
            .env("POTAGIA_TEST_IMAGE", &self.image_name)
            .env("POSTGRES_DB", DATABASE_NAME)
            .env("POSTGRES_USER", DATABASE_USER)
            .env(
                "POSTGRES_PASSWORD_FILE",
                self.password_file_directory
                    .path()
                    .join("postgres_password"),
            );
        command
    }

    fn run(&self, arguments: &[&str]) -> Result<Output> {
        self.run_with_timeout(arguments, COMMAND_TIMEOUT)
    }

    fn run_with_timeout(&self, arguments: &[&str], deadline: Duration) -> Result<Output> {
        let mut command = self.command();
        command.args(arguments);
        let description = format!("docker compose {arguments:?} for project '{}'", self.name);
        run_bounded_command(command, &description, deadline)
    }

    fn with_logs(&self, error: anyhow::Error) -> anyhow::Error {
        match self.run(&["logs", "--no-color"]) {
            Ok(output) => error.context(format!(
                "Compose container logs:\n{}",
                String::from_utf8_lossy(&output.stdout)
            )),
            Err(logs_error) => error.context(format!(
                "Compose container logs could not be collected: {logs_error:#}"
            )),
        }
    }

    fn tear_down(mut self) -> Result<()> {
        self.clean_up()
    }

    fn clean_up(&mut self) -> Result<()> {
        self.cleanup_state = CleanupState::Cleaned;
        let project_result = self.run_with_timeout(
            &["down", "--volumes", "--remove-orphans", "--timeout", "10"],
            CLEANUP_TIMEOUT,
        );
        let image_result = self.remove_image();

        match (project_result, image_result) {
            (Ok(_), Ok(_)) => Ok(()),
            (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
            (Err(project_error), Err(image_error)) => {
                Err(project_error
                    .context(format!("test image cleanup also failed: {image_error:#}")))
            }
        }
    }

    fn remove_image(&self) -> Result<Output> {
        let mut command = Command::new("docker");
        command.args(["image", "rm"]).arg(&self.image_name);
        let description = format!("docker image rm for test image '{}'", self.image_name);
        run_bounded_command(command, &description, CLEANUP_TIMEOUT)
    }
}

impl Drop for ComposeProject {
    fn drop(&mut self) {
        if self.cleanup_state == CleanupState::Cleaned {
            return;
        }

        if let Err(error) = self.clean_up() {
            eprintln!(
                "could not clean up Compose project '{}': {error:#}",
                self.name
            );
        }
    }
}

fn run_bounded_command(
    mut command: Command,
    description: &str,
    deadline: Duration,
) -> Result<Output> {
    let stdout = NamedTempFile::new().context("could not create the stdout capture file")?;
    let stderr = NamedTempFile::new().context("could not create the stderr capture file")?;
    command.process_group(0);
    command.stdout(Stdio::from(
        stdout
            .reopen()
            .context("could not reopen the stdout capture file")?,
    ));
    command.stderr(Stdio::from(
        stderr
            .reopen()
            .context("could not reopen the stderr capture file")?,
    ));

    let child = command
        .spawn()
        .with_context(|| format!("could not execute {description}"))?;
    let mut child = ChildGuard::new(child);
    let status = match child.wait_timeout(deadline) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let error = anyhow!("{description} exceeded {} seconds", deadline.as_secs());
            return Err(command_error_with_output(
                error, &mut child, &stdout, &stderr,
            ));
        }
        Err(error) => {
            let error = anyhow!(error).context(format!("could not wait for {description}"));
            return Err(command_error_with_output(
                error, &mut child, &stdout, &stderr,
            ));
        }
    };
    let output = captured_output(status, &stdout, &stderr)?;

    if output.status.success() {
        return Ok(output);
    }

    bail!(
        "{description} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

struct ChildGuard {
    child: Child,
    state: ChildState,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ChildState {
    Running,
    Reaped,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self {
            child,
            state: ChildState::Running,
        }
    }

    fn wait_timeout(&mut self, deadline: Duration) -> io::Result<Option<std::process::ExitStatus>> {
        let status = self.child.wait_timeout(deadline)?;
        if status.is_some() {
            self.state = ChildState::Reaped;
        }
        Ok(status)
    }

    fn kill_and_reap(&mut self) -> Result<()> {
        let termination_result = self.terminate_process_group();
        let reap_result = match self.child.wait_timeout(REAP_TIMEOUT) {
            Ok(Some(_)) => {
                self.state = ChildState::Reaped;
                Ok(())
            }
            Ok(None) => Err(anyhow!(
                "command process did not exit within {} seconds after termination",
                REAP_TIMEOUT.as_secs()
            )),
            Err(error) => Err(anyhow!(error).context("could not reap the command process")),
        };

        match (termination_result, reap_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(termination_error), Err(reap_error)) => {
                Err(termination_error
                    .context(format!("command reaping also failed: {reap_error:#}")))
            }
        }
    }

    fn terminate_process_group(&mut self) -> Result<()> {
        let process_id = self.child.id();
        match kill_process_group(process_id) {
            Ok(()) => Ok(()),
            Err(group_error) => match self.child.kill() {
                Ok(()) => Err(anyhow!(group_error).context(format!(
                    "could not terminate process group {process_id}; only its direct child was terminated"
                ))),
                Err(child_error) => Err(anyhow!(group_error).context(format!(
                    "could not terminate process group {process_id}; direct child termination also failed: {child_error}"
                ))),
            },
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.state == ChildState::Running
            && let Err(error) = self.kill_and_reap()
        {
            eprintln!(
                "could not terminate and reap command process group '{}': {error:#}",
                self.child.id()
            );
        }
    }
}

fn kill_process_group(process_id: u32) -> io::Result<()> {
    let process_id = i32::try_from(process_id).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "process ID exceeds the supported range",
        )
    })?;
    // SAFETY: the child starts with its own process group, so the negative PID
    // signals only that group and does not affect the test runner's group.
    let result = unsafe { libc::kill(-process_id, libc::SIGKILL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn command_error_with_output(
    error: anyhow::Error,
    child: &mut ChildGuard,
    stdout: &NamedTempFile,
    stderr: &NamedTempFile,
) -> anyhow::Error {
    let error = match child.kill_and_reap() {
        Ok(()) => error,
        Err(cleanup_error) => error.context(format!(
            "command termination or reaping also failed: {cleanup_error:#}"
        )),
    };
    let stdout = fs::read(stdout.path());
    let stderr = fs::read(stderr.path());

    match (stdout, stderr) {
        (Ok(stdout), Ok(stderr)) => error.context(format!(
            "command output:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        )),
        (Err(read_error), _) => {
            error.context(format!("could not read captured stdout: {read_error}"))
        }
        (_, Err(read_error)) => {
            error.context(format!("could not read captured stderr: {read_error}"))
        }
    }
}

fn captured_output(
    status: std::process::ExitStatus,
    stdout: &NamedTempFile,
    stderr: &NamedTempFile,
) -> Result<Output> {
    Ok(Output {
        status,
        stdout: fs::read(stdout.path()).context("could not read captured stdout")?,
        stderr: fs::read(stderr.path()).context("could not read captured stderr")?,
    })
}

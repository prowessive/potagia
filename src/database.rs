mod schema;
mod seed;

pub use schema::SchemaError;

use schema::{ColumnState, ConstraintState, DatabaseState, EMBEDDED_SCHEMA, Identifier, Schema};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs;
use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_postgres::error::SqlState;
use tokio_postgres::{Client, Config, NoTls, Transaction};

const DEFAULT_DATABASE: &str = "potagia";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_MAINTENANCE_DATABASE: &str = "postgres";
const DEFAULT_PORT: u16 = 5432;
const DEFAULT_USER: &str = "potagia";
const DEFAULT_PASSWORD_FILE: &str = ".secrets/postgres_password";
const ADVISORY_LOCK_ID: i64 = 3_172_753_927_482_914;

/// PostgreSQL connection settings and embedded schema management.
pub struct PostgresDatabase {
    config: Config,
    database_name: Identifier,
    maintenance_database: String,
}

/// A file retrieved from PostgreSQL content storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFile {
    name: String,
    content: Vec<u8>,
}

impl StoredFile {
    /// Creates a stored file with a name and binary content.
    pub fn new(name: impl Into<String>, content: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            content: content.into(),
        }
    }

    /// Returns the file name used to determine its media type.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Consumes the file and returns its binary content.
    pub fn into_content(self) -> Vec<u8> {
        self.content
    }
}

/// A cloneable PostgreSQL-backed content repository.
#[derive(Clone)]
pub struct PostgresContentRepository {
    client: Arc<Client>,
}

impl PostgresContentRepository {
    /// Finds a file by its public path.
    ///
    /// # Errors
    ///
    /// Returns an error when the content query cannot be completed.
    pub async fn find_file_by_path(&self, path: &str) -> Result<Option<StoredFile>, DatabaseError> {
        let row = self
            .client
            .query_opt(
                "SELECT files.name, files.content
                 FROM public.paths
                 JOIN public.files ON files.id = paths.file_id
                 WHERE paths.path = $1
                 ORDER BY paths.id
                 LIMIT 1",
                &[&path],
            )
            .await
            .map_err(|source| query_error("load file content", source))?;

        Ok(row.map(|row| StoredFile {
            name: row.get("name"),
            content: row.get("content"),
        }))
    }
}

impl PostgresDatabase {
    /// Loads PostgreSQL settings from `POSTGRES_*` environment variables.
    ///
    /// The password is read from `POSTGRES_PASSWORD_FILE`, which defaults to
    /// `.secrets/postgres_password`.
    ///
    /// # Errors
    ///
    /// Returns an error when the password file cannot be read, the password is
    /// empty, or the configured port or database name is invalid.
    pub fn from_env() -> Result<Self, DatabaseError> {
        let password_path = env::var_os("POSTGRES_PASSWORD_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_PASSWORD_FILE));
        let password = read_password(&password_path)?;
        Self::from_lookup(|name| env::var(name).ok(), password)
    }

    fn from_lookup(
        lookup: impl Fn(&str) -> Option<String>,
        password: String,
    ) -> Result<Self, DatabaseError> {
        let host = value_or_default(&lookup, "POSTGRES_HOST", DEFAULT_HOST);
        let user = value_or_default(&lookup, "POSTGRES_USER", DEFAULT_USER);
        let maintenance_database = value_or_default(
            &lookup,
            "POSTGRES_MAINTENANCE_DB",
            DEFAULT_MAINTENANCE_DATABASE,
        );
        if host.is_empty() || user.is_empty() || maintenance_database.is_empty() {
            return Err(DatabaseError::InvalidConfiguration(
                "PostgreSQL host, user, and maintenance database cannot be empty".to_owned(),
            ));
        }

        let port = match lookup("POSTGRES_PORT") {
            Some(value) => value
                .parse()
                .map_err(|source| DatabaseError::InvalidPort { value, source })?,
            None => DEFAULT_PORT,
        };
        let database_name = value_or_default(&lookup, "POSTGRES_DB", DEFAULT_DATABASE);
        let database_name = Identifier::new(database_name).map_err(|_| {
            DatabaseError::InvalidConfiguration(
                "POSTGRES_DB must be a lowercase PostgreSQL identifier of at most 63 bytes"
                    .to_owned(),
            )
        })?;

        let mut config = Config::new();
        config
            .host(&host)
            .port(port)
            .user(&user)
            .password(password)
            .dbname(database_name.as_str());

        Ok(Self {
            config,
            database_name,
            maintenance_database,
        })
    }

    /// Creates the database when absent and synchronizes its schema without
    /// destructive changes.
    ///
    /// The schema, initial content, and version are initialized atomically.
    /// Existing unmanaged databases, incompatible definitions, and downgrades
    /// are rejected. Initial content is never reinserted on later startups.
    ///
    /// # Errors
    ///
    /// Returns an error when PostgreSQL is unavailable, database creation or
    /// migration fails, or the existing database cannot be managed safely.
    pub async fn ensure_schema(&self) -> Result<i32, DatabaseError> {
        let schema = Schema::parse(EMBEDDED_SCHEMA)?;
        let target_version = schema.version()?;
        let mut client = self.connect_or_create().await?;
        self.synchronize_schema(&mut client, &schema, target_version)
            .await
    }

    /// Opens a content repository connected to the managed database.
    ///
    /// # Errors
    ///
    /// Returns an error when the database connection cannot be established.
    pub async fn content_repository(&self) -> Result<PostgresContentRepository, DatabaseError> {
        let client = connect(&self.config)
            .await
            .map_err(|source| DatabaseError::Connect {
                database: self.database_name.to_string(),
                source,
            })?;
        Ok(PostgresContentRepository {
            client: Arc::new(client),
        })
    }

    async fn connect_or_create(&self) -> Result<Client, DatabaseError> {
        match connect(&self.config).await {
            Ok(client) => Ok(client),
            Err(source)
                if source
                    .as_db_error()
                    .is_some_and(|error| error.code() == &SqlState::INVALID_CATALOG_NAME) =>
            {
                self.create_database().await?;
                connect(&self.config)
                    .await
                    .map_err(|source| DatabaseError::Connect {
                        database: self.database_name.to_string(),
                        source,
                    })
            }
            Err(source) => Err(DatabaseError::Connect {
                database: self.database_name.to_string(),
                source,
            }),
        }
    }

    async fn create_database(&self) -> Result<(), DatabaseError> {
        let mut maintenance_config = self.config.clone();
        maintenance_config.dbname(&self.maintenance_database);
        let client =
            connect(&maintenance_config)
                .await
                .map_err(|source| DatabaseError::Connect {
                    database: self.maintenance_database.clone(),
                    source,
                })?;
        let statement = format!("CREATE DATABASE {}", self.database_name.quoted());

        match client.execute(&statement, &[]).await {
            Ok(_) => Ok(()),
            Err(source)
                if source
                    .as_db_error()
                    .is_some_and(|error| error.code() == &SqlState::DUPLICATE_DATABASE) =>
            {
                Ok(())
            }
            Err(source) => Err(DatabaseError::CreateDatabase {
                database: self.database_name.to_string(),
                source,
            }),
        }
    }

    async fn synchronize_schema(
        &self,
        client: &mut Client,
        schema: &Schema,
        target_version: i32,
    ) -> Result<i32, DatabaseError> {
        let transaction = client
            .transaction()
            .await
            .map_err(|source| query_error("begin schema migration", source))?;
        transaction
            .batch_execute(
                "SET LOCAL lock_timeout = '5s';
             SET LOCAL statement_timeout = '30s';
             SET LOCAL idle_in_transaction_session_timeout = '30s'",
            )
            .await
            .map_err(|source| query_error("configure schema migration", source))?;
        transaction
            .query_one("SELECT pg_advisory_xact_lock($1)", &[&ADVISORY_LOCK_ID])
            .await
            .map_err(|source| query_error("acquire the schema migration lock", source))?;

        let metadata_exists = metadata_exists(&transaction).await?;
        let stored_version = if metadata_exists {
            Some(stored_version(&transaction).await?)
        } else {
            None
        };
        let has_user_relations = if metadata_exists {
            false
        } else {
            has_user_relations(&transaction).await?
        };
        let action = schema_action(stored_version, has_user_relations, target_version)?;

        if action == SchemaAction::Unchanged {
            transaction
                .commit()
                .await
                .map_err(|source| query_error("commit schema verification", source))?;
            return Ok(target_version);
        }

        let state = database_state(&transaction).await?;
        let statements = schema.migration_statements(&state)?;
        for statement in statements {
            transaction
                .batch_execute(&statement)
                .await
                .map_err(|source| query_error("apply schema migration", source))?;
        }

        if action == SchemaAction::Initialize {
            seed::seed_initial_content(&transaction).await?;
            transaction
                .batch_execute(
                    "CREATE TABLE public._potagia_schema_metadata (
                         singleton boolean PRIMARY KEY DEFAULT TRUE CHECK (singleton),
                         schema_version integer NOT NULL,
                         updated_at timestamp with time zone NOT NULL DEFAULT current_timestamp
                     )",
                )
                .await
                .map_err(|source| query_error("create schema metadata", source))?;
        }
        transaction
            .execute(
                "INSERT INTO public._potagia_schema_metadata (singleton, schema_version) \
                 VALUES (TRUE, $1) \
                 ON CONFLICT (singleton) DO UPDATE SET \
                 schema_version = EXCLUDED.schema_version, updated_at = current_timestamp",
                &[&target_version],
            )
            .await
            .map_err(|source| query_error("store schema version", source))?;
        transaction
            .commit()
            .await
            .map_err(|source| query_error("commit schema migration", source))?;

        Ok(target_version)
    }
}

impl Debug for PostgresDatabase {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresDatabase")
            .field("database_name", &self.database_name)
            .field("maintenance_database", &self.maintenance_database)
            .finish_non_exhaustive()
    }
}

async fn connect(config: &Config) -> Result<Client, tokio_postgres::Error> {
    let (client, connection) = config.connect(NoTls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("PostgreSQL connection ended with an error: {error}");
        }
    });
    Ok(client)
}

async fn metadata_exists(transaction: &Transaction<'_>) -> Result<bool, DatabaseError> {
    let row = transaction
        .query_one(
            "SELECT to_regclass('public._potagia_schema_metadata')::text IS NOT NULL",
            &[],
        )
        .await
        .map_err(|source| query_error("inspect schema metadata", source))?;
    Ok(row.get(0))
}

async fn stored_version(transaction: &Transaction<'_>) -> Result<i32, DatabaseError> {
    let rows = transaction
        .query(
            "SELECT schema_version FROM public._potagia_schema_metadata",
            &[],
        )
        .await
        .map_err(|source| query_error("read schema version", source))?;

    match rows.as_slice() {
        [row] => Ok(row.get(0)),
        _ => Err(DatabaseError::InvalidMetadata(
            "the schema metadata table must contain exactly one row".to_owned(),
        )),
    }
}

async fn has_user_relations(transaction: &Transaction<'_>) -> Result<bool, DatabaseError> {
    let row = transaction
        .query_one(
            "SELECT EXISTS (
                 SELECT 1
                 FROM pg_catalog.pg_class AS relations
                 JOIN pg_catalog.pg_namespace AS namespaces
                     ON namespaces.oid = relations.relnamespace
                 WHERE namespaces.nspname <> 'information_schema'
                     AND namespaces.nspname NOT LIKE 'pg\\_%'
                     AND relations.relkind IN ('r', 'p', 'v', 'm', 'f', 'S')
             )",
            &[],
        )
        .await
        .map_err(|source| query_error("check whether the database is empty", source))?;
    Ok(row.get(0))
}

async fn database_state(transaction: &Transaction<'_>) -> Result<DatabaseState, DatabaseError> {
    let table_rows = transaction
        .query(
            "SELECT tables.relname::text
             FROM pg_catalog.pg_class AS tables
             JOIN pg_catalog.pg_namespace AS namespaces
                 ON namespaces.oid = tables.relnamespace
             WHERE namespaces.nspname = 'public' AND tables.relkind IN ('r', 'p')",
            &[],
        )
        .await
        .map_err(|source| query_error("inspect database tables", source))?;
    let mut tables: HashMap<String, Vec<ColumnState>> = table_rows
        .iter()
        .map(|row| (row.get(0), Vec::new()))
        .collect();

    let column_rows = transaction
        .query(
            "SELECT table_name, column_name, data_type,
                 is_nullable = 'YES' AS is_nullable,
                 identity_generation
             FROM information_schema.columns
             WHERE table_schema = 'public'",
            &[],
        )
        .await
        .map_err(|source| query_error("inspect database columns", source))?;
    for row in column_rows {
        let table: String = row.get("table_name");
        if let Some(columns) = tables.get_mut(&table) {
            columns.push(ColumnState {
                name: row.get("column_name"),
                data_type: row.get("data_type"),
                is_nullable: row.get("is_nullable"),
                identity_generation: row.get("identity_generation"),
            });
        }
    }

    let constraint_rows = transaction
        .query(
            "SELECT constraints.conname::text AS name,
                 tables.relname::text AS table_name,
                 constraints.contype::text AS kind,
                 CASE WHEN referenced_tables.oid IS NULL THEN ''
                     ELSE referenced_namespaces.nspname::text || '.' || referenced_tables.relname::text
                 END AS referenced_table,
                 constraints.confdeltype::text AS on_delete,
                 constraints.convalidated AS validated,
                 ARRAY(
                     SELECT attributes.attname::text
                     FROM unnest(constraints.conkey) WITH ORDINALITY AS key_columns(attnum, position)
                     JOIN pg_catalog.pg_attribute AS attributes
                         ON attributes.attrelid = constraints.conrelid
                         AND attributes.attnum = key_columns.attnum
                     ORDER BY key_columns.position
                 ) AS columns,
                 ARRAY(
                     SELECT attributes.attname::text
                     FROM unnest(constraints.confkey) WITH ORDINALITY AS key_columns(attnum, position)
                     JOIN pg_catalog.pg_attribute AS attributes
                         ON attributes.attrelid = constraints.confrelid
                         AND attributes.attnum = key_columns.attnum
                     ORDER BY key_columns.position
                 ) AS referenced_columns
             FROM pg_catalog.pg_constraint AS constraints
             JOIN pg_catalog.pg_class AS tables ON tables.oid = constraints.conrelid
             JOIN pg_catalog.pg_namespace AS namespaces
                 ON namespaces.oid = tables.relnamespace
             LEFT JOIN pg_catalog.pg_class AS referenced_tables
                 ON referenced_tables.oid = constraints.confrelid
             LEFT JOIN pg_catalog.pg_namespace AS referenced_namespaces
                 ON referenced_namespaces.oid = referenced_tables.relnamespace
             WHERE namespaces.nspname = 'public' AND constraints.contype IN ('p', 'f')",
            &[],
        )
        .await
        .map_err(|source| query_error("inspect database constraints", source))?;
    let mut constraints: HashMap<String, Vec<ConstraintState>> = HashMap::new();
    for row in constraint_rows {
        let table: String = row.get("table_name");
        if tables.contains_key(&table) {
            constraints.entry(table).or_default().push(ConstraintState {
                name: row.get("name"),
                kind: row.get("kind"),
                columns: row.get("columns"),
                referenced_table: row.get("referenced_table"),
                referenced_columns: row.get("referenced_columns"),
                on_delete: row.get("on_delete"),
                validated: row.get("validated"),
            });
        }
    }

    Ok(DatabaseState {
        tables,
        constraints,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaAction {
    Initialize,
    Update,
    Unchanged,
}

fn schema_action(
    stored_version: Option<i32>,
    has_user_relations: bool,
    target_version: i32,
) -> Result<SchemaAction, DatabaseError> {
    match stored_version {
        None if has_user_relations => Err(DatabaseError::UnmanagedDatabase),
        None => Ok(SchemaAction::Initialize),
        Some(current) if current <= 0 => Err(DatabaseError::InvalidMetadata(
            "the stored schema version must be greater than zero".to_owned(),
        )),
        Some(current) if current > target_version => Err(DatabaseError::Downgrade {
            current,
            target: target_version,
        }),
        Some(current) if current < target_version => Ok(SchemaAction::Update),
        Some(_) => Ok(SchemaAction::Unchanged),
    }
}

fn value_or_default(lookup: &impl Fn(&str) -> Option<String>, name: &str, default: &str) -> String {
    lookup(name).unwrap_or_else(|| default.to_owned())
}

fn read_password(path: &Path) -> Result<String, DatabaseError> {
    let password = fs::read_to_string(path).map_err(|source| DatabaseError::PasswordRead {
        path: path.to_path_buf(),
        source,
    })?;
    let password = password.trim_end_matches(['\r', '\n']).to_owned();
    if password.is_empty() {
        return Err(DatabaseError::EmptyPassword(path.to_path_buf()));
    }
    Ok(password)
}

fn query_error(operation: &'static str, source: tokio_postgres::Error) -> DatabaseError {
    DatabaseError::Query { operation, source }
}

/// An error encountered while configuring, creating, or migrating PostgreSQL.
#[derive(Debug)]
pub enum DatabaseError {
    /// The PostgreSQL password file could not be read.
    PasswordRead {
        /// Path of the password file.
        path: PathBuf,
        /// Underlying filesystem error.
        source: io::Error,
    },
    /// The PostgreSQL password file was empty.
    EmptyPassword(PathBuf),
    /// `POSTGRES_PORT` was not a valid TCP port.
    InvalidPort {
        /// Invalid configured value.
        value: String,
        /// Underlying parsing error.
        source: ParseIntError,
    },
    /// Another connection setting was invalid.
    InvalidConfiguration(String),
    /// The embedded schema was invalid or incompatible with the database.
    Schema(SchemaError),
    /// A PostgreSQL connection could not be established.
    Connect {
        /// Name of the target or maintenance database.
        database: String,
        /// Underlying driver error.
        source: tokio_postgres::Error,
    },
    /// The target database could not be created.
    CreateDatabase {
        /// Name of the target database.
        database: String,
        /// Underlying driver error.
        source: tokio_postgres::Error,
    },
    /// A schema discovery or migration statement failed.
    Query {
        /// Operation being performed.
        operation: &'static str,
        /// Underlying driver error.
        source: tokio_postgres::Error,
    },
    /// A database without metadata already contained user relations.
    UnmanagedDatabase,
    /// The metadata table was malformed.
    InvalidMetadata(String),
    /// The database was created by a newer application schema.
    Downgrade {
        /// Version stored in PostgreSQL.
        current: i32,
        /// Version requested by the embedded YAML schema.
        target: i32,
    },
}

impl Display for DatabaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasswordRead { path, source } => write!(
                formatter,
                "could not read PostgreSQL password from '{}': {source}",
                path.display()
            ),
            Self::EmptyPassword(path) => write!(
                formatter,
                "PostgreSQL password file '{}' is empty",
                path.display()
            ),
            Self::InvalidPort { value, source } => {
                write!(formatter, "invalid POSTGRES_PORT '{value}': {source}")
            }
            Self::InvalidConfiguration(reason) => {
                write!(formatter, "invalid PostgreSQL configuration: {reason}")
            }
            Self::Schema(error) => Display::fmt(error, formatter),
            Self::Connect { database, source } => {
                write!(
                    formatter,
                    "could not connect to PostgreSQL database '{database}': {source}"
                )
            }
            Self::CreateDatabase { database, source } => write!(
                formatter,
                "could not create PostgreSQL database '{database}': {source}"
            ),
            Self::Query { operation, source } => {
                write!(formatter, "could not {operation}: {source}")
            }
            Self::UnmanagedDatabase => formatter.write_str(
                "an existing database without Potagia metadata contains user relations; \
                 refusing to modify it",
            ),
            Self::InvalidMetadata(reason) => {
                write!(formatter, "invalid Potagia schema metadata: {reason}")
            }
            Self::Downgrade { current, target } => write!(
                formatter,
                "database schema version {current} is newer than application version {target}; \
                 downgrades are not supported"
            ),
        }
    }
}

impl Error for DatabaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PasswordRead { source, .. } => Some(source),
            Self::InvalidPort { source, .. } => Some(source),
            Self::Schema(error) => Some(error),
            Self::Connect { source, .. }
            | Self::CreateDatabase { source, .. }
            | Self::Query { source, .. } => Some(source),
            Self::EmptyPassword(_)
            | Self::InvalidConfiguration(_)
            | Self::UnmanagedDatabase
            | Self::InvalidMetadata(_)
            | Self::Downgrade { .. } => None,
        }
    }
}

impl From<SchemaError> for DatabaseError {
    fn from(error: SchemaError) -> Self {
        Self::Schema(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn initializes_an_empty_database_without_metadata() {
        let action = schema_action(None, false, 1).expect("empty database should be initialized");

        assert_eq!(action, SchemaAction::Initialize);
    }

    #[test]
    fn rejects_a_nonempty_database_without_metadata() {
        let error =
            schema_action(None, true, 1).expect_err("an unmanaged database must not be modified");

        assert!(matches!(error, DatabaseError::UnmanagedDatabase));
        assert!(error.to_string().contains("refusing to modify it"));
    }

    #[test]
    fn updates_an_older_managed_schema() {
        let action = schema_action(Some(1), false, 2).expect("older schema should be updated");

        assert_eq!(action, SchemaAction::Update);
    }

    #[test]
    fn leaves_a_current_managed_schema_unchanged() {
        let action = schema_action(Some(1), false, 1).expect("current schema should be accepted");

        assert_eq!(action, SchemaAction::Unchanged);
    }

    #[test]
    fn rejects_a_newer_managed_schema() {
        let error = schema_action(Some(2), false, 1).expect_err("downgrade should be rejected");

        assert!(matches!(
            error,
            DatabaseError::Downgrade {
                current: 2,
                target: 1
            }
        ));
        assert!(error.to_string().contains("downgrades are not supported"));
    }

    #[test]
    fn rejects_a_nonpositive_stored_schema_version() {
        let error = schema_action(Some(0), false, 1)
            .expect_err("an invalid stored version should be rejected");

        assert!(matches!(error, DatabaseError::InvalidMetadata(_)));
    }

    #[test]
    fn loads_connection_defaults_without_exposing_the_password_in_debug_output() {
        let database = PostgresDatabase::from_lookup(|_| None, "secret-password".to_owned())
            .expect("default settings should be valid");

        let debug = format!("{database:?}");
        assert_eq!(database.database_name.as_str(), "potagia");
        assert_eq!(database.maintenance_database, "postgres");
        assert!(!debug.contains("secret-password"));
    }

    #[test]
    fn rejects_an_invalid_database_port() {
        let error = PostgresDatabase::from_lookup(
            |name| (name == "POSTGRES_PORT").then(|| "not-a-port".to_owned()),
            "password".to_owned(),
        )
        .expect_err("invalid port should be rejected");

        assert!(matches!(error, DatabaseError::InvalidPort { .. }));
    }

    #[test]
    fn rejects_an_unsafe_database_name() {
        let error = PostgresDatabase::from_lookup(
            |name| (name == "POSTGRES_DB").then(|| "potagia; DROP DATABASE postgres".to_owned()),
            "password".to_owned(),
        )
        .expect_err("unsafe database name should be rejected");

        assert!(matches!(error, DatabaseError::InvalidConfiguration(_)));
    }

    #[test]
    fn reads_a_password_without_stripping_significant_whitespace() {
        let mut file = tempfile::NamedTempFile::new().expect("password file should be created");
        file.write_all(b" password \r\n")
            .expect("password should be written");

        let password = read_password(file.path()).expect("password should be read");

        assert_eq!(password, " password ");
    }

    #[test]
    fn rejects_an_empty_password_file() {
        let file = tempfile::NamedTempFile::new().expect("password file should be created");

        let error = read_password(file.path()).expect_err("empty password should be rejected");

        assert!(matches!(error, DatabaseError::EmptyPassword(_)));
    }

    #[test]
    fn reports_a_missing_password_file() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("missing-password");

        let error = read_password(&path).expect_err("missing password file should be rejected");

        assert!(matches!(error, DatabaseError::PasswordRead { .. }));
    }
}

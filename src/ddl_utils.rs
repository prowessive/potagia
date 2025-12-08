use crate::config_utils::DatabaseConfig;
use sqlx::{mysql::MySqlPoolOptions, postgres::PgPoolOptions, MySql, Pool, Postgres};
use std::error::Error;

/// Database manager for creating and managing tables using sqlx
pub struct DatabaseManager {
    config: DatabaseConfig,
}

impl DatabaseManager {
    /// Creates a new DatabaseManager from a DatabaseConfig
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Builds the connection URL based on the database configuration
    fn build_connection_url(&self) -> String {
        format!(
            "{}://{}:{}@{}:{}/{}",
            self.config.db_type,
            self.config.username,
            self.config.password,
            self.config.host,
            self.config.port,
            self.config.database
        )
    }

    /// Creates a PostgreSQL connection pool
    pub async fn create_postgres_pool(&self) -> Result<Pool<Postgres>, Box<dyn Error>> {
        let url = self.build_connection_url();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        Ok(pool)
    }

    /// Creates a MySQL connection pool
    pub async fn create_mysql_pool(&self) -> Result<Pool<MySql>, Box<dyn Error>> {
        let url = self.build_connection_url();
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        Ok(pool)
    }

    /// Executes a CREATE TABLE statement on PostgreSQL
    pub async fn create_table_postgres(
        &self,
        pool: &Pool<Postgres>,
        create_statement: &str,
    ) -> Result<(), Box<dyn Error>> {
        sqlx::query(create_statement).execute(pool).await?;
        Ok(())
    }

    /// Executes a CREATE TABLE statement on MySQL
    pub async fn create_table_mysql(
        &self,
        pool: &Pool<MySql>,
        create_statement: &str,
    ) -> Result<(), Box<dyn Error>> {
        sqlx::query(create_statement).execute(pool).await?;
        Ok(())
    }
}

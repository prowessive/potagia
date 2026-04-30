use sqlx::PgPool;

pub struct DatabaseBootstrapper;

impl DatabaseBootstrapper {
    pub async fn ensure_required_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
        for table_name in ["permissions", "paths", "files"] {
            sqlx::query(&format!(
                "CREATE TABLE IF NOT EXISTS {} (id SERIAL PRIMARY KEY, content JSONB, created_at TIMESTAMP DEFAULT NOW())",
                table_name
            ))
            .execute(pool)
            .await?;
        }

        Ok(())
    }
}
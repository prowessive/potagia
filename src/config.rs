use std::collections::HashMap;
use std::error::Error;
use std::fs;

pub struct DatabaseConfigResolver;

impl DatabaseConfigResolver {
    pub fn resolve_database_url() -> Result<String, Box<dyn Error>> {
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            if !database_url.trim().is_empty() {
                return Ok(database_url);
            }
        }

        let env_file_values = Self::load_env_file("config/.env")?;

        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            if !database_url.trim().is_empty() {
                return Ok(database_url);
            }
        }

        let user = Self::resolve_setting("DB_USER", &env_file_values, "postgres");
        let password = Self::resolve_setting("DB_PASS", &env_file_values, "postgres");
        let host = Self::resolve_setting("DB_HOST", &env_file_values, "localhost");
        let port = Self::resolve_setting("DB_PORT", &env_file_values, "5432");
        let database = Self::resolve_app_database_name(&env_file_values);

        Ok(format!(
            "postgres://{}:{}@{}:{}/{}",
            user, password, host, port, database
        ))
    }

    fn load_env_file(path: &str) -> Result<HashMap<String, String>, Box<dyn Error>> {
        let content = match fs::read_to_string(path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(error) => return Err(Box::new(error)),
        };

        let mut values = HashMap::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = trimmed.split_once('=') {
                values.insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        Ok(values)
    }

    fn resolve_setting(name: &str, file_values: &HashMap<String, String>, fallback: &str) -> String {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                file_values
                    .get(name)
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| fallback.to_string())
    }

    fn resolve_app_database_name(file_values: &HashMap<String, String>) -> String {
        let configured_database = Self::resolve_setting("DB_NAME", file_values, "potagia");

        if configured_database == "postgres" {
            return "potagia".to_string();
        }

        configured_database
    }
}
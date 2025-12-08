use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;

// Config struct
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub schema: SchemaConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SchemaConfig {
    pub path: String,
}

pub async fn load_config(config_path: String) -> Result<Config, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(config_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(serde_yaml::from_str(&contents)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_config_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        file
    }

    #[tokio::test]
    async fn test_load_config_success() {
        let yaml_content = r#"
server:
  host: "127.0.0.1"
  port: 8080
database:
  db_type: "postgres"
  host: "localhost"
  port: 5432
  username: "user"
  password: "pass"
  database: "test_db"
schema:
  path: "config/schema.json"
"#;
        let temp_file = create_test_config_file(yaml_content);
        let config = load_config(temp_file.path().to_str().unwrap().to_string())
            .await
            .expect("Failed to load config");

        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.database.db_type, "postgres");
        assert_eq!(config.database.host, "localhost");
        assert_eq!(config.database.port, 5432);
        assert_eq!(config.database.username, "user");
        assert_eq!(config.database.password, "pass");
        assert_eq!(config.database.database, "test_db");
        assert_eq!(config.schema.path, "config/schema.json");
    }

    #[tokio::test]
    async fn test_load_config_file_not_found() {
        let result = load_config("nonexistent/path/config.yaml".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_config_invalid_yaml() {
        let invalid_yaml = r#"
server:
  host: [invalid
  port: not_a_number
"#;
        let temp_file = create_test_config_file(invalid_yaml);
        let result = load_config(temp_file.path().to_str().unwrap().to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_config_missing_fields() {
        let incomplete_yaml = r#"
server:
  host: "127.0.0.1"
"#;
        let temp_file = create_test_config_file(incomplete_yaml);
        let result = load_config(temp_file.path().to_str().unwrap().to_string()).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 3000,
            },
            database: DatabaseConfig {
                db_type: "mysql".to_string(),
                host: "db.example.com".to_string(),
                port: 3306,
                username: "admin".to_string(),
                password: "secret".to_string(),
                database: "my_db".to_string(),
            },
            schema: SchemaConfig {
                path: "config/schema.json".to_string(),
            },
        };

        let yaml = serde_yaml::to_string(&config).expect("Failed to serialize config");
        let deserialized: Config =
            serde_yaml::from_str(&yaml).expect("Failed to deserialize config");

        assert_eq!(deserialized.server.host, config.server.host);
        assert_eq!(deserialized.server.port, config.server.port);
        assert_eq!(deserialized.database.db_type, config.database.db_type);
    }

    #[test]
    fn test_schema_config_serialization() {
        let schema_config = SchemaConfig {
            path: "config/schema.json".to_string(),
        };

        let yaml =
            serde_yaml::to_string(&schema_config).expect("Failed to serialize schema config");
        let deserialized: SchemaConfig =
            serde_yaml::from_str(&yaml).expect("Failed to deserialize schema config");

        assert_eq!(deserialized.path, schema_config.path);
    }

    #[test]
    fn test_schema_config_deserialization() {
        let yaml_content = r#"
path: "schemas/my_schema.json"
"#;
        let schema_config: SchemaConfig =
            serde_yaml::from_str(yaml_content).expect("Failed to deserialize schema config");

        assert_eq!(schema_config.path, "schemas/my_schema.json");
    }

    #[test]
    fn test_schema_config_missing_path() {
        let yaml_content = r#"
other_field: "value"
"#;
        let result: Result<SchemaConfig, _> = serde_yaml::from_str(yaml_content);
        assert!(result.is_err());
    }
}

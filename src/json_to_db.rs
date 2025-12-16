use crate::config_utils::DatabaseConfig;
use crate::ddl_utils::DatabaseManager;
use serde::Deserialize;
use sqlx::{MySql, Pool, Postgres};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

/// Foreign key definition
#[derive(Debug, Deserialize)]
pub struct ForeignKey {
    pub table: String,
    pub column: String,
    pub on_delete: Option<String>,
}

/// Column definition from JSON schema
#[derive(Debug, Deserialize)]
pub struct ColumnDefinition {
    #[serde(rename = "type")]
    pub column_type: String,
    #[serde(default)]
    pub primary_key: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub not_null: bool,
    pub default: Option<serde_json::Value>,
    pub foreign_key: Option<ForeignKey>,
    #[allow(dead_code)]
    pub description: Option<String>,
}

/// Index definition
#[derive(Debug, Deserialize)]
pub struct IndexDefinition {
    pub name: String,
    pub columns: Vec<String>,
}

/// Unique constraint definition
#[derive(Debug, Deserialize)]
pub struct UniqueConstraint {
    pub name: String,
    pub columns: Vec<String>,
}

/// Table definition from JSON schema
#[derive(Debug, Deserialize)]
pub struct TableDefinition {
    #[allow(dead_code)]
    pub description: Option<String>,
    pub columns: HashMap<String, ColumnDefinition>,
    #[serde(default)]
    pub indexes: Vec<IndexDefinition>,
    #[serde(default)]
    pub unique_constraints: Vec<UniqueConstraint>,
}

/// Root schema structure
#[derive(Debug, Deserialize)]
pub struct DatabaseSchema {
    pub tables: HashMap<String, TableDefinition>,
}

#[cfg(test)]
fn load_schema_from_file<P: AsRef<Path>>(json_path: P) -> Result<DatabaseSchema, Box<dyn Error>> {
    let content = fs::read_to_string(json_path)?;
    let schema = serde_json::from_str(&content)?;
    Ok(schema)
}

#[cfg(test)]
fn generate_create_table_postgres(table_name: &str, table: &TableDefinition) -> String {
    JsonToDb::generate_create_table_static(table_name, table, |col_type| col_type.to_string(), "")
}

#[cfg(test)]
fn generate_create_indexes(table_name: &str, table: &TableDefinition) -> Vec<String> {
    JsonToDb::generate_indexes_static(table_name, table)
}

#[cfg(test)]
fn sort_tables_by_dependencies(schema: &DatabaseSchema) -> Vec<String> {
    JsonToDb::sort_tables_by_dependencies_static(schema)
}

/// JSON to Database converter
pub struct JsonToDb {
    schema: DatabaseSchema,
    db_manager: DatabaseManager,
}

impl JsonToDb {
    /// Creates a new JsonToDb from a JSON file path and database config
    pub fn from_file<P: AsRef<Path>>(
        json_path: P,
        config: DatabaseConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let content = fs::read_to_string(json_path)?;
        let schema: DatabaseSchema = serde_json::from_str(&content)?;
        let db_manager = DatabaseManager::new(config);

        Ok(Self { schema, db_manager })
    }

    /// Static method to generate CREATE TABLE statements (used for both instance and test methods)
    fn generate_create_table_static<F>(
        table_name: &str,
        table: &TableDefinition,
        type_converter: F,
        table_suffix: &str,
    ) -> String
    where
        F: Fn(&str) -> String,
    {
        let mut columns_sql = Vec::new();
        let mut primary_keys = Vec::new();
        let mut foreign_keys = Vec::new();

        // Sort columns to ensure consistent ordering (primary keys first)
        let mut sorted_columns: Vec<_> = table.columns.iter().collect();
        sorted_columns.sort_by(|a, b| {
            let a_pk = a.1.primary_key;
            let b_pk = b.1.primary_key;
            b_pk.cmp(&a_pk).then(a.0.cmp(b.0))
        });

        for (col_name, col_def) in sorted_columns {
            let col_type = type_converter(&col_def.column_type);
            let mut col_sql = format!("    {} {}", col_name, col_type);

            if col_def.not_null || col_def.primary_key {
                col_sql.push_str(" NOT NULL");
            }

            if col_def.unique && !col_def.primary_key {
                col_sql.push_str(" UNIQUE");
            }

            if let Some(ref default) = col_def.default {
                let default_str = match default {
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => default.to_string(),
                };
                col_sql.push_str(&format!(" DEFAULT {}", default_str));
            }

            columns_sql.push(col_sql);

            if col_def.primary_key {
                primary_keys.push(col_name.clone());
            }

            if let Some(ref fk) = col_def.foreign_key {
                let on_delete = fk.on_delete.as_deref().unwrap_or("NO ACTION");
                foreign_keys.push(format!(
                    "    CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} ({}) ON DELETE {}",
                    table_name, col_name, col_name, fk.table, fk.column, on_delete
                ));
            }
        }

        // Add primary key constraint
        if !primary_keys.is_empty() {
            columns_sql.push(format!("    PRIMARY KEY ({})", primary_keys.join(", ")));
        }

        // Add unique constraints
        for constraint in &table.unique_constraints {
            columns_sql.push(format!(
                "    CONSTRAINT {} UNIQUE ({})",
                constraint.name,
                constraint.columns.join(", ")
            ));
        }

        // Add foreign key constraints
        columns_sql.extend(foreign_keys);

        format!(
            "CREATE TABLE IF NOT EXISTS {} (\n{}\n){}",
            table_name,
            columns_sql.join(",\n"),
            table_suffix
        )
    }

    /// Generic method to generate CREATE TABLE statements
    fn generate_create_table<F>(
        &self,
        table_name: &str,
        table: &TableDefinition,
        type_converter: F,
        table_suffix: &str,
    ) -> String
    where
        F: Fn(&str) -> String,
    {
        Self::generate_create_table_static(table_name, table, type_converter, table_suffix)
    }

    /// Generates a CREATE TABLE statement for PostgreSQL
    fn generate_create_table_postgres(&self, table_name: &str, table: &TableDefinition) -> String {
        self.generate_create_table(table_name, table, |col_type| col_type.to_string(), "")
    }

    /// Generates CREATE INDEX statements for a table
    fn generate_indexes(&self, table_name: &str, table: &TableDefinition) -> Vec<String> {
        Self::generate_indexes_static(table_name, table)
    }

    /// Static version of generate_indexes for use in tests and internal methods
    fn generate_indexes_static(table_name: &str, table: &TableDefinition) -> Vec<String> {
        table
            .indexes
            .iter()
            .map(|idx| {
                format!(
                    "CREATE INDEX IF NOT EXISTS {} ON {} ({})",
                    idx.name,
                    table_name,
                    idx.columns.join(", ")
                )
            })
            .collect()
    }

    /// Determines the order to create tables based on foreign key dependencies
    fn get_table_creation_order(&self) -> Vec<String> {
        Self::sort_tables_by_dependencies_static(&self.schema)
    }

    /// Static version of table sorting for use in tests and internal methods
    fn sort_tables_by_dependencies_static(schema: &DatabaseSchema) -> Vec<String> {
        let mut ordered = Vec::new();
        let mut remaining: Vec<_> = schema.tables.keys().cloned().collect();

        while !remaining.is_empty() {
            let mut added_this_round = Vec::new();

            for table_name in &remaining {
                let table = &schema.tables[table_name];
                let dependencies: Vec<_> = table
                    .columns
                    .values()
                    .filter_map(|col| col.foreign_key.as_ref().map(|fk| fk.table.clone()))
                    .collect();

                // Check if all dependencies are already in an ordered list
                let all_deps_satisfied = dependencies
                    .iter()
                    .all(|dep| ordered.contains(dep) || dep == table_name);

                if all_deps_satisfied {
                    added_this_round.push(table_name.clone());
                }
            }

            if added_this_round.is_empty() && !remaining.is_empty() {
                // Circular dependency detected, add remaining tables
                ordered.extend(remaining.drain(..));
                break;
            }

            for table_name in &added_this_round {
                ordered.push(table_name.clone());
                remaining.retain(|t| t != table_name);
            }
        }

        ordered
    }

    /// Creates all tables in PostgreSQL database
    pub async fn create_tables_postgres(&self) -> Result<(), Box<dyn Error>> {
        let pool = self.db_manager.create_postgres_pool().await?;
        self.execute_schema_postgres(&pool).await
    }

    /// Executes schema creation on an existing PostgreSQL pool
    pub async fn execute_schema_postgres(
        &self,
        pool: &Pool<Postgres>,
    ) -> Result<(), Box<dyn Error>> {
        let table_order = self.get_table_creation_order();

        for table_name in &table_order {
            let table = &self.schema.tables[table_name];

            // Create table
            let create_sql = self.generate_create_table_postgres(table_name, table);
            self.db_manager
                .create_table_postgres(pool, &create_sql)
                .await?;

            // Create indexes
            for index_sql in self.generate_indexes(table_name, table) {
                sqlx::query(&index_sql).execute(pool).await?;
            }
        }

        Ok(())
    }

    /// Creates all tables in MySQL database
    pub async fn create_tables_mysql(&self) -> Result<(), Box<dyn Error>> {
        let pool = self.db_manager.create_mysql_pool().await?;
        self.execute_schema_mysql(&pool).await
    }

    /// Executes schema creation on an existing MySQL pool
    pub async fn execute_schema_mysql(&self, pool: &Pool<MySql>) -> Result<(), Box<dyn Error>> {
        let table_order = self.get_table_creation_order();

        for table_name in &table_order {
            let table = &self.schema.tables[table_name];

            // MySQL uses similar syntax, but some types may differ
            let create_sql = self.generate_create_table_mysql(table_name, table);
            self.db_manager
                .create_table_mysql(pool, &create_sql)
                .await?;

            // Create indexes
            for index_sql in self.generate_indexes(table_name, table) {
                sqlx::query(&index_sql).execute(pool).await?;
            }
        }

        Ok(())
    }

    /// Generates a CREATE TABLE statement for MySQL
    fn generate_create_table_mysql(&self, table_name: &str, table: &TableDefinition) -> String {
        self.generate_create_table(
            table_name,
            table,
            |col_type| self.convert_type_to_mysql(col_type),
            " ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
        )
    }

    /// Converts PostgreSQL types to MySQL equivalents
    fn convert_type_to_mysql(&self, pg_type: &str) -> String {
        match pg_type.to_uppercase().as_str() {
            "UUID" => "CHAR(36)".to_string(),
            "JSONB" => "JSON".to_string(),
            "TEXT" => "TEXT".to_string(),
            "BOOLEAN" => "TINYINT(1)".to_string(),
            "TIMESTAMP" => "DATETIME".to_string(),
            other => other.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_json_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");
        file
    }

    fn create_minimal_schema_json() -> &'static str {
        r#"{
            "tables": {
                "users": {
                    "description": "Users table",
                    "columns": {
                        "id": {
                            "type": "UUID",
                            "primary_key": true,
                            "description": "User ID"
                        },
                        "name": {
                            "type": "VARCHAR(255)",
                            "not_null": true,
                            "description": "User name"
                        }
                    },
                    "indexes": [],
                    "unique_constraints": []
                }
            }
        }"#
    }

    fn create_full_schema_json() -> &'static str {
        r#"{
            "tables": {
                "users": {
                    "description": "Users table",
                    "columns": {
                        "id": {
                            "type": "UUID",
                            "primary_key": true
                        },
                        "email": {
                            "type": "VARCHAR(255)",
                            "unique": true,
                            "not_null": true
                        },
                        "is_active": {
                            "type": "BOOLEAN",
                            "default": true
                        },
                        "created_at": {
                            "type": "TIMESTAMP",
                            "default": "CURRENT_TIMESTAMP"
                        }
                    },
                    "indexes": [
                        {"name": "idx_users_email", "columns": ["email"]}
                    ],
                    "unique_constraints": []
                },
                "posts": {
                    "description": "Posts table",
                    "columns": {
                        "id": {
                            "type": "UUID",
                            "primary_key": true
                        },
                        "user_id": {
                            "type": "UUID",
                            "not_null": true,
                            "foreign_key": {
                                "table": "users",
                                "column": "id",
                                "on_delete": "CASCADE"
                            }
                        },
                        "title": {
                            "type": "VARCHAR(255)",
                            "not_null": true
                        }
                    },
                    "indexes": [
                        {"name": "idx_posts_user_id", "columns": ["user_id"]}
                    ],
                    "unique_constraints": [
                        {"name": "unique_user_title", "columns": ["user_id", "title"]}
                    ]
                }
            }
        }"#
    }

    #[test]
    fn test_load_schema_from_file_success() {
        let temp_file = create_test_json_file(create_minimal_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        assert!(schema.tables.contains_key("users"));
        assert_eq!(schema.tables.len(), 1);
    }

    #[test]
    fn test_load_schema_from_file_not_found() {
        let result = load_schema_from_file("nonexistent/path.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_schema_from_file_invalid_json() {
        let temp_file = create_test_json_file("{ invalid json }");
        let result = load_schema_from_file(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_schema_from_file_missing_tables() {
        let temp_file = create_test_json_file(r#"{"other_field": {}}"#);
        let result = load_schema_from_file(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_column_definition_parsing() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let users = &schema.tables["users"];

        // Test primary key column
        let id_col = &users.columns["id"];
        assert!(id_col.primary_key);
        assert_eq!(id_col.column_type, "UUID");

        // Test unique column
        let email_col = &users.columns["email"];
        assert!(email_col.unique);
        assert!(email_col.not_null);

        // Test default boolean
        let is_active_col = &users.columns["is_active"];
        assert!(is_active_col.default.is_some());

        // Test default string
        let created_at_col = &users.columns["created_at"];
        assert!(created_at_col.default.is_some());
    }

    #[test]
    fn test_foreign_key_parsing() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let posts = &schema.tables["posts"];
        let user_id_col = &posts.columns["user_id"];

        assert!(user_id_col.foreign_key.is_some());
        let fk = user_id_col.foreign_key.as_ref().unwrap();
        assert_eq!(fk.table, "users");
        assert_eq!(fk.column, "id");
        assert_eq!(fk.on_delete.as_deref(), Some("CASCADE"));
    }

    #[test]
    fn test_indexes_parsing() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let users = &schema.tables["users"];
        assert_eq!(users.indexes.len(), 1);
        assert_eq!(users.indexes[0].name, "idx_users_email");
        assert_eq!(users.indexes[0].columns, vec!["email"]);
    }

    #[test]
    fn test_unique_constraints_parsing() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let posts = &schema.tables["posts"];
        assert_eq!(posts.unique_constraints.len(), 1);
        assert_eq!(posts.unique_constraints[0].name, "unique_user_title");
        assert_eq!(
            posts.unique_constraints[0].columns,
            vec!["user_id", "title"]
        );
    }

    #[test]
    fn test_generate_create_table_basic() {
        let temp_file = create_test_json_file(create_minimal_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sql = generate_create_table_postgres("users", &schema.tables["users"]);

        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(sql.contains("id UUID"));
        assert!(sql.contains("name VARCHAR(255)"));
        assert!(sql.contains("NOT NULL"));
        assert!(sql.contains("PRIMARY KEY"));
    }

    #[test]
    fn test_generate_create_table_with_unique() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sql = generate_create_table_postgres("users", &schema.tables["users"]);

        assert!(sql.contains("email VARCHAR(255)"));
        assert!(sql.contains("UNIQUE"));
    }

    #[test]
    fn test_generate_create_table_with_default_boolean() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sql = generate_create_table_postgres("users", &schema.tables["users"]);

        assert!(sql.contains("is_active BOOLEAN"));
        assert!(sql.contains("DEFAULT true"));
    }

    #[test]
    fn test_generate_create_table_with_default_string() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sql = generate_create_table_postgres("users", &schema.tables["users"]);

        assert!(sql.contains("created_at TIMESTAMP"));
        assert!(sql.contains("DEFAULT CURRENT_TIMESTAMP"));
    }

    #[test]
    fn test_generate_create_table_with_foreign_key() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sql = generate_create_table_postgres("posts", &schema.tables["posts"]);

        assert!(sql.contains("FOREIGN KEY (user_id)"));
        assert!(sql.contains("REFERENCES users"));
        assert!(sql.contains("ON DELETE CASCADE"));
    }

    #[test]
    fn test_generate_create_table_with_unique_constraint() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sql = generate_create_table_postgres("posts", &schema.tables["posts"]);

        assert!(sql.contains("CONSTRAINT unique_user_title UNIQUE"));
        assert!(sql.contains("user_id"));
        assert!(sql.contains("title"));
    }

    #[test]
    fn test_generate_create_indexes() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let indexes = generate_create_indexes("users", &schema.tables["users"]);

        assert_eq!(indexes.len(), 1);
        assert!(indexes[0].contains("CREATE INDEX IF NOT EXISTS idx_users_email ON users (email)"));
    }

    #[test]
    fn test_generate_create_indexes_empty() {
        let temp_file = create_test_json_file(create_minimal_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let indexes = generate_create_indexes("users", &schema.tables["users"]);
        assert!(indexes.is_empty());
    }

    #[test]
    fn test_sort_tables_by_dependencies_no_deps() {
        let temp_file = create_test_json_file(create_minimal_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sorted = sort_tables_by_dependencies(&schema);

        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0], "users");
    }

    #[test]
    fn test_sort_tables_by_dependencies_with_foreign_key() {
        let temp_file = create_test_json_file(create_full_schema_json());
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sorted = sort_tables_by_dependencies(&schema);

        assert_eq!(sorted.len(), 2);
        // users should come before posts (posts depends on users)
        let users_pos = sorted.iter().position(|t| t == "users").unwrap();
        let posts_pos = sorted.iter().position(|t| t == "posts").unwrap();
        assert!(users_pos < posts_pos);
    }

    #[test]
    fn test_sort_tables_complex_dependencies() {
        let json = r#"{
            "tables": {
                "c": {
                    "columns": {
                        "id": {"type": "UUID", "primary_key": true},
                        "b_id": {
                            "type": "UUID",
                            "foreign_key": {"table": "b", "column": "id"}
                        }
                    }
                },
                "a": {
                    "columns": {
                        "id": {"type": "UUID", "primary_key": true}
                    }
                },
                "b": {
                    "columns": {
                        "id": {"type": "UUID", "primary_key": true},
                        "a_id": {
                            "type": "UUID",
                            "foreign_key": {"table": "a", "column": "id"}
                        }
                    }
                }
            }
        }"#;

        let temp_file = create_test_json_file(json);
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sorted = sort_tables_by_dependencies(&schema);

        let a_pos = sorted.iter().position(|t| t == "a").unwrap();
        let b_pos = sorted.iter().position(|t| t == "b").unwrap();
        let c_pos = sorted.iter().position(|t| t == "c").unwrap();

        // a should come before b, and b should come before c
        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn test_sort_tables_self_reference() {
        let json = r#"{
            "tables": {
                "employees": {
                    "columns": {
                        "id": {"type": "UUID", "primary_key": true},
                        "manager_id": {
                            "type": "UUID",
                            "foreign_key": {"table": "employees", "column": "id"}
                        }
                    }
                }
            }
        }"#;

        let temp_file = create_test_json_file(json);
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let sorted = sort_tables_by_dependencies(&schema);

        // Should handle self-reference gracefully
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0], "employees");
    }

    #[test]
    fn test_load_rbac_model_json() {
        // Test loading the actual rbac_model.json file
        let result = load_schema_from_file("templates/rbac_model.json");

        if let Ok(schema) = result {
            assert!(schema.tables.contains_key("users"));
            assert!(schema.tables.contains_key("roles"));
            assert!(schema.tables.contains_key("permissions"));
            assert!(schema.tables.contains_key("user_roles"));
            assert!(schema.tables.contains_key("role_permissions"));
        }
    }

    #[test]
    fn test_empty_tables() {
        let json = r#"{"tables": {}}"#;
        let temp_file = create_test_json_file(json);
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        assert!(schema.tables.is_empty());

        let sorted = sort_tables_by_dependencies(&schema);
        assert!(sorted.is_empty());
    }

    #[test]
    fn test_column_without_optional_fields() {
        let json = r#"{
            "tables": {
                "test": {
                    "columns": {
                        "simple_col": {
                            "type": "TEXT"
                        }
                    }
                }
            }
        }"#;

        let temp_file = create_test_json_file(json);
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let col = &schema.tables["test"].columns["simple_col"];
        assert!(!col.primary_key);
        assert!(!col.unique);
        assert!(!col.not_null);
        assert!(col.default.is_none());
        assert!(col.foreign_key.is_none());
        assert!(col.description.is_none());
    }

    #[test]
    fn test_foreign_key_without_on_delete() {
        let json = r#"{
            "tables": {
                "parent": {
                    "columns": {
                        "id": {"type": "UUID", "primary_key": true}
                    }
                },
                "child": {
                    "columns": {
                        "id": {"type": "UUID", "primary_key": true},
                        "parent_id": {
                            "type": "UUID",
                            "foreign_key": {"table": "parent", "column": "id"}
                        }
                    }
                }
            }
        }"#;

        let temp_file = create_test_json_file(json);
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let fk = schema.tables["child"].columns["parent_id"]
            .foreign_key
            .as_ref()
            .unwrap();

        assert!(fk.on_delete.is_none());

        // Generated SQL should contain foreign key constraint
        let sql = generate_create_table_postgres("child", &schema.tables["child"]);
        assert!(sql.contains("FOREIGN KEY (parent_id)"));
        assert!(sql.contains("REFERENCES parent"));
    }

    #[test]
    fn test_generate_create_indexes_multi_column() {
        let json = r#"{
            "tables": {
                "test": {
                    "columns": {
                        "col1": {"type": "VARCHAR(50)"},
                        "col2": {"type": "VARCHAR(50)"},
                        "col3": {"type": "VARCHAR(50)"}
                    },
                    "indexes": [
                        {"name": "idx_multi", "columns": ["col1", "col2", "col3"]}
                    ]
                }
            }
        }"#;

        let temp_file = create_test_json_file(json);
        let schema = load_schema_from_file(temp_file.path()).unwrap();

        let indexes = generate_create_indexes("test", &schema.tables["test"]);

        assert_eq!(indexes.len(), 1);
        assert!(indexes[0].contains("(col1, col2, col3)"));
    }
}

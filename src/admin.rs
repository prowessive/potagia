use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::{Json, http::HeaderMap};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::fs;
use std::path::Path as FsPath;
use std::sync::Arc;

#[derive(Clone)]
pub struct AdminService {
    pool: Arc<PgPool>,
}

impl AdminService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn list_tables(&self, database_name: &str) -> Result<Vec<String>, sqlx::Error> {
        let schema_name = Self::schema_name_for_database(database_name)?;
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = $1 AND table_type = 'BASE TABLE'
            ORDER BY table_name
            "#,
        )
        .bind(schema_name)
        .fetch_all(self.pool.as_ref())
        .await
    }

    pub async fn list_records(
        &self,
        database_name: &str,
        table_name: &str,
    ) -> Result<Vec<RecordDto>, sqlx::Error> {
        let query = format!("SELECT id, content FROM {} ORDER BY id", Self::qualified_table_name(database_name, table_name)?);

        let rows = sqlx::query_as::<_, (i32, Value)>(&query)
            .fetch_all(self.pool.as_ref())
            .await?;

        Ok(rows
            .into_iter()
            .map(|(id, content)| RecordDto { id, content })
            .collect())
    }

    pub async fn get_record(
        &self,
        database_name: &str,
        table_name: &str,
        id: i32,
    ) -> Result<Option<RecordDto>, sqlx::Error> {
        let query = format!(
            "SELECT id, content FROM {} WHERE id = $1 LIMIT 1",
            Self::qualified_table_name(database_name, table_name)?
        );

        let record = sqlx::query_as::<_, (i32, Value)>(&query)
            .bind(id)
            .fetch_optional(self.pool.as_ref())
            .await?;

        Ok(record.map(|(id, content)| RecordDto { id, content }))
    }

    pub async fn create_record(
        &self,
        database_name: &str,
        table_name: &str,
        content: Value,
    ) -> Result<RecordDto, sqlx::Error> {
        let query = format!(
            "INSERT INTO {} (content) VALUES ($1) RETURNING id, content",
            Self::qualified_table_name(database_name, table_name)?
        );

        let (id, content) = sqlx::query_as::<_, (i32, Value)>(&query)
            .bind(content)
            .fetch_one(self.pool.as_ref())
            .await?;

        Ok(RecordDto { id, content })
    }

    pub async fn update_record(
        &self,
        database_name: &str,
        table_name: &str,
        id: i32,
        content: Value,
    ) -> Result<Option<RecordDto>, sqlx::Error> {
        let query = format!(
            "UPDATE {} SET content = $1 WHERE id = $2 RETURNING id, content",
            Self::qualified_table_name(database_name, table_name)?
        );

        let row = sqlx::query_as::<_, (i32, Value)>(&query)
            .bind(content)
            .bind(id)
            .fetch_optional(self.pool.as_ref())
            .await?;

        Ok(row.map(|(id, content)| RecordDto { id, content }))
    }

    pub async fn delete_record(
        &self,
        database_name: &str,
        table_name: &str,
        id: i32,
    ) -> Result<bool, sqlx::Error> {
        let query = format!("DELETE FROM {} WHERE id = $1", Self::qualified_table_name(database_name, table_name)?);

        let result = sqlx::query(&query)
            .bind(id)
            .execute(self.pool.as_ref())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub fn list_databases() -> Result<Vec<String>, sqlx::Error> {
        Ok(Self::load_admin_database_config()?
            .databases
            .into_iter()
            .map(|entry| entry.name)
            .collect())
    }

    pub fn is_read_only_database(database_name: &str) -> bool {
        Self::database_config(database_name)
            .map(|entry| entry.read_only)
            .unwrap_or(false)
    }

    pub fn schema_name_for_database(database_name: &str) -> Result<String, sqlx::Error> {
        Ok(Self::database_config(database_name)?.schema)
    }

    pub fn validated_identifier(identifier: &str) -> Result<&str, sqlx::Error> {
        let is_valid = !identifier.is_empty()
            && identifier
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_');

        if is_valid {
            Ok(identifier)
        } else {
            Err(sqlx::Error::Protocol("invalid identifier".to_string()))
        }
    }

    pub fn qualified_table_name(database_name: &str, table_name: &str) -> Result<String, sqlx::Error> {
        let schema_name = Self::schema_name_for_database(database_name)?;
        let validated_table_name = Self::validated_identifier(table_name)?;
        Ok(format!("\"{schema_name}\".\"{validated_table_name}\""))
    }

    fn database_config(database_name: &str) -> Result<DatabaseConfigEntry, sqlx::Error> {
        Self::load_admin_database_config()?
            .databases
            .into_iter()
            .find(|entry| entry.name == database_name)
            .ok_or_else(|| sqlx::Error::Protocol("invalid database name".to_string()))
    }

    fn load_admin_database_config() -> Result<AdminDatabasesConfig, sqlx::Error> {
        let config_path = FsPath::new("db/potagia.json");
        let content = fs::read_to_string(config_path)
            .map_err(|error| sqlx::Error::Protocol(format!("failed to read {}: {error}", config_path.display())))?;
        let config = parse_admin_databases_config_from_str(&content)?;

        if config.databases.is_empty() {
            return Err(sqlx::Error::Protocol("no databases configured".to_string()));
        }

        if !config.databases.iter().any(|entry| entry.default) {
            return Err(sqlx::Error::Protocol(
                "no default database configured".to_string(),
            ));
        }

        Ok(config)
    }
}

fn parse_admin_databases_config_from_str(content: &str) -> Result<AdminDatabasesConfig, sqlx::Error> {
    serde_json::from_str(content).map_err(|error| {
        sqlx::Error::Protocol(format!("invalid admin database configuration: {error}"))
    })
}

#[derive(Deserialize)]
struct AdminDatabasesConfig {
    databases: Vec<DatabaseConfigEntry>,
}

#[derive(Clone, Deserialize)]
struct DatabaseConfigEntry {
    name: String,
    schema: String,
    read_only: bool,
    default: bool,
}

#[derive(Serialize)]
pub struct DatabaseEntry {
    pub name: String,
    pub read_only: bool,
    pub tables: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct RecordDto {
    pub id: i32,
    pub content: Value,
}

#[derive(Deserialize)]
pub struct RecordPayload {
    pub content: Value,
}

#[derive(Deserialize)]
pub struct CrudPagePath {
    pub database_name: String,
    pub table_name: String,
}

pub async fn admin_page_handler() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    (StatusCode::OK, headers, admin_tree_page_html())
}

pub async fn crud_page_handler(Path(path): Path<CrudPagePath>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    (
        StatusCode::OK,
        headers,
        admin_crud_page_html(&path.database_name, &path.table_name),
    )
}

pub async fn list_databases_handler(State(state): State<crate::AppState>) -> Response {
    let mut entries = Vec::new();

    let database_names = match AdminService::list_databases() {
        Ok(entries) => entries,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to load databases: {error}") })),
            )
                .into_response();
        }
    };

    for database_name in database_names {
        let result = state.admin_service.list_tables(&database_name).await;
        match result {
            Ok(tables) => {
                let read_only = AdminService::is_read_only_database(&database_name);
                entries.push(DatabaseEntry {
                    name: database_name,
                    read_only,
                    tables,
                });
            }
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Failed to list tables for {database_name}: {error}") })),
                )
                    .into_response();
            }
        }
    }

    Json(json!({ "databases": entries })).into_response()
}

pub async fn list_tables_handler(
    State(state): State<crate::AppState>,
    Path(database_name): Path<String>,
) -> Response {
    match state.admin_service.list_tables(&database_name).await {
        Ok(tables) => Json(json!({ "tables": tables })).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to list tables: {error}") })),
        )
            .into_response(),
    }
}

pub async fn list_records_handler(
    State(state): State<crate::AppState>,
    Path((database_name, table_name)): Path<(String, String)>,
) -> Response {
    match state
        .admin_service
        .list_records(&database_name, &table_name)
        .await
    {
        Ok(records) => Json(json!({ "records": records })).into_response(),
        Err(error) => map_sql_error(error, "Failed to list records").into_response(),
    }
}

pub async fn get_record_handler(
    State(state): State<crate::AppState>,
    Path((database_name, table_name, id)): Path<(String, String, i32)>,
) -> Response {
    match state
        .admin_service
        .get_record(&database_name, &table_name, id)
        .await
    {
        Ok(Some(record)) => Json(record).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Record not found" })),
        )
            .into_response(),
        Err(error) => map_sql_error(error, "Failed to get record").into_response(),
    }
}

pub async fn create_record_handler(
    State(state): State<crate::AppState>,
    Path((database_name, table_name)): Path<(String, String)>,
    Json(payload): Json<RecordPayload>,
) -> Response {
    if AdminService::is_read_only_database(&database_name) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Read-only database" })),
        )
            .into_response();
    }

    match state
        .admin_service
        .create_record(&database_name, &table_name, payload.content)
        .await
    {
        Ok(record) => (StatusCode::CREATED, Json(record)).into_response(),
        Err(error) => map_sql_error(error, "Failed to create record").into_response(),
    }
}

pub async fn update_record_handler(
    State(state): State<crate::AppState>,
    Path((database_name, table_name, id)): Path<(String, String, i32)>,
    Json(payload): Json<RecordPayload>,
) -> Response {
    if AdminService::is_read_only_database(&database_name) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Read-only database" })),
        )
            .into_response();
    }

    match state
        .admin_service
        .update_record(&database_name, &table_name, id, payload.content)
        .await
    {
        Ok(Some(record)) => Json(record).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Record not found" })),
        )
            .into_response(),
        Err(error) => map_sql_error(error, "Failed to update record").into_response(),
    }
}

pub async fn delete_record_handler(
    State(state): State<crate::AppState>,
    Path((database_name, table_name, id)): Path<(String, String, i32)>,
) -> Response {
    if AdminService::is_read_only_database(&database_name) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Read-only database" })),
        )
            .into_response();
    }

    match state
        .admin_service
        .delete_record(&database_name, &table_name, id)
        .await
    {
        Ok(true) => Json(json!({ "deleted": true })).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Record not found" })),
        )
            .into_response(),
        Err(error) => map_sql_error(error, "Failed to delete record").into_response(),
    }
}

fn map_sql_error(error: sqlx::Error, message: &str) -> (StatusCode, Json<Value>) {
    match error {
        sqlx::Error::Protocol(_) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid identifier" })),
        ),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("{message}: {other}") })),
        ),
    }
}

fn admin_tree_page_html() -> &'static str {
    r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Potagia · Admin CRUD</title>
    <link rel="stylesheet" href="/styles.css" />
  </head>
  <body>
    <main class="container">
      <section class="card admin-shell">
        <div class="admin-header">
          <h1>Admin CRUD</h1>
          <p class="muted">Select a database and table:</p>
        </div>
        <div id="tree" class="admin-tree"></div>
      </section>
    </main>
    <script>
      async function buildTree() {
        const response = await fetch("/api/admin/databases");
        const payload = await response.json();
        const tree = document.getElementById("tree");

        for (const database of payload.databases) {
          const details = document.createElement("details");
          details.className = "tree-group";
          details.open = true;
          const summary = document.createElement("summary");
          summary.textContent = `${database.name}${database.read_only ? " (read-only)" : ""}`;
          details.appendChild(summary);

          const list = document.createElement("ul");
          list.className = "tree-list";
          for (const table of database.tables) {
            const item = document.createElement("li");
            const link = document.createElement("a");
            link.href = `/admin/crud/${database.name}/${table}`;
            link.textContent = table;
            link.className = "crud-link";
            item.appendChild(link);
            list.appendChild(item);
          }
          details.appendChild(list);
          tree.appendChild(details);
        }
      }

      buildTree().catch((error) => {
        document.getElementById("tree").textContent = `Error: ${error.message}`;
      });
    </script>
  </body>
</html>
"#
}

fn admin_crud_page_html(database_name: &str, table_name: &str) -> String {
    let read_only = AdminService::is_read_only_database(database_name);
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Potagia · CRUD {database_name}.{table_name}</title>
    <link rel="stylesheet" href="/styles.css" />
  </head>
  <body>
    <main class="container">
      <section class="card admin-shell">
        <p><a href="/admin">← Back to admin tree</a></p>
        <div class="admin-header">
          <h1>CRUD for <code>{database_name}.{table_name}</code></h1>
          <button id="createBtn" type="button" class="btn btn-success">Create New Item</button>
        </div>
        <p id="mode" class="muted"></p>
        <p class="status" id="status"></p>
        <div class="table-wrapper">
          <table class="crud-table">
          <thead><tr><th>ID</th><th>Content</th><th>Action</th></tr></thead>
          <tbody id="recordsBody"></tbody>
        </table>
        </div>
        <div class="editor-panel">
          <label for="recordId">ID</label>
          <input id="recordId" type="number" readonly />
          <label for="recordContent">Content (JSON)</label>
          <textarea id="recordContent" rows="10"></textarea>
          <div class="actions-row">
            <button id="showBtn" type="button" class="btn btn-info">Show</button>
            <button id="updateBtn" type="button" class="btn btn-primary">Edit</button>
            <button id="deleteBtn" type="button" class="btn btn-danger">Delete</button>
            <button id="refreshBtn" type="button" class="btn btn-neutral">Refresh</button>
          </div>
        </div>
      </section>
    </main>
    <script>
      const databaseName = {database_name:?};
      const tableName = {table_name:?};
      const readOnly = {read_only};
      const statusNode = document.getElementById("status");
      const recordsBody = document.getElementById("recordsBody");
      const recordId = document.getElementById("recordId");
      const recordContent = document.getElementById("recordContent");

      document.getElementById("mode").textContent = readOnly
        ? "Read-only mode for this database"
        : "Read and write mode";

      function setStatus(message) {{
        statusNode.textContent = message;
      }}

      function setWriteEnabled(enabled) {{
        for (const id of ["createBtn", "updateBtn", "deleteBtn"]) {{
          document.getElementById(id).disabled = !enabled;
        }}
      }}

      function renderRecords(records) {{
        recordsBody.innerHTML = "";
        for (const record of records) {{
          const row = document.createElement("tr");
          row.innerHTML = `
            <td>${{record.id}}</td>
            <td><code>${{JSON.stringify(record.content).slice(0, 120)}}</code></td>
            <td>
              <button type="button" class="btn btn-info btn-sm">show</button>
              <button type="button" class="btn btn-primary btn-sm">edit</button>
              <button type="button" class="btn btn-danger btn-sm">delete</button>
            </td>
          `;
          const [showButton, editButton, deleteButton] = row.querySelectorAll("button");
          row.addEventListener("click", () => {{
            recordId.value = String(record.id);
            recordContent.value = JSON.stringify(record.content, null, 2);
          }});
          showButton.addEventListener("click", (event) => {{
            event.stopPropagation();
            recordId.value = String(record.id);
            recordContent.value = JSON.stringify(record.content, null, 2);
            setStatus(`Loaded record #${{record.id}} in editor`);
          }});
          editButton.addEventListener("click", async (event) => {{
            event.stopPropagation();
            recordId.value = String(record.id);
            await updateCurrentRecord();
          }});
          deleteButton.addEventListener("click", async (event) => {{
            event.stopPropagation();
            recordId.value = String(record.id);
            await deleteCurrentRecord();
          }});
          recordsBody.appendChild(row);
        }}
      }}

      async function api(url, options = {{}}) {{
        const response = await fetch(url, {{
          headers: {{ "Content-Type": "application/json" }},
          ...options
        }});
        const body = await response.json().catch(() => ({{}}));
        if (!response.ok) {{
          throw new Error(body.error ?? `Request failed (${{response.status}})`);
        }}
        return body;
      }}

      async function refresh() {{
        const payload = await api(`/api/admin/databases/${{databaseName}}/tables/${{tableName}}/records`);
        renderRecords(payload.records);
        setStatus(`Loaded ${{payload.records.length}} records`);
      }}

      async function updateCurrentRecord() {{
        await api(`/api/admin/databases/${{databaseName}}/tables/${{tableName}}/records/${{recordId.value}}`, {{
          method: "PUT",
          body: JSON.stringify({{ content: JSON.parse(recordContent.value || "{{}}") }})
        }});
        await refresh();
      }}

      async function deleteCurrentRecord() {{
        await api(`/api/admin/databases/${{databaseName}}/tables/${{tableName}}/records/${{recordId.value}}`, {{
          method: "DELETE"
        }});
        await refresh();
      }}

      document.getElementById("showBtn").addEventListener("click", () => {{
        setStatus(recordId.value ? `Showing record #${{recordId.value}}` : "Select a record from the table");
      }});

      document.getElementById("createBtn").addEventListener("click", async () => {{
        try {{
          await api(`/api/admin/databases/${{databaseName}}/tables/${{tableName}}/records`, {{
            method: "POST",
            body: JSON.stringify({{ content: JSON.parse(recordContent.value || "{{}}") }})
          }});
          await refresh();
        }} catch (error) {{
          setStatus(error.message);
        }}
      }});

      document.getElementById("updateBtn").addEventListener("click", async () => {{
        try {{
          await updateCurrentRecord();
        }} catch (error) {{
          setStatus(error.message);
        }}
      }});

      document.getElementById("deleteBtn").addEventListener("click", async () => {{
        try {{
          await deleteCurrentRecord();
        }} catch (error) {{
          setStatus(error.message);
        }}
      }});

      document.getElementById("refreshBtn").addEventListener("click", () => {{
        refresh().catch((error) => setStatus(error.message));
      }});

      setWriteEnabled(!readOnly);
      refresh().catch((error) => setStatus(error.message));
    </script>
  </body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::AdminService;
    use super::parse_admin_databases_config_from_str;

    #[test]
    fn identifier_validation_accepts_safe_values() {
        assert!(AdminService::validated_identifier("files").is_ok());
        assert!(AdminService::validated_identifier("table_2026").is_ok());
    }

    #[test]
    fn identifier_validation_rejects_unsafe_values() {
        assert!(AdminService::validated_identifier("").is_err());
        assert!(AdminService::validated_identifier("files; DROP TABLE files").is_err());
        assert!(AdminService::validated_identifier("paths/public").is_err());
    }

    #[test]
    fn admin_databases_config_supports_database_entries() {
        let result = parse_admin_databases_config_from_str(
            r#"{
                "databases": [
                    {"name":"rbac","schema":"rbac","read_only":true,"default":false}
                ]
            }"#,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn schema_name_comes_from_config() {
        assert_eq!(
            AdminService::schema_name_for_database("potagia").unwrap(),
            "public"
        );
        assert_eq!(
            AdminService::schema_name_for_database("aqui_se_come_bien").unwrap(),
            "aqui_se_come_bien"
        );
    }
}
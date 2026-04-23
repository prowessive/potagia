use clap::Parser;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{ PgPoolOptions};
use sqlx::{Connection, Executor, PgConnection};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about = "Load nested JSON databases into PostgreSQL")]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    json_path: PathBuf,
    /// Connection string to 'postgres' database (e.g., postgres://user:pass@localhost/postgres)
    #[arg(short, long, env = "DATABASE_URL")]
    connection_string: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Root {
    business_name: String,
    databases: Vec<DatabaseEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
struct DatabaseEntry {
    database_name: String,
    structure: StructureValue,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum StructureValue {
    Path(String),
    Direct(TableList),
}

#[derive(Serialize, Deserialize, Debug)]
struct TableList {
    tables: Vec<TableData>,
}

#[derive(Serialize, Deserialize, Debug)]
struct TableData {
    name: String,
    data: Vec<serde_json::Value>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 1. Leer el archivo maestro
    let content = fs::read_to_string(&args.json_path)?;
    let root: Root = serde_json::from_str(&content)?;

    // 2. Conectar inicialmente a 'postgres' para crear las otras DBs
    let mut admin_conn = PgConnection::connect(&args.connection_string).await?;

    for db_entry in root.databases {
        let db_name = db_entry.database_name;
        println!("--- Processing Database: {} ---", db_name);

        // A. Crear la base de datos si no existe
        // Nota: Postgres no soporta "IF NOT EXISTS" para CREATE DATABASE de forma nativa en SQL estándar sin PL/pgSQL
        let check_db = sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM pg_database WHERE datname = $1")
            .bind(&db_name)
            .fetch_one(&mut admin_conn)
            .await?;

        if check_db.0 == 0 {
            println!("Creating database '{}'...", db_name);
            admin_conn.execute(format!("CREATE DATABASE {}", db_name).as_str()).await?;
        }

        // B. Obtener los datos de las tablas (ya sea del JSON actual o de un fichero externo)
        let table_list = match db_entry.structure {
            StructureValue::Direct(tl) => tl,
            StructureValue::Path(p) => {
                let path = Path::new(&p);
                let ext_content = fs::read_to_string(path)
                    .map_err(|e| format!("Could not read external file {}: {}", p, e))?;
                serde_json::from_str(&ext_content)?
            }
        };

        // C. Conectar a la nueva base de datos para insertar las tablas
        let new_db_url = replace_db_name(&args.connection_string, &db_name);
        let pool = PgPoolOptions::new().max_connections(3).connect(&new_db_url).await?;

        for table in table_list.tables {
            println!("Migrating table '{}' into '{}'", table.name, db_name);

            sqlx::query(&format!(
                "CREATE TABLE IF NOT EXISTS {} (id SERIAL PRIMARY KEY, content JSONB, created_at TIMESTAMP DEFAULT NOW())",
                table.name
            ))
                .execute(&pool)
                .await?;

            for row in table.data {
                sqlx::query(&format!("INSERT INTO {} (content) VALUES ($1)", table.name))
                    .bind(row)
                    .execute(&pool)
                    .await?;
            }
        }
        pool.close().await;
    }

    println!("Full migration complete!");
    Ok(())
}

fn replace_db_name(conn_str: &str, new_db: &str) -> String {
    let base = conn_str.rsplit_once('/').unwrap_or((conn_str, "")).0;
    format!("{}/{}", base, new_db)
}
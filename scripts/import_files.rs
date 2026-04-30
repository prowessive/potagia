use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about = "Import files from a directory into potagia tables")]
struct Args {
    /// Root directory to scan (e.g. examples/aqui_se_come_bien/web)
    #[arg(short, long, value_name = "DIR")]
    source_path: PathBuf,
    /// Connection string to potagia DB (e.g., postgres://user:pass@localhost/potagia)
    #[arg(short, long, env = "DATABASE_URL")]
    connection_string: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let source_path = fs::canonicalize(&args.source_path)?;

    if !source_path.is_dir() {
        return Err(format!("source path is not a directory: {}", source_path.display()).into());
    }

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&args.connection_string)
        .await?;

    ensure_required_tables(&pool).await?;
    ensure_default_permissions(&pool).await?;

    import_tree(&pool, &source_path).await?;

    println!("Import completed for {}", source_path.display());
    pool.close().await;
    Ok(())
}

async fn ensure_required_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
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

async fn ensure_default_permissions(pool: &PgPool) -> Result<(), sqlx::Error> {
    let defaults = [
        ("read", "Allows viewing and listing content"),
        ("write", "Allows modifying and creating content"),
        ("execute", "Allows running executable files"),
    ];

    for (level, description) in defaults {
        let exists = sqlx::query("SELECT id FROM permissions WHERE content->>'permission_level' = $1 LIMIT 1")
            .bind(level)
            .fetch_optional(pool)
            .await?;

        if exists.is_none() {
            sqlx::query(
                "INSERT INTO permissions (content) VALUES (jsonb_build_object('permission_level', $1, 'description', $2))"
            )
                .bind(level)
                .bind(description)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}

async fn import_tree(pool: &PgPool, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut dirs = vec![root.to_path_buf()];

    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                dirs.push(path);
                continue;
            }

            if path.is_file() {
                let metadata = fs::metadata(&path)?;
                let size_kb = bytes_to_kb(metadata.len());
                let filename = entry.file_name().to_string_lossy().to_string();
                let file_content = fs::read_to_string(&path).unwrap_or_else(|_| {
                    String::from_utf8_lossy(&fs::read(&path).unwrap_or_default()).to_string()
                });
                let web_path = web_path_from_file(root, &path)?;
                let path_id = ensure_path(pool, &web_path).await?;
                ensure_file(pool, &filename, path_id, size_kb, &file_content).await?;
            }
        }
    }

    Ok(())
}

async fn ensure_path(pool: &PgPool, path_string: &str) -> Result<i32, sqlx::Error> {
    if let Some(row) = sqlx::query("SELECT id FROM paths WHERE content->>'path_string' = $1 LIMIT 1")
        .bind(path_string)
        .fetch_optional(pool)
        .await?
    {
        let id = row.get::<i32, _>("id");
        ensure_path_content_has_id(pool, id).await?;
        return Ok(id);
    }

    let row = sqlx::query(
        "INSERT INTO paths (content) \
         VALUES (jsonb_build_object('id', 0, 'path_string', $1, 'is_restricted', $2)) \
         RETURNING id"
    )
        .bind(path_string)
        .bind(false)
        .fetch_one(pool)
        .await?;

    let id = row.get::<i32, _>("id");
    sqlx::query("UPDATE paths SET content = jsonb_set(content, '{id}', to_jsonb($1::int), true) WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(id)
}

async fn ensure_path_content_has_id(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE paths \
         SET content = jsonb_set(content, '{id}', to_jsonb($1::int), true) \
         WHERE id = $1 AND content->>'id' IS NULL"
    )
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

async fn ensure_file(
    pool: &PgPool,
    filename: &str,
    path_id: i32,
    size_kb: i32,
    file_content: &str,
) -> Result<(), sqlx::Error> {
    let exists = sqlx::query(
        "SELECT id FROM files WHERE content->>'filename' = $1 AND (content->>'path_id')::int = $2 LIMIT 1"
    )
        .bind(filename)
        .bind(path_id)
        .fetch_optional(pool)
        .await?;

    if exists.is_none() {
        sqlx::query(
            "INSERT INTO files (content) VALUES (jsonb_build_object('filename', $1, 'path_id', $2, 'size_kb', $3, 'content', $4))"
        )
            .bind(filename)
            .bind(path_id)
            .bind(size_kb)
            .bind(file_content)
            .execute(pool)
            .await?;
    } else {
        sqlx::query(
            "UPDATE files \
             SET content = jsonb_set( \
                 jsonb_set(content, '{size_kb}', to_jsonb($3::int), true), \
                 '{content}', to_jsonb($4::text), true \
             ) \
             WHERE content->>'filename' = $1 AND (content->>'path_id')::int = $2"
        )
            .bind(filename)
            .bind(path_id)
            .bind(size_kb)
            .bind(file_content)
            .execute(pool)
            .await?;
    }

    Ok(())
}

fn web_path_from_file(root: &Path, file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let relative = file_path.strip_prefix(root)?;
    let mut web_path = String::from("/");
    web_path.push_str(&relative.to_string_lossy().replace('\\', "/"));
    Ok(web_path)
}

fn bytes_to_kb(bytes: u64) -> i32 {
    let kb = bytes.div_ceil(1024);
    if kb > i32::MAX as u64 {
        i32::MAX
    } else {
        kb as i32
    }
}
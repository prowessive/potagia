mod config_utils;
mod ddl_utils;
mod json_to_db;

use crate::json_to_db::JsonToDb;
use actix_files as fs;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};

async fn index(req: HttpRequest) -> HttpResponse {
    println!(
        "GET {} - {}",
        req.path(),
        req.connection_info()
            .realip_remote_addr()
            .unwrap_or("unknown")
    );
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/index.html"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfg = config_utils::load_config("config/config.yaml".to_string())
        .await
        .expect("Failed to load config");
    // Load schema and create database tables
    let json_to_db = JsonToDb::from_file(&cfg.schema.path, cfg.database.clone())
        .expect("Failed to load schema from file");
    // Create tables based on a database type
    match cfg.database.db_type.as_str() {
        "postgres" => {
            json_to_db
                .create_tables_postgres()
                .await
                .expect("Failed to create postgres tables");
        }
        "mysql" => {
            json_to_db
                .create_tables_mysql()
                .await
                .expect("Failed to create MySQL tables");
        }
        _ => {
            panic!("Unsupported database type: {}", cfg.database.db_type);
        }
    }
    println!(
        "Server running on http://{}:{}",
        cfg.server.host, cfg.server.port
    );

    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .service(fs::Files::new("/", "./static").show_files_listing())
    })
    .bind((cfg.server.host, cfg.server.port))?
    .run()
    .await
}

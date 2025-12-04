use actix_web::{web, App, HttpResponse, HttpServer, HttpRequest};
use actix_files as fs;

mod db;

async fn index(req: HttpRequest) -> HttpResponse {
    println!("GET {} - {}", req.path(), req.connection_info().realip_remote_addr().unwrap_or("unknown"));
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(include_str!("../static/index.html"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Servidor iniciado en http://127.0.0.1:3000");

    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .service(fs::Files::new("/", "./static").show_files_listing())
    })
    .bind(("127.0.0.1", 3000))?
    .run()
    .await
}

use actix_files as fs;
use actix_web::{App, HttpServer};

pub async fn serve() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new().service(
            fs::Files::new("/", "./test/public")
                .show_files_listing()
                .redirect_to_slash_directory()
                .index_file("index.html"),
        )
    })
    .bind(("127.0.0.1", 9999))?
    .run()
    .await
}

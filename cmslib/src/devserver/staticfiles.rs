use poem::{
    handler,
    web::{Data, Path},
    Response,
};

#[derive(Clone, Debug)]
pub struct OutputRootDir(pub String);

#[handler]
pub fn handle(Path(path): Path<String>, mount_point: Data<&OutputRootDir>) -> Response {
    use poem::http::StatusCode;
    use std::path::PathBuf;

    let mount_point = mount_point.0;
    let path = path.replace("../", "");
    let path = PathBuf::from(format!("{}/{}", mount_point.0, path));
    let mime_type = {
        match mime_guess::from_path(&path).first() {
            Some(guess) => guess,
            None => mime::APPLICATION_OCTET_STREAM,
        }
    };
    match std::fs::read_to_string(path) {
        Ok(file) => {
            let file = {
                let mime_type = mime_type.essence_str();
                if mime_type == mime::HTML
                    || mime_type == mime::TEXT_HTML
                    || mime_type == mime::TEXT_HTML_UTF_8
                {
                    format!(
                        r#"{file}<script>{}</script>"#,
                        include_str!("live-reload.js")
                    )
                } else {
                    file
                }
            };
            Response::builder().content_type(mime_type).body(file)
        }
        Err(_) => Response::builder().status(StatusCode::NOT_FOUND).finish(),
    }
}

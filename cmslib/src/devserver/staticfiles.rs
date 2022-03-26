use poem::{
    handler,
    web::{Data, Path},
    Response,
};
use tracing::{instrument, trace};

#[derive(Clone, Debug)]
pub struct OutputRootDir(pub String);

#[instrument(skip(mount_point))]
#[handler]
pub fn handle(Path(path): Path<String>, mount_point: Data<&OutputRootDir>) -> Response {
    trace!("handling static file request");
    use poem::http::StatusCode;
    use std::path::PathBuf;

    let mount_point = mount_point.0;

    // remove relative paths
    let path = path.replace("../", "");

    // transform `some_page/` to `some_page/index.html`
    let path = {
        if path.ends_with("/") || path == "" {
            trace!("directory requested, serving index.html");
            format!("{}index.html", path)
        } else {
            path
        }
    };

    // Redirect `some_page` to `some_page/`. This will cause the above block to be
    // executed on the next request, which will then add `index.html` to the request.
    {
        if PathBuf::from(&path).as_path().extension().is_none() {
            trace!("no extension detected. redirecting to directory url");
            return Response::builder()
                .status(StatusCode::SEE_OTHER)
                .header("Location", format!("{}/", path))
                .finish();
        }
    }

    // determine the path on the system
    let mut system_path = PathBuf::from(format!("{}/{}", mount_point.0, path));

    // determine MIME
    let mime_type = {
        match mime_guess::from_path(&system_path).first() {
            Some(guess) => guess,
            None => mime::APPLICATION_OCTET_STREAM,
        }
    };

    // serve file
    match std::fs::read_to_string(system_path) {
        Ok(file) => {
            let file = {
                let mime_type = mime_type.essence_str();
                if mime_type == mime::HTML
                    || mime_type == mime::TEXT_HTML
                    || mime_type == mime::TEXT_HTML_UTF_8
                {
                    // inject live reload script on all HTML pages
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

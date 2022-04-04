use crate::core::broker::EngineBroker;
use crate::core::Uri;
use poem::http::StatusCode;
use poem::{
    handler,
    web::{Data, Path},
    Response,
};
use std::path::PathBuf;
use tracing::{instrument, trace};

#[derive(Clone, Debug)]
pub struct OutputRootDir(pub String);

fn path_to_file<S: AsRef<str>>(path: S) -> String {
    let path = path.as_ref();
    // remove relative paths
    let path = path.replace("../", "");

    if path.is_empty() {
        String::from("/index.html")
    } else {
        // transform `some_page/` to `some_page/index.html`
        if path.ends_with('/') || path.is_empty() {
            trace!("directory requested, serving index.html");
            format!("{}index.html", path)
        } else {
            path
        }
    }
}

fn html_with_live_reload_script(html: &str) -> String {
    format!(
        r#"{html}<script>{}</script>"#,
        include_str!("live-reload.js")
    )
}

pub fn try_static_file<S: AsRef<str>>(
    path: S,
    mount_point: &Data<&OutputRootDir>,
) -> Option<Response> {
    trace!("try to serve static file");

    let path = path.as_ref();

    let mount_point = mount_point.0;

    // Redirect `some_page` to `some_page/`. This will cause the above block to be
    // executed on the next request, which will then add `index.html` to the request.
    {
        if PathBuf::from(&path).as_path().extension().is_none() {
            trace!("no extension detected. redirecting to directory url");
            return Some(
                Response::builder()
                    .status(StatusCode::SEE_OTHER)
                    .header("Location", format!("/{}/", path))
                    .finish(),
            );
        }
    }

    // determine the path on the system
    let system_path = PathBuf::from(format!("{}/{}", mount_point.0, path));
    trace!(path = ?system_path, "using path");

    // determine MIME
    let mime_type = {
        match mime_guess::from_path(&system_path).first() {
            Some(guess) => guess,
            None => mime::APPLICATION_OCTET_STREAM,
        }
    };

    trace!(mime = ?mime_type);

    // serve file
    match std::fs::read(system_path) {
        Ok(file) => {
            trace!("getgot");
            let mime_type = mime_type.essence_str();
            if mime_type == mime::HTML
                || mime_type == mime::TEXT_HTML
                || mime_type == mime::TEXT_HTML_UTF_8
            {
                let page = String::from_utf8_lossy(&file);
                let page = html_with_live_reload_script(&page);
                Some(Response::builder().content_type(mime_type).body(page))
            } else {
                Some(Response::builder().content_type(mime_type).body(file))
            }
        }
        Err(_) => None,
    }
}

pub async fn try_rendered_file(
    path: String,
    broker: Data<&EngineBroker>,
) -> Result<Option<Response>, anyhow::Error> {
    use crate::core::broker::{EngineMsg, RenderPageRequest};

    trace!("try to serve rendered file");

    let uri = Uri::from_path(path);
    let (req, page) = RenderPageRequest::new(uri);
    broker.send_engine_msg(EngineMsg::RenderPage(req)).await?;
    match page.recv().await? {
        Some(page) => Ok(Some(
            Response::builder()
                .content_type(mime::TEXT_HTML_UTF_8)
                .body(html_with_live_reload_script(&page.html)),
        )),
        None => Ok(None),
    }
}

#[instrument(skip(mount_point, broker), ret)]
#[handler]
pub async fn handle(
    path: Path<String>,
    mount_point: Data<&OutputRootDir>,
    broker: Data<&EngineBroker>,
) -> Result<Response, poem::error::Error> {
    use poem::http::StatusCode;

    let path = path_to_file(path.to_string());

    if let Some(res) = try_static_file(path.clone(), &mount_point) {
        Ok(res)
    } else {
        trace!("static file not found");
        match try_rendered_file(path, broker)
            .await
            .expect("broken channel between devserver and engine. this is a bug")
        {
            Some(res) => Ok(res),
            None => Ok(Response::builder().status(StatusCode::NOT_FOUND).finish()),
        }
    }
}

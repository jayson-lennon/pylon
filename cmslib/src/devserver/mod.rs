pub mod fswatcher;
mod livereload;
mod staticfiles;

pub use livereload::DevServerEvent;

use std::{net::SocketAddr, sync::Arc};

use poem::{
    handler,
    web::{
        websocket::{Message, WebSocket},
        Data, Path,
    },
    EndpointExt, IntoResponse, Response,
};
use tokio::runtime::Runtime;
use tokio::time::Duration;

pub type DevServerSender = async_channel::Sender<crate::devserver::DevServerEvent>;
pub type DevServerReceiver = async_channel::Receiver<crate::devserver::DevServerEvent>;

pub async fn run<R: AsRef<std::path::Path>, B: Into<SocketAddr>>(
    rt: Arc<Runtime>,
    event_channel: (DevServerSender, DevServerReceiver),
    output_root: R,
    bind: B,
    debounce_duration: Duration,
) -> Result<(), std::io::Error> {
    use poem::listener::TcpListener;
    use poem::middleware::AddData;
    use poem::{get, Route, Server};
    let fs_watch = fswatcher::start_watching(rt, event_channel.0, debounce_duration);

    let output_root = output_root.as_ref().to_string_lossy().to_string();

    let app = Route::new()
        .at(
            "/ws",
            get(livereload::handle.data(tokio::sync::broadcast::channel::<String>(8).0)),
        )
        .at("/*path", get(staticfiles::handle))
        .with(AddData::new(staticfiles::OutputRootDir(output_root)))
        .with(AddData::new(livereload::LiveReloadReceiver(
            event_channel.1,
        )));

    Server::new(TcpListener::bind(bind.into().to_string()))
        .run(app)
        .await
}

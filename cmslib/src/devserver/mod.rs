pub mod fswatcher;
mod livereload;
mod staticfiles;

pub use livereload::DevServerMsg;

use std::{net::SocketAddr, sync::Arc};

use poem::EndpointExt;
use tokio::runtime::Runtime;
use tokio::time::Duration;

use crate::engine::EngineBroker;

/*
`run` starts up the fswatcher which responds to filesystem events by pushing an event into the event channel.
for each connected websocket client, a channel is created in the ConnectedClients struct to manage the client.
whenever the fswatcher sends an event, the connected clients struct monitors it and then sends a message
to all connected clients via channel for the reload. the websocket server read message from their specific
channel and then sends out a message to their respective clients.
 */

pub type DevServerSender = async_channel::Sender<crate::devserver::DevServerMsg>;
pub type DevServerReceiver = async_channel::Receiver<crate::devserver::DevServerMsg>;

pub async fn run<R: AsRef<std::path::Path>, B: Into<SocketAddr>>(
    broker: EngineBroker,
    output_root: R,
    bind: B,
    debounce_wait: Duration,
) -> Result<(), anyhow::Error> {
    use poem::listener::TcpListener;
    use poem::middleware::AddData;
    use poem::{get, Route, Server};

    fswatcher::start_watching(broker.clone(), debounce_wait)?;

    let connected_clients = livereload::ClientBroker::new(broker);

    let output_root = output_root.as_ref().to_string_lossy().to_string();

    let app = Route::new()
        .at(
            "/ws",
            get(livereload::handle.data(tokio::sync::broadcast::channel::<String>(8).0)),
        )
        .at("/*path", get(staticfiles::handle))
        .with(AddData::new(staticfiles::OutputRootDir(output_root)))
        .with(AddData::new(connected_clients));

    Ok(Server::new(TcpListener::bind(bind.into().to_string()))
        .run(app)
        .await?)
}

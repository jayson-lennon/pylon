pub mod fswatcher;
mod livereload;
mod staticfiles;

pub use livereload::DevServerMsg;

use std::net::SocketAddr;
use std::thread::JoinHandle;

use poem::EndpointExt;

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

#[derive(Debug)]
pub struct DevServer {
    server_thread: JoinHandle<()>,
    broker: EngineBroker,
}

impl DevServer {
    #[must_use]
    pub fn run<P: AsRef<std::path::Path>, B: Into<SocketAddr>>(
        broker: EngineBroker,
        output_root: P,
        bind: B,
    ) -> Self {
        let output_root = output_root.as_ref().to_owned();
        let bind = bind.into();

        let broker_clone = broker.clone();
        let handle = std::thread::spawn(move || {
            broker_clone
                .handle()
                .block_on(async move { run(broker_clone, output_root, bind).await })
                .expect("failed to start dev server")
        });

        Self {
            server_thread: handle,
            broker,
        }
    }
}

async fn run<R: AsRef<std::path::Path>, B: Into<SocketAddr>>(
    broker: EngineBroker,
    output_root: R,
    bind: B,
) -> Result<(), anyhow::Error> {
    use poem::listener::TcpListener;
    use poem::middleware::AddData;
    use poem::{get, Route, Server};

    let output_root = output_root.as_ref().to_string_lossy().to_string();
    let bind = bind.into();

    let connected_clients = livereload::ClientBroker::new(broker);

    let app = Route::new()
        .at(
            "/ws",
            get(livereload::handle.data(tokio::sync::broadcast::channel::<String>(8).0)),
        )
        .at("/*path", get(staticfiles::handle))
        .with(AddData::new(staticfiles::OutputRootDir(output_root)))
        .with(AddData::new(connected_clients));

    Ok(Server::new(TcpListener::bind(bind.to_string()))
        .run(app)
        .await?)
}

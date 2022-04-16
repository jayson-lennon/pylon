use super::{DevServerReceiver, EngineBroker};
use async_lock::Mutex;
use poem::{
    handler,
    web::{
        websocket::{Message, WebSocket},
        Data,
    },
    IntoResponse,
};
use serde::Serialize;
use slotmap::DenseSlotMap;
use std::sync::Arc;
use std::thread;
use tracing::{error, instrument, trace};

#[derive(Clone, Debug)]
pub struct LiveReloadReceiver(pub DevServerReceiver);

/// Message sent from the engine to the dev server
#[derive(Clone, Debug)]
pub enum DevServerMsg {
    ReloadPage,
    Notify(String),
}

#[derive(Debug, Clone, Serialize)]
enum ClientMessageType {
    #[serde(rename(serialize = "reload"))]
    Reload,
    #[serde(rename(serialize = "notify"))]
    Notify,
}

/// Message sent from the devserver to the client
#[derive(Debug, Clone, Serialize)]
struct ClientMessage {
    #[serde(rename(serialize = "type"))]
    message_type: ClientMessageType,
    payload: String,
}

impl ClientMessage {
    pub fn reload() -> Self {
        Self {
            message_type: ClientMessageType::Reload,
            payload: "".to_string(),
        }
    }

    pub fn notify<S: Into<String>>(msg: S) -> Self {
        Self {
            message_type: ClientMessageType::Notify,
            payload: msg.into(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self).expect("failed to serialize websocket message. this is a bug")
    }
}

type WsClientId = slotmap::DefaultKey;

#[derive(Debug)]
struct WsClient {
    #[allow(dead_code)]
    id: WsClientId,
    tx: async_channel::Sender<DevServerMsg>,
    rx: async_channel::Receiver<DevServerMsg>,
}

impl WsClient {
    #[must_use]
    pub fn new(id: WsClientId) -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self { id, tx, rx }
    }
}

#[derive(Debug, Clone)]
pub struct ClientBroker {
    clients: Arc<Mutex<DenseSlotMap<WsClientId, WsClient>>>,
    engine_broker: EngineBroker,
}

impl ClientBroker {
    #[must_use]
    pub fn new(engine_broker: EngineBroker) -> Self {
        let client_broker = ClientBroker {
            clients: Arc::new(Mutex::new(DenseSlotMap::new())),
            engine_broker: engine_broker.clone(),
        };

        let client_broker_clone = client_broker.clone();
        thread::spawn(move || {
            let client_broker = client_broker_clone;
            loop {
                match engine_broker.recv_devserver_msg_sync() {
                    Ok(msg) => client_broker.send_sync(msg),
                    Err(e) => panic!("devserver channel error: {}", e),
                }
            }
        });

        client_broker
    }

    pub async fn add(&self) -> WsClientId {
        let mut clients = self.clients.lock().await;
        clients.insert_with_key(WsClient::new)
    }

    pub async fn remove(&self, id: WsClientId) {
        let mut clients = self.clients.lock().await;
        clients.remove(id);
    }

    pub async fn send(&self, msg: DevServerMsg) {
        let clients = self.clients.lock().await;
        for (id, client) in clients.iter() {
            if let Err(e) = client.tx.send(msg.clone()).await {
                trace!("error sending dev server event to client {:?}: {:?}", id, e);
            }
        }
    }

    pub fn send_sync(&self, msg: DevServerMsg) {
        self.engine_broker
            .handle()
            .block_on(async { self.send(msg).await });
    }

    pub async fn receiver(&self, id: WsClientId) -> Option<async_channel::Receiver<DevServerMsg>> {
        let clients = self.clients.lock().await;
        clients.get(id).map(|client| client.rx.clone())
    }
}

#[instrument(skip_all)]
#[handler]
pub fn handle(ws: WebSocket, clients: Data<&ClientBroker>) -> impl IntoResponse {
    use futures_util::{SinkExt, StreamExt};

    trace!("incoming websocket connection");

    let clients = clients.clone();

    // on_upgrade corresponds to a successfully connected client
    ws.on_upgrade(move |socket| async move {
        trace!("upgrade successful");
        let (mut sink, _) = socket.split();

        // track client
        let client_id = clients.add().await;

        // each client will listen on their respective channel
        tokio::spawn(async move {
            trace!("spawned livereload websocket task");
            loop {
                if let Ok(msg) = clients
                    .receiver(client_id)
                    .await
                    .expect("receiver should exist for client connection. this is a bug")
                    .recv()
                    .await
                {
                    match msg {
                        DevServerMsg::ReloadPage => {
                            let client_msg = ClientMessage::reload().to_json();

                            if let Err(e) = sink.send(Message::Text(client_msg)).await {
                                trace!("error sending reload message to websocket client: {}", e);
                            }

                            trace!("live reload message sent to client {:?}", client_id);
                            clients.remove(client_id).await;
                            break;
                        }
                        DevServerMsg::Notify(msg) => {
                            let client_msg = ClientMessage::notify(msg).to_json();

                            if let Err(e) = sink.send(Message::Text(client_msg)).await {
                                trace!("error sending error message to websocket client: {}", e);
                            }
                        }
                    }
                } else {
                    error!("reading from client channel should never fail");
                    break;
                }
            }
        });
    })
}

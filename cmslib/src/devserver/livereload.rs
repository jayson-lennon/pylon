use super::DevServerReceiver;
use crate::core::broker::EngineBroker;
use async_lock::Mutex;
use poem::{
    handler,
    web::{
        websocket::{Message, WebSocket},
        Data,
    },
    IntoResponse,
};
use slotmap::DenseSlotMap;
use std::sync::Arc;
use std::thread;
use tracing::{instrument, trace};

#[derive(Clone, Debug)]
pub struct LiveReloadReceiver(pub DevServerReceiver);

#[derive(Copy, Clone, Debug)]
pub enum DevServerMsg {
    ReloadPage,
}

type WsClientId = slotmap::DefaultKey;

#[derive(Debug)]
struct WsClient {
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

    pub async fn send(
        &self,
        msg: DevServerMsg,
    ) -> Result<(), async_channel::SendError<DevServerMsg>> {
        self.tx.send(msg).await
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
        clients.insert_with_key(|k| (WsClient::new(k)))
    }

    pub async fn remove(&self, id: WsClientId) {
        let mut clients = self.clients.lock().await;
        clients.remove(id);
    }

    pub async fn send(&self, msg: DevServerMsg) {
        let clients = self.clients.lock().await;
        for (id, client) in clients.iter() {
            if let Err(e) = client.tx.send(msg).await {
                trace!("error sending dev server event to client {:?}: {:?}", id, e);
            }
        }
    }

    pub fn send_sync(&self, msg: DevServerMsg) {
        self.engine_broker
            .handle()
            .block_on(async { self.send(msg).await })
    }

    pub async fn receiver(&self, id: WsClientId) -> Option<async_channel::Receiver<DevServerMsg>> {
        let clients = self.clients.lock().await;
        clients.get(id).map(|client| client.rx.clone())
    }
}

#[instrument(skip_all)]
#[handler]
pub fn handle(ws: WebSocket, clients: Data<&ClientBroker>) -> impl IntoResponse {
    trace!("incoming websocket connection");
    use futures_util::{SinkExt, StreamExt};

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
                            trace!("live reload message sent to client {:?}", client_id);
                            if let Err(e) = sink.send(Message::Text(format!("RELOAD"))).await {
                                trace!("error sending message to live reload client: {}. terminating websocket connection (probably left page)", e);
                                clients.remove(client_id).await;
                                return;
                            }
                        }
                    }
                } else {
                    trace!("reading from client channel should never fail; closing corresponding websocket connection");
                    clients.remove(client_id).await;
                    return;
                }
            }
        });
    })
}

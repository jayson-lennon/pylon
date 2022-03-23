use slotmap::DenseSlotMap;
use std::thread;

use super::DevServerReceiver;
use async_lock::Mutex;
use poem::{
    handler,
    web::{
        websocket::{Message, WebSocket},
        Data,
    },
    IntoResponse,
};
use std::sync::Arc;
use tokio::runtime::Runtime;

#[derive(Clone, Debug)]
pub struct LiveReloadReceiver(pub DevServerReceiver);

#[derive(Copy, Clone, Debug)]
pub enum DevServerEvent {
    ReloadPage,
}

type WsClientId = slotmap::DefaultKey;

#[derive(Debug)]
struct WsClient {
    id: WsClientId,
    tx: async_channel::Sender<DevServerEvent>,
    rx: async_channel::Receiver<DevServerEvent>,
}

impl WsClient {
    #[must_use]
    pub fn new(id: WsClientId) -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self { id, tx, rx }
    }

    pub async fn send(
        &self,
        msg: DevServerEvent,
    ) -> Result<(), async_channel::SendError<DevServerEvent>> {
        self.tx.send(msg).await
    }
}

#[derive(Debug, Clone)]
pub struct ConnectedClients {
    clients: Arc<Mutex<DenseSlotMap<WsClientId, WsClient>>>,
}

impl ConnectedClients {
    #[must_use]
    pub fn new(rt: Arc<Runtime>, fs_recv: DevServerReceiver) -> Self {
        let clients = ConnectedClients {
            clients: Arc::new(Mutex::new(DenseSlotMap::new())),
        };

        let clients_thread = clients.clone();
        thread::spawn(move || {
            let clients = clients_thread;
            rt.block_on(async move {
                loop {
                    match fs_recv.recv().await {
                        Ok(msg) => clients.send(msg).await,
                        Err(e) => eprintln!("filesystem watcher channel error: {}", e),
                    }
                }
            });
        });

        clients
    }

    pub async fn add(&self) -> WsClientId {
        let mut clients = self.clients.lock().await;
        clients.insert_with_key(|k| (WsClient::new(k)))
    }

    pub async fn remove(&self, id: WsClientId) {
        let mut clients = self.clients.lock().await;
        clients.remove(id);
    }

    pub async fn send(&self, msg: DevServerEvent) {
        let clients = self.clients.lock().await;
        for (id, client) in clients.iter() {
            if let Err(e) = client.tx.send(msg).await {
                eprintln!("error sending dev server event to client {:?}: {:?}", id, e);
            }
        }
    }

    pub async fn receiver(
        &self,
        id: WsClientId,
    ) -> Option<async_channel::Receiver<DevServerEvent>> {
        let clients = self.clients.lock().await;
        clients.get(id).map(|client| client.rx.clone())
    }
}

#[handler]
pub fn handle(
    ws: WebSocket,
    _: Data<&tokio::sync::broadcast::Sender<String>>,
    clients: Data<&ConnectedClients>,
) -> impl IntoResponse {
    use futures_util::{SinkExt, StreamExt};

    let clients = clients.clone();

    ws.on_upgrade(move |socket| async move {
        let (mut sink, _) = socket.split();
        let client_id = clients.add().await;

        tokio::spawn(async move {
            loop {
                match clients.receiver(client_id).await.unwrap().recv().await {
                    Ok(msg) => match msg {
                        DevServerEvent::ReloadPage => {
                            if sink.send(Message::Text(format!("RELOAD"))).await.is_err() {
                                return;
                            } else {
                                dbg!("send msg");
                            }
                        }
                    },
                    Err(_) => {
                        dbg!("websocket error");
                        return;
                    }
                }
            }
        });
    })
}

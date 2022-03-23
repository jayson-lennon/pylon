use std::net::SocketAddr;

use super::DevServerReceiver;
use poem::{
    handler,
    web::{
        websocket::{Message, WebSocket},
        Data, Path,
    },
    EndpointExt, IntoResponse, Response,
};
use tokio::time::Duration;

#[derive(Clone, Debug)]
pub struct LiveReloadReceiver(pub DevServerReceiver);

#[derive(Clone, Debug)]
pub enum DevServerEvent {
    ReloadPage,
}

#[handler]
pub fn handle(
    ws: WebSocket,
    _: Data<&tokio::sync::broadcast::Sender<String>>,
    recv: Data<&LiveReloadReceiver>,
) -> impl IntoResponse {
    use futures_util::{SinkExt, StreamExt};

    let recv = recv.0.clone();
    ws.on_upgrade(move |socket| async move {
        let (mut sink, _) = socket.split();

        tokio::spawn(async move {
            loop {
                match recv.0.recv().await {
                    Ok(msg) => match msg {
                        DevServerEvent::ReloadPage => {
                            if sink.send(Message::Text(format!("RELOAD"))).await.is_err() {
                                return;
                            }
                        }
                    },
                    Err(_) => return,
                }
            }
        });
    })
}

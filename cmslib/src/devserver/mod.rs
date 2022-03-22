mod staticfiles;

use futures_util::{SinkExt, StreamExt};
use poem::{
    endpoint::StaticFilesEndpoint,
    get, handler,
    listener::TcpListener,
    web::{
        websocket::{Message, WebSocket},
        Data, Html, Path,
    },
    EndpointExt, IntoResponse, Route, Server,
};
use tokio::time::{self, Duration};

#[handler]
fn ws(ws: WebSocket, sender: Data<&tokio::sync::broadcast::Sender<String>>) -> impl IntoResponse {
    let sender = sender.clone();
    let mut receiver = sender.subscribe();
    ws.on_upgrade(move |socket| async move {
        let (mut sink, _) = socket.split();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;
                if sink.send(Message::Text(format!("sup"))).await.is_err() {
                    break;
                }
            }
        });
    })
}

pub async fn serve() -> Result<(), std::io::Error> {
    let app = Route::new()
        .at(
            "/ws",
            get(ws.data(tokio::sync::broadcast::channel::<String>(32).0)),
        )
        .nest(
            "/",
            staticfiles::StaticFilesEndpoint::new("./test/public")
                .index_file("index.html")
                .inject_script(r#"<script>console.log("injected");</script>"#),
        );
    Server::new(TcpListener::bind("127.0.0.1:3000"))
        .run(app)
        .await
}

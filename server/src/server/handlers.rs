use axum::{
    extract::{Path, State, WebSocketUpgrade, ws::Message},
    response::Response,
};
use futures::{Sink, SinkExt, Stream, StreamExt};

use crate::store::Store;

pub async fn root() -> &'static str {
    "Silent Payment Server"
}

pub async fn get_chain_tip(State(db): State<Store>) -> String {
    db.get_synced_blocks_height().await.to_string()
}
pub async fn get_block_by_height(State(_db): State<Store>, Path(_height): Path<i64>) -> String {
    todo!()
}

// Websockets

pub async fn ws_subscribe_handler(state: State<Store>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|socket| {
        let (write, read) = socket.split();
        ws_subscribe_socket(state, write, read)
    })
}

pub async fn ws_subscribe_socket<W, R>(State(db): State<Store>, mut write: W, mut _read: R)
where
    W: Sink<Message> + Unpin,
    R: Stream<Item = Result<Message, axum::Error>>,
{
    let mut rx = db.subscribe_blocks();
    while let Ok(block) = rx.recv().await {
        if write
            .send(Message::Text(format!("new block: {:?}", block).into()))
            .await
            .is_err()
        {
            break;
        }
    }
}

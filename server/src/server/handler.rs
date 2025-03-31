use axum::{
    Json,
    extract::{Path, State, WebSocketUpgrade, ws::Message},
    response::{IntoResponse, Response},
};
use futures::{Sink, SinkExt, Stream, StreamExt};
use serde_json::json;

use crate::{
    Error,
    store::{
        Store,
        model::{Block, Scalars, Transactions},
    },
};

use crate::Result;

pub async fn root() -> &'static str {
    "Silent Payment Server"
}

// GET /blocks/latest/scalars
pub async fn get_latest_scalars(State(db): State<Store>) -> Result<Json<Scalars>> {
    let scalars = db.get_latest_scalars().await?;
    Ok(Json(scalars))
}

// GET /blocks/height/<height>/scalars
pub async fn get_scalars(
    State(db): State<Store>,
    Path(height): Path<i64>,
) -> Result<Json<Scalars>> {
    let scalars = db.get_scalars_by_height(height).await?;
    Ok(Json(scalars))
}

// GET /transactions/<txid>/scalar
pub async fn get_scalar(
    State(db): State<Store>,
    Path(txid): Path<String>,
) -> Result<impl IntoResponse> {
    db.get_scalar_by_txid(txid)
        .await?
        .map(Json)
        .ok_or_else(|| Error::NotFound)
}

// GET /blocks/latest/transactions
pub async fn get_latest_transactions(State(db): State<Store>) -> Result<Json<Transactions>> {
    let transactions = db.get_latest_transactions().await?;
    Ok(Json(transactions))
}
// GET /blocks/height/<height>/transactions
pub async fn get_transactions(
    State(db): State<Store>,
    Path(height): Path<i64>,
) -> Result<Json<Transactions>> {
    let transactions = db.get_transactions_by_height(height).await?;
    Ok(Json(transactions))
}

// GET /transactions/<txid>
pub async fn get_transaction(
    State(db): State<Store>,
    Path(txid): Path<String>,
) -> Result<impl IntoResponse> {
    db.get_transaction_by_txid(txid)
        .await?
        .map(Json)
        .ok_or_else(|| Error::NotFound)
}

// GET /blocks/tip
pub async fn get_chain_tip(State(db): State<Store>) -> String {
    db.get_synced_blocks_height()
        .await
        .unwrap()
        .unwrap()
        .to_string()
}

// Websockets

pub enum SubscriptionKind {
    Scalars,
    Transactions,
}

pub async fn ws_subscribe(
    state: State<Store>,
    ws: WebSocketUpgrade,
    kind: SubscriptionKind,
) -> Response {
    ws.on_upgrade(|socket| {
        let (write, read) = socket.split();
        ws_subscribe_socket(state, write, read, kind)
    })
}

pub async fn ws_subscribe_socket<W, R>(
    State(db): State<Store>,
    mut write: W,
    mut _read: R,
    kind: SubscriptionKind,
) where
    W: Sink<Message> + Unpin,
    R: Stream<Item = core::result::Result<Message, axum::Error>>,
{
    let mut rx = db.subscribe_blocks();
    let ser_msg = match kind {
        SubscriptionKind::Scalars => |block: Block| {
            let scalars = block.transactions.into_iter().map(|tx| tx.scalar).collect();
            json!(Scalars { scalars }).to_string()
        },
        SubscriptionKind::Transactions => |block: Block| {
            let transactions = block.transactions;
            json!(Transactions { transactions }).to_string()
        },
    };

    while let Ok(block) = rx.recv().await {
        if block.transactions.is_empty() {
            continue;
        }

        let msg = ser_msg(block);
        if write.send(Message::Text(msg.into())).await.is_err() {
            break;
        }
    }
}

// TODO:
// POST /wallet (create wallet and return wallet id or mnemonic) (only use for development)
// WS /ws/<wallet_id>/outputs: Stream outputs owned by wallet

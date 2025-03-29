use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State, WebSocketUpgrade, ws::Message},
    response::Response,
};
use futures::{Sink, SinkExt, Stream, StreamExt};
use serde::Serialize;

use crate::store::{Store, TransactionRecord};

#[derive(Serialize)]
pub struct Scalar {
    scalar: String,
}

#[derive(Serialize)]
pub struct Scalars {
    scalars: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct Output {
    vout: usize,
    value: u64,
    spk: String,
}

#[derive(Serialize)]
pub struct Transaction {
    txid: String,
    scalar: String,
    outputs: Vec<Output>,
}

#[derive(Serialize)]
pub struct Transactions {
    transactions: Vec<Transaction>,
}

// TODO:
// POST /wallet (create wallet and return wallet id or mnemonic) (only use for development)
// WS /ws/tweaks: Stream new tweaks.
// WS /ws/transactions: Stream new transactions.
// WS /ws/<wallet_id>/outputs: Stream outputs owned by wallet

pub async fn root() -> &'static str {
    "Silent Payment Server"
}

// GET /blocks/latest/scalars
pub async fn get_latest_scalars(State(db): State<Store>) -> Json<Scalars> {
    let scalars = db.get_latest_scalars().await;
    Json(Scalars { scalars })
}

// GET /blocks/height/<height>/scalars
pub async fn get_scalars(State(db): State<Store>, Path(height): Path<i64>) -> Json<Scalars> {
    let scalars = db.get_scalars_by_height(height).await;
    Json(Scalars { scalars })
}

// GET /transactions/<txid>/scalar
pub async fn get_scalar(State(db): State<Store>, Path(txid): Path<String>) -> Json<Scalar> {
    let scalar = db.get_scalar_by_txid(txid).await;
    Json(Scalar { scalar })
}

// GET /blocks/latest/transactions
pub async fn get_latest_transactions(State(db): State<Store>) -> Json<Transactions> {
    let transaction_records = db.get_latest_transactions().await;
    Json(transaction_records.into())
}
// GET /blocks/height/<height>/transactions
pub async fn get_transactions(
    State(db): State<Store>,
    Path(height): Path<i64>,
) -> Json<Transactions> {
    let transaction_records = db.get_transactions_by_height(height).await;
    Json(transaction_records.into())
}

// GET /transactions/<txid>
pub async fn get_transaction(
    State(db): State<Store>,
    Path(txid): Path<String>,
) -> Json<Transaction> {
    let transaction_record = db.get_transaction_by_txid(txid).await;
    Json(transaction_record.try_into().unwrap())
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

impl TryFrom<Vec<TransactionRecord>> for Transaction {
    type Error = Box<dyn std::error::Error>;

    fn try_from(value: Vec<TransactionRecord>) -> Result<Self, Self::Error> {
        // Since we query_as in the DB there should be at least one otherwise we would've paniced
        // at the unwrap in the store (which should be fixed too). So this should never happen but
        // makes sense to put this here. I'll probably refactor this away anyways bcs this whole
        // thing is weird.
        if value.is_empty() {
            return Err("Transaction not found".into());
        }
        // All records will have the same txid and scalar. Only the output fields will be different
        // because this comes from a join sql query.
        let txid = value[0].txid.clone();
        let scalar = value[0].scalar.clone();
        let mut outputs = vec![];

        for record in value.iter() {
            outputs.push(Output {
                vout: record.vout as usize,
                value: record.value as u64,
                spk: record.script_pub_key.clone(),
            });
        }

        Ok(Self {
            txid,
            scalar,
            outputs,
        })
    }
}

impl From<Vec<TransactionRecord>> for Transactions {
    fn from(value: Vec<TransactionRecord>) -> Self {
        let mut tx_map: HashMap<String, Transaction> = HashMap::new();

        for tx_record in value.iter() {
            let txid = tx_record.txid.clone();
            let scalar = tx_record.scalar.clone();

            let output = Output {
                vout: tx_record.vout as usize,
                value: tx_record.value as u64,
                spk: tx_record.script_pub_key.clone(),
            };

            tx_map
                .entry(txid.clone())
                .and_modify(|tx| tx.outputs.push(output.clone()))
                .or_insert_with(|| Transaction {
                    txid,
                    scalar,
                    outputs: vec![output],
                });
        }

        Self {
            transactions: tx_map.into_values().collect(),
        }
    }
}

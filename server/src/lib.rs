use bitcoincore_rpc::{
    Auth, Client, RpcApi,
    bitcoin::{OutPoint, Transaction, Txid},
};
use rpc::*;
use std::{str::FromStr, sync::Arc, time::Duration};
use sync::Syncer;
use tokio::{sync::mpsc, time::sleep};

use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

pub mod rpc;
mod silentpayments;
mod sync;

pub struct ServerConfig {
    pub host: String,
    pub db_url: String,
    //rpcuser: String,
    //rpcpass: String,
}

pub struct Server {
    cfg: ServerConfig,
    db: SqlitePool,
    // TODO: Add syncer handle.
}

impl Server {
    pub async fn new(cfg: ServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let options = SqliteConnectOptions::from_str(&cfg.db_url)?.create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;

        Ok(Self { cfg, db: pool })
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (sync_tx, mut sync_rx) = mpsc::channel(64);
        // TODO: RPC Config / Syncer config.
        let auth = Auth::UserPass("sus".into(), "sus".into());
        let client = Client::new("http://localhost:18443", auth).unwrap();

        // Run syncer.
        let mut syncer = Syncer::new(client, 1000);
        let sync_from_height = get_synced_blocks_height(&self.db).await as u64;
        tokio::task::spawn(async move { syncer.sync_from(sync_from_height, sync_tx).await });

        // Receive blocks from syncer and add them to DB.
        let sync_pool = self.db.clone();
        tokio::task::spawn(async move {
            while let Some(msg) = sync_rx.recv().await {
                println!("new block: {:?}", msg);
                // FIXME: Pass block height with msg.
                add_block(msg, &sync_pool).await;
            }
        });

        // SqlitePool is Arc<T>.
        let state = self.db.clone();

        let app = Router::new()
            .route("/", get(root))
            .route("/blocks/{height}", get(get_block_by_height))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(&self.cfg.host).await?;

        axum::serve(listener, app).await?;
        Ok(())
    }
}

// API handlers

pub async fn root() -> &'static str {
    "Silent Payment Server"
}

pub async fn get_block_by_height(
    State(pool): State<SqlitePool>,
    Path(height): Path<i64>,
) -> String {
    todo!()
}

// DB

struct BlockModel {
    // Primary key
    height: i64,
    hash: String,
    tx_count: i64,
}

struct TransactionModel {
    // Primary key
    id: Option<i64>,
    // References block(height)
    block: i64,
    txid: String,
    scalar: String,
    // Hex encoded public scalar
}

struct OutputModel {
    // Primary key
    id: Option<i64>,
    // References transaction(id).
    tx: i64,
    vout: i64,
    value: i64,
    // Hex encoded scriptPubKey
    script_pub_key: String,
}

async fn add_block(block: silentpayments::SPBlock, pool: &SqlitePool) {
    // FIXME: Sqlite uses i64 for everything but we use u64 a lot..
    let block_model = BlockModel {
        height: block.height as i64,
        hash: block.hash,
        tx_count: block.txs.len() as i64,
    };

    let mut db_tx = pool.begin().await.unwrap();

    // Insert block meta.
    sqlx::query!(
        "INSERT INTO blocks (height, hash, tx_count) VALUES (?, ?, ?)",
        block_model.height,
        block_model.hash,
        block_model.tx_count
    )
    .execute(&mut *db_tx)
    .await
    .unwrap();

    // FIXME: Batch insert all txs and outputs would be better. But I struggled to find out how to
    // do this as I need the tx primary keys when inserting the outputs...there is probably a way
    // but for now let's just add every tx and output iteratively...

    // Insert transactions and its outputs.
    for block_tx in block.txs.iter() {
        let tx_model = TransactionModel {
            id: None,
            block: block.height as i64,
            txid: block_tx.tx.compute_txid().to_string(),
            scalar: block_tx.scalar.clone(),
        };

        let query_result = sqlx::query!(
            r#"
        INSERT INTO transactions (id, block, txid, scalar) VALUES (NULL, ?, ?, ?)
        "#,
            tx_model.block,
            tx_model.txid,
            tx_model.scalar
        )
        .execute(&mut *db_tx)
        .await
        .unwrap();

        let tx_id = query_result.last_insert_rowid();

        for (i, output) in block_tx.tx.output.iter().enumerate() {
            let value_sat = output.value.to_sat() as i64;
            let script_pubkey_hex = output.script_pubkey.to_hex_string();
            let vout = i as i64;
            sqlx::query!(
                r#"
        INSERT INTO outputs (id, tx, vout, value, script_pub_key) VALUES (NULL, ?, ?, ?, ?)
        "#,
                tx_id,
                vout,
                value_sat,
                script_pubkey_hex,
            )
            .execute(&mut *db_tx)
            .await
            .unwrap();
        }
    }

    db_tx.commit().await.unwrap();
}
async fn get_block() {}

async fn get_synced_blocks_height(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar!("SELECT MAX(height) FROM blocks")
        .fetch_one(pool)
        .await
        .unwrap()
        .unwrap()
}

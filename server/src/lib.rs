use bitcoincore_rpc::{Auth, Client};
use store::Store;
use sync::Syncer;
use tokio::sync::mpsc;

use axum::{Router, routing::get};

mod handlers;
mod rpc;
mod silentpayments;
mod store;
mod sync;

// TODO: Bitcoin Core RPC Config.
pub struct ServerConfig {
    pub host: String,
    pub db_url: String,
}

pub struct Server {
    cfg: ServerConfig,
    db: Store,
}

impl Server {
    pub async fn new(cfg: ServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Store::new(cfg.db_url.clone()).await?;

        Ok(Self { cfg, db })
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (sync_tx, mut sync_rx) = mpsc::channel(64);

        let auth = Auth::UserPass("sus".into(), "sus".into());
        let client = Client::new("http://localhost:18443", auth).unwrap();

        // Run syncer.
        let mut syncer = Syncer::new(client, 1000);
        //let sync_from_height = get_synced_blocks_height(&self.db).await as u64;
        let sync_from_height = self.db.get_synced_blocks_height().await as u64;
        tokio::task::spawn(async move { syncer.sync_from(sync_from_height, sync_tx).await });

        // Receive blocks from syncer and add them to DB.
        let sync_db = self.db.clone();
        tokio::task::spawn(async move {
            while let Some(msg) = sync_rx.recv().await {
                println!("new block: {:?}", msg);
                sync_db.add_block(msg).await;
            }
        });

        // SqlitePool is Arc<T>.
        let state = self.db.clone();

        let app = Router::new()
            .route("/", get(handlers::root))
            .route("/blocks/tip", get(handlers::get_chain_tip))
            .route("/blocks/{height}", get(handlers::get_block_by_height))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(&self.cfg.host).await?;

        axum::serve(listener, app).await?;
        Ok(())
    }
}

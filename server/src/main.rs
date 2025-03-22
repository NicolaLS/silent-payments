use bitcoincore_rpc::{Auth, Client};
use silent_payments_server::server::{Server, ServerConfig};
use tokio::sync::mpsc;

use silent_payments_server::store::Store;
use silent_payments_server::sync::Syncer;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cfg = ServerConfig {
        host: "127.0.0.1:3000".into(),
        db_url: "sqlite://dev.db".into(),
    };

    let db = Store::new(cfg.db_url.clone()).await.unwrap();

    let server_db = db.clone();
    let server = Server::new(cfg, server_db);

    let auth = Auth::UserPass("sus".into(), "sus".into());
    let client = Client::new("http://localhost:18443", auth).unwrap();

    let (sync_tx, mut sync_rx) = mpsc::channel(64);

    // Run syncer.
    let mut syncer = Syncer::new(client, 1000);
    //let sync_from_height = get_synced_blocks_height(&self.db).await as u64;
    let sync_from_height = db.get_synced_blocks_height().await as u64;
    tokio::task::spawn(async move { syncer.sync_from(sync_from_height, sync_tx).await });

    // Receive blocks from syncer and add them to DB.
    let sync_db = db.clone();
    tokio::task::spawn(async move {
        while let Some(msg) = sync_rx.recv().await {
            println!("new block: {:?}", msg);
            sync_db.add_block(msg).await;
        }
    });

    server.run().await.unwrap();
}

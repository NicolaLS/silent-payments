use bitcoincore_rpc::{Auth, Client};
use silent_payments_server::server::{Server, ServerConfig};

use silent_payments_server::Result;
use silent_payments_server::store::Store;
use silent_payments_server::sync::Syncer;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::from_default_env()
        .add_directive("silent_payments_server=debug".parse().unwrap());
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cfg = ServerConfig {
        host: "127.0.0.1:3000".into(),
        db_url: "sqlite://dev.db".into(),
    };

    let db = Store::new(cfg.db_url.clone()).await?;

    let server_db = db.clone();
    let server = Server::new(cfg, server_db);

    let auth = Auth::UserPass("sus".into(), "sus".into());
    let client = Client::new("http://localhost:18443", auth)?;

    // Run syncer.
    info!("Running syncer in task");
    let mut syncer = Syncer::new(client, db.clone(), 1000);
    tokio::task::spawn(async move { syncer.sync_from().await });

    // Subscribe blocks that were added to DB.
    info!("Subscibing to blocks in task.");
    let sub_db = db.clone();
    tokio::task::spawn(async move {
        let mut rx = sub_db.subscribe_blocks();
        loop {
            match rx.recv().await {
                Ok(block) => info!("new block: {:?}", block),

                Err(_) => {
                    info!("Sender (store) dropped.");
                    break;
                }
            }
        }
    });

    server.run().await?;
    Ok(())
}

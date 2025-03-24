use bitcoincore_rpc::{Auth, Client};
use silent_payments_server::server::{Server, ServerConfig};

use silent_payments_server::Result;
use silent_payments_server::store::Store;
use silent_payments_server::sync::Syncer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

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
    let mut syncer = Syncer::new(client, db.clone(), 1000);
    tokio::task::spawn(async move { syncer.sync_from().await });

    // Subscribe blocks that were added to DB.
    let sub_db = db.clone();
    tokio::task::spawn(async move {
        let mut rx = sub_db.subscribe_blocks();
        loop {
            match rx.recv().await {
                Ok(block) => {
                    println!("new block: {:?}", block);
                }
                Err(_) => {
                    println!("Subscription dropped");
                }
            }
        }
    });

    server.run().await?;
    Ok(())
}

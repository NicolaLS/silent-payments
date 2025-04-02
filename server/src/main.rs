use bitcoincore_rpc::{Auth, Client};
use silent_payments_server::config::Config;
use silent_payments_server::server::Server;

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

    let cfg = Config::try_from_env()?;

    let db = Store::new(cfg.database).await?;

    let server_db = db.clone();
    let server = Server::new(cfg.server, server_db);

    let rpcurl = cfg.syncer.rpc_url.clone();
    let rpcuser = cfg.syncer.rpc_user.clone();
    let rpcpass = cfg.syncer.rpc_pass.clone();
    let auth = Auth::UserPass(rpcuser.into(), rpcpass.into());
    let client = Client::new(&rpcurl, auth)?;

    // Run syncer.
    info!("Running syncer in task");
    let mut syncer = Syncer::new(cfg.syncer, client, db.clone());
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

use server::ServerConfig;

use::server::Server;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cfg = ServerConfig { host: "127.0.0.1:3000".into(), db_url: "sqlite://dev.db".into() };

    let server = Server::new(cfg).await.unwrap();
    server.run().await.unwrap();



    // Task that syncs. blocks filters them calculates tweaks and adds them to DB.
    // Websokcet handlers for subscriptions that have a receiver per socket for notifications
    // (broadcast channel ?).
    // Handlers get stuff from database and return it.
    // Wallet task that is spawned per wallet created and first scanns for its outputs and then
    // watches new blocks, so either also needs a receiver like the websockets or poll db.
    //
    //
    // Filtering: Create a HashMap of txids from all outpoints to avoid getrawtransaction more than
    // once. Also try to get tx. from DB first before using RPC.
    //
    // Tweaking: You could wait for enough tweaks to come in but honestly just do it per tx. Or
    // even do it on demand. Make the field nullable and when a client requets then calculate it.
    // Well but this does not work for subscriptions.
}

use server::ServerConfig;

use::server::Server;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cfg = ServerConfig { host: "127.0.0.1:3000".into(), db_url: "sqlite://dev.db".into() };

    let server = Server::new(cfg).await.unwrap();
    server.run().await.unwrap();
}

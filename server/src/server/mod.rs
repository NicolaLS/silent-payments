use axum::{Router, routing::get};
use tracing::info;

use crate::Result;
use crate::store::Store;

mod handlers;

pub struct ServerConfig {
    pub host: String,
    pub db_url: String,
}

pub struct Server {
    cfg: ServerConfig,
    db: Store,
}

impl Server {
    pub fn new(cfg: ServerConfig, db: Store) -> Self {
        Self { cfg, db }
    }

    pub async fn run(&self) -> Result<()> {
        // SqlitePool is Arc<T>.
        let state = self.db.clone();

        let app = Router::new()
            .route("/", get(handlers::root))
            .route("/blocks/tip", get(handlers::get_chain_tip))
            .route("/blocks/latest/scalars", get(handlers::get_latest_scalars))
            .route(
                "/blocks/latest/transactions",
                get(handlers::get_latest_transactions),
            )
            .route(
                "/blocks/height/{height}/scalars",
                get(handlers::get_scalars),
            )
            .route(
                "/blocks/height/{height}/transactions",
                get(handlers::get_transactions),
            )
            .route("/transactions/{txid}", get(handlers::get_transaction))
            .route("/transactions/{txid}/scalar", get(handlers::get_scalar))
            .route("/ws", get(handlers::ws_subscribe_handler))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(&self.cfg.host).await?;

        info!("HTTP Server listening on: {}", self.cfg.host);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

use axum::{Router, routing::get};

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
            .route("/blocks/{height}", get(handlers::get_block_by_height))
            .route("/ws", get(handlers::ws_subscribe_handler))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(&self.cfg.host).await?;

        axum::serve(listener, app).await?;
        Ok(())
    }
}

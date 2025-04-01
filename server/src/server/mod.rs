use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use tracing::info;

use crate::config::ServerConfig;
use crate::store::Store;
use crate::{Error, Result};

mod handler;

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
            .route("/", get(handler::root))
            .route("/blocks/tip", get(handler::get_chain_tip))
            .route("/blocks/latest/scalars", get(handler::get_latest_scalars))
            .route(
                "/blocks/latest/transactions",
                get(handler::get_latest_transactions),
            )
            .route("/blocks/height/{height}/scalars", get(handler::get_scalars))
            .route(
                "/blocks/height/{height}/transactions",
                get(handler::get_transactions),
            )
            .route("/transactions/{txid}", get(handler::get_transaction))
            .route("/transactions/{txid}/scalar", get(handler::get_scalar))
            .route(
                "/ws/scalars",
                get(|state, ws| {
                    handler::ws_subscribe(state, ws, handler::SubscriptionKind::Scalars)
                }),
            )
            .route(
                "/ws/transactions",
                get(|state, ws| {
                    handler::ws_subscribe(state, ws, handler::SubscriptionKind::Transactions)
                }),
            )
            .with_state(state);

        let host = format!("{}:{}", self.cfg.server_host, self.cfg.server_port);
        let listener = tokio::net::TcpListener::bind(&host).await?;

        info!("HTTP Server listening on: {}", host);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::NotFound => StatusCode::NOT_FOUND.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

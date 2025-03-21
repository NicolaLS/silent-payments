use axum::extract::{Path, State};

use crate::store::Store;

pub async fn root() -> &'static str {
    "Silent Payment Server"
}

pub async fn get_chain_tip(State(db): State<Store>) -> String {
    db.get_synced_blocks_height().await.to_string()
}
pub async fn get_block_by_height(State(_db): State<Store>, Path(_height): Path<i64>) -> String {
    todo!()
}

// TODO: Consider implementing a trait "DB" for better testability.

use std::str::FromStr;

use sqlx::sqlite::SqliteQueryResult;
use sqlx::{Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
use tokio::sync::broadcast;
use tracing::info;

use crate::Result;
use crate::SPBlock;

pub struct BlockModel {
    pub height: i64,
    pub hash: String,
    pub tx_count: i64,
}

pub struct TransactionModel {
    #[allow(dead_code)]
    pub id: Option<i64>,
    pub block: i64,
    pub txid: String,
    pub scalar: String,
}

pub struct OutputModel {
    #[allow(dead_code)]
    pub id: Option<i64>,
    pub tx: i64,
    pub vout: i64,
    pub value: i64,
    pub script_pub_key: String,
}

pub struct TransactionRecord {
    pub txid: String,
    pub scalar: String,
    pub vout: i64,
    pub value: i64,
    pub script_pub_key: String,
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    sub_tx: broadcast::Sender<SPBlock>,
}

impl Store {
    pub async fn new(url: String) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(&url)?.create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;

        let (sub_tx, _) = broadcast::channel(512);

        Ok(Self { pool, sub_tx })
    }

    pub fn subscribe_blocks(&self) -> broadcast::Receiver<SPBlock> {
        self.sub_tx.subscribe()
    }

    pub async fn get_latest_scalars(&self) -> Vec<String> {
        sqlx::query_scalar!(
            "SELECT scalar FROM transactions WHERE block = (SELECT MAX(height) FROM blocks)"
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    pub async fn get_scalars_by_height(&self, height: i64) -> Vec<String> {
        sqlx::query_scalar!("SELECT scalar FROM transactions WHERE block = ?", height)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }

    pub async fn get_scalar_by_txid(&self, txid: String) -> String {
        sqlx::query_scalar!("SELECT scalar FROM transactions WHERE txid = ?", txid)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    pub async fn get_latest_transactions(&self) -> Vec<TransactionRecord> {
        sqlx::query_as!(
            TransactionRecord,
            r#"
        SELECT 
            t.txid, 
            t.scalar, 
            o.vout, 
            o.value, 
            o.script_pub_key 
        FROM transactions t
        INNER JOIN outputs o ON t.id = o.tx
        WHERE t.block = (SELECT MAX(height) FROM blocks)
        "#,
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    pub async fn get_transactions_by_height(&self, height: i64) -> Vec<TransactionRecord> {
        sqlx::query_as!(
            TransactionRecord,
            r#"
        SELECT 
            t.txid, 
            t.scalar, 
            o.vout, 
            o.value, 
            o.script_pub_key 
        FROM transactions t
        INNER JOIN outputs o ON t.id = o.tx
        WHERE t.block = ? 
        "#,
            height
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    // Returns Vec aswell because we use join to get the outputs. This means one transaction with
    // e.g. three outputs will result in three TransactionRecord.
    pub async fn get_transaction_by_txid(&self, txid: String) -> Vec<TransactionRecord> {
        sqlx::query_as!(
            TransactionRecord,
            r#"
        SELECT 
            t.txid, 
            t.scalar, 
            o.vout, 
            o.value, 
            o.script_pub_key 
        FROM transactions t
        INNER JOIN outputs o ON t.id = o.tx
        WHERE t.txid = ? 
        "#,
            txid
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    pub async fn get_synced_blocks_height(&self) -> Result<Option<i64>> {
        let height = sqlx::query_scalar!("SELECT MAX(height) FROM blocks")
            .fetch_one(&self.pool)
            .await?;
        Ok(height)
    }

    async fn add_output(
        db_tx: &mut sqlx::Transaction<'static, Sqlite>,
        output_model: OutputModel,
    ) -> Result<SqliteQueryResult> {
        let query_result = sqlx::query!(
            r#"
        INSERT INTO outputs (id, tx, vout, value, script_pub_key) VALUES (NULL, ?, ?, ?, ?)
        "#,
            output_model.tx,
            output_model.vout,
            output_model.value,
            output_model.script_pub_key,
        )
        .execute(&mut **db_tx)
        .await?;
        Ok(query_result)
    }

    async fn add_transaction(
        db_tx: &mut sqlx::Transaction<'static, Sqlite>,
        tx: TransactionModel,
    ) -> Result<i64> {
        let query_result = sqlx::query!(
            r#"
        INSERT INTO transactions (id, block, txid, scalar) VALUES (NULL, ?, ?, ?)
        "#,
            tx.block,
            tx.txid,
            tx.scalar,
        )
        .execute(&mut **db_tx)
        .await?;

        Ok(query_result.last_insert_rowid())
    }
    async fn add_block_meta(
        db_tx: &mut sqlx::Transaction<'static, Sqlite>,
        block_model: BlockModel,
    ) -> Result<SqliteQueryResult> {
        let query_result = sqlx::query!(
            "INSERT INTO blocks (height, hash, tx_count) VALUES (?, ?, ?)",
            block_model.height,
            block_model.hash,
            block_model.tx_count
        )
        .execute(&mut **db_tx)
        .await?;
        Ok(query_result)
    }
    pub async fn add_block(&self, block: SPBlock) -> Result<()> {
        let mut db_tx = self.pool.begin().await?;

        let block_model_height = block.height.try_into().expect("FIXME: Store as BLOB");
        let block_model_tx_count = block
            .txs
            .len()
            .try_into()
            .expect("txs length not larger than i64");

        let block_model = BlockModel {
            height: block_model_height,
            hash: block.hash.clone(),
            tx_count: block_model_tx_count,
        };

        Store::add_block_meta(&mut db_tx, block_model).await?;

        // FIXME: Batch insert would be better, but now sure how to insert the outputs then since
        // they refer to the transaction db primary keys.
        for tx in block.txs.iter() {
            let tx_model = TransactionModel {
                id: None,
                block: block.height as i64,
                txid: tx.tx.compute_txid().to_string(),
                scalar: tx.scalar.clone(),
            };

            let tx_id = Store::add_transaction(&mut db_tx, tx_model).await?;

            for (i, output) in tx.tx.output.iter().enumerate() {
                let value = output.value.to_sat() as i64;
                let script_pub_key = output.script_pubkey.to_hex_string();
                let vout = i as i64;
                let output_model = OutputModel {
                    id: None,
                    tx: tx_id,
                    vout,
                    value,
                    script_pub_key,
                };
                Store::add_output(&mut db_tx, output_model).await?;
            }
        }

        db_tx.commit().await?;
        match self.sub_tx.send(block) {
            Ok(num_sub) => info!("Notified {} subscribers of new block.", num_sub),
            Err(_) => info!("There are no subscribers for new blocks."),
        }
        Ok(())
    }
}

impl From<SPBlock> for BlockModel {
    fn from(value: SPBlock) -> Self {
        let height = value.height.try_into().expect("FIXME: Store as BLOB");
        let tx_count = value
            .txs
            .len()
            .try_into()
            .expect("txs length not larger than i64");
        Self {
            height,
            hash: value.hash,
            tx_count,
        }
    }
}

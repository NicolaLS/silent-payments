use std::str::FromStr;

use model::Output;
use model::{
    Block, JoinedTransactionOutput, JoinedTransactionOutputCollection, Scalar, Scalars,
    Transaction, Transactions,
};
use sqlx::sqlite::SqliteQueryResult;
use sqlx::{Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
use tokio::sync::broadcast;
use tracing::debug;

use crate::config::DatabaseConfig;
use crate::Result;

pub mod model;

// FIXME: Right now in case the store is queried with height or txid it might return an empty vec.
// However an empty vec could mean the height/txid exists but there is no txs/scalars for example,
// or that the provided height/txid was wrong. In the latter case I'd like to return 404 not found
// instead of an empty vec...but using fetch_all with the single query style I use right now, I
// don't know if the parameter was wrong, or there's just no data.. Need to solve this elegantly..
// for now I'll just return empty vecs..

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    sub_tx: broadcast::Sender<Block>,
}

impl Store {
    pub async fn new(cfg: DatabaseConfig) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(&cfg.database_url)?.create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await?;

        let (sub_tx, _) = broadcast::channel(512);

        Ok(Self { pool, sub_tx })
    }

    async fn insert_output<'a>(
        db_tx: &mut sqlx::Transaction<'a, Sqlite>,
        output: &Output,
        tx_id: i64,
    ) -> Result<SqliteQueryResult> {
        let query_result = sqlx::query!(
            r#"
        INSERT INTO outputs (id, tx, vout, value, script_pub_key) VALUES (NULL, ?, ?, ?, ?)
        "#,
            tx_id,
            output.vout,
            output.value,
            output.spk,
        )
        .execute(&mut **db_tx)
        .await?;
        Ok(query_result)
    }

    async fn insert_transaction<'a>(
        db_tx: &mut sqlx::Transaction<'a, Sqlite>,
        transaction: &Transaction,
        block_height: i64,
    ) -> Result<SqliteQueryResult> {
        let query_result = sqlx::query_scalar!(
            r#"
        INSERT INTO transactions (id, block, txid, scalar) VALUES (NULL, ?, ?, ?)
            "#,
            block_height,
            transaction.txid,
            transaction.scalar,
        )
        .execute(&mut **db_tx)
        .await?;

        Ok(query_result)
    }

    async fn insert_transactions<'a>(
        db_tx: &mut sqlx::Transaction<'a, Sqlite>,
        transactions: &[Transaction], // Use slice for flexibility
        block_height: i64,
    ) -> Result<()> {
        for transaction in transactions.iter() {
            let id = Store::insert_transaction(db_tx, transaction, block_height)
                .await?
                .last_insert_rowid();

            for output in transaction.outputs.iter() {
                Store::insert_output(db_tx, output, id).await?;
            }
        }

        Ok(())
    }

    async fn insert_block(
        db_tx: &mut sqlx::Transaction<'static, Sqlite>,
        block: &Block,
    ) -> Result<SqliteQueryResult> {
        let tx_count = block.transactions.len() as i64;
        let query_result = sqlx::query!(
            "INSERT INTO blocks (height, hash, tx_count) VALUES (?, ?, ?)",
            block.height,
            block.hash,
            tx_count
        )
        .execute(&mut **db_tx)
        .await?;
        Ok(query_result)
    }

    pub fn subscribe_blocks(&self) -> broadcast::Receiver<Block> {
        self.sub_tx.subscribe()
    }

    pub async fn get_latest_scalars(&self) -> Result<Scalars> {
        let scalars = sqlx::query_scalar!(
            "SELECT scalar FROM transactions WHERE block = (SELECT MAX(height) FROM blocks)"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(Scalars { scalars })
    }

    pub async fn get_scalars_by_height(&self, height: i64) -> Result<Scalars> {
        let scalars =
            sqlx::query_scalar!("SELECT scalar FROM transactions WHERE block = ?", height)
                .fetch_all(&self.pool)
                .await?;

        Ok(Scalars { scalars })
    }

    pub async fn get_scalar_by_txid(&self, txid: String) -> Result<Option<Scalar>> {
        let scalar = sqlx::query_scalar!("SELECT scalar FROM transactions WHERE txid = ?", txid)
            .fetch_optional(&self.pool)
            .await?;

        Ok(scalar.map(|scalar| Scalar { scalar }))
    }

    pub async fn get_latest_transactions(&self) -> Result<Transactions> {
        let collection: JoinedTransactionOutputCollection = sqlx::query_as!(
            JoinedTransactionOutput,
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
        .await?
        .into();

        Ok(collection.into())
    }

    pub async fn get_transactions_by_height(&self, height: i64) -> Result<Transactions> {
        let collection: JoinedTransactionOutputCollection = sqlx::query_as!(
            JoinedTransactionOutput,
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
        .await?
        .into();

        Ok(collection.into())
    }

    // Returns Vec aswell because we use join to get the outputs. This means one transaction with
    // e.g. three outputs will result in three TransactionRecord.
    pub async fn get_transaction_by_txid(&self, txid: String) -> Result<Option<Transaction>> {
        let collection: JoinedTransactionOutputCollection = sqlx::query_as!(
            JoinedTransactionOutput,
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
        .await?
        .into();

        Ok(collection.into())
    }

    pub async fn get_synced_blocks_height(&self) -> Result<Option<i64>> {
        let height = sqlx::query_scalar!("SELECT MAX(height) FROM blocks")
            .fetch_one(&self.pool)
            .await?;
        Ok(height)
    }

    pub async fn add_block(&self, block: Block) -> Result<()> {
        let mut db_tx = self.pool.begin().await?;

        Store::insert_block(&mut db_tx, &block).await?;
        Store::insert_transactions(&mut db_tx, &block.transactions, block.height).await?;

        db_tx.commit().await?;

        self.notify_subscribers(block);

        Ok(())
    }

    fn notify_subscribers(&self, block: Block) {
        match self.sub_tx.send(block) {
            Ok(num_sub) => debug!("Notified {} subscribers of new block.", num_sub),
            Err(_) => debug!("There are no subscribers for new blocks."),
        }
    }
}

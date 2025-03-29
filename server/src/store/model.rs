use std::collections::HashMap;

use serde::Serialize;

// Models for DB table schema.

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

// Intermediate utility types.
pub struct JoinedTransactionOutput {
    pub txid: String,
    pub scalar: String,
    pub vout: i64,
    pub value: i64,
    pub script_pub_key: String,
}

pub struct JoinedTransactionOutputCollection(pub Vec<JoinedTransactionOutput>);

// ORM-like method return types. Serialized as responses for REST API/WS.

#[derive(Serialize)]
pub struct Scalar {
    pub scalar: String,
}

#[derive(Serialize)]
pub struct Scalars {
    pub scalars: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct Output {
    pub vout: usize,
    pub value: u64,
    pub spk: String,
}

#[derive(Serialize)]
pub struct Transaction {
    pub txid: String,
    pub scalar: String,
    pub outputs: Vec<Output>,
}

#[derive(Serialize)]
pub struct Transactions {
    pub transactions: Vec<Transaction>,
}

impl From<Vec<JoinedTransactionOutput>> for JoinedTransactionOutputCollection {
    fn from(value: Vec<JoinedTransactionOutput>) -> Self {
        Self(value)
    }
}
impl From<JoinedTransactionOutputCollection> for Option<Transaction> {
    fn from(value: JoinedTransactionOutputCollection) -> Self {
        if value.0.is_empty() {
            return None;
        }
        // All records will have the same txid and scalar. Only the output fields will be different
        // because this comes from a join sql query.
        let txid = value.0[0].txid.clone();
        let scalar = value.0[0].scalar.clone();
        let mut outputs = vec![];

        for record in value.0.iter() {
            outputs.push(Output {
                vout: record.vout as usize,
                value: record.value as u64,
                spk: record.script_pub_key.clone(),
            });
        }

        Some(Transaction {
            txid,
            scalar,
            outputs,
        })
    }
}

impl From<JoinedTransactionOutputCollection> for Transactions {
    fn from(value: JoinedTransactionOutputCollection) -> Self {
        let mut tx_map: HashMap<String, Transaction> = HashMap::new();

        for tx_record in value.0.iter() {
            let txid = tx_record.txid.clone();
            let scalar = tx_record.scalar.clone();

            let output = Output {
                vout: tx_record.vout as usize,
                value: tx_record.value as u64,
                spk: tx_record.script_pub_key.clone(),
            };

            tx_map
                .entry(txid.clone())
                .and_modify(|tx| tx.outputs.push(output.clone()))
                .or_insert_with(|| Transaction {
                    txid,
                    scalar,
                    outputs: vec![output],
                });
        }

        let transactions = tx_map.into_values().collect::<Vec<Transaction>>();
        Transactions { transactions }
    }
}

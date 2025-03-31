use std::collections::HashMap;

use serde::Serialize;

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

#[derive(Debug, Clone)]
pub struct Block {
    pub height: i64,
    pub hash: String,
    pub transactions: Vec<Transaction>,
}
#[derive(Serialize)]
pub struct Scalar {
    pub scalar: String,
}

#[derive(Serialize)]
pub struct Scalars {
    pub scalars: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Output {
    pub vout: i64,
    pub value: i64,
    pub spk: String,
}

#[derive(Serialize, Debug, Clone)]
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
                vout: record.vout,
                value: record.value,
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
                vout: tx_record.vout,
                value: tx_record.value,
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

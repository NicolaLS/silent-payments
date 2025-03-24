use std::usize;

use bitcoincore_rpc::bitcoin::{OutPoint, Transaction, TxIn, TxOut, WitnessVersion};

pub mod server;
pub mod store;
pub mod sync;

mod error;

pub use self::error::{Error, Result};

pub fn has_taproot_outputs(tx: &Transaction) -> bool {
    tx.output.iter().any(|txout| txout.script_pubkey.is_p2tr())
}

pub fn has_output_witness_version_greater_v1(outputs: &Vec<TxOut>) -> bool {
    outputs
        .iter()
        .any(|txout| output_witness_version_greater_v1(txout))
}

pub fn has_input_for_shared_secret(inputs: &Vec<TxIn>, prevouts: &Vec<TxOut>) -> bool {
    inputs
        .iter()
        .zip(prevouts)
        .any(|(txin, prevout)| is_input_for_shared_secret(txin, prevout))
}

pub fn output_witness_version_greater_v1(txout: &TxOut) -> bool {
    match txout.script_pubkey.witness_version() {
        Some(version) => version > WitnessVersion::V1,
        None => false,
    }
}

pub fn is_input_for_shared_secret(txin: &TxIn, prevout: &TxOut) -> bool {
    let s = &prevout.script_pubkey;
    let is_p2wpkh_witness = txin.witness.len() == 2;
    s.is_p2tr() || s.is_p2wpkh() || s.is_p2pkh() || (s.is_p2sh() && is_p2wpkh_witness)
}

pub fn sum_input_public_keys(
    inputs: &Vec<TxIn>,
    prevouts: &Vec<TxOut>,
) -> Option<(usize, OutPoint)> {
    let mut input_pk_sum: Option<usize> = None;
    let mut lex_low_outpoint = inputs
        .get(0)
        .expect("tx. has at least one input")
        .previous_output;
    for (txin, prevout) in inputs.iter().zip(prevouts) {
        if !is_input_for_shared_secret(txin, prevout) {
            continue;
        }

        if txin.previous_output < lex_low_outpoint {
            lex_low_outpoint = txin.previous_output;
        }

        let input_public_key = get_input_public_key(txin, prevout)
            .expect("can get public key from input otherwise it would have been skipped");
        if let Some(sum) = input_pk_sum.as_mut() {
            *sum += input_public_key;
        } else {
            let _ = input_pk_sum.insert(input_public_key);
        }
    }
    input_pk_sum.map(|sum| (sum, lex_low_outpoint))
}
pub fn get_input_public_key(_txin: &TxIn, _prevout: &TxOut) -> Result<usize> {
    Ok(0)
}

#[derive(Debug, Clone)]
pub struct SPBlock {
    pub height: u64,
    pub hash: String,
    pub txs: Vec<SPTransaction>,
}

#[derive(Debug, Clone)]
pub struct SPTransaction {
    // Edited bitcoin transaction. Contains only taproot outputs.
    pub tx: Transaction,
    // Hex encoded public tweak.
    pub scalar: String,
}

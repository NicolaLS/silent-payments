use std::str::FromStr;

use bitcoincore_rpc::bitcoin::{
    Block, OutPoint, PublicKey, ScriptBuf, Transaction, TxIn, TxOut, WitnessVersion,
};

fn get_input_public_key(txin: &TxIn) -> PublicKey {
    PublicKey::from_str("03e262cb486947a9601ffbc064818afe6cb0d545830ac4e693706cbd4b12171dfe")
        .unwrap()
}

fn is_input_for_shared_secret(s: &ScriptBuf, is_p2wpkh_witness: bool) -> bool {
    s.is_p2tr() || s.is_p2wpkh() || s.is_p2pkh() || (s.is_p2sh() && is_p2wpkh_witness)
}

fn filter_and_process_txs<F>(tx: &mut Transaction, prevout_getter: &mut F) -> Option<SPTransaction>
where
    F: FnMut(&OutPoint) -> bitcoincore_rpc::Result<TxOut>,
{
    // Coinbase transactions can't be used.
    if tx.is_coinbase() {
        println!("tx is coinbase return None");
        return None;
    }

    // The transaction contains at least one BIP341 taproot output (note: spent transactions
    // optionally can be skipped by only considering transactions with at least one unspent taproot
    // output)
    for output in tx.output.iter() {
        println!("Script hex: {}", output.script_pubkey.to_hex_string());
        println!("Is taproot: {}", output.script_pubkey.is_p2tr());
    }
    tx.output.retain(|txout| txout.script_pubkey.is_p2tr());

    if tx.output.is_empty() {
        println!("tx does not have taproot outputs");
        return None;
    }

    let mut lexographically_lowest_outpoint = tx
        .input
        .get(0)
        .expect("Transaction has at least one input")
        .previous_output;

    // TODO: Use Public Keys
    let mut sum_public_keys: Option<u8> = None;

    for txin in tx.input.iter() {
        // FIXME: Unwrap.
        let prevout = prevout_getter(&txin.previous_output).unwrap();
        let prevout_script_pubkey = &prevout.script_pubkey;

        // The transaction does not spend an output with SegWit version > 1
        if let Some(version) = prevout_script_pubkey.witness_version() {
            if version > WitnessVersion::V1 {
                println!("tx spends input witness versoin above 1, return None");
                return None;
            }
        }

        // NOTE: This has to come after checking the version. The BIP explicitly says that no input
        // at all can spend greater witness version 1. Probably should refactor this.

        let is_p2wpkh_witness = txin.witness.len() == 2;
        if !is_input_for_shared_secret(prevout_script_pubkey, is_p2wpkh_witness) {
            // In case no inputs are eligible sum_public_keys will be None and this transaction is
            // not eligible.
            println!("input not eligible");
            continue;
        }

        // TODO: Sum input public keys. No sum means no eligible inputs.
        let public_key = get_input_public_key(txin);
        if let Some(public_key_sum) = sum_public_keys {
            // TODO: CAn't use bitocin Public Key need to use secp.
            // -> Actually use sum public key, for now just do nothing..
            let _ = sum_public_keys.insert(public_key_sum);
        } else {
            let _ = sum_public_keys.insert(69);
        }

        // Note that bitcoin::OutPoint derives Eq, Ord, PartialOrd.
        // TODO: Check if this ordering derived by Outpoint is what we want here.
        if txin.previous_output < lexographically_lowest_outpoint {
            lexographically_lowest_outpoint = txin.previous_output;
        }
    }

    // The transaction has at least one input from the Inputs For Shared Secret Derivation list. In
    // case there was no eligible input the sum of public keys will be None.
    if let Some(public_key_sum) = sum_public_keys {
        Some(SPTransaction {
            tx: tx.clone(),
            scalar: "".into(),
        })
    } else {
        println!("no eligible inputs, pk sum is None");
        None
    }
}

#[derive(Debug)]
pub struct SPBlock {
    pub height: u64,
    pub hash: String,
    pub txs: Vec<SPTransaction>,
}

#[derive(Debug)]
pub struct SPTransaction {
    // Edited bitcoin transaction. Contains only taproot outputs.
    pub tx: Transaction,
    // Hex encoded public tweak.
    pub scalar: String,
}

impl SPBlock {
    // TODO: Pass getprevout closure instead of client.
    pub fn new<F: FnMut(&OutPoint) -> bitcoincore_rpc::Result<TxOut>>(
        height: u64,
        mut block: Block,
        prevout_getter: &mut F,
    ) -> Self {
        let txs: Vec<SPTransaction> = block
            .txdata
            .iter_mut()
            .filter_map(|tx| filter_and_process_txs(tx, &mut *prevout_getter))
            .collect();

        let hash = block.block_hash().to_string();
        Self { height, hash, txs }
    }
}

use std::{collections::HashMap, str::FromStr, usize};

use bitcoincore_rpc::bitcoin::{
    Block, OutPoint, PublicKey, ScriptBuf, Transaction, TxIn, TxOut, Txid, WitnessVersion,
};

fn get_input_public_key(txin: &TxIn) -> PublicKey {
    PublicKey::from_str("03e262cb486947a9601ffbc064818afe6cb0d545830ac4e693706cbd4b12171dfe")
        .unwrap()
}

fn is_input_for_shared_secret(s: &ScriptBuf, is_p2wpkh_witness: bool) -> bool {
    s.is_p2tr() || s.is_p2wpkh() || s.is_p2pkh() || (s.is_p2sh() && is_p2wpkh_witness)
}

fn filter_and_process_txs<F>(
    tx: &mut Transaction,
    prevout_cache: &mut PrevOutCache<F>,
) -> Option<SPTransaction> 
where  F: Fn(&OutPoint) -> bitcoincore_rpc::Result<Vec<TxOut>>{
    // Coinbase transactions can't be used.
    if tx.is_coinbase() {
        return None;
    }

    // The transaction contains at least one BIP341 taproot output (note: spent transactions
    // optionally can be skipped by only considering transactions with at least one unspent taproot
    // output)
    tx.output.retain(|txout| txout.script_pubkey.is_p2tr());

    if tx.output.is_empty() {
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
        let prevout = prevout_cache.get_prevout(txin.previous_output).unwrap();
        let prevout_script_pubkey = &prevout.script_pubkey;

        // The transaction does not spend an output with SegWit version > 1
        if let Some(version) = prevout_script_pubkey.witness_version() {
            if version > WitnessVersion::V1 {
                return None;
            }
        }

        // NOTE: This has to come after checking the version. The BIP explicitly says that no input
        // at all can spend greater witness version 1. Probably should refactor this.

        let is_p2wpkh_witness = txin.witness.len() == 2;
        if !is_input_for_shared_secret(prevout_script_pubkey, is_p2wpkh_witness) {
            // In case no inputs are eligible sum_public_keys will be None and this transaction is
            // not eligible.
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

struct PrevOutCache<F> {
    cache: HashMap<Txid, Vec<TxOut>>,
    prevout_getter: F,
}

// NOTE: Could also just be moved into the prevout_getter closure...
impl<F> PrevOutCache<F>
where
    F: Fn(&OutPoint) -> bitcoincore_rpc::Result<Vec<TxOut>>,
{
    // Avoid fetching txs. more than once with bitocin-rpc. Don't get confused on
    // initialization here, the main purpose is that the filter/process function will get the
    // prevout from this, and insert it incase it is not present. The only reason why I
    // initialize it with the txs. from the block is because it is relatively cheap (we don't
    // process multiple blocks concurently right now) so it does not hurt in case there are
    // some inputs that have outpoints that are already in this block.
    fn new(block: &Block, prevout_getter: F) -> Self {
        let cache = block
            .txdata
            .iter()
            .map(|tx| (tx.compute_txid(), tx.output.clone()))
            .collect();
        Self {
            cache,
            prevout_getter,
        }
    }

    fn get_prevout(&mut self, outpoint: OutPoint) -> Result<TxOut, Box<dyn std::error::Error>> {
        if let Some(txouts) = self.cache.get(&outpoint.txid) {
            // FIXME: Error handling here is weird.
            let txout = txouts
                .get(outpoint.vout as usize)
                .ok_or("vout not present")?;
            return Ok(txout.clone());
        }

        let txouts = (self.prevout_getter)(&outpoint)?;
        // FIXME: Error handling here is weird.
        let txout = txouts
            .get(outpoint.vout as usize)
            .ok_or("vout not present")?;
        self.cache.insert(outpoint.txid, txouts.clone());
        Ok(txout.clone())
    }
}
impl SPBlock {
    // TODO: Pass getprevout closure instead of client.
    pub fn new<F: Fn(&OutPoint) -> bitcoincore_rpc::Result<Vec<TxOut>>>(
        height: u64,
        mut block: Block,
        prevout_getter: F,
    ) -> Self {
        // Filtering order:
        // What's the cheapest we can do?
        // Check if tx. has at least one taproot output first! so we don't fetch prevouts for
        // non-eligible txs.

        let mut prevout_cache = PrevOutCache::new(&block, prevout_getter);

        let txs: Vec<SPTransaction> = block
            .txdata
            .iter_mut()
            .filter_map(|tx| filter_and_process_txs(tx, &mut prevout_cache))
            .collect();

        let hash = block.block_hash().to_string();
        Self { height, hash, txs }
    }
}

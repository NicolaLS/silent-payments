use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
    usize,
};

use bitcoincore_rpc::bitcoin::{Block, OutPoint, TxOut};
use secp256k1::{PublicKey, Scalar};
use tokio::time::sleep;
use tracing::{debug, info};

use crate::{
    Result, calculate_input_hash, has_output_witness_version_greater_v1, has_taproot_outputs,
    store::model, try_get_input_public_key,
};
use crate::{config::SyncerConfig, store::Store};

mod rpc;

pub use rpc::BitcionRpc;

pub struct Syncer<C: BitcionRpc> {
    client: C,
    store: Store,
    prevout_cache: PrevoutCache,
    sync_from: i64,
}

#[derive(Debug)]
struct PrevoutCache {
    map: HashMap<OutPoint, Vec<TxOut>>,
    order: VecDeque<OutPoint>,
    size: usize,
}

impl PrevoutCache {
    #[tracing::instrument(name = "PrevoutCache::new" level = "debug")]
    fn new(size: usize) -> Self {
        Self {
            map: HashMap::with_capacity(size),
            order: VecDeque::with_capacity(size),
            size,
        }
    }

    #[tracing::instrument(
        name = "PrevoutCache::insert"
        level = "debug"
        skip(self, key)
        fields(key = %key)
        ret
    )]
    fn insert(&mut self, key: OutPoint, value: Vec<TxOut>) {
        if self.map.len() >= self.size {
            if let Some(oldest_key) = self.order.pop_front() {
                info!("Size limit reached, removing oldest item: {:?}", oldest_key);
                self.map.remove(&oldest_key);
            }
        }

        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    #[tracing::instrument(
        name = "PrevoutCache::get"
        level = "debug"
        skip(self, key)
        fields(key = %key)
        ret
    )]
    fn get(&self, key: &OutPoint) -> Option<&Vec<TxOut>> {
        self.map.get(key)
    }
}

impl<C: BitcionRpc> Syncer<C> {
    pub fn new(cfg: SyncerConfig, client: C, store: Store) -> Self {
        info!("Initializing Syncer.");
        let prevout_cache = PrevoutCache::new(cfg.cache_size);

        Self {
            client,
            store,
            prevout_cache,
            sync_from: cfg.sync_from,
        }
    }

    pub fn get_prevout(&mut self, outpoint: &OutPoint) -> Result<TxOut> {
        if let Some(previous_outputs) = self.prevout_cache.get(outpoint) {
            info!("Got previous outputs from cache.");
            let txout = previous_outputs
                .get(outpoint.vout as usize)
                .expect("vout is present int tx");
            return Ok(txout.clone());
        }

        info!(
            "Previous outputs not in cache. Using Bitcoin Core RPC client to fetch and insert them into cache."
        );
        let previous_outputs = self.client.get_transaction(&outpoint.txid)?.output;
        let txout = previous_outputs
            .get(outpoint.vout as usize)
            .expect("vout is present int tx");
        self.prevout_cache
            .insert(outpoint.clone(), previous_outputs.clone());
        Ok(txout.clone())
    }

    fn process_block(&mut self, block: Block, height: u64) -> Result<model::Block> {
        let block_hash = block.block_hash().to_string();
        info!(
            "Processing new block with hash: {} with {} transactions.",
            block_hash,
            block.txdata.len()
        );
        let mut eligible_txs = vec![];
        for tx in block.txdata.iter() {
            // Filter coinbase.
            if tx.is_coinbase() {
                debug!("Transaction is coinbase. Skipping.");
                continue;
            }

            // The transaction contains at least one BIP341 taproot output (note: spent transactions
            // optionally can be skipped by only considering transactions with at least one unspent taproot
            // output)
            if !has_taproot_outputs(tx) {
                debug!("Transaction has no taproot outputs. Skipping.");
                continue;
            }

            let prevouts = tx
                .input
                .iter()
                .map(|txin| self.get_prevout(&txin.previous_output))
                .collect::<Result<Vec<TxOut>>>()?;

            // The transaction does not spend an output with SegWit version > 1
            if has_output_witness_version_greater_v1(&prevouts) {
                debug!("Transaction spends an output with SegWit version > 1. Skipping.");
                continue;
            }

            // The transaction has at least one input from the Inputs For Shared Secret Derivation list. In
            // case there was no eligible input the sum of public keys will be None.
            let public_keys_for_shared_secret_derivation: Vec<PublicKey> = tx
                .input
                .iter()
                .zip(&prevouts)
                .map(|(input, prevout)| try_get_input_public_key(input, prevout))
                .flatten()
                .collect();

            if public_keys_for_shared_secret_derivation.len() == 0 {
                debug!("Transaction does not have any inputs for shared secret derivation");
                continue;
            }

            let (first_input, rest) = public_keys_for_shared_secret_derivation
                .split_first()
                .unwrap();
            let input_public_key_sum = rest
                .iter()
                .try_fold(*first_input, |acc, item| acc.combine(item))
                .unwrap();

            let lowest_outpoint = tx
                .input
                .iter()
                .map(|txin| txin.previous_output)
                .min()
                .expect("Transaction has at least one input");

            let input_hash = calculate_input_hash(lowest_outpoint, input_public_key_sum);
            let secp = secp256k1::Secp256k1::new();
            let scalar = input_public_key_sum
                .mul_tweak(&secp, &Scalar::from_be_bytes(input_hash).unwrap())
                .unwrap();
            let scalar_bytes = scalar.serialize();
            let scalar_hex = hex::encode(scalar_bytes);

            let relevant_outputs: Vec<model::Output> = tx
                .output
                .iter()
                .enumerate()
                .filter_map(|(i, out)| {
                    if out.script_pubkey.is_p2tr() {
                        Some(model::Output {
                            vout: i as i64,
                            value: out.value.to_sat() as i64,
                            spk: out.script_pubkey.to_hex_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();

            let eligible_tx = model::Transaction {
                txid: tx.compute_txid().to_string(),
                scalar: scalar_hex,
                outputs: relevant_outputs,
            };
            info!("Adding transaction to eligible transactions");
            eligible_txs.push(eligible_tx);
        }
        info!(
            "Eligible transactions after filtering: {}",
            eligible_txs.len()
        );

        Ok(model::Block {
            height: height as i64,
            hash: block_hash,
            transactions: eligible_txs,
        })
    }

    pub async fn sync_from(&mut self) -> Result<()> {
        let mut synced_blocks = self
            .store
            .get_synced_blocks_height()
            .await?
            .unwrap_or(self.sync_from) as u64;

        info!("Start syncing blocks from height: {}", synced_blocks);
        loop {
            let chain_tip = self.client.get_chain_tip()? as u64;
            info!("Got best block height from RPC: {}", chain_tip);

            if synced_blocks < chain_tip {
                info!("Best block height greater than synced height. Fetching new block...");
                let block = self.client.get_block_by_height(synced_blocks + 1)?;
                synced_blocks += 1;

                let block = self.process_block(block, synced_blocks)?;
                info!("Proccessed block successfully");

                self.store.add_block(block).await?;
            } else {
                info!("Already synced up to this height. Waiting 5 seconds.");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bitcoincore_rpc::bitcoin::{Amount, ScriptBuf, Txid};

    use super::*;

    #[test]
    fn test_prevout_cache() {
        let mut outpoints = vec![];
        let mut txouts = vec![];

        for i in 0..10 {
            let txid =
                Txid::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
                    .unwrap();
            let outpoint = OutPoint::new(txid, i);
            outpoints.push(outpoint);

            let txout_dummy = TxOut {
                value: Amount::from_sat(i.into()),
                script_pubkey: ScriptBuf::new(),
            };
            txouts.push(vec![txout_dummy]);
        }

        // Cache size 5 outpoint vecs.
        let mut cache = PrevoutCache::new(5);

        for op in outpoints.iter() {
            assert_eq!(cache.get(&op), None);
        }

        cache.insert(outpoints[0], txouts[0].clone());
        assert_eq!(cache.get(&outpoints[0]), Some(&txouts[0]));
        assert_ne!(cache.get(&outpoints[0]), Some(&txouts[1]));

        // Insert 5 more so the first one should be dropped.
        cache.insert(outpoints[1], txouts[1].clone());
        cache.insert(outpoints[2], txouts[2].clone());
        cache.insert(outpoints[3], txouts[3].clone());
        cache.insert(outpoints[4], txouts[4].clone());
        assert_eq!(cache.get(&outpoints[0]), Some(&txouts[0]));
        cache.insert(outpoints[5], txouts[5].clone());
        assert_eq!(cache.get(&outpoints[0]), None);
        assert!(cache.map.len() <= cache.size);
        // Insert all, now only num. 5 to 10 (index 4 to 9) should be some.
        cache.insert(outpoints[6], txouts[6].clone());
        cache.insert(outpoints[7], txouts[7].clone());
        cache.insert(outpoints[8], txouts[8].clone());
        cache.insert(outpoints[9], txouts[9].clone());

        assert!(cache.map.len() <= cache.size);

        for i in 0..5 {
            assert_eq!(cache.get(&outpoints[i]), None);
        }

        for i in 5..10 {
            assert_eq!(cache.get(&outpoints[i]), Some(&txouts[i]));
        }
    }
}

use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
    usize,
};

use bitcoincore_rpc::bitcoin::{Block, OutPoint, TxOut};
use tokio::time::sleep;

use crate::{
    Result, SPTransaction, has_output_witness_version_greater_v1, has_taproot_outputs,
    sum_input_public_keys,
};
use crate::{SPBlock, store::Store};

mod rpc;

pub use rpc::BitcionRpc;

pub struct Syncer<C: BitcionRpc> {
    client: C,
    store: Store,
    prevout_cache: PrevoutCache,
}

struct PrevoutCache {
    map: HashMap<OutPoint, Vec<TxOut>>,
    order: VecDeque<OutPoint>,
    size: usize,
}

impl PrevoutCache {
    fn new(size: usize) -> Self {
        Self {
            map: HashMap::with_capacity(size),
            order: VecDeque::with_capacity(size),
            size,
        }
    }

    fn insert(&mut self, key: OutPoint, value: Vec<TxOut>) {
        if self.map.len() >= self.size {
            if let Some(oldest_key) = self.order.pop_front() {
                self.map.remove(&oldest_key);
            }
        }

        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    fn get(&self, key: &OutPoint) -> Option<&Vec<TxOut>> {
        self.map.get(key)
    }
}

impl<C: BitcionRpc> Syncer<C> {
    pub fn new(client: C, store: Store, cache_size: usize) -> Self {
        let prevout_cache = PrevoutCache::new(cache_size);

        Self {
            client,
            store,
            prevout_cache,
        }
    }

    pub fn get_prevout(&mut self, outpoint: &OutPoint) -> Result<TxOut> {
        if let Some(previous_outputs) = self.prevout_cache.get(outpoint) {
            let txout = previous_outputs
                .get(outpoint.vout as usize)
                .expect("vout is present int tx");
            return Ok(txout.clone());
        }

        let previous_outputs = self.client.get_transaction(&outpoint.txid)?.output;
        let txout = previous_outputs
            .get(outpoint.vout as usize)
            .expect("vout is present int tx");
        self.prevout_cache
            .insert(outpoint.clone(), previous_outputs.clone());
        Ok(txout.clone())
    }

    fn process_block(&mut self, block: Block, height: u64) -> Result<SPBlock> {
        let block_hash = block.block_hash().to_string();
        let mut eligible_txs = vec![];
        for tx in block.txdata.iter() {
            // Filter coinbase.
            if tx.is_coinbase() {
                continue;
            }

            // The transaction contains at least one BIP341 taproot output (note: spent transactions
            // optionally can be skipped by only considering transactions with at least one unspent taproot
            // output)
            if !has_taproot_outputs(tx) {
                continue;
            }

            let prevouts = tx
                .input
                .iter()
                .map(|txin| self.get_prevout(&txin.previous_output))
                .collect::<Result<Vec<TxOut>>>()?;

            // The transaction does not spend an output with SegWit version > 1
            if has_output_witness_version_greater_v1(&prevouts) {
                continue;
            }

            // The transaction has at least one input from the Inputs For Shared Secret Derivation list. In
            // case there was no eligible input the sum of public keys will be None.
            if let Some((sum, _lex_low_outpoint)) = sum_input_public_keys(&tx.input, &prevouts) {
                // Create the SPTransaction
                // TODO: Use real public key sum and input hash to compute the public
                // tweak.
                let mut edited_tx = tx.clone();
                edited_tx
                    .output
                    .retain(|txout| txout.script_pubkey.is_p2tr());
                let eligible_tx = SPTransaction {
                    tx: edited_tx,
                    scalar: sum.to_string(),
                };
                eligible_txs.push(eligible_tx);
            }
        }

        Ok(SPBlock {
            height,
            hash: block_hash,
            txs: eligible_txs,
        })
    }
    // NOTE: Instead of message passing this could also return a stream that yields new blocks. Or
    // just give syncer. a DB.
    pub async fn sync_from(&mut self) -> Result<()> {
        let mut synced_blocks = self
            .store
            .get_synced_blocks_height()
            .await?
            .unwrap_or_default() as u64;
        loop {
            let chain_tip = self.client.get_chain_tip()? as u64;

            if synced_blocks < chain_tip {
                let block = self.client.get_block_by_height(synced_blocks + 1)?;
                synced_blocks += 1;

                let sp_block = self.process_block(block, synced_blocks)?;

                self.store.add_block(sp_block).await?;
            } else {
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

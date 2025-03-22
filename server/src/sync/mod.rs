use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
    usize,
};

use bitcoincore_rpc::bitcoin::{OutPoint, TxOut};
use rpc::BitcionRpc;
use tokio::{sync::mpsc, time::sleep};

use crate::SPBlock;

mod rpc;

pub struct Syncer<C: BitcionRpc> {
    client: C,
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
    pub fn new(client: C, cache_size: usize) -> Self {
        let prevout_cache = PrevoutCache::new(cache_size);

        Self {
            client,
            prevout_cache,
        }
    }

    pub fn get_prevout(&mut self, outpoint: &OutPoint) -> bitcoincore_rpc::Result<TxOut> {
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

    // NOTE: Instead of message passing this could also return a stream that yields new blocks.
    pub async fn sync_from(&mut self, height: u64, tx: mpsc::Sender<SPBlock>) {
        let mut synced_blocks = height;
        loop {
            let chain_tip = self.client.get_chain_tip().unwrap();

            if synced_blocks < chain_tip as u64 {
                let block = self.client.get_block_by_height(synced_blocks + 1).unwrap();
                synced_blocks += 1;
                // FIXME: The prevout getter stuff is weird, I don't think &mut closure makes much
                // sense but my rust is too bad to do it differently, I just tried to stop the
                // compiler to scream at me until it worked.
                let sp_block =
                    SPBlock::new(synced_blocks, block, &mut |outpoint| {
                        self.get_prevout(outpoint)
                    });

                if let Err(err) = tx.send(sp_block).await {
                    // TODO: Handle error / pass it to server.
                    println!("error: {}", err);
                    break;
                }
            } else {
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

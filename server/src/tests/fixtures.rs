use std::collections::HashMap;

use bitcoincore_rpc::bitcoin::{Block, Transaction, Txid};

use crate::sync::BitcionRpc;

pub struct ClientMock {
    height: usize,
    blocks: Vec<Block>,
    txs: HashMap<Txid, Transaction>,
}

impl BitcionRpc for ClientMock {
    fn get_block_by_height(&self, height: u64) -> crate::Result<Block> {
        Ok(self.blocks.get(height as usize).cloned().ok_or(
            bitcoincore_rpc::Error::ReturnedError("invalid height".into()),
        )?)
    }

    fn get_chain_tip(&self) -> crate::Result<usize> {
        Ok(self.height)
    }

    fn get_transaction(&self, txid: &bitcoincore_rpc::bitcoin::Txid) -> crate::Result<Transaction> {
        Ok(self
            .txs
            .get(txid)
            .cloned()
            .ok_or(bitcoincore_rpc::Error::ReturnedError("invalid txid".into()))?)
    }
}

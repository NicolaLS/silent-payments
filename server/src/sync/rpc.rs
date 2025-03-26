use bitcoincore_rpc::{
    Client, RpcApi,
    bitcoin::{Block, Transaction, Txid},
};

use crate::Result;

pub trait BitcionRpc {
    fn get_block_by_height(&self, height: u64) -> Result<Block>;
    fn get_chain_tip(&self) -> Result<usize>;
    fn get_transaction(&self, txid: &Txid) -> Result<Transaction>;
}

impl BitcionRpc for Client {
    fn get_block_by_height(&self, height: u64) -> Result<Block> {
        let block_hash = self.get_block_hash(height)?;
        Ok(self.get_block(&block_hash)?)
    }

    fn get_chain_tip(&self) -> Result<usize> {
        let best_block_hash = self.get_best_block_hash()?;
        let best_block_info = self.get_block_info(&best_block_hash)?;
        Ok(best_block_info.height)
    }

    fn get_transaction(&self, txid: &Txid) -> Result<Transaction> {
        Ok(self.get_raw_transaction(txid, None)?)
    }
}

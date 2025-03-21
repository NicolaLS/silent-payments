use bitcoincore_rpc::{bitcoin::{Transaction, Txid}, Client, RpcApi};

pub trait BitcionRpc {
    fn get_block_by_height(
        &self,
        height: u64,
    ) -> bitcoincore_rpc::Result<bitcoincore_rpc::bitcoin::Block>;
    fn get_chain_tip(&self) -> bitcoincore_rpc::Result<usize>;
    fn get_transaction(&self, txid: &Txid) -> bitcoincore_rpc::Result<Transaction>;
}

impl BitcionRpc for Client {
    fn get_block_by_height(
        &self,
        height: u64,
    ) -> bitcoincore_rpc::Result<bitcoincore_rpc::bitcoin::Block> {
        let block_hash = self.get_block_hash(height)?;
        self.get_block(&block_hash)
    }

    fn get_chain_tip(&self) -> bitcoincore_rpc::Result<usize> {
        let best_block_hash = self.get_best_block_hash()?;
        let best_block_info = self.get_block_info(&best_block_hash)?;
        Ok(best_block_info.height)
    }

    fn get_transaction(&self, txid: &Txid) -> bitcoincore_rpc::Result<Transaction> {
        self.get_raw_transaction(txid, None)
    }
}

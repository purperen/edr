use std::sync::{Arc, OnceLock};

use edr_eth::{
    block::{BlobGas, Header},
    receipt::BlockReceipt,
    transaction::Transaction,
    withdrawal::Withdrawal,
    B256,
};
use edr_rpc_eth::{client::EthRpcClient, spec::EthRpcSpec};
use tokio::runtime;

use crate::{
    blockchain::{BlockchainError, ForkedBlockchainError},
    chain_spec::L1ChainSpec,
    Block, ExecutableTransaction, SyncBlock, TransactionConversionError,
};

/// Error that occurs when trying to convert the JSON-RPC `Block` type.
#[derive(Debug, thiserror::Error)]
pub enum CreationError {
    /// Missing hash
    #[error("Missing hash")]
    MissingHash,
    /// Missing miner
    #[error("Missing miner")]
    MissingMiner,
    /// Missing mix hash
    #[error("Missing mix hash")]
    MissingMixHash,
    /// Missing nonce
    #[error("Missing nonce")]
    MissingNonce,
    /// Missing number
    #[error("Missing numbeer")]
    MissingNumber,
    /// Transaction conversion error
    #[error(transparent)]
    TransactionConversionError(#[from] TransactionConversionError),
}

/// A remote block, which lazily loads receipts.
#[derive(Clone, Debug)]
pub struct RemoteBlock {
    header: Header,
    transactions: Vec<ExecutableTransaction<L1ChainSpec>>,
    /// The receipts of the block's transactions
    receipts: OnceLock<Vec<Arc<BlockReceipt>>>,
    /// The hashes of the block's ommers
    ommer_hashes: Vec<B256>,
    /// The staking withdrawals
    withdrawals: Option<Vec<Withdrawal>>,
    /// The block's hash
    hash: B256,
    /// The length of the RLP encoding of this block in bytes
    size: u64,
    // The RPC client is needed to lazily fetch receipts
    rpc_client: Arc<EthRpcClient<EthRpcSpec>>,
    runtime: runtime::Handle,
}

impl RemoteBlock {
    /// Constructs a new instance with the provided JSON-RPC block and client.
    pub fn new(
        block: edr_rpc_eth::Block<edr_rpc_eth::Transaction>,
        rpc_client: Arc<EthRpcClient<EthRpcSpec>>,
        runtime: runtime::Handle,
    ) -> Result<Self, CreationError> {
        let header = Header {
            parent_hash: block.parent_hash,
            ommers_hash: block.sha3_uncles,
            beneficiary: block.miner.ok_or(CreationError::MissingMiner)?,
            state_root: block.state_root,
            transactions_root: block.transactions_root,
            receipts_root: block.receipts_root,
            logs_bloom: block.logs_bloom,
            difficulty: block.difficulty,
            number: block.number.ok_or(CreationError::MissingNumber)?,
            gas_limit: block.gas_limit,
            gas_used: block.gas_used,
            timestamp: block.timestamp,
            extra_data: block.extra_data,
            mix_hash: block.mix_hash.ok_or(CreationError::MissingMixHash)?,
            nonce: block.nonce.ok_or(CreationError::MissingNonce)?,
            base_fee_per_gas: block.base_fee_per_gas,
            withdrawals_root: block.withdrawals_root,
            blob_gas: block.blob_gas_used.and_then(|gas_used| {
                block.excess_blob_gas.map(|excess_gas| BlobGas {
                    gas_used,
                    excess_gas,
                })
            }),
            parent_beacon_block_root: block.parent_beacon_block_root,
        };

        let transactions = block
            .transactions
            .into_iter()
            .map(ExecutableTransaction::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        let hash = block.hash.ok_or(CreationError::MissingHash)?;

        Ok(Self {
            header,
            transactions,
            receipts: OnceLock::new(),
            ommer_hashes: block.uncles,
            withdrawals: block.withdrawals,
            hash,
            rpc_client,
            size: block.size,
            runtime,
        })
    }
}

impl Block for RemoteBlock {
    type Error = BlockchainError;

    fn hash(&self) -> &B256 {
        &self.hash
    }

    fn header(&self) -> &Header {
        &self.header
    }

    fn ommer_hashes(&self) -> &[B256] {
        self.ommer_hashes.as_slice()
    }

    fn rlp_size(&self) -> u64 {
        self.size
    }

    fn transactions(&self) -> &[ExecutableTransaction<L1ChainSpec>] {
        &self.transactions
    }

    fn transaction_receipts(&self) -> Result<Vec<Arc<BlockReceipt>>, Self::Error> {
        if let Some(receipts) = self.receipts.get() {
            return Ok(receipts.clone());
        }

        let receipts: Vec<Arc<BlockReceipt>> = tokio::task::block_in_place(|| {
            self.runtime.block_on(
                self.rpc_client.get_transaction_receipts(
                    self.transactions
                        .iter()
                        .map(ExecutableTransaction::transaction_hash),
                ),
            )
        })
        .map_err(ForkedBlockchainError::RpcClient)?
        .ok_or_else(|| ForkedBlockchainError::MissingReceipts {
            block_hash: *self.hash(),
        })?
        .into_iter()
        .map(Arc::new)
        .collect();

        self.receipts
            .set(receipts.clone())
            .expect("We checked that receipts are not set");

        Ok(receipts)
    }

    fn withdrawals(&self) -> Option<&[Withdrawal]> {
        self.withdrawals.as_deref()
    }
}

impl From<RemoteBlock> for Arc<dyn SyncBlock<Error = BlockchainError>> {
    fn from(value: RemoteBlock) -> Self {
        Arc::new(value)
    }
}

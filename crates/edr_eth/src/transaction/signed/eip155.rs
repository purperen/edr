use std::sync::OnceLock;

use alloy_rlp::{RlpDecodable, RlpEncodable};
use hashbrown::HashMap;
use revm_primitives::{keccak256, TxEnv};

use super::kind_to_transact_to;
use crate::{
    signature::{Signature, SignatureError},
    transaction::{self, fake_signature::recover_fake_signature, TxKind},
    Address, Bytes, B256, U256,
};

#[derive(Clone, Debug, Eq, RlpDecodable, RlpEncodable)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Eip155 {
    // The order of these fields determines de-/encoding order.
    #[cfg_attr(feature = "serde", serde(with = "crate::serde::u64"))]
    pub nonce: u64,
    pub gas_price: U256,
    #[cfg_attr(feature = "serde", serde(with = "crate::serde::u64"))]
    pub gas_limit: u64,
    pub kind: TxKind,
    pub value: U256,
    pub input: Bytes,
    pub signature: Signature,
    /// Cached transaction hash
    #[rlp(default)]
    #[rlp(skip)]
    #[cfg_attr(feature = "serde", serde(skip))]
    pub hash: OnceLock<B256>,
    /// Whether the signed transaction is from an impersonated account.
    #[rlp(default)]
    #[rlp(skip)]
    #[cfg_attr(feature = "serde", serde(skip))]
    pub is_fake: bool,
}

impl Eip155 {
    pub fn hash(&self) -> &B256 {
        self.hash.get_or_init(|| keccak256(alloy_rlp::encode(self)))
    }

    /// Recovers the Ethereum address which was used to sign the transaction.
    pub fn recover(&self) -> Result<Address, SignatureError> {
        if self.is_fake {
            return Ok(recover_fake_signature(&self.signature));
        }
        self.signature
            .recover(transaction::request::Eip155::from(self).hash())
    }

    pub fn chain_id(&self) -> u64 {
        (self.signature.v - 35) / 2
    }

    /// Converts this transaction into a `TxEnv` struct.
    pub fn into_tx_env(self, caller: Address) -> TxEnv {
        let chain_id = self.chain_id();
        TxEnv {
            caller,
            gas_limit: self.gas_limit,
            gas_price: self.gas_price,
            transact_to: kind_to_transact_to(self.kind),
            value: self.value,
            data: self.input,
            nonce: Some(self.nonce),
            chain_id: Some(chain_id),
            access_list: Vec::new(),
            gas_priority_fee: None,
            blob_hashes: Vec::new(),
            max_fee_per_blob_gas: None,
            eof_initcodes: Vec::new(),
            eof_initcodes_hashed: HashMap::new(),
        }
    }
}

impl From<transaction::signed::legacy::Legacy> for Eip155 {
    fn from(tx: transaction::signed::legacy::Legacy) -> Self {
        Self {
            nonce: tx.nonce,
            gas_price: tx.gas_price,
            gas_limit: tx.gas_limit,
            kind: tx.kind,
            value: tx.value,
            input: tx.input,
            signature: tx.signature,
            hash: tx.hash,
            is_fake: tx.is_fake,
        }
    }
}

impl PartialEq for Eip155 {
    fn eq(&self, other: &Self) -> bool {
        self.nonce == other.nonce
            && self.gas_price == other.gas_price
            && self.gas_limit == other.gas_limit
            && self.kind == other.kind
            && self.value == other.value
            && self.input == other.input
            && self.signature == other.signature
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy_rlp::Decodable;
    use k256::SecretKey;

    use super::*;
    use crate::signature::secret_key_from_str;

    fn dummy_request() -> transaction::request::Eip155 {
        let to = Address::from_str("0xc014ba5ec014ba5ec014ba5ec014ba5ec014ba5e").unwrap();
        let input = hex::decode("1234").unwrap();
        transaction::request::Eip155 {
            nonce: 1,
            gas_price: U256::from(2),
            gas_limit: 3,
            kind: TxKind::Call(to),
            value: U256::from(4),
            input: Bytes::from(input),
            chain_id: 1,
        }
    }

    fn dummy_secret_key() -> SecretKey {
        secret_key_from_str("e331b6d69882b4cb4ea581d88e0b604039a3de5967688d3dcffdd2270c0fd109")
            .unwrap()
    }

    #[test]
    fn test_eip155_signed_transaction_encoding() {
        // Generated by Hardhat
        let expected =
            hex::decode("f85f01020394c014ba5ec014ba5ec014ba5ec014ba5ec014ba5e0482123426a0fc9f82c3002f9ed8c05d6e8e821cf14eab65a1b4647e002957e170149393f40ba077f230fafdb096cf80762af3d3f4243f02e754f363fb9443d914c3a286fa2774")
                .unwrap();

        let request = dummy_request();
        let signed = request.sign(&dummy_secret_key()).unwrap();

        let encoded = alloy_rlp::encode(&signed);
        assert_eq!(expected, encoded);
    }

    #[test]
    fn test_eip155_signed_transaction_hash() {
        // Generated by hardhat
        let expected = B256::from_slice(
            &hex::decode("4da115513cdabaed0e9c9e503acd2fa7af29e5baae7f79a6ffa878b3ff380de6")
                .unwrap(),
        );

        let request = dummy_request();
        let signed = request.sign(&dummy_secret_key()).unwrap();

        assert_eq!(expected, *signed.hash());
    }

    #[test]
    fn test_eip155_signed_transaction_rlp() {
        let request = dummy_request();
        let signed = request.sign(&dummy_secret_key()).unwrap();

        let encoded = alloy_rlp::encode(&signed);
        assert_eq!(signed, Eip155::decode(&mut encoded.as_slice()).unwrap());
    }
}

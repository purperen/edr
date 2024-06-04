use std::sync::OnceLock;

use alloy_rlp::{BufMut, Encodable};
use k256::SecretKey;
use revm_primitives::keccak256;

use crate::{
    signature::{Signature, SignatureError},
    transaction::{self, fake_signature::make_fake_signature, TxKind},
    Address, Bytes, B256, U256,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eip155 {
    // The order of these fields determines encoding order.
    pub nonce: u64,
    pub gas_price: U256,
    pub gas_limit: u64,
    pub kind: TxKind,
    pub value: U256,
    pub input: Bytes,
    pub chain_id: u64,
}

impl Eip155 {
    /// Computes the hash of the transaction.
    pub fn hash(&self) -> B256 {
        keccak256(alloy_rlp::encode(self))
    }

    /// Signs the transaction with the provided secret key.
    pub fn sign(
        self,
        secret_key: &SecretKey,
    ) -> Result<transaction::signed::Eip155, SignatureError> {
        let hash = self.hash();

        let mut signature = Signature::new(hash, secret_key)?;
        signature.v += self.v_value_adjustment();

        Ok(transaction::signed::Eip155 {
            nonce: self.nonce,
            gas_price: self.gas_price,
            gas_limit: self.gas_limit,
            kind: self.kind,
            value: self.value,
            input: self.input,
            signature,
            hash: OnceLock::new(),
            is_fake: false,
        })
    }

    /// Creates a fake signature for an impersonated account.
    pub fn fake_sign(self, address: &Address) -> transaction::signed::Eip155 {
        let mut signature = make_fake_signature::<0>(address);
        signature.v += self.v_value_adjustment();

        transaction::signed::Eip155 {
            nonce: self.nonce,
            gas_price: self.gas_price,
            gas_limit: self.gas_limit,
            kind: self.kind,
            value: self.value,
            input: self.input,
            signature,
            hash: OnceLock::new(),
            is_fake: true,
        }
    }

    fn rlp_payload_length(&self) -> usize {
        self.nonce.length()
            + self.gas_price.length()
            + self.gas_limit.length()
            + self.kind.length()
            + self.value.length()
            + self.input.length()
            + self.chain_id.length()
            + 2
    }

    fn v_value_adjustment(&self) -> u64 {
        // `CHAIN_ID * 2 + 35` comes from EIP-155 and we subtract the Bitcoin magic
        // number 27, because `Signature::new` adds that.
        self.chain_id * 2 + 35 - 27
    }
}

impl From<&transaction::signed::Eip155> for Eip155 {
    fn from(tx: &transaction::signed::Eip155) -> Self {
        let chain_id = tx.chain_id();
        Self {
            nonce: tx.nonce,
            gas_price: tx.gas_price,
            gas_limit: tx.gas_limit,
            kind: tx.kind,
            value: tx.value,
            input: tx.input.clone(),
            chain_id,
        }
    }
}

impl Encodable for Eip155 {
    fn length(&self) -> usize {
        let payload_length = self.rlp_payload_length();
        payload_length + alloy_rlp::length_of_length(payload_length)
    }

    fn encode(&self, out: &mut dyn BufMut) {
        alloy_rlp::Header {
            list: true,
            payload_length: self.rlp_payload_length(),
        }
        .encode(out);

        self.nonce.encode(out);
        self.gas_price.encode(out);
        self.gas_limit.encode(out);
        self.kind.encode(out);
        self.value.encode(out);
        self.input.encode(out);
        self.chain_id.encode(out);
        // Appending these two values requires a custom implementation of
        // `Encodable`
        0u8.encode(out);
        0u8.encode(out);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::transaction::fake_signature::tests::test_fake_sign_properties;

    fn dummy_request() -> Eip155 {
        let to = Address::from_str("0xc014ba5ec014ba5ec014ba5ec014ba5ec014ba5e").unwrap();
        let input = hex::decode("1234").unwrap();
        Eip155 {
            nonce: 1,
            gas_price: U256::from(2),
            gas_limit: 3,
            kind: TxKind::Call(to),
            value: U256::from(4),
            input: Bytes::from(input),
            chain_id: 1,
        }
    }

    #[test]
    fn test_eip155_transaction_request_encoding() {
        // Generated by Hardhat
        let expected =
            hex::decode("df01020394c014ba5ec014ba5ec014ba5ec014ba5ec014ba5e04821234018080")
                .unwrap();

        let request = dummy_request();

        let encoded = alloy_rlp::encode(&request);
        assert_eq!(expected, encoded);
    }

    #[test]
    fn test_eip155_transaction_request_hash() {
        // Generated by hardhat
        let expected = B256::from_slice(
            &hex::decode("df5aea488af414bd517742f599bcba94ba801a581cf71d86a85777cecdbe6743")
                .unwrap(),
        );

        let request = dummy_request();
        assert_eq!(expected, request.hash());
    }

    test_fake_sign_properties!();

    #[test]
    fn test_fake_sign_test_vector() -> anyhow::Result<()> {
        let transaction = Eip155 {
            nonce: 0,
            gas_price: U256::from(678_912),
            gas_limit: 30_000,
            kind: TxKind::Call("0xb5bc06d4548a3ac17d72b372ae1e416bf65b8ead".parse()?),
            value: U256::from(1),
            input: Bytes::default(),
            chain_id: 123,
        };

        let fake_sender: Address = "0xa5bc06d4548a3ac17d72b372ae1e416bf65b8ead".parse()?;

        let signed = transaction.fake_sign(&fake_sender);

        let expected_hash: B256 =
            "bcdd3230665912079522dfbfe605e70443c81bf78db768a688a8d8007accf14b".parse()?;
        assert_eq!(signed.hash(), &expected_hash);

        Ok(())
    }
}

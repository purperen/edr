//! Port of the hardhat-network's `library-utils.ts` to Rust.

use edr_evm::hex;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

/// Normalizes the compiler output bytecode by replacing the library addresses
/// with zeros.
pub fn normalize_compiler_output_bytecode(
    mut compiler_output_bytecode_object: String,
    addresses_positions: &[u32],
) -> napi::Result<Buffer> {
    const ZERO_ADDRESS: &str = "0000000000000000000000000000000000000000";

    for &pos in addresses_positions {
        compiler_output_bytecode_object = edr_solidity::library_utils::link_hex_string_bytecode(
            compiler_output_bytecode_object,
            ZERO_ADDRESS,
            pos,
        );
    }

    Ok(Buffer::from(
        hex::decode(compiler_output_bytecode_object)
            .map_err(|e| napi::Error::from_reason(format!("Failed to decode hex: {e:?}")))?,
    ))
}

#[napi]
pub fn link_hex_string_bytecode(code: String, address: String, position: u32) -> String {
    edr_solidity::library_utils::link_hex_string_bytecode(code, &address, position)
}

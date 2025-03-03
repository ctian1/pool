#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy::sol_types::SolValue;
use pool_lib::{process_withdrawal, WithdrawalInput};

pub fn main() {
    // let input = sp1_zkvm::io::read::<WithdrawalInput>();
    let bytes = sp1_zkvm::io::read_vec();
    let input = serde_cbor::from_slice::<WithdrawalInput>(&bytes).unwrap();

    let data = process_withdrawal(&input).unwrap();

    sp1_zkvm::io::commit_slice(&data.abi_encode());
}

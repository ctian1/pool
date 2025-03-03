use alloy::{
    consensus::Header,
    primitives::{keccak256, Address, Bytes, B256, U256},
    rlp,
    rpc::types::{BlockNumHash, EIP1186AccountProofResponse},
    sol,
};
use alloy_trie::{proof::verify_proof, Nibbles, TrieAccount};
use eyre::{ensure, Result};
use serde::{Deserialize, Serialize};

sol! {
    #[derive(Debug)]
    struct WithdrawalData {
        bytes32 nullifier;
        bytes32 blockHash;
        bytes32 exclusionSetRoot;
        uint256 relayerFee;
        address recipient;
        address relayer;
        address contractAddress;
        uint64 blockNumber;
    }
}

/// Inclusion branches and an index for proving that a commitment is in an array of commitments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InclusionBranches {
    pub index: u32,
    pub proof: Vec<B256>,
}

/// The private inputs for the withdrawal proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalInput {
    pub secret: B256,
    pub array_index: U256,
    pub account_proof: EIP1186AccountProofResponse,
    pub block_header: Header,
    pub inclusion_set_branches: Option<InclusionBranches>,
    pub contract_address: Address,
    pub array_slot: U256,
    pub relayer_fee: U256,
    pub recipient: Address,
    pub relayer: Address,
}

/// Compute commitment and nullifier from secret.
pub fn compute_commitment(secret: &B256) -> (B256, B256) {
    let u256 = U256::from_be_slice(&secret.0);
    let commitment = keccak256(u256.to_be_bytes::<32>());
    let nullifier = keccak256(u256.wrapping_add(U256::from(1)).to_be_bytes::<32>());
    (commitment, nullifier)
}

/// Compute inclusion set root from commitment, index, and branches.
pub fn compute_inclusion_root(commitment: B256, proof: InclusionBranches) -> B256 {
    let bits = proof.index;

    let mut root = commitment;
    for (i, hash) in proof.proof.iter().enumerate() {
        if bits & (1 << i) == 0 {
            let mut input = [0u8; 64];
            input[..32].copy_from_slice(&root.0);
            input[32..].copy_from_slice(&hash.0);
            root = keccak256(input);
        } else {
            let mut input = [0u8; 64];
            input[..32].copy_from_slice(&hash.0);
            input[32..].copy_from_slice(&root.0);
            root = keccak256(input);
        }
    }

    root
}

/// Hash block header.
pub fn hash_block_header(header: &Header) -> BlockNumHash {
    header.num_hash_slow()
}

/// Verify the commitment is in array[array_index] where array is stored in array_slot in contract_address.
pub fn verify_storage_slot(
    contract_address: &Address,
    array_slot: &U256,
    commitment: &B256,
    array_index: &U256,
    state_root: &B256,
    proof: &EIP1186AccountProofResponse,
) -> Result<()> {
    // Verify contract address
    ensure!(
        *contract_address == proof.address,
        "invalid contract address"
    );

    // Verify account proof from state_root
    let account = TrieAccount {
        nonce: proof.nonce,
        balance: proof.balance,
        code_hash: proof.code_hash,
        storage_root: proof.storage_hash,
    };
    verify_mpt_proof(state_root, proof.address, account, &proof.account_proof)?;

    // Verify storage proofs
    ensure!(proof.storage_proof.len() == 2, "invalid storage proof");

    // First storage proof: len of array, key is array_slot
    let array_len_proof = proof.storage_proof.first().unwrap();
    verify_mpt_proof(
        &proof.storage_hash,
        array_slot.to_be_bytes::<32>(),
        array_len_proof.value,
        &array_len_proof.proof,
    )?;

    // Ensure array_index is in range
    ensure!(*array_index < array_len_proof.value, "invalid array index");

    // Verify storage_hash -> array[array_index] == commitment
    let commitment_proof = proof.storage_proof.get(1).unwrap();
    // Calculate correct array index
    let base_key = keccak256(array_slot.to_be_bytes::<32>());
    let index_key = U256::from_be_bytes(base_key.into()) + array_index;
    verify_mpt_proof(
        &proof.storage_hash,
        index_key.to_be_bytes::<32>(),
        commitment,
        &commitment_proof.proof,
    )?;

    Ok(())
}

/// Verify a Merkle Patricia Trie proof.
pub fn verify_mpt_proof<K: AsRef<[u8]>, V: rlp::Encodable>(
    root: &B256,
    raw_key: K,
    raw_value: V,
    proof: &[Bytes],
) -> Result<()> {
    let key = Nibbles::unpack(keccak256(raw_key));
    let value = rlp::encode(raw_value);

    verify_proof(*root, key, Some(value), proof).map_err(|_| eyre::eyre!("invalid proof"))
}

/// Compute storage keys for a given array slot and index.
pub fn compute_storage_keys(array_slot: U256, array_index: U256) -> (B256, B256) {
    let bytes = array_slot.to_be_bytes::<32>();
    let base_key = keccak256(bytes);
    let index_key = U256::from_be_bytes(base_key.into()) + array_index;
    (bytes.into(), index_key.to_be_bytes::<32>().into())
}

/// Process a withdrawal, fully verifying it and returning public data.
pub fn process_withdrawal(input: &WithdrawalInput) -> Result<WithdrawalData> {
    let WithdrawalInput {
        secret,
        array_index,
        account_proof,
        block_header,
        inclusion_set_branches,
        contract_address,
        array_slot,
        relayer_fee,
        recipient,
        relayer,
    } = input;

    let (commitment, nullifier) = compute_commitment(secret);
    let state_root = block_header.state_root;
    let block_hash = hash_block_header(block_header);

    // Verify storage proofs
    verify_storage_slot(
        contract_address,
        array_slot,
        &commitment,
        array_index,
        &state_root,
        account_proof,
    )?;

    let inclusion_root = inclusion_set_branches
        .clone()
        .map(|branches| compute_inclusion_root(commitment, branches))
        .unwrap_or(B256::ZERO);

    Ok(WithdrawalData {
        nullifier,
        blockNumber: block_hash.number,
        blockHash: block_hash.hash,
        contractAddress: *contract_address,
        exclusionSetRoot: inclusion_root,
        relayerFee: *relayer_fee,
        recipient: *recipient,
        relayer: *relayer,
    })
}

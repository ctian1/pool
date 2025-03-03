use alloy::{
    consensus::BlockHeader,
    eips::BlockNumberOrTag,
    network::Ethereum,
    primitives::{Address, B256, U256},
    providers::{Provider, RootProvider},
    rpc::types::BlockTransactionsKind,
    sol,
    transports::http::reqwest::Url,
};
use clap::Parser;
use eyre::{ensure, Result};
use pool_lib::{compute_commitment, compute_storage_keys, process_withdrawal, WithdrawalInput};
use rand::Rng;
use sp1_sdk::{include_elf, setup_logger, ProverClient, SP1Stdin};
use std::io::Write;

/// The ELF (executable and linkable format) file for the Succinct RISC-V zkVM.
pub const ELF: &[u8] = include_elf!("pool-program");

sol! {
    #[sol(rpc)]
    contract Pool {
        bytes32[] public deposits;
    }
}

// CLI with deposit and withdraw commands
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Deposit(DepositArgs),
    Withdraw(WithdrawArgs),
}

#[derive(Parser, Debug)]
struct DepositArgs {}

#[derive(Parser, Debug)]
struct WithdrawArgs {
    #[clap(long)]
    rpc_url: Url,

    address: Address,

    secret: B256,

    #[clap(long)]
    execute: bool,

    #[clap(long)]
    prove: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger();

    // Handle the command line arguments.
    let args = Args::parse();

    match args.command {
        Command::Deposit(_args) => {
            println!("Depositing...");
            // Generate random B256
            let mut rng = rand::rng();
            let secret = rng.random::<[u8; 32]>();
            let (commitment, nullifier) = pool_lib::compute_commitment(&secret.into());
            println!("Commitment: {:?}", commitment);
            println!("Nullifier: {:?}", nullifier);
            println!("Secret: {}", hex::encode(secret));
        }
        Command::Withdraw(args) => {
            println!("Withdrawing...");
            println!("Address: {}", args.address);
            println!("Secret: {}", args.secret);

            let provider = RootProvider::<Ethereum>::new_http(args.rpc_url);
            let keys = compute_storage_keys(U256::from(0_u32), U256::from(1_u32));
            println!("Keys: {:?}", keys);
            let header = provider
                .get_block_by_number(BlockNumberOrTag::Finalized, BlockTransactionsKind::Hashes)
                .await?
                .unwrap();
            let block_number = header.header.number();
            println!("Block: {}", block_number);

            let contract = Pool::new(args.address, &provider);
            let length = provider
                .get_storage_at(args.address, U256::from(0_u32))
                .number(block_number)
                .await?;
            println!("Length: {}", length);

            let (target_commitment, nullifier) = compute_commitment(&args.secret);
            println!("Commitment: {:?}", target_commitment);
            println!("Nullifier: {:?}", nullifier);
            let mut found_index = None;
            for i in 0..length.to::<u64>() {
                let commitment = contract
                    .deposits(U256::from(i))
                    .block(block_number.into())
                    .call()
                    .await?
                    ._0;
                if commitment == target_commitment {
                    found_index = Some(i);
                    break;
                }
            }
            ensure!(found_index.is_some(), "commitment not found");
            let found_index = found_index.unwrap();
            println!("Found index: {}", found_index);

            let proof = provider
                .get_proof(args.address, vec![keys.0, keys.1])
                .number(block_number)
                .await
                .unwrap();

            let input = WithdrawalInput {
                secret: args.secret,
                account_proof: proof,
                array_index: U256::from(found_index),
                block_header: header.header.inner,
                inclusion_set_branches: None,
                contract_address: args.address,
                array_slot: U256::from(0_u32),
                relayer_fee: U256::from(0_u32),
                recipient: Address::with_last_byte(0),
                relayer: Address::with_last_byte(0),
            };

            let data = process_withdrawal(&input).unwrap();
            println!("Data: {:?}", data);

            if !args.execute && !args.prove {
                return Ok(());
            }

            let prover = ProverClient::from_env();
            if args.execute {
                let mut stdin = SP1Stdin::new();
                let serialized = serde_cbor::to_vec(&input).unwrap();
                stdin.write_slice(&serialized);
                let (_output, report) = prover.execute(ELF, &stdin).run().unwrap();
                println!("Cycles: {}", report.total_instruction_count());
                println!("Report: {}", report);
            }

            if args.prove {
                let mut stdin = SP1Stdin::new();
                let serialized = serde_cbor::to_vec(&input).unwrap();
                stdin.write_slice(&serialized);
                let (pk, _vk) = prover.setup(ELF);
                let start = std::time::Instant::now();
                let proof = prover.prove(&pk, &stdin).compressed().run().unwrap();
                println!("Successfully generated proof after {:?}", start.elapsed());

                // Write proof to file
                let mut file = std::fs::File::create("proof.bin").unwrap();
                let serialized = bincode::serialize(&proof).unwrap();
                file.write_all(&serialized).unwrap();
            }
        }
    }

    Ok(())
}

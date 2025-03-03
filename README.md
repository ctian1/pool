# Privacy Pool in SP1

This is a proof-of-concept for a privacy pool implemented in [SP1](https://github.com/succinctlabs/sp1).

Depositing into the pool requires ~75k gas and simply consists of paying the pool contract with a set
amount of ETH and appending a commitment hash to the `deposits` array.

Withdrawals are done by providing an SP1 proof which proves inclusion of the commitment in the `deposits` array.
The array is proven using MPT account proof + storage proof and block hash is verified onchain using the `BLOCKHASH` opcode.

The withdrawal program can optionally prove inclusion of the commitment in a separate inclusion set
to for example prove that the withdrawal is not associated with certain deposits in the pool. This
concept is described in Vitalik's [Privacy Pools](https://www.sciencedirect.com/science/article/pii/S2096720923000519)
paper.

The CLI script can be used to generate a secret and commitment for deposits and generate a proof for withdrawals. No offchain indexing is required.

Relaying is supported as relayer address and fee are public inputs to the proof.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

// @title Privacy Pool using an SP1 program for withdrawals.
contract Pool {
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

    event Withdrawal(
        bytes32 indexed nullifier, bytes32 exclusionSetRoot, address recipient, address relayer, uint256 relayerFee
    );

    address public immutable verifier;

    bytes32 public immutable programVkey;

    uint256 public immutable amount;

    bytes32[] public deposits;

    mapping(bytes32 => bool) public nullifiers;

    constructor(address _verifier, bytes32 _vkey, uint256 _amount) {
        verifier = _verifier;
        programVkey = _vkey;
        amount = _amount;
    }

    // @notice Withdraw funds from the pool using an SP1 proof.
    function withdraw(bytes calldata _publicValues, bytes calldata _proofBytes) public {
        ISP1Verifier(verifier).verifyProof(programVkey, _publicValues, _proofBytes);
        WithdrawalData memory withdrawal = abi.decode(_publicValues, (WithdrawalData));
        require(!nullifiers[withdrawal.nullifier], "Already withdrawn");
        require(blockhash(withdrawal.blockNumber) == withdrawal.blockHash, "Invalid block hash");
        require(withdrawal.contractAddress == address(this), "Invalid contract address");
        nullifiers[withdrawal.nullifier] = true;

        emit Withdrawal(
            withdrawal.nullifier,
            withdrawal.exclusionSetRoot,
            withdrawal.recipient,
            withdrawal.relayer,
            withdrawal.relayerFee
        );

        (bool success,) = withdrawal.recipient.call{value: amount - withdrawal.relayerFee}("");
        require(success, "Failed to send withdrawal");

        if (withdrawal.relayerFee > 0) {
            (bool success2,) = withdrawal.relayer.call{value: withdrawal.relayerFee}("");
            require(success2, "Failed to send relayer fee");
        }
    }

    // @notice Deposit funds into the pool. The commitment should be the keccak256 of a known and unused bytes32 secret.
    function deposit(bytes32 _commitment) public payable {
        require(msg.value == amount, "Invalid deposit amount");
        deposits.push(_commitment);
    }
}

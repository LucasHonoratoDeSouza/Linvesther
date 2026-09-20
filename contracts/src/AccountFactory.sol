// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {WebAuthnAccount} from "./WebAuthnAccount.sol";
import {P256VaultAccount} from "./P256VaultAccount.sol";

/// @notice Deploys per-user `WebAuthnAccount`/`P256VaultAccount` clones
/// at deterministic addresses derived from their P-256 public key
/// `(qx, qy)` — the same pair that derives a session's `subjectKey`
/// off-chain, so the on-chain address and the app-level identifier stay
/// derivable from the same input. Idempotent: calling `deploy*` again
/// for an already-deployed `(qx, qy)` returns the existing address
/// instead of reverting on a CREATE2 collision.
contract AccountFactory {
    address public immutable webAuthnImplementation;
    address public immutable p256VaultImplementation;

    constructor(address webAuthnImplementation_, address p256VaultImplementation_) {
        webAuthnImplementation = webAuthnImplementation_;
        p256VaultImplementation = p256VaultImplementation_;
    }

    function _salt(bytes32 qx, bytes32 qy) private pure returns (bytes32) {
        return keccak256(abi.encodePacked(qx, qy));
    }

    /// @notice The address `deployWebAuthnAccount(qx, qy)` will produce,
    /// computable before that account exists.
    function predictWebAuthnAccountAddress(bytes32 qx, bytes32 qy) public view returns (address) {
        return Clones.predictDeterministicAddress(webAuthnImplementation, _salt(qx, qy), address(this));
    }

    /// @notice The address `deployP256VaultAccount(qx, qy)` will produce,
    /// computable before that account exists.
    function predictP256VaultAccountAddress(bytes32 qx, bytes32 qy) public view returns (address) {
        return Clones.predictDeterministicAddress(p256VaultImplementation, _salt(qx, qy), address(this));
    }

    /// @notice Deploys (or returns the existing) `WebAuthnAccount` clone
    /// for `(qx, qy)`.
    function deployWebAuthnAccount(bytes32 qx, bytes32 qy) external returns (address account) {
        account = predictWebAuthnAccountAddress(qx, qy);
        if (account.code.length == 0) {
            Clones.cloneDeterministic(webAuthnImplementation, _salt(qx, qy));
            WebAuthnAccount(account).initialize(qx, qy);
        }
    }

    /// @notice Deploys (or returns the existing) `P256VaultAccount`
    /// clone for `(qx, qy)`.
    function deployP256VaultAccount(bytes32 qx, bytes32 qy) external returns (address account) {
        account = predictP256VaultAccountAddress(qx, qy);
        if (account.code.length == 0) {
            Clones.cloneDeterministic(p256VaultImplementation, _salt(qx, qy));
            P256VaultAccount(account).initialize(qx, qy);
        }
    }
}

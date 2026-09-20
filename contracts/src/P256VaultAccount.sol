// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {SignerP256} from "@openzeppelin/contracts/utils/cryptography/signers/SignerP256.sol";
import {P256} from "@openzeppelin/contracts/utils/cryptography/P256.sol";
import {IERC1271} from "@openzeppelin/contracts/interfaces/IERC1271.sol";
import {Initializable} from "@openzeppelin/contracts/proxy/utils/Initializable.sol";

/// @notice ERC-1271 account whose owner is a single raw P-256 key (the
/// password-derived vault, not WebAuthn) — signature format is a plain
/// 64-byte `r||s`, unlike `WebAuthnAccount`'s WebAuthn-enveloped
/// signature. Lets a vault-derived key stand in as
/// `IdentityRegistry`/`AccountRegistry`'s `owner`/`signer` without any
/// change to those contracts, the same way `WebAuthnAccount` does for
/// passkeys.
///
/// `isValidSignature` never reverts on an invalid signature — it returns
/// the ERC-1271 failure magic value (`0xffffffff`) instead, matching what
/// `SignatureChecker.isValidSignatureNow` expects from a well-behaved
/// contract wallet.
///
/// Deployed as an `Initializable` implementation behind per-user
/// `Clones.cloneDeterministic` proxies (`AccountFactory`) — see
/// `WebAuthnAccount`'s NatSpec for why the constructor sets the signer
/// to the P-256 generator point before disabling further
/// initialization.
///
/// The vault's private key only ever signs through the browser's
/// SubtleCrypto ECDSA API (apps/web/lib/vault.ts), which has no "sign
/// this exact digest" mode — `sign({name:"ECDSA", hash:"SHA-256"}, ...)`
/// always hashes its input with SHA-256 before the raw ECDSA operation.
/// So the bytes actually covered by the signature are `sha256(digest)`,
/// not `digest` itself. `SignerP256._rawSignatureValidation` (inherited
/// as-is by `WebAuthnAccount`) assumes the opposite — that `hash` is
/// exactly what got signed — which is true for a real WebAuthn
/// authenticator's own signing operation, but not for SubtleCrypto's.
/// This override re-hashes before verifying so the two sides agree on
/// what was actually signed.
contract P256VaultAccount is Initializable, SignerP256, IERC1271 {
    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() SignerP256(bytes32(P256.GX), bytes32(P256.GY)) {
        _disableInitializers();
    }

    /// @notice Sets this clone's signer. Callable once — see
    /// `WebAuthnAccount.initialize`'s NatSpec for why an open caller is
    /// safe here.
    function initialize(bytes32 qx, bytes32 qy) external initializer {
        _setSigner(qx, qy);
    }

    /// @inheritdoc SignerP256
    function _rawSignatureValidation(bytes32 hash, bytes calldata signature)
        internal
        view
        override
        returns (bool)
    {
        return super._rawSignatureValidation(sha256(abi.encodePacked(hash)), signature);
    }

    /// @inheritdoc IERC1271
    function isValidSignature(bytes32 hash, bytes calldata signature) external view returns (bytes4) {
        return _rawSignatureValidation(hash, signature) ? IERC1271.isValidSignature.selector : bytes4(0xffffffff);
    }
}

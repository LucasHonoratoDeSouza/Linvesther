// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {SignerWebAuthn} from "@openzeppelin/contracts/utils/cryptography/signers/SignerWebAuthn.sol";
import {SignerP256} from "@openzeppelin/contracts/utils/cryptography/signers/SignerP256.sol";
import {P256} from "@openzeppelin/contracts/utils/cryptography/P256.sol";
import {IERC1271} from "@openzeppelin/contracts/interfaces/IERC1271.sol";
import {Initializable} from "@openzeppelin/contracts/proxy/utils/Initializable.sol";

/// @notice ERC-1271 account whose owner is a single WebAuthn passkey
/// (P-256 public key, WebAuthn-enveloped signature). Lets a passkey stand
/// in as `IdentityRegistry`/`AccountRegistry`'s `owner`/`signer` without
/// any change to those contracts, since both already authorize commands
/// via `SignatureChecker.isValidSignatureNow`, which accepts any
/// ERC-1271-compliant address.
///
/// `isValidSignature` never reverts on an invalid signature — it returns
/// the ERC-1271 failure magic value (`0xffffffff`) instead, matching what
/// `SignatureChecker.isValidSignatureNow` expects from a well-behaved
/// contract wallet (a revert there is treated as "invalid", but the
/// non-reverting path is the documented one).
///
/// Deployed as an `Initializable` implementation behind per-user
/// `Clones.cloneDeterministic` proxies (`AccountFactory`), per
/// `SignerP256`'s own NatSpec recommendation for factory usage. The
/// implementation's own constructor sets the signer to the P-256
/// generator point purely to satisfy `SignerP256`'s validating
/// constructor, then immediately disables further initialization — the
/// generator point is never a real account's signer, and no clone ever
/// reads the implementation's storage.
contract WebAuthnAccount is Initializable, SignerWebAuthn, IERC1271 {
    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() SignerP256(bytes32(P256.GX), bytes32(P256.GY)) {
        _disableInitializers();
    }

    /// @notice Sets this clone's signer. Callable once, by anyone —
    /// safe because the resulting account is only ever useful to
    /// whoever controls the corresponding P-256 private key; setting it
    /// to the wrong key just makes an account nobody can sign for.
    function initialize(bytes32 qx, bytes32 qy) external initializer {
        _setSigner(qx, qy);
    }

    /// @inheritdoc IERC1271
    function isValidSignature(bytes32 hash, bytes calldata signature) external view returns (bytes4) {
        return _rawSignatureValidation(hash, signature) ? IERC1271.isValidSignature.selector : bytes4(0xffffffff);
    }
}

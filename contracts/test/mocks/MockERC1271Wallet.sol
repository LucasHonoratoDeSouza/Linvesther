// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {IERC1271} from "@openzeppelin/contracts/interfaces/IERC1271.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

/// @notice Minimal single-signer ERC-1271 wallet, for testing that
/// IdentityRegistry accepts contract-wallet owners, not just EOAs.
contract MockERC1271Wallet is IERC1271 {
    address public immutable authorizedSigner;

    constructor(address signer) {
        authorizedSigner = signer;
    }

    function isValidSignature(bytes32 hash, bytes memory signature) external view returns (bytes4) {
        (address recovered, ECDSA.RecoverError err,) = ECDSA.tryRecover(hash, signature);
        if (err == ECDSA.RecoverError.NoError && recovered == authorizedSigner) {
            return IERC1271.isValidSignature.selector;
        }
        return 0xffffffff;
    }
}

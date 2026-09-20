// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {P256VaultAccount} from "../src/P256VaultAccount.sol";
import {IERC1271} from "@openzeppelin/contracts/interfaces/IERC1271.sol";
import {P256} from "@openzeppelin/contracts/utils/cryptography/P256.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";

contract P256VaultAccountTest is Test {
    uint256 internal constant OWNER_KEY = 0xA11CE;
    uint256 internal constant STRANGER_KEY = 0xB0B;

    bytes32 internal ownerQx;
    bytes32 internal ownerQy;
    P256VaultAccount internal account;

    function setUp() public {
        (uint256 x, uint256 y) = vm.publicKeyP256(OWNER_KEY);
        ownerQx = bytes32(x);
        ownerQy = bytes32(y);
        P256VaultAccount implementation = new P256VaultAccount();
        account = P256VaultAccount(Clones.clone(address(implementation)));
        account.initialize(ownerQx, ownerQy);
    }

    function test_Initialize_RevertsOnSecondCall() public {
        vm.expectRevert();
        account.initialize(ownerQx, ownerQy);
    }

    function _sign(uint256 privateKey, bytes32 hash) internal pure returns (bytes memory) {
        // Mirrors what a real vault signature actually covers: the
        // browser's SubtleCrypto ECDSA API always SHA-256-hashes its
        // input before signing (no raw-digest mode exists), so the
        // wire signature is produced over sha256(hash), not hash
        // itself — see P256VaultAccount's _rawSignatureValidation
        // override.
        (bytes32 r, bytes32 s) = vm.signP256(privateKey, sha256(abi.encodePacked(hash)));
        // P256 signatures are malleable (s and N-s both verify); normalize
        // to canonical lower-s form, same as OZ's own P256.t.sol does when
        // building vectors from vm.signP256.
        s = bytes32(Math.min(uint256(s), P256.N - uint256(s)));
        return abi.encodePacked(r, s);
    }

    function test_IsValidSignature_AcceptsValidRawP256Signature() public view {
        bytes32 hash = keccak256("create-track-digest");
        bytes memory signature = _sign(OWNER_KEY, hash);

        assertEq(account.isValidSignature(hash, signature), IERC1271.isValidSignature.selector);
    }

    function test_IsValidSignature_RejectsSignatureFromWrongKey() public view {
        bytes32 hash = keccak256("create-track-digest");
        bytes memory signature = _sign(STRANGER_KEY, hash);

        assertEq(account.isValidSignature(hash, signature), bytes4(0xffffffff));
    }

    function test_IsValidSignature_RejectsMalformedSignatureUnder64Bytes() public view {
        bytes32 hash = keccak256("create-track-digest");
        bytes memory tooShort = hex"1234";

        assertEq(account.isValidSignature(hash, tooShort), bytes4(0xffffffff));
    }
}

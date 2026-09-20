// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {WebAuthnAccount} from "../src/WebAuthnAccount.sol";
import {IERC1271} from "@openzeppelin/contracts/interfaces/IERC1271.sol";
import {P256} from "@openzeppelin/contracts/utils/cryptography/P256.sol";
import {WebAuthn} from "@openzeppelin/contracts/utils/cryptography/WebAuthn.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {Base64} from "@openzeppelin/contracts/utils/Base64.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";

contract WebAuthnAccountTest is Test {
    uint256 internal constant OWNER_KEY = 0xA11CE;
    uint256 internal constant STRANGER_KEY = 0xB0B;

    bytes32 internal ownerQx;
    bytes32 internal ownerQy;
    WebAuthnAccount internal account;

    function setUp() public {
        (uint256 x, uint256 y) = vm.publicKeyP256(OWNER_KEY);
        ownerQx = bytes32(x);
        ownerQy = bytes32(y);
        // The implementation itself is deployed once and permanently
        // locked (_disableInitializers in its constructor) — real usage
        // is always through a clone, per SignerP256's own NatSpec
        // recommendation and this contract's factory (AccountFactory).
        WebAuthnAccount implementation = new WebAuthnAccount();
        account = WebAuthnAccount(Clones.clone(address(implementation)));
        account.initialize(ownerQx, ownerQy);
    }

    function test_Initialize_RevertsOnSecondCall() public {
        vm.expectRevert();
        account.initialize(ownerQx, ownerQy);
    }

    // --- helpers, mirroring the pattern OZ's own WebAuthn.t.sol uses to
    // build real WebAuthnAuth vectors from Foundry's P-256 cheatcodes ---

    function _encodeAuthenticatorData() internal pure returns (bytes memory) {
        // UP (0x01) | UV (0x04): SignerWebAuthn forces requireUV=true via
        // the 4-arg WebAuthn.verify overload, so UV must be set or every
        // signature fails regardless of the cryptography being correct.
        return abi.encodePacked(bytes32(0), bytes1(0x05), bytes4(0));
    }

    function _encodeClientDataJSON(bytes memory challenge) internal pure returns (string memory) {
        // solhint-disable-next-line quotes
        return string.concat('{"type":"webauthn.get","challenge":"', Base64.encodeURL(challenge), '"}');
    }

    function _sign(uint256 privateKey, bytes memory authenticatorData, string memory clientDataJSON)
        internal
        pure
        returns (bytes32 r, bytes32 s)
    {
        bytes32 messageHash = sha256(abi.encodePacked(authenticatorData, sha256(bytes(clientDataJSON))));
        (r, s) = vm.signP256(privateKey, messageHash);
        // P256 signatures are malleable (s and N-s both verify); normalize
        // to the canonical lower-s form, same as OZ's own P256/WebAuthn
        // Foundry tests do when building vectors from vm.signP256.
        s = bytes32(Math.min(uint256(s), P256.N - uint256(s)));
    }

    function _buildSignature(uint256 privateKey, bytes32 hash) internal pure returns (bytes memory) {
        bytes memory authenticatorData = _encodeAuthenticatorData();
        string memory clientDataJSON = _encodeClientDataJSON(abi.encodePacked(hash));
        (bytes32 r, bytes32 s) = _sign(privateKey, authenticatorData, clientDataJSON);
        // Matches WebAuthn.WebAuthnAuth's field order/offsets, as confirmed
        // by tryDecodeAuth's own test vectors (challengeIndex=23,
        // typeIndex=1 for this exact JSON shape).
        return abi.encode(r, s, uint256(23), uint256(1), authenticatorData, clientDataJSON);
    }

    // --- tests ---

    function test_IsValidSignature_AcceptsValidWebAuthnAssertion() public view {
        bytes32 hash = keccak256("create-track-digest");
        bytes memory signature = _buildSignature(OWNER_KEY, hash);

        assertEq(account.isValidSignature(hash, signature), IERC1271.isValidSignature.selector);
    }

    function test_IsValidSignature_RejectsSignatureFromWrongKey() public view {
        bytes32 hash = keccak256("create-track-digest");
        bytes memory signature = _buildSignature(STRANGER_KEY, hash);

        assertEq(account.isValidSignature(hash, signature), bytes4(0xffffffff));
    }

    function test_IsValidSignature_RejectsTamperedChallenge() public view {
        // Assertion is genuinely signed by the owner key, but over a
        // different hash than the one passed to isValidSignature — the
        // clientDataJSON's embedded challenge won't match, so the WebAuthn
        // challenge check must fail even though the P-256 signature itself
        // is cryptographically valid for its own (different) message.
        bytes32 signedHash = keccak256("signed-digest");
        bytes32 presentedHash = keccak256("different-digest");
        bytes memory signature = _buildSignature(OWNER_KEY, signedHash);

        assertEq(account.isValidSignature(presentedHash, signature), bytes4(0xffffffff));
    }

    function test_IsValidSignature_DoesNotRevertOnMalformedSignature() public view {
        bytes32 hash = keccak256("create-track-digest");
        assertEq(account.isValidSignature(hash, hex"1234"), bytes4(0xffffffff));
    }
}

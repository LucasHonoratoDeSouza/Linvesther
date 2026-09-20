// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../src/IdentityRegistry.sol";
import {WebAuthnAccount} from "../src/WebAuthnAccount.sol";
import {P256VaultAccount} from "../src/P256VaultAccount.sol";
import {P256} from "@openzeppelin/contracts/utils/cryptography/P256.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {Base64} from "@openzeppelin/contracts/utils/Base64.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";

/// @notice Confirms IdentityRegistry.createTrack's
/// SignatureChecker.isValidSignatureNow path — the only integration point
/// this feature relies on — accepts WebAuthnAccount and P256VaultAccount
/// as `owner` with zero changes to IdentityRegistry itself.
contract WebAuthnAccountIdentityRegistryTest is Test {
    bytes32 internal constant CREATE_TRACK_TYPEHASH = keccak256(
        "CreateTrack(bytes32 identityId,bytes32 financialProfileId,bytes32 denominationCommitment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    uint256 internal constant WEBAUTHN_OWNER_KEY = 0xA11CE;
    uint256 internal constant VAULT_OWNER_KEY = 0xB0B;
    uint256 internal constant STRANGER_KEY = 0xEEEE;

    IdentityRegistry internal registry;
    WebAuthnAccount internal webAuthnAccount;
    P256VaultAccount internal vaultAccount;

    function setUp() public {
        registry = new IdentityRegistry();

        (uint256 wx, uint256 wy) = vm.publicKeyP256(WEBAUTHN_OWNER_KEY);
        webAuthnAccount = WebAuthnAccount(Clones.clone(address(new WebAuthnAccount())));
        webAuthnAccount.initialize(bytes32(wx), bytes32(wy));

        (uint256 vx, uint256 vy) = vm.publicKeyP256(VAULT_OWNER_KEY);
        vaultAccount = P256VaultAccount(Clones.clone(address(new P256VaultAccount())));
        vaultAccount.initialize(bytes32(vx), bytes32(vy));
    }

    // --- EIP-712 digest helpers, matching IdentityRegistry.t.sol ---

    function _domainSeparator() internal view returns (bytes32) {
        bytes32 domainTypeHash =
            keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
        return keccak256(
            abi.encode(
                domainTypeHash,
                keccak256(bytes("LinvestherZK-IdentityRegistry")),
                keccak256(bytes("1")),
                block.chainid,
                address(registry)
            )
        );
    }

    function _digest(bytes32 structHash) internal view returns (bytes32) {
        return keccak256(abi.encodePacked("\x19\x01", _domainSeparator(), structHash));
    }

    function _createTrackDigest(
        bytes32 identityId,
        bytes32 financialProfileId,
        bytes32 denominationCommitment,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline
    ) internal view returns (bytes32) {
        bytes32 structHash = keccak256(
            abi.encode(
                CREATE_TRACK_TYPEHASH, identityId, financialProfileId, denominationCommitment, ownerEpoch, nonce, deadline
            )
        );
        return _digest(structHash);
    }

    // --- WebAuthn signature envelope helpers (mirrors WebAuthnAccount.t.sol) ---

    function _encodeAuthenticatorData() internal pure returns (bytes memory) {
        return abi.encodePacked(bytes32(0), bytes1(0x05), bytes4(0)); // UP | UV
    }

    function _encodeClientDataJSON(bytes memory challenge) internal pure returns (string memory) {
        // solhint-disable-next-line quotes
        return string.concat('{"type":"webauthn.get","challenge":"', Base64.encodeURL(challenge), '"}');
    }

    function _normalizeS(bytes32 s) internal pure returns (bytes32) {
        return bytes32(Math.min(uint256(s), P256.N - uint256(s)));
    }

    function _webAuthnSignature(uint256 privateKey, bytes32 digest) internal pure returns (bytes memory) {
        bytes memory authenticatorData = _encodeAuthenticatorData();
        string memory clientDataJSON = _encodeClientDataJSON(abi.encodePacked(digest));
        bytes32 messageHash = sha256(abi.encodePacked(authenticatorData, sha256(bytes(clientDataJSON))));
        (bytes32 r, bytes32 s) = vm.signP256(privateKey, messageHash);
        return abi.encode(r, _normalizeS(s), uint256(23), uint256(1), authenticatorData, clientDataJSON);
    }

    function _rawP256Signature(uint256 privateKey, bytes32 digest) internal pure returns (bytes memory) {
        // A real vault signature covers sha256(digest), not digest
        // itself — see P256VaultAccount's _rawSignatureValidation
        // override.
        (bytes32 r, bytes32 s) = vm.signP256(privateKey, sha256(abi.encodePacked(digest)));
        return abi.encodePacked(r, _normalizeS(s));
    }

    // --- tests ---

    function test_CreateTrack_AcceptsWebAuthnAccountOwnerSignature() public {
        vm.prank(address(webAuthnAccount));
        bytes32 identityId = registry.createIdentity(IdentityRegistry.IdentityType.Person);

        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 digest = _createTrackDigest(identityId, financialProfileId, denomination, 0, 0, deadline);
        bytes memory signature = _webAuthnSignature(WEBAUTHN_OWNER_KEY, digest);

        bytes32 trackId = registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, signature);
        (,,,, bool exists) = registry.tracks(trackId);
        assertTrue(exists);
    }

    function test_CreateTrack_AcceptsP256VaultAccountOwnerSignature() public {
        vm.prank(address(vaultAccount));
        bytes32 identityId = registry.createIdentity(IdentityRegistry.IdentityType.Person);

        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 digest = _createTrackDigest(identityId, financialProfileId, denomination, 0, 0, deadline);
        bytes memory signature = _rawP256Signature(VAULT_OWNER_KEY, digest);

        bytes32 trackId = registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, signature);
        (,,,, bool exists) = registry.tracks(trackId);
        assertTrue(exists);
    }

    function test_CreateTrack_RevertsForWebAuthnAccountWithWrongKey() public {
        vm.prank(address(webAuthnAccount));
        bytes32 identityId = registry.createIdentity(IdentityRegistry.IdentityType.Person);

        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 digest = _createTrackDigest(identityId, financialProfileId, denomination, 0, 0, deadline);
        // Signed by a P-256 key that is not the one configured on
        // webAuthnAccount — the account's own isValidSignature will
        // reject it, so the registry must reject the whole call too.
        bytes memory signature = _webAuthnSignature(STRANGER_KEY, digest);

        vm.expectRevert(IdentityRegistry.InvalidSignature.selector);
        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, signature);
    }

    function test_CreateTrack_RevertsForP256VaultAccountWithWrongKey() public {
        vm.prank(address(vaultAccount));
        bytes32 identityId = registry.createIdentity(IdentityRegistry.IdentityType.Person);

        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 digest = _createTrackDigest(identityId, financialProfileId, denomination, 0, 0, deadline);
        bytes memory signature = _rawP256Signature(STRANGER_KEY, digest);

        vm.expectRevert(IdentityRegistry.InvalidSignature.selector);
        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, signature);
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {AccountFactory} from "../src/AccountFactory.sol";
import {WebAuthnAccount} from "../src/WebAuthnAccount.sol";
import {P256VaultAccount} from "../src/P256VaultAccount.sol";
import {IERC1271} from "@openzeppelin/contracts/interfaces/IERC1271.sol";
import {P256} from "@openzeppelin/contracts/utils/cryptography/P256.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {Base64} from "@openzeppelin/contracts/utils/Base64.sol";

contract AccountFactoryTest is Test {
    uint256 internal constant OWNER_KEY = 0xA11CE;

    AccountFactory internal factory;
    bytes32 internal qx;
    bytes32 internal qy;

    function setUp() public {
        factory = new AccountFactory(address(new WebAuthnAccount()), address(new P256VaultAccount()));
        (uint256 x, uint256 y) = vm.publicKeyP256(OWNER_KEY);
        qx = bytes32(x);
        qy = bytes32(y);
    }

    function test_DeployWebAuthnAccount_MatchesPredictedAddress() public {
        address predicted = factory.predictWebAuthnAccountAddress(qx, qy);
        address deployed = factory.deployWebAuthnAccount(qx, qy);

        assertEq(deployed, predicted);
        assertGt(predicted.code.length, 0);
    }

    function test_DeployP256VaultAccount_MatchesPredictedAddress() public {
        address predicted = factory.predictP256VaultAccountAddress(qx, qy);
        address deployed = factory.deployP256VaultAccount(qx, qy);

        assertEq(deployed, predicted);
        assertGt(predicted.code.length, 0);
    }

    function test_DeployWebAuthnAccount_IsIdempotent() public {
        address first = factory.deployWebAuthnAccount(qx, qy);
        address second = factory.deployWebAuthnAccount(qx, qy);

        assertEq(first, second);
    }

    function test_DeployP256VaultAccount_IsIdempotent() public {
        address first = factory.deployP256VaultAccount(qx, qy);
        address second = factory.deployP256VaultAccount(qx, qy);

        assertEq(first, second);
    }

    function test_DeployedWebAuthnAccount_ValidatesRealSignature() public {
        address deployed = factory.deployWebAuthnAccount(qx, qy);

        bytes32 hash = keccak256("create-track-digest");
        bytes memory authenticatorData = abi.encodePacked(bytes32(0), bytes1(0x05), bytes4(0));
        string memory clientDataJSON = string.concat(
            '{"type":"webauthn.get","challenge":"',
            Base64.encodeURL(abi.encodePacked(hash)),
            '"}'
        );
        bytes32 messageHash = sha256(abi.encodePacked(authenticatorData, sha256(bytes(clientDataJSON))));
        (bytes32 r, bytes32 s) = vm.signP256(OWNER_KEY, messageHash);
        s = bytes32(Math.min(uint256(s), P256.N - uint256(s)));
        bytes memory signature = abi.encode(r, s, uint256(23), uint256(1), authenticatorData, clientDataJSON);

        assertEq(WebAuthnAccount(deployed).isValidSignature(hash, signature), IERC1271.isValidSignature.selector);
    }

    function test_DeployedP256VaultAccount_ValidatesRealSignature() public {
        address deployed = factory.deployP256VaultAccount(qx, qy);

        bytes32 hash = keccak256("create-track-digest");
        // A real vault signature covers sha256(hash), not hash itself —
        // see P256VaultAccount's _rawSignatureValidation override.
        (bytes32 r, bytes32 s) = vm.signP256(OWNER_KEY, sha256(abi.encodePacked(hash)));
        s = bytes32(Math.min(uint256(s), P256.N - uint256(s)));
        bytes memory signature = abi.encodePacked(r, s);

        assertEq(P256VaultAccount(deployed).isValidSignature(hash, signature), IERC1271.isValidSignature.selector);
    }
}

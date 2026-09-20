// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../src/IdentityRegistry.sol";
import {ProfileRegistry, IIdentityOwnership} from "../src/ProfileRegistry.sol";

contract ProfileRegistryTest is Test {
    IdentityRegistry internal identities;
    ProfileRegistry internal profiles;

    bytes32 internal constant SET_PROFILE_TYPEHASH =
        keccak256("SetProfile(bytes32 identityId,string name,string bio,uint64 nonce,uint256 deadline)");

    uint256 internal ownerKey = 0xA11CE;
    uint256 internal strangerKey = 0xEEEE;
    address internal owner;
    bytes32 internal identityId;

    event ProfileUpdated(bytes32 indexed identityId, address indexed owner, string name, string bio, uint64 updatedAt);

    function setUp() public {
        identities = new IdentityRegistry();
        profiles = new ProfileRegistry(IIdentityOwnership(address(identities)));
        owner = vm.addr(ownerKey);
        vm.prank(owner);
        identityId = identities.createIdentity(IdentityRegistry.IdentityType.Person);
    }

    function _sign(uint256 key, bytes32 id, string memory name, string memory bio, uint64 nonce, uint256 deadline)
        internal
        view
        returns (bytes memory)
    {
        bytes32 structHash =
            keccak256(abi.encode(SET_PROFILE_TYPEHASH, id, keccak256(bytes(name)), keccak256(bytes(bio)), nonce, deadline));
        bytes32 domain = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256(bytes("LinvestherZK-ProfileRegistry")),
                keccak256(bytes("1")),
                block.chainid,
                address(profiles)
            )
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, keccak256(abi.encodePacked("\x19\x01", domain, structHash)));
        return abi.encodePacked(r, s, v);
    }

    function test_OwnerSetsAndAnyoneReadsIt_ThroughAnyRelayer() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory signature = _sign(ownerKey, identityId, "Lucas", "Trader since 2019", 0, deadline);

        vm.expectEmit(true, true, false, true);
        emit ProfileUpdated(identityId, owner, "Lucas", "Trader since 2019", uint64(block.timestamp));
        vm.prank(address(0xBEEF)); // a relayer, not the owner
        profiles.setProfile(identityId, "Lucas", "Trader since 2019", 0, deadline, signature);

        (string memory name, string memory bio, uint64 updatedAt) = profiles.profileOf(identityId);
        assertEq(name, "Lucas");
        assertEq(bio, "Trader since 2019");
        assertEq(updatedAt, uint64(block.timestamp));
        assertEq(profiles.nonces(identityId), 1);
    }

    function test_OwnerCanChangeItAgain_AndClearIt() public {
        uint256 deadline = block.timestamp + 1 hours;
        profiles.setProfile(identityId, "A", "one", 0, deadline, _sign(ownerKey, identityId, "A", "one", 0, deadline));
        profiles.setProfile(identityId, "", "the word my friend asked for", 1, deadline, _sign(ownerKey, identityId, "", "the word my friend asked for", 1, deadline));
        (string memory name, string memory bio,) = profiles.profileOf(identityId);
        assertEq(name, "");
        assertEq(bio, "the word my friend asked for");
    }

    function test_RevertWhen_SignedByAStranger() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory signature = _sign(strangerKey, identityId, "Fake", "not mine", 0, deadline);
        vm.expectRevert(ProfileRegistry.InvalidSignature.selector);
        profiles.setProfile(identityId, "Fake", "not mine", 0, deadline, signature);
    }

    function test_RevertWhen_TheSignedTextIsSwappedForOtherText() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory signature = _sign(ownerKey, identityId, "Lucas", "hello", 0, deadline);
        vm.expectRevert(ProfileRegistry.InvalidSignature.selector);
        profiles.setProfile(identityId, "Lucas", "something else", 0, deadline, signature);
    }

    function test_RevertWhen_ASignatureIsReplayed() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory signature = _sign(ownerKey, identityId, "Lucas", "hello", 0, deadline);
        profiles.setProfile(identityId, "Lucas", "hello", 0, deadline, signature);
        vm.expectRevert(abi.encodeWithSelector(ProfileRegistry.NonceMismatch.selector, uint64(1), uint64(0)));
        profiles.setProfile(identityId, "Lucas", "hello", 0, deadline, signature);
    }

    function test_RevertWhen_Expired() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory signature = _sign(ownerKey, identityId, "Lucas", "hello", 0, deadline);
        vm.warp(deadline + 1);
        vm.expectRevert(abi.encodeWithSelector(ProfileRegistry.CommandExpired.selector, deadline, block.timestamp));
        profiles.setProfile(identityId, "Lucas", "hello", 0, deadline, signature);
    }

    function test_RevertWhen_TextIsTooLong() public {
        uint256 deadline = block.timestamp + 1 hours;
        string memory longName = "0123456789012345678901234567890123456789X";
        vm.expectRevert(abi.encodeWithSelector(ProfileRegistry.NameTooLong.selector, uint256(41)));
        profiles.setProfile(identityId, longName, "", 0, deadline, _sign(ownerKey, identityId, longName, "", 0, deadline));

        bytes memory bio = new bytes(281);
        vm.expectRevert(abi.encodeWithSelector(ProfileRegistry.BioTooLong.selector, uint256(281)));
        profiles.setProfile(identityId, "", string(bio), 0, deadline, _sign(ownerKey, identityId, "", string(bio), 0, deadline));
    }

    function test_RevertWhen_IdentityDoesNotExist() public {
        bytes32 missing = keccak256("missing");
        uint256 deadline = block.timestamp + 1 hours;
        vm.expectRevert(abi.encodeWithSelector(ProfileRegistry.IdentityNotFound.selector, missing));
        profiles.setProfile(missing, "x", "", 0, deadline, _sign(ownerKey, missing, "x", "", 0, deadline));
    }

    function test_AProfileFollowsOwnershipWhenTheOwnerRotates() public {
        uint256 newKey = 0xB0B;
        address newOwner = vm.addr(newKey);
        // Rotate: propose (by old owner) then confirm (by new owner), via the registry's own signed commands.
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 domain = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256(bytes("LinvestherZK-IdentityRegistry")),
                keccak256(bytes("1")),
                block.chainid,
                address(identities)
            )
        );
        bytes32 proposeHash = keccak256(
            abi.encode(
                keccak256("ProposeOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"),
                identityId, newOwner, uint64(0), uint64(0), deadline
            )
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ownerKey, keccak256(abi.encodePacked("\x19\x01", domain, proposeHash)));
        identities.proposeOwnerRotation(identityId, newOwner, 0, 0, deadline, abi.encodePacked(r, s, v));
        bytes32 confirmHash = keccak256(
            abi.encode(
                keccak256("ConfirmOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"),
                identityId, newOwner, uint64(0), uint64(1), deadline
            )
        );
        (v, r, s) = vm.sign(newKey, keccak256(abi.encodePacked("\x19\x01", domain, confirmHash)));
        identities.confirmOwnerRotation(identityId, newOwner, 0, 1, deadline, abi.encodePacked(r, s, v));

        vm.expectRevert(ProfileRegistry.InvalidSignature.selector);
        profiles.setProfile(identityId, "old", "", 0, deadline, _sign(ownerKey, identityId, "old", "", 0, deadline));
        profiles.setProfile(identityId, "new", "", 0, deadline, _sign(newKey, identityId, "new", "", 0, deadline));
    }
}

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../src/IdentityRegistry.sol";
import {MockERC1271Wallet} from "./mocks/MockERC1271Wallet.sol";

contract IdentityRegistryTest is Test {
    IdentityRegistry internal registry;

    bytes32 internal constant CREATE_TRACK_TYPEHASH = keccak256(
        "CreateTrack(bytes32 identityId,bytes32 financialProfileId,bytes32 denominationCommitment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant PROPOSE_OWNER_ROTATION_TYPEHASH = keccak256(
        "ProposeOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant CONFIRM_OWNER_ROTATION_TYPEHASH = keccak256(
        "ConfirmOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    uint256 internal ownerKey = 0xA11CE;
    uint256 internal newOwnerKey = 0xB0B;
    uint256 internal strangerKey = 0xEEEE;
    address internal owner;
    address internal newOwner;
    address internal stranger;

    function setUp() public {
        registry = new IdentityRegistry();
        owner = vm.addr(ownerKey);
        newOwner = vm.addr(newOwnerKey);
        stranger = vm.addr(strangerKey);
    }

    // --- helpers --------------------------------------------------------

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

    function _signCreateTrack(
        uint256 signerKey,
        bytes32 identityId,
        bytes32 financialProfileId,
        bytes32 denominationCommitment,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline
    ) internal view returns (bytes memory) {
        bytes32 structHash = keccak256(
            abi.encode(
                CREATE_TRACK_TYPEHASH, identityId, financialProfileId, denominationCommitment, ownerEpoch, nonce, deadline
            )
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(signerKey, _digest(structHash));
        return abi.encodePacked(r, s, v);
    }

    function _signProposeRotation(
        uint256 signerKey,
        bytes32 identityId,
        address proposedNewOwner,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline
    ) internal view returns (bytes memory) {
        bytes32 structHash = keccak256(
            abi.encode(PROPOSE_OWNER_ROTATION_TYPEHASH, identityId, proposedNewOwner, ownerEpoch, nonce, deadline)
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(signerKey, _digest(structHash));
        return abi.encodePacked(r, s, v);
    }

    function _signConfirmRotation(
        uint256 signerKey,
        bytes32 identityId,
        address proposedNewOwner,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline
    ) internal view returns (bytes memory) {
        bytes32 structHash = keccak256(
            abi.encode(CONFIRM_OWNER_ROTATION_TYPEHASH, identityId, proposedNewOwner, ownerEpoch, nonce, deadline)
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(signerKey, _digest(structHash));
        return abi.encodePacked(r, s, v);
    }

    function _createIdentity(address as_) internal returns (bytes32 identityId) {
        vm.prank(as_);
        identityId = registry.createIdentity(IdentityRegistry.IdentityType.Person);
    }

    // --- identity creation -----------------------------------------------

    function test_CreateIdentity_PersistsStableIdAndFields() public {
        bytes32 identityId = _createIdentity(owner);
        (address storedOwner, address pendingOwner, uint64 createdAt, uint64 ownerEpoch, uint64 nonce,, bool exists) =
            registry.identities(identityId);
        assertTrue(exists);
        assertEq(storedOwner, owner);
        assertEq(pendingOwner, address(0));
        assertEq(createdAt, block.timestamp);
        assertEq(ownerEpoch, 0);
        assertEq(nonce, 0);
    }

    function test_CreateIdentity_DifferentCallsYieldDifferentIds() public {
        bytes32 a = _createIdentity(owner);
        bytes32 b = _createIdentity(owner);
        assertTrue(a != b);
    }

    function test_CreateIdentity_IdIsNotTheOwnerAddress() public {
        bytes32 identityId = _createIdentity(owner);
        assertTrue(identityId != bytes32(uint256(uint160(owner))));
    }

    // --- track creation / replay protection -------------------------------

    function test_CreateTrack_SucceedsWithValidOwnerSignature() public {
        bytes32 identityId = _createIdentity(owner);
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig = _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 0, 0, deadline);

        bytes32 trackId = registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);

        (bytes32 trackIdentityId, bytes32 storedProfile, bytes32 storedDenomination,, bool exists) =
            registry.tracks(trackId);
        assertTrue(exists);
        assertEq(trackIdentityId, identityId);
        assertEq(storedProfile, financialProfileId);
        assertEq(storedDenomination, denomination);
    }

    function test_CreateTrack_RevertsOnReplayOfSameSignature() public {
        bytes32 identityId = _createIdentity(owner);
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig = _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 0, 0, deadline);

        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);

        // Same signature, same nonce: the nonce was already consumed.
        vm.expectRevert(abi.encodeWithSelector(IdentityRegistry.NonceMismatch.selector, 1, 0));
        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);
    }

    function test_CreateTrack_RevertsWhenSignedByNonOwner() public {
        bytes32 identityId = _createIdentity(owner);
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig = _signCreateTrack(strangerKey, identityId, financialProfileId, denomination, 0, 0, deadline);

        vm.expectRevert(IdentityRegistry.InvalidSignature.selector);
        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);
    }

    function test_CreateTrack_RevertsAfterDeadline() public {
        bytes32 identityId = _createIdentity(owner);
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig = _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 0, 0, deadline);

        vm.warp(deadline + 1);
        vm.expectRevert(abi.encodeWithSelector(IdentityRegistry.CommandExpired.selector, deadline, deadline + 1));
        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);
    }

    function test_CreateTrack_RevertsOnWrongEpoch() public {
        bytes32 identityId = _createIdentity(owner);
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        // Signed for epoch 1, but the identity is still at epoch 0.
        bytes memory sig = _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 1, 0, deadline);

        vm.expectRevert(abi.encodeWithSelector(IdentityRegistry.OwnerEpochMismatch.selector, 0, 1));
        registry.createTrack(identityId, financialProfileId, denomination, 1, 0, deadline, sig);
    }

    function test_CreateTrack_RevertsForUnknownIdentity() public {
        bytes32 unknownId = keccak256("nope");
        vm.expectRevert(abi.encodeWithSelector(IdentityRegistry.IdentityNotFound.selector, unknownId));
        registry.createTrack(unknownId, bytes32(0), bytes32(0), 0, 0, block.timestamp + 1 hours, "");
    }

    function test_CreateTrack_SignatureForOneIdentityDoesNotAuthorizeAnother() public {
        bytes32 identityA = _createIdentity(owner);
        bytes32 identityB = _createIdentity(owner); // same owner, different identity
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        // Signed for identity A...
        bytes memory sig = _signCreateTrack(ownerKey, identityA, financialProfileId, denomination, 0, 0, deadline);

        // ...must not authorize the same action on identity B, even though
        // both are owned by the same key and both are at nonce 0/epoch 0.
        vm.expectRevert(IdentityRegistry.InvalidSignature.selector);
        registry.createTrack(identityB, financialProfileId, denomination, 0, 0, deadline, sig);
    }

    // --- owner rotation ---------------------------------------------------

    function test_RotateOwner_TwoStepSuccessIncrementsEpochAndChangesOwner() public {
        bytes32 identityId = _createIdentity(owner);
        uint256 deadline = block.timestamp + 1 hours;

        bytes memory proposeSig = _signProposeRotation(ownerKey, identityId, newOwner, 0, 0, deadline);
        registry.proposeOwnerRotation(identityId, newOwner, 0, 0, deadline, proposeSig);

        (address ownerAfterPropose, address pendingAfterPropose,,,,,) = registry.identities(identityId);
        assertEq(ownerAfterPropose, owner, "owner must not change until confirmation");
        assertEq(pendingAfterPropose, newOwner);

        bytes memory confirmSig = _signConfirmRotation(newOwnerKey, identityId, newOwner, 0, 1, deadline);
        registry.confirmOwnerRotation(identityId, newOwner, 0, 1, deadline, confirmSig);

        (address finalOwner, address finalPending,, uint64 finalEpoch,,,) = registry.identities(identityId);
        assertEq(finalOwner, newOwner);
        assertEq(finalPending, address(0));
        assertEq(finalEpoch, 1);
    }

    function test_RotateOwner_RevertsWhenConfirmedByNonPendingOwner() public {
        bytes32 identityId = _createIdentity(owner);
        uint256 deadline = block.timestamp + 1 hours;

        bytes memory proposeSig = _signProposeRotation(ownerKey, identityId, newOwner, 0, 0, deadline);
        registry.proposeOwnerRotation(identityId, newOwner, 0, 0, deadline, proposeSig);

        // Stranger signs a confirmation claiming to be newOwner: the
        // signature check binds the signer address, so this must fail
        // even though the (identityId, newOwner, epoch, nonce) fields match.
        bytes memory confirmSig = _signConfirmRotation(strangerKey, identityId, newOwner, 0, 1, deadline);
        vm.expectRevert(IdentityRegistry.InvalidSignature.selector);
        registry.confirmOwnerRotation(identityId, newOwner, 0, 1, deadline, confirmSig);
    }

    function test_RotateOwner_RevertsWithoutPendingRotation() public {
        bytes32 identityId = _createIdentity(owner);
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory confirmSig = _signConfirmRotation(newOwnerKey, identityId, newOwner, 0, 0, deadline);
        vm.expectRevert(abi.encodeWithSelector(IdentityRegistry.NoPendingRotation.selector, identityId));
        registry.confirmOwnerRotation(identityId, newOwner, 0, 0, deadline, confirmSig);
    }

    function test_RotateOwner_OldOwnerKeyIsRevokedAfterRotation() public {
        bytes32 identityId = _createIdentity(owner);
        uint256 deadline = block.timestamp + 1 hours;

        bytes memory proposeSig = _signProposeRotation(ownerKey, identityId, newOwner, 0, 0, deadline);
        registry.proposeOwnerRotation(identityId, newOwner, 0, 0, deadline, proposeSig);
        bytes memory confirmSig = _signConfirmRotation(newOwnerKey, identityId, newOwner, 0, 1, deadline);
        registry.confirmOwnerRotation(identityId, newOwner, 0, 1, deadline, confirmSig);

        // Old owner tries to authorize a new track post-rotation, signing
        // with what it (incorrectly) believes is the still-current epoch
        // (0) and the next nonce (2): must fail on epoch, since the epoch
        // was bumped to 1 by the rotation and the old owner is no longer
        // the registered signer for this identity at all.
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        bytes memory staleSig = _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 0, 2, deadline);
        vm.expectRevert(abi.encodeWithSelector(IdentityRegistry.OwnerEpochMismatch.selector, 1, 0));
        registry.createTrack(identityId, financialProfileId, denomination, 0, 2, deadline, staleSig);

        // Even signing with the correct current epoch (1), the old owner's
        // key is simply no longer the identity's owner, so the signature
        // itself is invalid.
        bytes memory staleSigCorrectEpoch =
            _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 1, 2, deadline);
        vm.expectRevert(IdentityRegistry.InvalidSignature.selector);
        registry.createTrack(identityId, financialProfileId, denomination, 1, 2, deadline, staleSigCorrectEpoch);
    }

    function test_RotateOwner_RevertsOnZeroAddress() public {
        bytes32 identityId = _createIdentity(owner);
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig = _signProposeRotation(ownerKey, identityId, address(0), 0, 0, deadline);
        vm.expectRevert(IdentityRegistry.ZeroAddress.selector);
        registry.proposeOwnerRotation(identityId, address(0), 0, 0, deadline, sig);
    }

    function test_RotateOwner_RevertsWhenProposingSameOwner() public {
        bytes32 identityId = _createIdentity(owner);
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig = _signProposeRotation(ownerKey, identityId, owner, 0, 0, deadline);
        vm.expectRevert(IdentityRegistry.SameOwner.selector);
        registry.proposeOwnerRotation(identityId, owner, 0, 0, deadline, sig);
    }

    // --- ERC-1271 contract wallet coverage --------------------------------

    function test_ContractWalletCanOwnIdentityAndAuthorizeCreateTrack() public {
        MockERC1271Wallet wallet = new MockERC1271Wallet(owner);
        bytes32 identityId = _createIdentity(address(wallet));

        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        // The wallet's authorized EOA signs; the contract's isValidSignature
        // is what actually authorizes the call, not a direct EOA owner.
        bytes memory sig = _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 0, 0, deadline);

        bytes32 trackId = registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);
        (,,,, bool exists) = registry.tracks(trackId);
        assertTrue(exists);
    }

    function test_ContractWalletCanBecomeNewOwnerViaRotation() public {
        MockERC1271Wallet wallet = new MockERC1271Wallet(newOwner);
        bytes32 identityId = _createIdentity(owner);
        uint256 deadline = block.timestamp + 1 hours;

        bytes memory proposeSig = _signProposeRotation(ownerKey, identityId, address(wallet), 0, 0, deadline);
        registry.proposeOwnerRotation(identityId, address(wallet), 0, 0, deadline, proposeSig);

        // The wallet confirms via its authorized signer's ECDSA signature,
        // checked through ERC-1271 by the registry.
        bytes memory confirmSig = _signConfirmRotation(newOwnerKey, identityId, address(wallet), 0, 1, deadline);
        registry.confirmOwnerRotation(identityId, address(wallet), 0, 1, deadline, confirmSig);

        (address finalOwner,,,,,,) = registry.identities(identityId);
        assertEq(finalOwner, address(wallet));
    }

    function test_ContractWalletRejectsSignatureFromUnauthorizedSigner() public {
        MockERC1271Wallet wallet = new MockERC1271Wallet(owner); // authorized: owner, not stranger
        bytes32 identityId = _createIdentity(address(wallet));

        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig = _signCreateTrack(strangerKey, identityId, financialProfileId, denomination, 0, 0, deadline);

        vm.expectRevert(IdentityRegistry.InvalidSignature.selector);
        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);
    }

    // --- fuzz ---------------------------------------------------------

    function testFuzz_CreateTrack_RevertsOnAnyWrongNonce(uint64 wrongNonce) public {
        vm.assume(wrongNonce != 0);
        bytes32 identityId = _createIdentity(owner);
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp + 1 hours;
        bytes memory sig =
            _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 0, wrongNonce, deadline);

        vm.expectRevert(abi.encodeWithSelector(IdentityRegistry.NonceMismatch.selector, 0, wrongNonce));
        registry.createTrack(identityId, financialProfileId, denomination, 0, wrongNonce, deadline, sig);
    }

    function testFuzz_CreateTrack_RevertsOnAnyDeadlineInThePast(uint256 pastOffset) public {
        vm.warp(365 days);
        pastOffset = bound(pastOffset, 1, block.timestamp);
        bytes32 identityId = _createIdentity(owner);
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        uint256 deadline = block.timestamp - pastOffset;
        bytes memory sig = _signCreateTrack(ownerKey, identityId, financialProfileId, denomination, 0, 0, deadline);

        vm.expectRevert(
            abi.encodeWithSelector(IdentityRegistry.CommandExpired.selector, deadline, block.timestamp)
        );
        registry.createTrack(identityId, financialProfileId, denomination, 0, 0, deadline, sig);
    }
}

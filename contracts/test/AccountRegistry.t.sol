// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../src/IdentityRegistry.sol";
import {AccountRegistry} from "../src/AccountRegistry.sol";

contract AccountRegistryTest is Test {
    IdentityRegistry internal identityRegistry;
    AccountRegistry internal registry;

    bytes32 internal constant CREATE_TRACK_TYPEHASH = keccak256(
        "CreateTrack(bytes32 identityId,bytes32 financialProfileId,bytes32 denominationCommitment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant PROPOSE_OWNER_ROTATION_TYPEHASH = keccak256(
        "ProposeOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant CONFIRM_OWNER_ROTATION_TYPEHASH = keccak256(
        "ConfirmOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant REGISTER_ACCOUNT_TYPEHASH = keccak256(
        "RegisterAccount(bytes32 trackId,bytes32 venueId,bytes32 authenticatedIdCommitment,uint8 environment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant ACTIVATE_ACCOUNT_TYPEHASH = keccak256(
        "ActivateAccount(bytes32 accountId,uint64 eligibleFrom,bytes32 reasonHash,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant REMOVE_ACCOUNT_TYPEHASH = keccak256(
        "RemoveAccount(bytes32 accountId,bytes32 reasonHash,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    uint256 internal ownerKey = 0xA11CE;
    uint256 internal strangerKey = 0xEEEE;
    address internal owner;
    address internal stranger;

    bytes32 internal identityId;
    bytes32 internal trackId;
    bytes32 internal constant VENUE_ID = keccak256("binance-global");
    bytes32 internal constant FINANCIAL_PROFILE_ID = keccak256("spot-nav-twr-v1");
    bytes32 internal constant DENOMINATION = keccak256("USDT");

    function setUp() public {
        identityRegistry = new IdentityRegistry();
        registry = new AccountRegistry(identityRegistry);
        owner = vm.addr(ownerKey);
        stranger = vm.addr(strangerKey);

        vm.prank(owner);
        identityId = identityRegistry.createIdentity(IdentityRegistry.IdentityType.Person);

        bytes32 structHash = keccak256(
            abi.encode(CREATE_TRACK_TYPEHASH, identityId, FINANCIAL_PROFILE_ID, DENOMINATION, uint64(0), uint64(0), block.timestamp + 1 hours)
        );
        bytes memory sig = _sign(ownerKey, _identityDigest(structHash));
        trackId =
            identityRegistry.createTrack(identityId, FINANCIAL_PROFILE_ID, DENOMINATION, 0, 0, block.timestamp + 1 hours, sig);
    }

    // --- helpers --------------------------------------------------------

    function _identityDomainSeparator() internal view returns (bytes32) {
        bytes32 domainTypeHash =
            keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
        return keccak256(
            abi.encode(
                domainTypeHash,
                keccak256(bytes("LinvestherZK-IdentityRegistry")),
                keccak256(bytes("1")),
                block.chainid,
                address(identityRegistry)
            )
        );
    }

    function _identityDigest(bytes32 structHash) internal view returns (bytes32) {
        return keccak256(abi.encodePacked("\x19\x01", _identityDomainSeparator(), structHash));
    }

    function _accountDomainSeparator() internal view returns (bytes32) {
        bytes32 domainTypeHash =
            keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
        return keccak256(
            abi.encode(
                domainTypeHash,
                keccak256(bytes("LinvestherZK-AccountRegistry")),
                keccak256(bytes("1")),
                block.chainid,
                address(registry)
            )
        );
    }

    function _accountDigest(bytes32 structHash) internal view returns (bytes32) {
        return keccak256(abi.encodePacked("\x19\x01", _accountDomainSeparator(), structHash));
    }

    function _sign(uint256 key, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, digest);
        return abi.encodePacked(r, s, v);
    }

    function _registerAccount(uint64 nonce, uint256 deadline) internal returns (bytes32 accountId) {
        bytes32 structHash = keccak256(
            abi.encode(
                REGISTER_ACCOUNT_TYPEHASH, trackId, VENUE_ID, keccak256("uid-commitment"), uint8(0), uint64(0), nonce, deadline
            )
        );
        bytes memory sig = _sign(ownerKey, _accountDigest(structHash));
        return registry.registerAccount(trackId, VENUE_ID, keccak256("uid-commitment"), AccountRegistry.Environment.Production, 0, nonce, deadline, sig);
    }

    function _activateAccount(bytes32 accountId, uint64 eligibleFrom, bytes32 reasonHash, uint64 nonce, uint256 deadline)
        internal
    {
        bytes32 structHash = keccak256(
            abi.encode(ACTIVATE_ACCOUNT_TYPEHASH, accountId, eligibleFrom, reasonHash, uint64(0), nonce, deadline)
        );
        bytes memory sig = _sign(ownerKey, _accountDigest(structHash));
        registry.activateAccount(accountId, eligibleFrom, reasonHash, 0, nonce, deadline, sig);
    }

    function _removeAccount(bytes32 accountId, bytes32 reasonHash, uint64 nonce, uint256 deadline) internal {
        bytes32 structHash =
            keccak256(abi.encode(REMOVE_ACCOUNT_TYPEHASH, accountId, reasonHash, uint64(0), nonce, deadline));
        bytes memory sig = _sign(ownerKey, _accountDigest(structHash));
        registry.removeAccount(accountId, reasonHash, 0, nonce, deadline, sig);
    }

    // --- registration -----------------------------------------------------

    function test_RegisterAccount_CreatesPendingBaselineBinding() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 accountId = _registerAccount(0, deadline);

        (
            bytes32 storedTrackId,
            bytes32 venueId,
            ,
            ,
            uint64 registeredAt,
            uint64 eligibleFrom,
            uint64 removedAt,
            uint64 bindingEpoch,
            AccountRegistry.AccountState state,
            bool exists
        ) = registry.accounts(accountId);

        assertTrue(exists);
        assertEq(storedTrackId, trackId);
        assertEq(venueId, VENUE_ID);
        assertEq(registeredAt, block.timestamp);
        assertEq(eligibleFrom, 0);
        assertEq(removedAt, 0);
        assertEq(bindingEpoch, 0);
        assertTrue(state == AccountRegistry.AccountState.PendingBaseline);
    }

    function test_RegisterAccount_DoesNotYetJoinMembership() public {
        _registerAccount(0, block.timestamp + 1 hours);
        assertEq(registry.currentMembers(trackId).length, 0);
        assertEq(registry.membershipEpochCount(trackId), 0);
    }

    function test_RegisterAccount_RevertsWhenSignedByNonOwner() public {
        bytes32 structHash = keccak256(
            abi.encode(
                REGISTER_ACCOUNT_TYPEHASH,
                trackId,
                VENUE_ID,
                keccak256("uid"),
                uint8(0),
                uint64(0),
                uint64(0),
                block.timestamp + 1 hours
            )
        );
        bytes memory sig = _sign(strangerKey, _accountDigest(structHash));
        vm.expectRevert(AccountRegistry.InvalidSignature.selector);
        registry.registerAccount(
            trackId, VENUE_ID, keccak256("uid"), AccountRegistry.Environment.Production, 0, 0, block.timestamp + 1 hours, sig
        );
    }

    function test_RegisterAccount_RevertsForUnknownTrack() public {
        bytes32 unknownTrack = keccak256("nope");
        vm.expectRevert(abi.encodeWithSelector(AccountRegistry.TrackNotFound.selector, unknownTrack));
        registry.registerAccount(
            unknownTrack, VENUE_ID, keccak256("uid"), AccountRegistry.Environment.Production, 0, 0, block.timestamp + 1 hours, ""
        );
    }

    // --- activation / membership is prospective ----------------------------

    function test_ActivateAccount_JoinsMembershipProspectivelyFromNow() public {
        uint256 t0 = block.timestamp;
        bytes32 accountId = _registerAccount(0, t0 + 1 hours);

        vm.warp(t0 + 10);
        _activateAccount(accountId, uint64(t0 + 10), keccak256("baseline-ok"), 1, t0 + 1 hours);

        bytes32[] memory members = registry.currentMembers(trackId);
        assertEq(members.length, 1);
        assertEq(members[0], accountId);
        assertEq(registry.membershipEpochCount(trackId), 1);

        (,, bytes32 previousRoot, uint64 effectiveFrom,) = registry.membershipEpochAt(trackId, 0);
        assertEq(previousRoot, bytes32(0));
        assertEq(effectiveFrom, t0 + 10, "epoch takes effect at the activation block time, not retroactively");
    }

    function test_ActivateAccount_RevertsWhenEligibleFromPredatesRegistration() public {
        uint256 t0 = block.timestamp;
        bytes32 accountId = _registerAccount(0, t0 + 1 hours);

        vm.expectRevert(
            abi.encodeWithSelector(AccountRegistry.InvalidEligibleFrom.selector, uint64(t0), uint64(t0 - 1))
        );
        _activateAccount(accountId, uint64(t0 - 1), keccak256("baseline-ok"), 1, t0 + 1 hours);
    }

    function test_ActivateAccount_RevertsWhenAlreadyActive() public {
        uint256 t0 = block.timestamp;
        bytes32 accountId = _registerAccount(0, t0 + 1 hours);
        _activateAccount(accountId, uint64(t0), keccak256("baseline-ok"), 1, t0 + 1 hours);

        vm.expectRevert(
            abi.encodeWithSelector(
                AccountRegistry.AccountNotPendingBaseline.selector, accountId, AccountRegistry.AccountState.Active
            )
        );
        _activateAccount(accountId, uint64(t0), keccak256("baseline-ok-again"), 2, t0 + 1 hours);
    }

    function test_TwoAccounts_MembershipEpochsChainRoots() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 accountA = _registerAccount(0, deadline);
        _activateAccount(accountA, uint64(block.timestamp), keccak256("a"), 1, deadline);

        bytes32 structHash = keccak256(
            abi.encode(
                REGISTER_ACCOUNT_TYPEHASH, trackId, VENUE_ID, keccak256("uid-b"), uint8(0), uint64(0), uint64(2), deadline
            )
        );
        bytes memory sig = _sign(ownerKey, _accountDigest(structHash));
        bytes32 accountB = registry.registerAccount(
            trackId, VENUE_ID, keccak256("uid-b"), AccountRegistry.Environment.Production, 0, 2, deadline, sig
        );
        _activateAccount(accountB, uint64(block.timestamp), keccak256("b"), 3, deadline);

        assertEq(registry.membershipEpochCount(trackId), 2);
        (bytes32[] memory epoch0Ids, bytes32 epoch0Root,,,) = registry.membershipEpochAt(trackId, 0);
        (bytes32[] memory epoch1Ids, bytes32 epoch1Root, bytes32 epoch1PreviousRoot,,) =
            registry.membershipEpochAt(trackId, 1);

        assertEq(epoch0Ids.length, 1);
        assertEq(epoch1Ids.length, 2);
        assertEq(epoch1PreviousRoot, epoch0Root, "epochs chain to the previous root");
        assertTrue(epoch1Root != epoch0Root);

        bytes32[] memory members = registry.currentMembers(trackId);
        assertEq(members.length, 2);
        assertEq(members[0], accountA);
        assertEq(members[1], accountB);
    }

    // --- removal preserves history -----------------------------------------

    function test_RemoveAccount_PreservesBindingRecordAndHistory() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 accountId = _registerAccount(0, deadline);
        _activateAccount(accountId, uint64(block.timestamp), keccak256("ok"), 1, deadline);

        _removeAccount(accountId, keccak256("owner ended tracking"), 2, deadline);

        (
            bytes32 storedTrackId,
            bytes32 venueId,
            ,
            ,
            uint64 registeredAt,
            uint64 eligibleFrom,
            uint64 removedAt,
            ,
            AccountRegistry.AccountState state,
            bool exists
        ) = registry.accounts(accountId);
        assertTrue(exists, "removal must not delete the binding record");
        assertEq(storedTrackId, trackId);
        assertEq(venueId, VENUE_ID);
        assertTrue(registeredAt > 0);
        assertTrue(eligibleFrom > 0, "the prior eligibleFrom period stays on record");
        assertEq(removedAt, block.timestamp);
        assertTrue(state == AccountRegistry.AccountState.Removed);

        // Membership epoch history is untouched: epoch 0 (with the account)
        // still shows it as a member; only a new epoch 1 excludes it.
        (bytes32[] memory epoch0Ids,,,,) = registry.membershipEpochAt(trackId, 0);
        assertEq(epoch0Ids.length, 1);
        assertEq(epoch0Ids[0], accountId);

        assertEq(registry.membershipEpochCount(trackId), 2);
        bytes32[] memory currentMembers = registry.currentMembers(trackId);
        assertEq(currentMembers.length, 0);
    }

    function test_RemoveAccount_BeforeActivation_DoesNotOpenMembershipEpoch() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 accountId = _registerAccount(0, deadline);
        // never activated
        _removeAccount(accountId, keccak256("changed mind"), 1, deadline);

        (,,,,,,,, AccountRegistry.AccountState state,) = registry.accounts(accountId);
        assertTrue(state == AccountRegistry.AccountState.Removed);
        assertEq(registry.membershipEpochCount(trackId), 0, "never having joined, removal opens no epoch");
    }

    function test_RemoveAccount_RevertsWhenAlreadyRemoved() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 accountId = _registerAccount(0, deadline);
        _removeAccount(accountId, keccak256("first"), 1, deadline);

        vm.expectRevert(abi.encodeWithSelector(AccountRegistry.AccountAlreadyRemoved.selector, accountId));
        _removeAccount(accountId, keccak256("second"), 2, deadline);
    }

    // --- old owner revoked, replay rejected --------------------------------

    function test_OldIdentityOwnerCannotAuthorizeAccountActionsAfterRotation() public {
        uint256 deadline = block.timestamp + 1 hours;
        uint256 newOwnerKey = 0xB0B;
        address newOwner = vm.addr(newOwnerKey);

        bytes32 proposeStruct = keccak256(
            abi.encode(PROPOSE_OWNER_ROTATION_TYPEHASH, identityId, newOwner, uint64(0), uint64(1), deadline)
        );
        identityRegistry.proposeOwnerRotation(identityId, newOwner, 0, 1, deadline, _sign(ownerKey, _identityDigest(proposeStruct)));
        bytes32 confirmStruct = keccak256(
            abi.encode(CONFIRM_OWNER_ROTATION_TYPEHASH, identityId, newOwner, uint64(0), uint64(2), deadline)
        );
        identityRegistry.confirmOwnerRotation(
            identityId, newOwner, 0, 2, deadline, _sign(newOwnerKey, _identityDigest(confirmStruct))
        );

        // Old owner signs a registerAccount command with ownerEpoch=0,
        // still believing it is current: the identity's ownerEpoch is now
        // 1, so this must fail, and the old owner is no longer even the
        // registered signer.
        vm.expectRevert(abi.encodeWithSelector(AccountRegistry.OwnerEpochMismatch.selector, 1, 0));
        _registerAccount(0, deadline);
    }

    function test_RegisterAccount_RevertsOnReplay() public {
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 structHash = keccak256(
            abi.encode(
                REGISTER_ACCOUNT_TYPEHASH, trackId, VENUE_ID, keccak256("uid"), uint8(0), uint64(0), uint64(0), deadline
            )
        );
        bytes memory sig = _sign(ownerKey, _accountDigest(structHash));
        registry.registerAccount(trackId, VENUE_ID, keccak256("uid"), AccountRegistry.Environment.Production, 0, 0, deadline, sig);

        vm.expectRevert(abi.encodeWithSelector(AccountRegistry.NonceMismatch.selector, 1, 0));
        registry.registerAccount(trackId, VENUE_ID, keccak256("uid"), AccountRegistry.Environment.Production, 0, 0, deadline, sig);
    }

    // --- fuzz ---------------------------------------------------------

    function testFuzz_RegisterAccount_RevertsOnAnyWrongNonce(uint64 wrongNonce) public {
        vm.assume(wrongNonce != 0);
        uint256 deadline = block.timestamp + 1 hours;
        bytes32 structHash = keccak256(
            abi.encode(
                REGISTER_ACCOUNT_TYPEHASH, trackId, VENUE_ID, keccak256("uid"), uint8(0), uint64(0), wrongNonce, deadline
            )
        );
        bytes memory sig = _sign(ownerKey, _accountDigest(structHash));
        vm.expectRevert(abi.encodeWithSelector(AccountRegistry.NonceMismatch.selector, 0, wrongNonce));
        registry.registerAccount(
            trackId, VENUE_ID, keccak256("uid"), AccountRegistry.Environment.Production, 0, wrongNonce, deadline, sig
        );
    }
}

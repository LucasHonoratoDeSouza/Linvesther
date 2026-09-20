// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {PolicyRegistry} from "../src/PolicyRegistry.sol";

contract PolicyRegistryTest is Test {
    PolicyRegistry internal registry;

    address internal governorA = address(0xA1);
    address internal governorB = address(0xB2);
    address internal governorC = address(0xC3);
    address internal stranger = address(0xEEEE);

    bytes32 internal constant SOURCE_POLICY = keccak256("sourcePolicy");

    function setUp() public {
        registry = new PolicyRegistry([governorA, governorB, governorC]);
    }

    // --- construction -----------------------------------------------------

    function test_Constructor_RevertsOnDuplicateGovernor() public {
        vm.expectRevert(abi.encodeWithSelector(PolicyRegistry.DuplicateGovernor.selector, governorA));
        new PolicyRegistry([governorA, governorA, governorC]);
    }

    function test_Constructor_RevertsOnZeroAddress() public {
        vm.expectRevert(PolicyRegistry.ZeroAddress.selector);
        new PolicyRegistry([governorA, address(0), governorC]);
    }

    // --- scheduling a new version: immutable + timelocked ------------------

    function test_ProposeVersion_RevertsFromNonGovernor() public {
        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(PolicyRegistry.NotGovernor.selector, stranger));
        registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
    }

    function test_ProposeVersion_DoesNotYetCreateAVersion() public {
        vm.prank(governorA);
        registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));

        (,,,,,,,,,,, bool exists) = registry.versions(SOURCE_POLICY, keccak256("v1"));
        assertFalse(exists, "a single proposal is not yet 2-of-3 consensus");
    }

    function test_SecondApproval_PublishesImmutableVersionWithTimelock() public {
        vm.prank(governorA);
        bytes32 actionId = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));

        uint256 approvalTime = block.timestamp;
        vm.prank(governorB);
        registry.approveVersion(actionId);

        (
            bytes32 policyKind,
            bytes32 policyHash,
            bytes32 metadataHash,
            address proposer,
            uint64 publishedAt,
            uint64 effectiveFrom,
            PolicyRegistry.VersionStatus status,
            ,
            ,
            ,
            ,
            bool exists
        ) = registry.versions(SOURCE_POLICY, keccak256("v1"));

        assertTrue(exists);
        assertEq(policyKind, SOURCE_POLICY);
        assertEq(policyHash, keccak256("v1"));
        assertEq(metadataHash, keccak256("meta-v1"));
        assertEq(proposer, governorA);
        assertEq(publishedAt, approvalTime, "publishedAt records the original proposal time");
        assertEq(effectiveFrom, approvalTime + registry.TIMELOCK_DELAY());
        assertTrue(status == PolicyRegistry.VersionStatus.Scheduled);
    }

    function test_IsEffective_FalseBeforeTimelockElapses() public {
        vm.prank(governorA);
        bytes32 actionId = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(actionId);

        assertFalse(registry.isEffective(SOURCE_POLICY, keccak256("v1")));

        vm.warp(block.timestamp + registry.TIMELOCK_DELAY() - 1);
        assertFalse(registry.isEffective(SOURCE_POLICY, keccak256("v1")), "one second before the timelock, still not effective");
    }

    function test_IsEffective_TrueAfterTimelockElapses() public {
        vm.prank(governorA);
        bytes32 actionId = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(actionId);

        vm.warp(block.timestamp + registry.TIMELOCK_DELAY());
        assertTrue(registry.isEffective(SOURCE_POLICY, keccak256("v1")));
    }

    function test_ApproveVersion_RevertsOnDoubleApprovalBySameGovernor() public {
        vm.prank(governorA);
        bytes32 actionId = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));

        vm.prank(governorA);
        vm.expectRevert(abi.encodeWithSelector(PolicyRegistry.AlreadyApproved.selector, actionId, governorA));
        registry.approveVersion(actionId);
    }

    function test_ApproveVersion_RevertsAfterAlreadyExecuted() public {
        vm.prank(governorA);
        bytes32 actionId = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(actionId);

        vm.prank(governorC);
        vm.expectRevert(abi.encodeWithSelector(PolicyRegistry.ActionAlreadyExecuted.selector, actionId));
        registry.approveVersion(actionId);
    }

    function test_ProposeVersion_RevertsWhenVersionAlreadyExists() public {
        vm.prank(governorA);
        bytes32 actionId = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(actionId);

        vm.prank(governorC);
        vm.expectRevert(abi.encodeWithSelector(PolicyRegistry.PolicyVersionAlreadyExists.selector, SOURCE_POLICY, keccak256("v1")));
        registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1-again"));
    }

    function test_VersionHistory_PreservesAllPublishedVersions() public {
        vm.prank(governorA);
        bytes32 action1 = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(action1);

        vm.prank(governorB);
        bytes32 action2 = registry.proposeVersion(SOURCE_POLICY, keccak256("v2"), keccak256("meta-v2"));
        vm.prank(governorC);
        registry.approveVersion(action2);

        assertEq(registry.versionHistoryLength(SOURCE_POLICY), 2);
        assertEq(registry.versionHistoryAt(SOURCE_POLICY, 0), keccak256("v1"));
        assertEq(registry.versionHistoryAt(SOURCE_POLICY, 1), keccak256("v2"));

        // v1 is untouched by v2's publication: still there, still Scheduled.
        (,,,,,, PolicyRegistry.VersionStatus statusV1,,,,, bool existsV1) = registry.versions(SOURCE_POLICY, keccak256("v1"));
        assertTrue(existsV1);
        assertTrue(statusV1 == PolicyRegistry.VersionStatus.Scheduled);
    }

    // --- invalidation: immediate, never deletes -----------------------------

    function test_ProposeInvalidation_RevertsForUnknownVersion() public {
        vm.prank(governorA);
        vm.expectRevert(abi.encodeWithSelector(PolicyRegistry.PolicyVersionNotFound.selector, SOURCE_POLICY, keccak256("nope")));
        registry.proposeInvalidation(SOURCE_POLICY, keccak256("nope"), keccak256("reason"), 0, 0);
    }

    function test_Invalidation_TakesEffectImmediatelyNoTimelock() public {
        vm.prank(governorA);
        bytes32 publishAction = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(publishAction);

        vm.prank(governorA);
        bytes32 invalidateAction =
            registry.proposeInvalidation(SOURCE_POLICY, keccak256("v1"), keccak256("compromised"), 100, 200);
        vm.prank(governorC);
        registry.approveInvalidation(invalidateAction);

        (
            ,
            ,
            ,
            ,
            uint64 publishedAt,
            uint64 effectiveFrom,
            PolicyRegistry.VersionStatus status,
            bytes32 invalidationReasonHash,
            uint64 invalidatedAt,
            uint64 exposureStart,
            uint64 exposureEnd,
            bool exists
        ) = registry.versions(SOURCE_POLICY, keccak256("v1"));

        assertTrue(exists, "invalidation never deletes the record");
        assertTrue(status == PolicyRegistry.VersionStatus.Invalidated);
        assertEq(invalidationReasonHash, keccak256("compromised"));
        assertEq(invalidatedAt, block.timestamp);
        assertEq(exposureStart, 100);
        assertEq(exposureEnd, 200);
        // Original publish fields are preserved exactly, not overwritten.
        assertTrue(publishedAt > 0);
        assertTrue(effectiveFrom > 0);

        assertFalse(registry.isEffective(SOURCE_POLICY, keccak256("v1")), "an invalidated version is never effective");
    }

    function test_Invalidation_DoesNotRequireTimelockWait() public {
        vm.prank(governorA);
        bytes32 publishAction = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(publishAction);
        // Note: no vm.warp — invalidation happens in the same block as
        // publication, well before the 72h timelock would elapse.

        vm.prank(governorA);
        bytes32 invalidateAction = registry.proposeInvalidation(SOURCE_POLICY, keccak256("v1"), keccak256("bug"), 0, 0);
        vm.prank(governorC);
        registry.approveInvalidation(invalidateAction);

        (,,,,,, PolicyRegistry.VersionStatus status,,,,,) = registry.versions(SOURCE_POLICY, keccak256("v1"));
        assertTrue(status == PolicyRegistry.VersionStatus.Invalidated);
    }

    function test_ProposeInvalidation_RevertsWhenAlreadyInvalidated() public {
        vm.prank(governorA);
        bytes32 publishAction = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(publishAction);

        vm.prank(governorA);
        bytes32 invalidateAction1 = registry.proposeInvalidation(SOURCE_POLICY, keccak256("v1"), keccak256("first"), 0, 0);
        vm.prank(governorC);
        registry.approveInvalidation(invalidateAction1);

        vm.prank(governorB);
        vm.expectRevert(
            abi.encodeWithSelector(PolicyRegistry.PolicyVersionAlreadyInvalidated.selector, SOURCE_POLICY, keccak256("v1"))
        );
        registry.proposeInvalidation(SOURCE_POLICY, keccak256("v1"), keccak256("second"), 0, 0);
    }

    function test_ApproveVersion_RevertsOnWrongActionKind() public {
        vm.prank(governorA);
        bytes32 publishAction = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(publishAction);

        vm.prank(governorA);
        bytes32 invalidateAction = registry.proposeInvalidation(SOURCE_POLICY, keccak256("v1"), keccak256("bug"), 0, 0);

        // Calling approveVersion on an invalidation action must fail: the
        // two approval flows are not interchangeable.
        vm.prank(governorC);
        vm.expectRevert(
            abi.encodeWithSelector(
                PolicyRegistry.WrongActionKind.selector, PolicyRegistry.ActionKind.ScheduleVersion, PolicyRegistry.ActionKind.InvalidateVersion
            )
        );
        registry.approveVersion(invalidateAction);
    }

    // --- fuzz ---------------------------------------------------------

    function testFuzz_IsEffective_MatchesTimelockBoundaryExactly(uint32 warpSeconds) public {
        vm.prank(governorA);
        bytes32 actionId = registry.proposeVersion(SOURCE_POLICY, keccak256("v1"), keccak256("meta-v1"));
        vm.prank(governorB);
        registry.approveVersion(actionId);

        uint256 t0 = block.timestamp;
        vm.warp(t0 + warpSeconds);
        bool expected = warpSeconds >= registry.TIMELOCK_DELAY();
        assertEq(registry.isEffective(SOURCE_POLICY, keccak256("v1")), expected);
    }
}

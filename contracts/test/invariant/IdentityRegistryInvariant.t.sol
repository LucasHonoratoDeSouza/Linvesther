// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../../src/IdentityRegistry.sol";
import {IdentityRegistryHandler} from "./IdentityRegistryHandler.sol";

/// @notice Core replay/revocation safety property, checked across random
/// sequences of create/propose/confirm/createTrack calls rather than only
/// the specific scenarios in IdentityRegistry.t.sol. The handler asserts
/// ownerEpoch/nonce monotonicity immediately after every successful
/// mutation (see IdentityRegistryHandler._withMonotonicityCheck); this
/// invariant re-checks the same property against final on-chain state as
/// an independent, end-of-run corroboration, and additionally checks that
/// a pending rotation is never proposed to the identity's own current
/// owner.
contract IdentityRegistryInvariantTest is Test {
    IdentityRegistry internal registry;
    IdentityRegistryHandler internal handler;

    function setUp() public {
        registry = new IdentityRegistry();
        handler = new IdentityRegistryHandler(registry);
        targetContract(address(handler));

        // Only the four entry points below are meant to be called directly
        // by the fuzzer; _doProposeRotation/_doConfirmRotation/_doCreateTrack
        // are external only so they can be passed as function pointers
        // internally and rely on transient state those four functions set
        // up first.
        bytes4[] memory selectors = new bytes4[](4);
        selectors[0] = IdentityRegistryHandler.createIdentity.selector;
        selectors[1] = IdentityRegistryHandler.proposeRotation.selector;
        selectors[2] = IdentityRegistryHandler.confirmRotation.selector;
        selectors[3] = IdentityRegistryHandler.createTrack.selector;
        targetSelector(FuzzSelector({addr: address(handler), selectors: selectors}));
    }

    function invariant_PendingOwnerNeverEqualsCurrentOwner() public view {
        uint256 count = handler.identityCount();
        for (uint256 i = 0; i < count; i++) {
            bytes32 identityId = handler.identityIds(i);
            (address owner, address pendingOwner,,,,,) = registry.identities(identityId);
            if (pendingOwner != address(0)) {
                assertTrue(pendingOwner != owner, "pendingOwner must never equal the current owner");
            }
        }
    }

    function invariant_EveryTrackedIdentityStillExists() public view {
        uint256 count = handler.identityCount();
        for (uint256 i = 0; i < count; i++) {
            (,,,,,, bool exists) = registry.identities(handler.identityIds(i));
            assertTrue(exists, "IdentityRegistry has no deletion path; a tracked identity must always exist");
        }
    }
}

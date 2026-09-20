// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../../src/IdentityRegistry.sol";

/// @notice Drives IdentityRegistry through randomized sequences of
/// correctly-signed calls (the fuzzer cannot forge EIP-712 signatures on
/// its own, so this handler signs with a small fixed set of known keys on
/// the fuzzer's behalf) for the invariant suite in
/// IdentityRegistryInvariant.t.sol. Precondition failures (e.g.
/// confirming a rotation that was never proposed) are skipped rather than
/// reverting the whole run, matching the standard Foundry handler pattern.
///
/// Monotonicity of `ownerEpoch`/`nonce` is asserted here, immediately
/// after each successful mutation, by comparing against the value
/// observed just before that same call — not merely against a running
/// maximum, which would trivially hold and prove nothing.
contract IdentityRegistryHandler is Test {
    IdentityRegistry public immutable registry;

    bytes32 private constant CREATE_TRACK_TYPEHASH = keccak256(
        "CreateTrack(bytes32 identityId,bytes32 financialProfileId,bytes32 denominationCommitment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 private constant PROPOSE_OWNER_ROTATION_TYPEHASH = keccak256(
        "ProposeOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 private constant CONFIRM_OWNER_ROTATION_TYPEHASH = keccak256(
        "ConfirmOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    uint256 internal constant KEY_COUNT = 4;
    bytes32[] public identityIds;

    constructor(IdentityRegistry registry_) {
        registry = registry_;
    }

    function _keyFor(uint256 seed) internal pure returns (uint256) {
        return 0x1000 + (seed % KEY_COUNT);
    }

    function _keyForAddress(address addr) internal pure returns (uint256) {
        for (uint256 i = 0; i < KEY_COUNT; i++) {
            uint256 key = 0x1000 + i;
            if (vm.addr(key) == addr) return key;
        }
        return 0;
    }

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

    function _sign(uint256 key, bytes32 structHash) internal view returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, _digest(structHash));
        return abi.encodePacked(r, s, v);
    }

    /// @dev Snapshots (epoch, nonce) before a mutation and asserts neither
    /// decreased after it, wrapping `mutate`. `mutate` returns `true` if it
    /// actually performed the registry call (vs. skipping on a
    /// precondition it couldn't satisfy).
    function _withMonotonicityCheck(bytes32 identityId, function() external returns (bool) mutate) internal {
        (,,, uint64 epochBefore, uint64 nonceBefore,,) = registry.identities(identityId);
        bool performed = mutate();
        if (!performed) return;
        (,,, uint64 epochAfter, uint64 nonceAfter,,) = registry.identities(identityId);
        assertGe(epochAfter, epochBefore, "ownerEpoch must never decrease");
        assertGe(nonceAfter, nonceBefore, "nonce must never decrease");
    }

    function createIdentity(uint256 ownerSeed) external {
        address owner = vm.addr(_keyFor(ownerSeed));
        vm.prank(owner);
        bytes32 identityId = registry.createIdentity(IdentityRegistry.IdentityType.Person);
        identityIds.push(identityId);
    }

    function _randomIdentity(uint256 seed) internal view returns (bytes32, bool) {
        if (identityIds.length == 0) return (bytes32(0), false);
        return (identityIds[seed % identityIds.length], true);
    }

    bytes32 private _txIdentityId;
    address private _txNewOwner;
    uint64 private _txEpoch;
    uint64 private _txNonce;
    uint256 private _txDeadline;
    uint256 private _txSignerKey;

    function proposeRotation(uint256 identitySeed, uint256 newOwnerSeed, uint256 deadlineOffset) external {
        (bytes32 identityId, bool ok) = _randomIdentity(identitySeed);
        if (!ok) return;
        (address owner,,, uint64 epoch, uint64 nonce,,) = registry.identities(identityId);
        uint256 ownerKey = _keyForAddress(owner);
        if (ownerKey == 0) return;
        address newOwner = vm.addr(_keyFor(newOwnerSeed));
        if (newOwner == owner) return;

        _txIdentityId = identityId;
        _txNewOwner = newOwner;
        _txEpoch = epoch;
        _txNonce = nonce;
        _txDeadline = block.timestamp + bound(deadlineOffset, 1, 30 days);
        _txSignerKey = ownerKey;
        _withMonotonicityCheck(identityId, this._doProposeRotation);
    }

    function _doProposeRotation() external returns (bool) {
        bytes32 structHash = keccak256(
            abi.encode(PROPOSE_OWNER_ROTATION_TYPEHASH, _txIdentityId, _txNewOwner, _txEpoch, _txNonce, _txDeadline)
        );
        try registry.proposeOwnerRotation(
            _txIdentityId, _txNewOwner, _txEpoch, _txNonce, _txDeadline, _sign(_txSignerKey, structHash)
        ) {
            return true;
        } catch {
            return false;
        }
    }

    function confirmRotation(uint256 identitySeed, uint256 deadlineOffset) external {
        (bytes32 identityId, bool ok) = _randomIdentity(identitySeed);
        if (!ok) return;
        (, address pendingOwner,, uint64 epoch, uint64 nonce,,) = registry.identities(identityId);
        if (pendingOwner == address(0)) return;
        uint256 pendingKey = _keyForAddress(pendingOwner);
        if (pendingKey == 0) return;

        _txIdentityId = identityId;
        _txNewOwner = pendingOwner;
        _txEpoch = epoch;
        _txNonce = nonce;
        _txDeadline = block.timestamp + bound(deadlineOffset, 1, 30 days);
        _txSignerKey = pendingKey;
        _withMonotonicityCheck(identityId, this._doConfirmRotation);
    }

    function _doConfirmRotation() external returns (bool) {
        bytes32 structHash = keccak256(
            abi.encode(CONFIRM_OWNER_ROTATION_TYPEHASH, _txIdentityId, _txNewOwner, _txEpoch, _txNonce, _txDeadline)
        );
        try registry.confirmOwnerRotation(
            _txIdentityId, _txNewOwner, _txEpoch, _txNonce, _txDeadline, _sign(_txSignerKey, structHash)
        ) {
            return true;
        } catch {
            return false;
        }
    }

    function createTrack(uint256 identitySeed, uint256 deadlineOffset) external {
        (bytes32 identityId, bool ok) = _randomIdentity(identitySeed);
        if (!ok) return;
        (address owner,,, uint64 epoch, uint64 nonce,,) = registry.identities(identityId);
        uint256 ownerKey = _keyForAddress(owner);
        if (ownerKey == 0) return;

        _txIdentityId = identityId;
        _txEpoch = epoch;
        _txNonce = nonce;
        _txDeadline = block.timestamp + bound(deadlineOffset, 1, 30 days);
        _txSignerKey = ownerKey;
        _withMonotonicityCheck(identityId, this._doCreateTrack);
    }

    function _doCreateTrack() external returns (bool) {
        bytes32 financialProfileId = keccak256("spot-nav-twr-v1");
        bytes32 denomination = keccak256("USDT");
        bytes32 structHash = keccak256(
            abi.encode(CREATE_TRACK_TYPEHASH, _txIdentityId, financialProfileId, denomination, _txEpoch, _txNonce, _txDeadline)
        );
        try registry.createTrack(
            _txIdentityId, financialProfileId, denomination, _txEpoch, _txNonce, _txDeadline, _sign(_txSignerKey, structHash)
        ) {
            return true;
        } catch {
            return false;
        }
    }

    function identityCount() external view returns (uint256) {
        return identityIds.length;
    }
}

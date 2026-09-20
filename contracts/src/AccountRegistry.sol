// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {SignatureChecker} from "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";
import {IdentityRegistry} from "./IdentityRegistry.sol";

/// @notice Registers `AccountBinding`s and `MembershipEpoch`s per
/// the protocol specification, and their prospective-only lifecycle per
/// the EVM adapter specification.
///
/// Every state-changing call is authorized by an EIP-712 command signed by
/// the current owner of the identity that owns the account's track (read
/// live from `IdentityRegistry`, including its current `ownerEpoch` — an
/// owner revoked by a rotation there is rejected here too, without this
/// contract needing to mirror that state), verified the same way as
/// `IdentityRegistry`: `SignatureChecker.isValidSignatureNow`, covering
/// both EOA and ERC-1271 contract-wallet owners. Replay protection uses a
/// nonce scoped per track (all account actions on a track share one
/// sequence), separate from `IdentityRegistry`'s own per-identity nonce —
/// each registry has its own EIP-712 domain and nonce space by design.
///
/// `changeMembership` from the EVM adapter specification's contract table is
/// implemented as the shared internal mechanism `activateAccount` and
/// `removeAccount` both use to open a new `MembershipEpoch`, not as a
/// separate public entry point: P1 only ever changes membership by
/// activating or removing one account at a time, and a generic
/// caller-supplied bulk-set entry point would need its own invariant
/// review (e.g. against skipping straight to a set that never went
/// through individual admissibility checks) that nothing here currently
/// requires. Revisit if a real bulk-membership use case appears.
///
/// `accountSetRoot` is never accepted from the caller — "não root
/// arbitrária fornecida pelo owner" — it is always
/// `keccak256(abi.encode(accountIds))` computed on-chain from the
/// contract's own stored membership array, so it cannot be forged to
/// claim a set the contract does not actually hold.
contract AccountRegistry is EIP712 {
    enum AccountState {
        PendingBaseline,
        Active,
        Removed
    }

    enum Environment {
        Production,
        Test
    }

    struct AccountBinding {
        bytes32 trackId;
        bytes32 venueId;
        bytes32 authenticatedIdCommitment;
        Environment environment;
        uint64 registeredAt;
        uint64 eligibleFrom;
        uint64 removedAt;
        uint64 bindingEpoch;
        AccountState state;
        bool exists;
    }

    struct MembershipEpoch {
        bytes32[] accountIds;
        bytes32 accountSetRoot;
        bytes32 previousRoot;
        uint64 effectiveFrom;
        bytes32 reasonHash;
    }

    bytes32 private constant REGISTER_ACCOUNT_TYPEHASH = keccak256(
        "RegisterAccount(bytes32 trackId,bytes32 venueId,bytes32 authenticatedIdCommitment,uint8 environment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 private constant ACTIVATE_ACCOUNT_TYPEHASH = keccak256(
        "ActivateAccount(bytes32 accountId,uint64 eligibleFrom,bytes32 reasonHash,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 private constant REMOVE_ACCOUNT_TYPEHASH = keccak256(
        "RemoveAccount(bytes32 accountId,bytes32 reasonHash,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 private constant ROTATE_BINDING_CREDENTIAL_TYPEHASH = keccak256(
        "RotateBindingCredential(bytes32 accountId,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    IdentityRegistry public immutable identityRegistry;

    mapping(bytes32 accountId => AccountBinding) public accounts;
    mapping(bytes32 trackId => MembershipEpoch[]) private _membershipEpochs;
    mapping(bytes32 trackId => uint64) public trackNonce;

    uint256 private _accountCounter;

    event AccountRegistered(
        bytes32 indexed trackId, bytes32 indexed accountId, bytes32 venueId, Environment environment, uint64 registeredAt
    );
    event AccountActivated(bytes32 indexed trackId, bytes32 indexed accountId, uint64 eligibleFrom, bytes32 reasonHash);
    event AccountRemoved(bytes32 indexed trackId, bytes32 indexed accountId, uint64 removedAt, bytes32 reasonHash);
    event BindingCredentialRotated(bytes32 indexed accountId, uint64 newBindingEpoch);
    event MembershipChanged(
        bytes32 indexed trackId, uint64 indexed epochIndex, bytes32 accountSetRoot, bytes32 previousRoot, uint64 effectiveFrom
    );

    error TrackNotFound(bytes32 trackId);
    error AccountNotFound(bytes32 accountId);
    error AccountNotPendingBaseline(bytes32 accountId, AccountState actual);
    error AccountAlreadyRemoved(bytes32 accountId);
    error InvalidEligibleFrom(uint64 registeredAt, uint64 eligibleFrom);
    error InvalidSignature();
    error CommandExpired(uint256 deadline, uint256 blockTimestamp);
    error OwnerEpochMismatch(uint64 expected, uint64 provided);
    error NonceMismatch(uint64 expected, uint64 provided);

    constructor(IdentityRegistry identityRegistry_) EIP712("LinvestherZK-AccountRegistry", "1") {
        identityRegistry = identityRegistry_;
    }

    // --- lifecycle ------------------------------------------------------

    function registerAccount(
        bytes32 trackId,
        bytes32 venueId,
        bytes32 authenticatedIdCommitment,
        Environment environment,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external returns (bytes32 accountId) {
        address owner = _requireTrackOwner(trackId);

        bytes32 structHash = keccak256(
            abi.encode(
                REGISTER_ACCOUNT_TYPEHASH, trackId, venueId, authenticatedIdCommitment, environment, ownerEpoch, nonce, deadline
            )
        );
        _consumeCommand(trackId, owner, structHash, ownerEpoch, nonce, deadline, signature);

        // forge-lint: disable-next-line(unsafe-typecast)
        // block.timestamp always fits in uint64 (see IdentityRegistry).
        uint64 registeredAt = uint64(block.timestamp);
        accountId = keccak256(abi.encodePacked(block.chainid, address(this), trackId, _accountCounter++));
        accounts[accountId] = AccountBinding({
            trackId: trackId,
            venueId: venueId,
            authenticatedIdCommitment: authenticatedIdCommitment,
            environment: environment,
            registeredAt: registeredAt,
            eligibleFrom: 0,
            removedAt: 0,
            bindingEpoch: 0,
            state: AccountState.PendingBaseline,
            exists: true
        });
        emit AccountRegistered(trackId, accountId, venueId, environment, registeredAt);
    }

    /// @notice Admits `accountId` into the track's membership set,
    /// starting at `eligibleFrom`. Origin admissibility itself cannot be
    /// verified on-chain (this contract has no knowledge of Binance or any
    /// other venue's state); it is attested off-chain and this call only
    /// anchors the owner-authorized result.
    function activateAccount(
        bytes32 accountId,
        uint64 eligibleFrom,
        bytes32 reasonHash,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        AccountBinding storage account = _requireAccount(accountId);
        if (account.state != AccountState.PendingBaseline) {
            revert AccountNotPendingBaseline(accountId, account.state);
        }
        if (eligibleFrom < account.registeredAt) {
            revert InvalidEligibleFrom(account.registeredAt, eligibleFrom);
        }
        address owner = _requireTrackOwner(account.trackId);

        bytes32 structHash = keccak256(
            abi.encode(ACTIVATE_ACCOUNT_TYPEHASH, accountId, eligibleFrom, reasonHash, ownerEpoch, nonce, deadline)
        );
        _consumeCommand(account.trackId, owner, structHash, ownerEpoch, nonce, deadline, signature);

        account.state = AccountState.Active;
        account.eligibleFrom = eligibleFrom;
        emit AccountActivated(account.trackId, accountId, eligibleFrom, reasonHash);

        bytes32[] memory members = _currentMembers(account.trackId);
        bytes32[] memory nextMembers = new bytes32[](members.length + 1);
        for (uint256 i = 0; i < members.length; i++) {
            nextMembers[i] = members[i];
        }
        nextMembers[members.length] = accountId;
        _openMembershipEpoch(account.trackId, nextMembers, reasonHash);
    }

    /// @notice Removes `accountId`. The binding record is never deleted —
    /// only its state changes to `Removed` — so the account, its periods
    /// and the removal reason stay publicly queryable forever.
    function removeAccount(
        bytes32 accountId,
        bytes32 reasonHash,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        AccountBinding storage account = _requireAccount(accountId);
        if (account.state == AccountState.Removed) revert AccountAlreadyRemoved(accountId);
        address owner = _requireTrackOwner(account.trackId);

        bytes32 structHash =
            keccak256(abi.encode(REMOVE_ACCOUNT_TYPEHASH, accountId, reasonHash, ownerEpoch, nonce, deadline));
        _consumeCommand(account.trackId, owner, structHash, ownerEpoch, nonce, deadline, signature);

        bool wasActive = account.state == AccountState.Active;
        account.state = AccountState.Removed;
        // forge-lint: disable-next-line(unsafe-typecast)
        account.removedAt = uint64(block.timestamp);
        emit AccountRemoved(account.trackId, accountId, account.removedAt, reasonHash);

        if (wasActive) {
            bytes32[] memory members = _currentMembers(account.trackId);
            bytes32[] memory nextMembers = new bytes32[](members.length - 1);
            uint256 writeIndex = 0;
            for (uint256 i = 0; i < members.length; i++) {
                if (members[i] != accountId) {
                    nextMembers[writeIndex++] = members[i];
                }
            }
            _openMembershipEpoch(account.trackId, nextMembers, reasonHash);
        }
    }

    /// @notice Records that the account's underlying authenticated
    /// credential (e.g. an API key) was rotated, without resetting any
    /// history: the Binance adapter specification — "rotação registra
    /// CredentialRotated... sem reset de reputação."
    function rotateBindingCredential(
        bytes32 accountId,
        bytes32 newAuthenticatedIdCommitment,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        AccountBinding storage account = _requireAccount(accountId);
        if (account.state == AccountState.Removed) revert AccountAlreadyRemoved(accountId);
        address owner = _requireTrackOwner(account.trackId);

        bytes32 structHash = keccak256(
            abi.encode(ROTATE_BINDING_CREDENTIAL_TYPEHASH, accountId, ownerEpoch, nonce, deadline)
        );
        _consumeCommand(account.trackId, owner, structHash, ownerEpoch, nonce, deadline, signature);

        account.authenticatedIdCommitment = newAuthenticatedIdCommitment;
        account.bindingEpoch += 1;
        emit BindingCredentialRotated(accountId, account.bindingEpoch);
    }

    // --- membership views -------------------------------------------------

    function membershipEpochCount(bytes32 trackId) external view returns (uint256) {
        return _membershipEpochs[trackId].length;
    }

    function membershipEpochAt(bytes32 trackId, uint256 index)
        external
        view
        returns (bytes32[] memory accountIds, bytes32 accountSetRoot, bytes32 previousRoot, uint64 effectiveFrom, bytes32 reasonHash)
    {
        MembershipEpoch storage epoch = _membershipEpochs[trackId][index];
        return (epoch.accountIds, epoch.accountSetRoot, epoch.previousRoot, epoch.effectiveFrom, epoch.reasonHash);
    }

    function currentMembers(bytes32 trackId) external view returns (bytes32[] memory) {
        return _currentMembers(trackId);
    }

    // --- internals ----------------------------------------------------

    function _currentMembers(bytes32 trackId) private view returns (bytes32[] memory) {
        MembershipEpoch[] storage epochs = _membershipEpochs[trackId];
        if (epochs.length == 0) return new bytes32[](0);
        return epochs[epochs.length - 1].accountIds;
    }

    /// @dev Always appends a new epoch; never mutates a previous one, and
    /// always stamps `effectiveFrom` with the current block time, which
    /// the EVM guarantees is never less than any previous block's
    /// timestamp — membership changes are structurally prospective-only.
    function _openMembershipEpoch(bytes32 trackId, bytes32[] memory newAccountIds, bytes32 reasonHash) private {
        MembershipEpoch[] storage epochs = _membershipEpochs[trackId];
        bytes32 previousRoot = epochs.length == 0 ? bytes32(0) : epochs[epochs.length - 1].accountSetRoot;
        bytes32 root = keccak256(abi.encode(newAccountIds));
        // forge-lint: disable-next-line(unsafe-typecast)
        uint64 effectiveFrom = uint64(block.timestamp);
        epochs.push(
            MembershipEpoch({
                accountIds: newAccountIds,
                accountSetRoot: root,
                previousRoot: previousRoot,
                effectiveFrom: effectiveFrom,
                reasonHash: reasonHash
            })
        );
        emit MembershipChanged(trackId, uint64(epochs.length - 1), root, previousRoot, effectiveFrom);
    }

    function _requireAccount(bytes32 accountId) private view returns (AccountBinding storage account) {
        account = accounts[accountId];
        if (!account.exists) revert AccountNotFound(accountId);
    }

    function _requireTrackOwner(bytes32 trackId) private view returns (address owner) {
        (bytes32 identityId,,,, bool trackExists) = identityRegistry.tracks(trackId);
        if (!trackExists) revert TrackNotFound(trackId);
        (address trackOwner,,,,,,) = identityRegistry.identities(identityId);
        return trackOwner;
    }

    function _requireTrackOwnerEpoch(bytes32 trackId) private view returns (uint64 ownerEpoch) {
        (bytes32 identityId,,,,) = identityRegistry.tracks(trackId);
        (,,, uint64 epoch,,,) = identityRegistry.identities(identityId);
        return epoch;
    }

    /// @dev Validates deadline, the identity owner's current epoch (read
    /// live from IdentityRegistry, so a rotation there revokes signing
    /// power here too), this track's nonce, and the signature, then
    /// consumes the nonce. Reverts on any mismatch.
    function _consumeCommand(
        bytes32 trackId,
        address signer,
        bytes32 structHash,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) private {
        // deadlines here are expected in minutes/hours; see IdentityRegistry
        // for why block.timestamp manipulation tolerance is immaterial.
        // forge-lint: disable-next-line(block-timestamp)
        if (block.timestamp > deadline) revert CommandExpired(deadline, block.timestamp);

        uint64 currentOwnerEpoch = _requireTrackOwnerEpoch(trackId);
        if (ownerEpoch != currentOwnerEpoch) revert OwnerEpochMismatch(currentOwnerEpoch, ownerEpoch);

        uint64 currentNonce = trackNonce[trackId];
        if (nonce != currentNonce) revert NonceMismatch(currentNonce, nonce);

        bytes32 digest = _hashTypedDataV4(structHash);
        if (!SignatureChecker.isValidSignatureNow(signer, digest, signature)) revert InvalidSignature();

        trackNonce[trackId] = currentNonce + 1;
    }
}

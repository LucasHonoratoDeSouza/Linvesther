// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

/// @notice Registers immutable, hash-identified policy versions (source
/// policies, metric policies, price policies, trust manifests, guest
/// image IDs — any `policyKind` the caller declares) per
/// the protocol specification's "Governança de protocolo": "Schemas,
/// fórmulas, source policies, price policies e image IDs são artefatos
/// imutáveis identificados por hash. Nova versão tem autor, changelog,
/// ativação futura... Não existe migração silenciosa de histórico."
///
/// Authorization here is a genuinely different model from
/// IdentityRegistry/AccountRegistry/CheckpointRegistry: those are
/// authorized by a single identity owner's EIP-712 signature (supporting
/// relayer submission and ERC-1271 wallets). Policy governance in the
/// reference is "multisig 2-de-3 com timelock 72h"
/// (the EVM adapter specification) — a fixed set of 3 governors, each
/// submitting their own transaction directly (no EIP-712/relayer layer;
/// a real multisig-of-signers doesn't need a second signature-relay
/// mechanism layered on top of governors who are already each submitting
/// individually). `governors` is fixed at construction; rotating
/// governors is not implemented in this pilot (no task currently requires
/// it) and would need its own careful design (who authorizes a rotation?).
///
/// Two action kinds share one 2-of-3 approval mechanism:
/// - **Schedule a new version** (`proposeVersion`/`approveVersion`): on
///   the 2nd distinct governor approval, the version is created —
///   forever immutable from that point — with `effectiveFrom` set to
///   `now + TIMELOCK_DELAY`. It exists and is queryable immediately, but
///   {isEffective} only returns true once the timelock has elapsed:
///   "ativação futura", not silent/instant activation.
/// - **Invalidate a version** (`proposeInvalidation`/`approveInvalidation`):
///   on the 2nd distinct approval, the existing version's status flips to
///   `Invalidated` immediately (no timelock — "Desativação emergencial...
///   não remove lifecycle ou bloqueia leitura/exportação"). The version
///   record itself is never deleted or edited; only `status`,
///   `invalidationReasonHash`, `exposureStart/End` and `invalidatedAt`
///   are set, once, going forward.
contract PolicyRegistry {
    enum ActionKind {
        ScheduleVersion,
        InvalidateVersion
    }

    enum VersionStatus {
        Scheduled,
        Invalidated
    }

    struct GovernanceAction {
        ActionKind kind;
        bytes32 policyKind;
        bytes32 policyHash;
        bytes32 dataHash; // metadataHash for ScheduleVersion, reasonHash for InvalidateVersion
        uint64 exposureStart; // only meaningful for InvalidateVersion
        uint64 exposureEnd; // only meaningful for InvalidateVersion
        uint64 proposedAt;
        address proposer;
        uint8 approvalCount;
        bool executed;
        bool exists;
    }

    struct PolicyVersion {
        bytes32 policyKind;
        bytes32 policyHash;
        bytes32 metadataHash;
        address proposer;
        uint64 publishedAt;
        uint64 effectiveFrom;
        VersionStatus status;
        bytes32 invalidationReasonHash;
        uint64 invalidatedAt;
        uint64 exposureStart;
        uint64 exposureEnd;
        bool exists;
    }

    uint64 public constant TIMELOCK_DELAY = 72 hours;
    uint8 public constant REQUIRED_APPROVALS = 2;

    address[3] public governors;
    mapping(address => bool) public isGovernor;

    mapping(bytes32 actionId => GovernanceAction) public actions;
    mapping(bytes32 actionId => mapping(address => bool)) public hasApproved;
    uint256 private _actionCounter;

    mapping(bytes32 policyKind => mapping(bytes32 policyHash => PolicyVersion)) public versions;
    mapping(bytes32 policyKind => bytes32[]) private _versionHistory;

    event VersionProposed(bytes32 indexed actionId, bytes32 indexed policyKind, bytes32 policyHash, address proposer);
    event ActionApproved(bytes32 indexed actionId, address approver, uint8 approvalCount);
    event PolicyPublished(
        bytes32 indexed policyKind, bytes32 indexed policyHash, bytes32 metadataHash, uint64 publishedAt, uint64 effectiveFrom
    );
    event InvalidationProposed(bytes32 indexed actionId, bytes32 indexed policyKind, bytes32 policyHash, address proposer);
    event PolicyInvalidated(
        bytes32 indexed policyKind,
        bytes32 indexed policyHash,
        bytes32 reasonHash,
        uint64 exposureStart,
        uint64 exposureEnd,
        uint64 invalidatedAt
    );

    error ZeroAddress();
    error DuplicateGovernor(address governor);
    error NotGovernor(address caller);
    error ActionNotFound(bytes32 actionId);
    error ActionAlreadyExecuted(bytes32 actionId);
    error AlreadyApproved(bytes32 actionId, address governor);
    error WrongActionKind(ActionKind expected, ActionKind actual);
    error PolicyVersionAlreadyExists(bytes32 policyKind, bytes32 policyHash);
    error PolicyVersionNotFound(bytes32 policyKind, bytes32 policyHash);
    error PolicyVersionAlreadyInvalidated(bytes32 policyKind, bytes32 policyHash);

    modifier onlyGovernor() {
        if (!isGovernor[msg.sender]) revert NotGovernor(msg.sender);
        _;
    }

    constructor(address[3] memory governors_) {
        for (uint256 i = 0; i < 3; i++) {
            address governor = governors_[i];
            if (governor == address(0)) revert ZeroAddress();
            if (isGovernor[governor]) revert DuplicateGovernor(governor);
            isGovernor[governor] = true;
            governors[i] = governor;
        }
    }

    // --- schedule a new version -------------------------------------------

    function proposeVersion(bytes32 policyKind, bytes32 policyHash, bytes32 metadataHash)
        external
        onlyGovernor
        returns (bytes32 actionId)
    {
        if (versions[policyKind][policyHash].exists) revert PolicyVersionAlreadyExists(policyKind, policyHash);

        actionId = keccak256(abi.encodePacked(block.chainid, address(this), policyKind, policyHash, _actionCounter++));
        // forge-lint: disable-next-line(unsafe-typecast)
        uint64 proposedAt = uint64(block.timestamp);
        actions[actionId] = GovernanceAction({
            kind: ActionKind.ScheduleVersion,
            policyKind: policyKind,
            policyHash: policyHash,
            dataHash: metadataHash,
            exposureStart: 0,
            exposureEnd: 0,
            proposedAt: proposedAt,
            proposer: msg.sender,
            approvalCount: 1,
            executed: false,
            exists: true
        });
        hasApproved[actionId][msg.sender] = true;
        emit VersionProposed(actionId, policyKind, policyHash, msg.sender);
    }

    function approveVersion(bytes32 actionId) external onlyGovernor {
        GovernanceAction storage action = _requirePendingAction(actionId, ActionKind.ScheduleVersion);
        _approve(actionId, action);

        if (action.approvalCount >= REQUIRED_APPROVALS) {
            action.executed = true;
            // forge-lint: disable-next-line(unsafe-typecast)
            uint64 effectiveFrom = uint64(block.timestamp) + TIMELOCK_DELAY;
            versions[action.policyKind][action.policyHash] = PolicyVersion({
                policyKind: action.policyKind,
                policyHash: action.policyHash,
                metadataHash: action.dataHash,
                proposer: action.proposer,
                publishedAt: action.proposedAt,
                effectiveFrom: effectiveFrom,
                status: VersionStatus.Scheduled,
                invalidationReasonHash: bytes32(0),
                invalidatedAt: 0,
                exposureStart: 0,
                exposureEnd: 0,
                exists: true
            });
            _versionHistory[action.policyKind].push(action.policyHash);
            emit PolicyPublished(action.policyKind, action.policyHash, action.dataHash, action.proposedAt, effectiveFrom);
        }
    }

    // --- invalidate an existing version ------------------------------------

    function proposeInvalidation(bytes32 policyKind, bytes32 policyHash, bytes32 reasonHash, uint64 exposureStart, uint64 exposureEnd)
        external
        onlyGovernor
        returns (bytes32 actionId)
    {
        PolicyVersion storage version = versions[policyKind][policyHash];
        if (!version.exists) revert PolicyVersionNotFound(policyKind, policyHash);
        if (version.status == VersionStatus.Invalidated) revert PolicyVersionAlreadyInvalidated(policyKind, policyHash);

        actionId =
            keccak256(abi.encodePacked(block.chainid, address(this), "invalidate", policyKind, policyHash, _actionCounter++));
        actions[actionId] = GovernanceAction({
            kind: ActionKind.InvalidateVersion,
            policyKind: policyKind,
            policyHash: policyHash,
            dataHash: reasonHash,
            exposureStart: exposureStart,
            exposureEnd: exposureEnd,
            // forge-lint: disable-next-line(unsafe-typecast)
            proposedAt: uint64(block.timestamp),
            proposer: msg.sender,
            approvalCount: 1,
            executed: false,
            exists: true
        });
        hasApproved[actionId][msg.sender] = true;
        emit InvalidationProposed(actionId, policyKind, policyHash, msg.sender);
    }

    function approveInvalidation(bytes32 actionId) external onlyGovernor {
        GovernanceAction storage action = _requirePendingAction(actionId, ActionKind.InvalidateVersion);
        _approve(actionId, action);

        if (action.approvalCount >= REQUIRED_APPROVALS) {
            action.executed = true;
            PolicyVersion storage version = versions[action.policyKind][action.policyHash];
            // Re-check: a second invalidation proposal for the same
            // version could have already executed between this proposal
            // and its 2nd approval.
            if (version.status == VersionStatus.Invalidated) {
                revert PolicyVersionAlreadyInvalidated(action.policyKind, action.policyHash);
            }
            version.status = VersionStatus.Invalidated;
            version.invalidationReasonHash = action.dataHash;
            // forge-lint: disable-next-line(unsafe-typecast)
            version.invalidatedAt = uint64(block.timestamp);
            version.exposureStart = action.exposureStart;
            version.exposureEnd = action.exposureEnd;
            emit PolicyInvalidated(
                action.policyKind, action.policyHash, action.dataHash, action.exposureStart, action.exposureEnd, version.invalidatedAt
            );
        }
    }

    // --- views ----------------------------------------------------------

    /// @notice True only once the timelock has elapsed and the version was
    /// never invalidated. A version that exists but has not yet reached
    /// its `effectiveFrom`, or that was invalidated, is not effective.
    function isEffective(bytes32 policyKind, bytes32 policyHash) external view returns (bool) {
        PolicyVersion storage version = versions[policyKind][policyHash];
        if (!version.exists || version.status == VersionStatus.Invalidated) return false;
        return block.timestamp >= version.effectiveFrom;
    }

    function versionHistoryLength(bytes32 policyKind) external view returns (uint256) {
        return _versionHistory[policyKind].length;
    }

    function versionHistoryAt(bytes32 policyKind, uint256 index) external view returns (bytes32) {
        return _versionHistory[policyKind][index];
    }

    // --- internals ----------------------------------------------------

    function _requirePendingAction(bytes32 actionId, ActionKind expectedKind)
        private
        view
        returns (GovernanceAction storage action)
    {
        action = actions[actionId];
        if (!action.exists) revert ActionNotFound(actionId);
        if (action.executed) revert ActionAlreadyExecuted(actionId);
        if (action.kind != expectedKind) revert WrongActionKind(expectedKind, action.kind);
    }

    function _approve(bytes32 actionId, GovernanceAction storage action) private {
        if (hasApproved[actionId][msg.sender]) revert AlreadyApproved(actionId, msg.sender);
        hasApproved[actionId][msg.sender] = true;
        action.approvalCount += 1;
        emit ActionApproved(actionId, msg.sender, action.approvalCount);
    }
}

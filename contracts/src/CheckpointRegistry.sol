// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {SignatureChecker} from "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";
import {IdentityRegistry} from "./IdentityRegistry.sol";

/// @notice Anchors `CheckpointHeader` records per
/// the protocol specification and enforces the structural
/// invariants from the EVM adapter specification: a single head per track,
/// strictly incrementing sequence, non-overlapping periods, and explicit
/// lateness against the header's own publication deadline. No function
/// deletes a checkpoint or a correction proposal.
///
/// `checkpointHash` (`H("LZK/checkpoint/v1", JCS(header))`) is computed
/// off-chain by the same canonical codec every reader uses
/// (`crates/commitments` / `packages/protocol`) — RFC 8785 JSON
/// canonicalization is not something this contract attempts to
/// reimplement in Solidity. This contract does not re-derive or verify
/// that hash; it validates the structural invariants from the individual
/// header fields supplied alongside it, and anchors the caller-asserted
/// hash as the checkpoint's identifier. An off-chain verifier that
/// distrusts the caller recomputes `checkpointHash` itself from the
/// on-chain-stored fields (all public) and compares.
///
/// Proof verification is out of scope for this pilot contract by design
/// (the EVM adapter specification: "priorizar verificação de receipt fora da
/// cadeia e digest on-chain no piloto"): `attachProof` only records
/// digests, so `proofAnchored` can become true but `proofVerifiedOnChain`
/// never does here — the public interface still exposes both fields so a
/// reader never confuses the two.
///
/// `publishCorrection` only ever registers a `Proposed` correction:
/// "Separar armazenamento público de aceitação... Um contrato que apenas
/// armazena hashes NÃO certifica admissibilidade" — admissibility
/// (Accepted/Rejected/Disputed) is decided by off-chain projections
/// applying the protocol specification's authority rules, not by
/// this contract, so any caller may anchor a proposal (no owner
/// signature required for this one call).
///
/// Session-key delegation for `appendCheckpoint` ("Append por owner ou
/// session key delegada") is not implemented in this pilot: only the
/// track's current identity owner (EOA or ERC-1271) authorizes appends.
/// Adding scoped session keys is a real, separate authorization surface
/// (own scoping/revocation invariants) that nothing in this task requires
/// yet — revisit when a task actually needs it.
contract CheckpointRegistry is EIP712 {
    enum CoverageStatus {
        PolicyComplete,
        Incomplete,
        Unavailable
    }

    enum CheckpointTiming {
        OnTime,
        Late
    }

    enum CorrectionState {
        Proposed
    }

    struct CheckpointInput {
        bytes32 trackId;
        bytes32 checkpointHash;
        uint64 sequence;
        bytes32 previousCheckpointHash;
        uint64 periodStart;
        uint64 periodEnd;
        uint64 membershipEpoch;
        bytes32 accountSetRoot;
        bytes32 sourcePolicyHash;
        bytes32 metricPolicyHash;
        bytes32 pricePolicyHash;
        bytes32 rawEvidenceRoot;
        bytes32 normalizedRoot;
        bytes32 valuationRoot;
        bytes32 previousStateCommitment;
        bytes32 nextStateCommitment;
        CoverageStatus coverageStatus;
        bytes32 coverageManifestHash;
        uint64 publicationDeadline;
        bytes32 sourceAttestationHash;
    }

    struct StoredCheckpoint {
        CheckpointInput input;
        CheckpointTiming timing;
        uint64 anchoredAt;
        bool proofAnchored;
        bool proofVerifiedOnChain;
        bytes32 proofDigest;
        bytes32 claimSetDigest;
        bytes32 correctionHead;
        bool exists;
    }

    struct Correction {
        bytes32 targetCheckpointHash;
        bytes32 replacementDigest;
        bytes32 sourceEvidenceDigest;
        bytes32 reasonCode;
        bytes32 previousCorrectionHead;
        address proposer;
        uint64 proposedAt;
        CorrectionState state;
        bool exists;
    }

    bytes32 private constant CHECKPOINT_INPUT_TYPEHASH = keccak256(
        "CheckpointInput(bytes32 trackId,bytes32 checkpointHash,uint64 sequence,bytes32 previousCheckpointHash,uint64 periodStart,uint64 periodEnd,uint64 membershipEpoch,bytes32 accountSetRoot,bytes32 sourcePolicyHash,bytes32 metricPolicyHash,bytes32 pricePolicyHash,bytes32 rawEvidenceRoot,bytes32 normalizedRoot,bytes32 valuationRoot,bytes32 previousStateCommitment,bytes32 nextStateCommitment,uint8 coverageStatus,bytes32 coverageManifestHash,uint64 publicationDeadline,bytes32 sourceAttestationHash)"
    );
    bytes32 private constant APPEND_CHECKPOINT_TYPEHASH = keccak256(
        "AppendCheckpoint(CheckpointInput input,uint64 ownerEpoch,uint64 nonce,uint256 deadline)CheckpointInput(bytes32 trackId,bytes32 checkpointHash,uint64 sequence,bytes32 previousCheckpointHash,uint64 periodStart,uint64 periodEnd,uint64 membershipEpoch,bytes32 accountSetRoot,bytes32 sourcePolicyHash,bytes32 metricPolicyHash,bytes32 pricePolicyHash,bytes32 rawEvidenceRoot,bytes32 normalizedRoot,bytes32 valuationRoot,bytes32 previousStateCommitment,bytes32 nextStateCommitment,uint8 coverageStatus,bytes32 coverageManifestHash,uint64 publicationDeadline,bytes32 sourceAttestationHash)"
    );
    bytes32 private constant ATTACH_PROOF_TYPEHASH = keccak256(
        "AttachProof(bytes32 checkpointHash,bytes32 proofDigest,bytes32 claimSetDigest,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    IdentityRegistry public immutable identityRegistry;

    // Not `public`: StoredCheckpoint nests CheckpointInput (20 fields), and
    // Solidity's auto-generated getter would flatten every nested scalar
    // field into one long return tuple instead of returning `input` as a
    // struct; `getCheckpoint` below is the explicit, structured accessor.
    mapping(bytes32 checkpointHash => StoredCheckpoint) private _checkpoints;
    mapping(bytes32 trackId => bytes32) public head;
    mapping(bytes32 trackId => uint64) public headSequence;
    mapping(bytes32 trackId => uint64) public headPeriodEnd;
    mapping(bytes32 trackId => uint64) public trackNonce;
    mapping(bytes32 correctionId => Correction) public corrections;
    uint256 private _correctionCounter;

    event CheckpointCommitted(
        bytes32 indexed trackId, bytes32 indexed checkpointHash, uint64 sequence, CheckpointTiming timing, uint64 anchoredAt
    );
    event ProofAttached(bytes32 indexed checkpointHash, bytes32 proofDigest, bytes32 claimSetDigest);
    event CorrectionPublished(
        bytes32 indexed correctionId, bytes32 indexed targetCheckpointHash, bytes32 replacementDigest, bytes32 reasonCode
    );

    error TrackNotFound(bytes32 trackId);
    error CheckpointNotFound(bytes32 checkpointHash);
    error CheckpointAlreadyExists(bytes32 checkpointHash);
    error SequenceMismatch(uint64 expected, uint64 provided);
    error PreviousHashMismatch(bytes32 expected, bytes32 provided);
    error InvalidPeriod(uint64 periodStart, uint64 periodEnd);
    error PeriodOverlapsOrRegresses(uint64 requiredMinimumStart, uint64 providedStart);
    error InvalidSignature();
    error CommandExpired(uint256 deadline, uint256 blockTimestamp);
    error OwnerEpochMismatch(uint64 expected, uint64 provided);
    error NonceMismatch(uint64 expected, uint64 provided);

    constructor(IdentityRegistry identityRegistry_) EIP712("LinvestherZK-CheckpointRegistry", "1") {
        identityRegistry = identityRegistry_;
    }

    /// @notice Appends the next checkpoint for `input.trackId`. Reverts if
    /// `input.sequence` is not exactly the current head's sequence + 1
    /// (genesis: 0), if `input.previousCheckpointHash` does not equal the
    /// current head's hash (genesis: zero), or if the period is inverted
    /// or overlaps/regresses against the previous checkpoint's
    /// `periodEnd` — together these make a second, conflicting head
    /// structurally unrepresentable.
    function appendCheckpoint(CheckpointInput calldata input, uint64 ownerEpoch, uint64 nonce, uint256 deadline, bytes calldata signature)
        external
    {
        if (_checkpoints[input.checkpointHash].exists) revert CheckpointAlreadyExists(input.checkpointHash);
        address owner = _requireTrackOwner(input.trackId);

        // `headSequence[trackId] == 0` is ambiguous between "no checkpoint
        // yet" and "genesis checkpoint (sequence 0) already anchored", so
        // head existence is tracked by whether `head[trackId]` is set.
        bool hasHead = head[input.trackId] != bytes32(0);
        uint64 requiredSequence = hasHead ? headSequence[input.trackId] + 1 : 0;
        if (input.sequence != requiredSequence) revert SequenceMismatch(requiredSequence, input.sequence);

        bytes32 requiredPreviousHash = hasHead ? head[input.trackId] : bytes32(0);
        if (input.previousCheckpointHash != requiredPreviousHash) {
            revert PreviousHashMismatch(requiredPreviousHash, input.previousCheckpointHash);
        }

        if (input.periodEnd <= input.periodStart) revert InvalidPeriod(input.periodStart, input.periodEnd);
        uint64 minimumStart = hasHead ? headPeriodEnd[input.trackId] : 0;
        if (hasHead && input.periodStart < minimumStart) {
            revert PeriodOverlapsOrRegresses(minimumStart, input.periodStart);
        }

        bytes32 structHash = _hashAppendCheckpoint(input, ownerEpoch, nonce, deadline);
        _consumeCommand(input.trackId, owner, structHash, ownerEpoch, nonce, deadline, signature);

        // forge-lint: disable-next-line(unsafe-typecast)
        uint64 anchoredAt = uint64(block.timestamp);
        // forge-lint: disable-next-line(block-timestamp)
        CheckpointTiming timing = anchoredAt > input.publicationDeadline ? CheckpointTiming.Late : CheckpointTiming.OnTime;

        _checkpoints[input.checkpointHash] = StoredCheckpoint({
            input: input,
            timing: timing,
            anchoredAt: anchoredAt,
            proofAnchored: false,
            proofVerifiedOnChain: false,
            proofDigest: bytes32(0),
            claimSetDigest: bytes32(0),
            correctionHead: bytes32(0),
            exists: true
        });
        head[input.trackId] = input.checkpointHash;
        headSequence[input.trackId] = input.sequence;
        headPeriodEnd[input.trackId] = input.periodEnd;

        emit CheckpointCommitted(input.trackId, input.checkpointHash, input.sequence, timing, anchoredAt);
    }

    /// @notice Records that a proof was produced for `checkpointHash`.
    /// Does not verify the receipt on-chain (see contract NatSpec):
    /// `proofVerifiedOnChain` stays false. `ProofAttached` is a separate
    /// event from checkpoint commitment specifically to avoid the
    /// circularity the protocol specification calls out: the
    /// receipt's journal binds to `checkpointHash`, so the checkpoint
    /// cannot itself depend on the receipt's digest.
    function attachProof(
        bytes32 checkpointHash,
        bytes32 proofDigest,
        bytes32 claimSetDigest,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        StoredCheckpoint storage checkpoint = _checkpoints[checkpointHash];
        if (!checkpoint.exists) revert CheckpointNotFound(checkpointHash);
        address owner = _requireTrackOwner(checkpoint.input.trackId);

        bytes32 structHash = keccak256(
            abi.encode(ATTACH_PROOF_TYPEHASH, checkpointHash, proofDigest, claimSetDigest, ownerEpoch, nonce, deadline)
        );
        _consumeCommand(checkpoint.input.trackId, owner, structHash, ownerEpoch, nonce, deadline, signature);

        checkpoint.proofAnchored = true;
        checkpoint.proofDigest = proofDigest;
        checkpoint.claimSetDigest = claimSetDigest;
        emit ProofAttached(checkpointHash, proofDigest, claimSetDigest);
    }

    /// @notice Anchors a correction proposal against `targetCheckpointHash`.
    /// Open to any caller (see contract NatSpec) and always stored as
    /// `Proposed`: this function never marks a correction Accepted,
    /// Rejected or Disputed.
    function publishCorrection(
        bytes32 targetCheckpointHash,
        bytes32 replacementDigest,
        bytes32 sourceEvidenceDigest,
        bytes32 reasonCode
    ) external returns (bytes32 correctionId) {
        if (!_checkpoints[targetCheckpointHash].exists) revert CheckpointNotFound(targetCheckpointHash);

        correctionId = keccak256(abi.encodePacked(block.chainid, address(this), targetCheckpointHash, _correctionCounter++));
        bytes32 previousHead = _checkpoints[targetCheckpointHash].correctionHead;
        corrections[correctionId] = Correction({
            targetCheckpointHash: targetCheckpointHash,
            replacementDigest: replacementDigest,
            sourceEvidenceDigest: sourceEvidenceDigest,
            reasonCode: reasonCode,
            previousCorrectionHead: previousHead,
            proposer: msg.sender,
            // forge-lint: disable-next-line(unsafe-typecast)
            proposedAt: uint64(block.timestamp),
            state: CorrectionState.Proposed,
            exists: true
        });
        _checkpoints[targetCheckpointHash].correctionHead = correctionId;

        emit CorrectionPublished(correctionId, targetCheckpointHash, replacementDigest, reasonCode);
    }

    // --- views ----------------------------------------------------------

    function getCheckpoint(bytes32 checkpointHash)
        external
        view
        returns (
            CheckpointInput memory input,
            CheckpointTiming timing,
            uint64 anchoredAt,
            bool proofAnchored,
            bool proofVerifiedOnChain,
            bytes32 proofDigest,
            bytes32 claimSetDigest,
            bytes32 correctionHead,
            bool exists
        )
    {
        StoredCheckpoint storage checkpoint = _checkpoints[checkpointHash];
        return (
            checkpoint.input,
            checkpoint.timing,
            checkpoint.anchoredAt,
            checkpoint.proofAnchored,
            checkpoint.proofVerifiedOnChain,
            checkpoint.proofDigest,
            checkpoint.claimSetDigest,
            checkpoint.correctionHead,
            checkpoint.exists
        );
    }

    // --- internals ----------------------------------------------------

    function _hashAppendCheckpoint(CheckpointInput calldata input, uint64 ownerEpoch, uint64 nonce, uint256 deadline)
        private
        pure
        returns (bytes32)
    {
        bytes32 inputHash = keccak256(
            abi.encode(
                CHECKPOINT_INPUT_TYPEHASH,
                input.trackId,
                input.checkpointHash,
                input.sequence,
                input.previousCheckpointHash,
                input.periodStart,
                input.periodEnd,
                input.membershipEpoch,
                input.accountSetRoot,
                input.sourcePolicyHash,
                input.metricPolicyHash,
                input.pricePolicyHash,
                input.rawEvidenceRoot,
                input.normalizedRoot,
                input.valuationRoot,
                input.previousStateCommitment,
                input.nextStateCommitment,
                input.coverageStatus,
                input.coverageManifestHash,
                input.publicationDeadline,
                input.sourceAttestationHash
            )
        );
        return keccak256(abi.encode(APPEND_CHECKPOINT_TYPEHASH, inputHash, ownerEpoch, nonce, deadline));
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

    function _consumeCommand(
        bytes32 trackId,
        address signer,
        bytes32 structHash,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) private {
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

// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {IdentityRegistry} from "../src/IdentityRegistry.sol";
import {CheckpointRegistry} from "../src/CheckpointRegistry.sol";

contract CheckpointRegistryTest is Test {
    IdentityRegistry internal identityRegistry;
    CheckpointRegistry internal registry;

    bytes32 internal constant CREATE_TRACK_TYPEHASH = keccak256(
        "CreateTrack(bytes32 identityId,bytes32 financialProfileId,bytes32 denominationCommitment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 internal constant CHECKPOINT_INPUT_TYPEHASH = keccak256(
        "CheckpointInput(bytes32 trackId,bytes32 checkpointHash,uint64 sequence,bytes32 previousCheckpointHash,uint64 periodStart,uint64 periodEnd,uint64 membershipEpoch,bytes32 accountSetRoot,bytes32 sourcePolicyHash,bytes32 metricPolicyHash,bytes32 pricePolicyHash,bytes32 rawEvidenceRoot,bytes32 normalizedRoot,bytes32 valuationRoot,bytes32 previousStateCommitment,bytes32 nextStateCommitment,uint8 coverageStatus,bytes32 coverageManifestHash,uint64 publicationDeadline,bytes32 sourceAttestationHash)"
    );
    bytes32 internal constant APPEND_CHECKPOINT_TYPEHASH = keccak256(
        "AppendCheckpoint(CheckpointInput input,uint64 ownerEpoch,uint64 nonce,uint256 deadline)CheckpointInput(bytes32 trackId,bytes32 checkpointHash,uint64 sequence,bytes32 previousCheckpointHash,uint64 periodStart,uint64 periodEnd,uint64 membershipEpoch,bytes32 accountSetRoot,bytes32 sourcePolicyHash,bytes32 metricPolicyHash,bytes32 pricePolicyHash,bytes32 rawEvidenceRoot,bytes32 normalizedRoot,bytes32 valuationRoot,bytes32 previousStateCommitment,bytes32 nextStateCommitment,uint8 coverageStatus,bytes32 coverageManifestHash,uint64 publicationDeadline,bytes32 sourceAttestationHash)"
    );
    bytes32 internal constant ATTACH_PROOF_TYPEHASH = keccak256(
        "AttachProof(bytes32 checkpointHash,bytes32 proofDigest,bytes32 claimSetDigest,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    uint256 internal ownerKey = 0xA11CE;
    uint256 internal strangerKey = 0xEEEE;
    address internal owner;
    address internal stranger;

    bytes32 internal identityId;
    bytes32 internal trackId;

    uint64 internal constant DAY = 1 days;

    function setUp() public {
        identityRegistry = new IdentityRegistry();
        registry = new CheckpointRegistry(identityRegistry);
        owner = vm.addr(ownerKey);
        stranger = vm.addr(strangerKey);

        vm.warp(10 * uint256(DAY));

        vm.prank(owner);
        identityId = identityRegistry.createIdentity(IdentityRegistry.IdentityType.Person);

        bytes32 structHash = keccak256(
            abi.encode(
                CREATE_TRACK_TYPEHASH,
                identityId,
                keccak256("spot-nav-twr-v1"),
                keccak256("USDT"),
                uint64(0),
                uint64(0),
                block.timestamp + 1 hours
            )
        );
        bytes memory sig = _sign(ownerKey, _identityDigest(structHash));
        trackId = identityRegistry.createTrack(
            identityId, keccak256("spot-nav-twr-v1"), keccak256("USDT"), 0, 0, block.timestamp + 1 hours, sig
        );
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

    function _checkpointDomainSeparator() internal view returns (bytes32) {
        bytes32 domainTypeHash =
            keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
        return keccak256(
            abi.encode(
                domainTypeHash,
                keccak256(bytes("LinvestherZK-CheckpointRegistry")),
                keccak256(bytes("1")),
                block.chainid,
                address(registry)
            )
        );
    }

    function _checkpointDigest(bytes32 structHash) internal view returns (bytes32) {
        return keccak256(abi.encodePacked("\x19\x01", _checkpointDomainSeparator(), structHash));
    }

    function _sign(uint256 key, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(key, digest);
        return abi.encodePacked(r, s, v);
    }

    function _input(uint64 sequence, bytes32 previousHash, uint64 periodStart, uint64 periodEnd, uint64 publicationDeadline)
        internal
        view
        returns (CheckpointRegistry.CheckpointInput memory)
    {
        bytes32 checkpointHash = keccak256(
            abi.encodePacked("checkpoint", trackId, sequence, periodStart, periodEnd)
        );
        return CheckpointRegistry.CheckpointInput({
            trackId: trackId,
            checkpointHash: checkpointHash,
            sequence: sequence,
            previousCheckpointHash: previousHash,
            periodStart: periodStart,
            periodEnd: periodEnd,
            membershipEpoch: 0,
            accountSetRoot: keccak256("accounts"),
            sourcePolicyHash: keccak256("source-policy"),
            metricPolicyHash: keccak256("metric-policy"),
            pricePolicyHash: keccak256("price-policy"),
            rawEvidenceRoot: keccak256("raw"),
            normalizedRoot: keccak256("normalized"),
            valuationRoot: keccak256("valuation"),
            previousStateCommitment: bytes32(0),
            nextStateCommitment: keccak256("next-state"),
            coverageStatus: CheckpointRegistry.CoverageStatus.PolicyComplete,
            coverageManifestHash: keccak256("coverage-manifest"),
            publicationDeadline: publicationDeadline,
            sourceAttestationHash: keccak256("attestation")
        });
    }

    function _inputHash(CheckpointRegistry.CheckpointInput memory input) internal pure returns (bytes32) {
        return keccak256(
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
    }

    function _appendSigned(
        CheckpointRegistry.CheckpointInput memory input,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        uint256 signerKey
    ) internal {
        bytes32 structHash = keccak256(abi.encode(APPEND_CHECKPOINT_TYPEHASH, _inputHash(input), ownerEpoch, nonce, deadline));
        bytes memory sig = _sign(signerKey, _checkpointDigest(structHash));
        registry.appendCheckpoint(input, ownerEpoch, nonce, deadline, sig);
    }

    // --- single head / sequence / previous hash ----------------------------

    function test_AppendCheckpoint_Genesis() public {
        uint64 deadline = uint64(block.timestamp) + DAY;
        CheckpointRegistry.CheckpointInput memory input = _input(0, bytes32(0), 9 * DAY, 10 * DAY, deadline);
        _appendSigned(input, 0, 0, block.timestamp + 1 hours, ownerKey);

        assertEq(registry.head(trackId), input.checkpointHash);
        assertEq(registry.headSequence(trackId), 0);
        (,, uint64 anchoredAt,,,,,,) = registry.getCheckpoint(input.checkpointHash);
        assertEq(anchoredAt, block.timestamp);
    }

    function test_AppendCheckpoint_SecondCheckpointChainsToHead() public {
        uint64 deadline = uint64(block.timestamp) + DAY;
        CheckpointRegistry.CheckpointInput memory genesis = _input(0, bytes32(0), 9 * DAY, 10 * DAY, deadline);
        _appendSigned(genesis, 0, 0, block.timestamp + 1 hours, ownerKey);

        CheckpointRegistry.CheckpointInput memory next =
            _input(1, genesis.checkpointHash, 10 * DAY, 11 * DAY, uint64(block.timestamp) + DAY);
        _appendSigned(next, 0, 1, block.timestamp + 1 hours, ownerKey);

        assertEq(registry.head(trackId), next.checkpointHash);
        assertEq(registry.headSequence(trackId), 1);
    }

    function test_AppendCheckpoint_RevertsOnSequenceNotHeadPlusOne() public {
        uint64 deadline = uint64(block.timestamp) + DAY;
        CheckpointRegistry.CheckpointInput memory genesis = _input(0, bytes32(0), 9 * DAY, 10 * DAY, deadline);
        _appendSigned(genesis, 0, 0, block.timestamp + 1 hours, ownerKey);

        // Skips sequence 1, jumps to 2.
        CheckpointRegistry.CheckpointInput memory bad =
            _input(2, genesis.checkpointHash, 10 * DAY, 11 * DAY, uint64(block.timestamp) + DAY);
        vm.expectRevert(abi.encodeWithSelector(CheckpointRegistry.SequenceMismatch.selector, 1, 2));
        _appendSigned(bad, 0, 1, block.timestamp + 1 hours, ownerKey);
    }

    function test_AppendCheckpoint_RevertsOnConflictingSecondHead() public {
        uint64 deadline = uint64(block.timestamp) + DAY;
        CheckpointRegistry.CheckpointInput memory genesis = _input(0, bytes32(0), 9 * DAY, 10 * DAY, deadline);
        _appendSigned(genesis, 0, 0, block.timestamp + 1 hours, ownerKey);

        CheckpointRegistry.CheckpointInput memory next =
            _input(1, genesis.checkpointHash, 10 * DAY, 11 * DAY, uint64(block.timestamp) + DAY);
        _appendSigned(next, 0, 1, block.timestamp + 1 hours, ownerKey);

        // A conflicting fork at the correct next sequence (2) but
        // pointing at genesis instead of the actual head (`next`) must be
        // rejected: the head already advanced past genesis.
        CheckpointRegistry.CheckpointInput memory conflicting =
            _input(2, genesis.checkpointHash, 11 * DAY, 12 * DAY, uint64(block.timestamp) + DAY);
        vm.expectRevert(
            abi.encodeWithSelector(CheckpointRegistry.PreviousHashMismatch.selector, next.checkpointHash, genesis.checkpointHash)
        );
        _appendSigned(conflicting, 0, 2, block.timestamp + 1 hours, ownerKey);
    }

    function test_AppendCheckpoint_RevertsOnWrongPreviousHashAtGenesis() public {
        CheckpointRegistry.CheckpointInput memory bad =
            _input(0, keccak256("not-zero"), 9 * DAY, 10 * DAY, uint64(block.timestamp) + DAY);
        vm.expectRevert(
            abi.encodeWithSelector(CheckpointRegistry.PreviousHashMismatch.selector, bytes32(0), keccak256("not-zero"))
        );
        _appendSigned(bad, 0, 0, block.timestamp + 1 hours, ownerKey);
    }

    // --- period correctness --------------------------------------------

    function test_AppendCheckpoint_RevertsOnInvertedPeriod() public {
        CheckpointRegistry.CheckpointInput memory bad = _input(0, bytes32(0), 10 * DAY, 9 * DAY, uint64(block.timestamp) + DAY);
        vm.expectRevert(abi.encodeWithSelector(CheckpointRegistry.InvalidPeriod.selector, 10 * DAY, 9 * DAY));
        _appendSigned(bad, 0, 0, block.timestamp + 1 hours, ownerKey);
    }

    function test_AppendCheckpoint_RevertsOnOverlappingPeriod() public {
        uint64 deadline = uint64(block.timestamp) + DAY;
        CheckpointRegistry.CheckpointInput memory genesis = _input(0, bytes32(0), 9 * DAY, 10 * DAY, deadline);
        _appendSigned(genesis, 0, 0, block.timestamp + 1 hours, ownerKey);

        // periodStart before the previous periodEnd: overlap, must revert.
        CheckpointRegistry.CheckpointInput memory overlapping =
            _input(1, genesis.checkpointHash, 10 * DAY - 1, 11 * DAY, uint64(block.timestamp) + DAY);
        vm.expectRevert(
            abi.encodeWithSelector(CheckpointRegistry.PeriodOverlapsOrRegresses.selector, 10 * DAY, 10 * DAY - 1)
        );
        _appendSigned(overlapping, 0, 1, block.timestamp + 1 hours, ownerKey);
    }

    function test_AppendCheckpoint_AllowsGapBetweenPeriods() public {
        uint64 deadline = uint64(block.timestamp) + DAY;
        CheckpointRegistry.CheckpointInput memory genesis = _input(0, bytes32(0), 9 * DAY, 10 * DAY, deadline);
        _appendSigned(genesis, 0, 0, block.timestamp + 1 hours, ownerKey);

        // Segment resumes 2 days after the previous periodEnd: an explicit
        // gap, not an overlap, so this must succeed (spec allows gaps;
        // gap *detection* itself is a later task's concern).
        CheckpointRegistry.CheckpointInput memory afterGap =
            _input(1, genesis.checkpointHash, 12 * DAY, 13 * DAY, uint64(block.timestamp) + DAY);
        _appendSigned(afterGap, 0, 1, block.timestamp + 1 hours, ownerKey);
        assertEq(registry.head(trackId), afterGap.checkpointHash);
    }

    // --- lateness --------------------------------------------------------

    function test_AppendCheckpoint_OnTimeWhenBeforeDeadline() public {
        CheckpointRegistry.CheckpointInput memory input =
            _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) + 1 hours);
        _appendSigned(input, 0, 0, block.timestamp + 1 hours, ownerKey);

        (, CheckpointRegistry.CheckpointTiming timing,,,,,,,) = registry.getCheckpoint(input.checkpointHash);
        assertTrue(timing == CheckpointRegistry.CheckpointTiming.OnTime);
    }

    function test_AppendCheckpoint_LateWhenAfterDeadline() public {
        // publicationDeadline already in the past relative to now.
        CheckpointRegistry.CheckpointInput memory input = _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) - 1);
        _appendSigned(input, 0, 0, block.timestamp + 1 hours, ownerKey);

        (, CheckpointRegistry.CheckpointTiming timing,,,,,,,) = registry.getCheckpoint(input.checkpointHash);
        assertTrue(timing == CheckpointRegistry.CheckpointTiming.Late, "a late checkpoint must never be recorded as on-time");
    }

    function test_AppendCheckpoint_LateCheckpointStillBecomesHead() public {
        // A late checkpoint is still anchored (attachable), just labeled
        // late — it must not be silently dropped.
        CheckpointRegistry.CheckpointInput memory input = _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) - 1);
        _appendSigned(input, 0, 0, block.timestamp + 1 hours, ownerKey);
        assertEq(registry.head(trackId), input.checkpointHash);
    }

    // --- authorization / replay --------------------------------------------

    function test_AppendCheckpoint_RevertsWhenSignedByNonOwner() public {
        CheckpointRegistry.CheckpointInput memory input =
            _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) + DAY);
        bytes32 structHash =
            keccak256(abi.encode(APPEND_CHECKPOINT_TYPEHASH, _inputHash(input), uint64(0), uint64(0), block.timestamp + 1 hours));
        bytes memory sig = _sign(strangerKey, _checkpointDigest(structHash));
        vm.expectRevert(CheckpointRegistry.InvalidSignature.selector);
        registry.appendCheckpoint(input, 0, 0, block.timestamp + 1 hours, sig);
    }

    function test_AppendCheckpoint_RevertsOnReplay() public {
        CheckpointRegistry.CheckpointInput memory genesis = _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) + DAY);
        bytes32 structHash =
            keccak256(abi.encode(APPEND_CHECKPOINT_TYPEHASH, _inputHash(genesis), uint64(0), uint64(0), block.timestamp + 1 hours));
        bytes memory sig = _sign(ownerKey, _checkpointDigest(structHash));
        registry.appendCheckpoint(genesis, 0, 0, block.timestamp + 1 hours, sig);

        vm.expectRevert(abi.encodeWithSelector(CheckpointRegistry.CheckpointAlreadyExists.selector, genesis.checkpointHash));
        registry.appendCheckpoint(genesis, 0, 0, block.timestamp + 1 hours, sig);
    }

    // --- no delete -------------------------------------------------------

    function test_NoDeleteFunctionExists() public {
        // Structural: there is no function selector that could delete a
        // checkpoint. This test documents the property; the absence is
        // enforced by the contract simply never defining such a function.
        CheckpointRegistry.CheckpointInput memory input = _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) + DAY);
        _appendSigned(input, 0, 0, block.timestamp + 1 hours, ownerKey);
        (,,,,,,,, bool exists) = registry.getCheckpoint(input.checkpointHash);
        assertTrue(exists);
    }

    // --- proof attachment / correction -----------------------------------

    function test_AttachProof_RecordsDigestsWithoutOnChainVerification() public {
        CheckpointRegistry.CheckpointInput memory input = _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) + DAY);
        _appendSigned(input, 0, 0, block.timestamp + 1 hours, ownerKey);

        bytes32 proofDigest = keccak256("proof");
        bytes32 claimSetDigest = keccak256("claims");
        bytes32 structHash = keccak256(
            abi.encode(ATTACH_PROOF_TYPEHASH, input.checkpointHash, proofDigest, claimSetDigest, uint64(0), uint64(1), block.timestamp + 1 hours)
        );
        bytes memory sig = _sign(ownerKey, _checkpointDigest(structHash));
        registry.attachProof(input.checkpointHash, proofDigest, claimSetDigest, 0, 1, block.timestamp + 1 hours, sig);

        (,,, bool proofAnchored, bool proofVerifiedOnChain, bytes32 storedProofDigest,,,) =
            registry.getCheckpoint(input.checkpointHash);
        assertTrue(proofAnchored);
        assertFalse(proofVerifiedOnChain, "pilot never verifies the seal on-chain");
        assertEq(storedProofDigest, proofDigest);
    }

    function test_PublishCorrection_AnyCallerCanProposeAlwaysAsProposed() public {
        CheckpointRegistry.CheckpointInput memory input = _input(0, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) + DAY);
        _appendSigned(input, 0, 0, block.timestamp + 1 hours, ownerKey);

        vm.prank(stranger);
        bytes32 correctionId = registry.publishCorrection(
            input.checkpointHash, keccak256("replacement"), keccak256("evidence"), keccak256("reason")
        );

        (,,,,, address proposer,, CheckpointRegistry.CorrectionState state, bool exists) = registry.corrections(correctionId);
        assertTrue(exists);
        assertEq(proposer, stranger);
        assertTrue(state == CheckpointRegistry.CorrectionState.Proposed);
    }

    // --- fuzz ---------------------------------------------------------

    function testFuzz_AppendCheckpoint_RevertsOnAnyWrongSequence(uint64 wrongSequence) public {
        vm.assume(wrongSequence != 0);
        CheckpointRegistry.CheckpointInput memory bad =
            _input(wrongSequence, bytes32(0), 9 * DAY, 10 * DAY, uint64(block.timestamp) + DAY);
        vm.expectRevert(abi.encodeWithSelector(CheckpointRegistry.SequenceMismatch.selector, 0, wrongSequence));
        _appendSigned(bad, 0, 0, block.timestamp + 1 hours, ownerKey);
    }
}

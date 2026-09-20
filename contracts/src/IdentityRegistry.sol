// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {SignatureChecker} from "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";

/// @notice Registers `PerformanceIdentity` and `TrackRecord` per
/// the protocol specification, and owner lifecycle (rotation) per
/// the EVM adapter specification.
///
/// `identityId`/`trackId` are internal 32-byte opaque identifiers derived
/// on-chain from a monotonic counter, not caller-supplied: they must be
/// unpredictable-enough to not be trivially guessable and, more
/// importantly, collision-free by construction, and must never equal the
/// owner's address (the protocol specification: "identityId não é o
/// endereço do owner").
///
/// Every state-changing call after identity creation is authorized by an
/// EIP-712 signature from the identity's current owner (or its pending
/// owner, for rotation confirmation), verified via
/// `SignatureChecker.isValidSignatureNow`, which accepts both ECDSA
/// signatures from EOAs and ERC-1271 signatures from contract wallets.
/// This lets any relayer submit the transaction while only the owner's
/// key authorizes the action ("the architecture design":
/// "Relayer é substituível; SDK permite gerar comando e transmitir
/// diretamente").
///
/// Owner rotation is two-step: the current owner proposes a new owner,
/// and only a signature from that new owner finalizes the rotation. This
/// guards against transferring control to an address that cannot sign
/// (a typo, or a contract that does not implement ERC-1271). Finalizing a
/// rotation increments `ownerEpoch`; every signed command carries the
/// epoch it was signed under, so a signature produced by a since-rotated
/// owner is rejected by construction, without needing an explicit
/// revocation list.
contract IdentityRegistry is EIP712 {
    enum IdentityType {
        Person,
        Organization,
        Strategy,
        Bot
    }

    struct Identity {
        address owner;
        address pendingOwner;
        uint64 createdAt;
        uint64 ownerEpoch;
        uint64 nonce;
        IdentityType identityType;
        bool exists;
    }

    struct Track {
        bytes32 identityId;
        bytes32 financialProfileId;
        bytes32 denominationCommitment;
        uint64 createdAt;
        bool exists;
    }

    bytes32 private constant CREATE_TRACK_TYPEHASH = keccak256(
        "CreateTrack(bytes32 identityId,bytes32 financialProfileId,bytes32 denominationCommitment,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 private constant PROPOSE_OWNER_ROTATION_TYPEHASH = keccak256(
        "ProposeOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );
    bytes32 private constant CONFIRM_OWNER_ROTATION_TYPEHASH = keccak256(
        "ConfirmOwnerRotation(bytes32 identityId,address newOwner,uint64 ownerEpoch,uint64 nonce,uint256 deadline)"
    );

    mapping(bytes32 identityId => Identity) public identities;
    mapping(bytes32 trackId => Track) public tracks;

    uint256 private _identityCounter;
    uint256 private _trackCounter;

    event IdentityCreated(bytes32 indexed identityId, address indexed owner, IdentityType identityType, uint64 createdAt);
    event TrackCreated(
        bytes32 indexed identityId, bytes32 indexed trackId, bytes32 financialProfileId, bytes32 denominationCommitment
    );
    event OwnerRotationProposed(bytes32 indexed identityId, address indexed currentOwner, address indexed pendingOwner, uint64 ownerEpoch);
    event OwnerRotated(bytes32 indexed identityId, address indexed previousOwner, address indexed newOwner, uint64 newOwnerEpoch);

    error IdentityNotFound(bytes32 identityId);
    error InvalidSignature();
    error CommandExpired(uint256 deadline, uint256 blockTimestamp);
    error OwnerEpochMismatch(uint64 expected, uint64 provided);
    error NonceMismatch(uint64 expected, uint64 provided);
    error NoPendingRotation(bytes32 identityId);
    error PendingOwnerMismatch(address expected, address provided);
    error ZeroAddress();
    error SameOwner();

    constructor() EIP712("LinvestherZK-IdentityRegistry", "1") {}

    /// @notice Self-registration: the caller becomes the identity's owner
    /// directly, since there is no prior owner state to authorize this.
    function createIdentity(IdentityType identityType) external returns (bytes32 identityId) {
        identityId = keccak256(abi.encodePacked(block.chainid, address(this), msg.sender, _identityCounter++));
        // casting to 'uint64' is safe: block.timestamp does not exceed
        // type(uint64).max (year ~584 billion) on any real chain.
        // forge-lint: disable-next-line(unsafe-typecast)
        uint64 createdAt = uint64(block.timestamp);
        identities[identityId] = Identity({
            owner: msg.sender,
            pendingOwner: address(0),
            createdAt: createdAt,
            ownerEpoch: 0,
            nonce: 0,
            identityType: identityType,
            exists: true
        });
        emit IdentityCreated(identityId, msg.sender, identityType, createdAt);
    }

    /// @notice Creates a track under `identityId`, authorized by the
    /// identity's current owner.
    function createTrack(
        bytes32 identityId,
        bytes32 financialProfileId,
        bytes32 denominationCommitment,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external returns (bytes32 trackId) {
        Identity storage identity = _requireIdentity(identityId);

        bytes32 structHash = keccak256(
            abi.encode(CREATE_TRACK_TYPEHASH, identityId, financialProfileId, denominationCommitment, ownerEpoch, nonce, deadline)
        );
        _consumeOwnerCommand(identity, identity.owner, structHash, ownerEpoch, nonce, deadline, signature);

        trackId = keccak256(abi.encodePacked(block.chainid, address(this), identityId, _trackCounter++));
        tracks[trackId] = Track({
            identityId: identityId,
            financialProfileId: financialProfileId,
            denominationCommitment: denominationCommitment,
            // see createIdentity: block.timestamp always fits in uint64.
            // forge-lint: disable-next-line(unsafe-typecast)
            createdAt: uint64(block.timestamp),
            exists: true
        });
        emit TrackCreated(identityId, trackId, financialProfileId, denominationCommitment);
    }

    /// @notice Current owner proposes `newOwner`; takes effect only once
    /// `newOwner` confirms via {confirmOwnerRotation}.
    function proposeOwnerRotation(
        bytes32 identityId,
        address newOwner,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        if (newOwner == address(0)) revert ZeroAddress();
        Identity storage identity = _requireIdentity(identityId);
        if (newOwner == identity.owner) revert SameOwner();

        bytes32 structHash =
            keccak256(abi.encode(PROPOSE_OWNER_ROTATION_TYPEHASH, identityId, newOwner, ownerEpoch, nonce, deadline));
        _consumeOwnerCommand(identity, identity.owner, structHash, ownerEpoch, nonce, deadline, signature);

        identity.pendingOwner = newOwner;
        emit OwnerRotationProposed(identityId, identity.owner, newOwner, identity.ownerEpoch);
    }

    /// @notice The proposed new owner confirms the rotation with their own
    /// signature, finalizing the ownership change and revoking the
    /// previous owner's key for this identity (via the epoch bump).
    function confirmOwnerRotation(
        bytes32 identityId,
        address newOwner,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        Identity storage identity = _requireIdentity(identityId);
        if (identity.pendingOwner == address(0)) revert NoPendingRotation(identityId);
        if (identity.pendingOwner != newOwner) revert PendingOwnerMismatch(identity.pendingOwner, newOwner);

        bytes32 structHash =
            keccak256(abi.encode(CONFIRM_OWNER_ROTATION_TYPEHASH, identityId, newOwner, ownerEpoch, nonce, deadline));
        _consumeOwnerCommand(identity, identity.pendingOwner, structHash, ownerEpoch, nonce, deadline, signature);

        address previousOwner = identity.owner;
        identity.owner = newOwner;
        identity.pendingOwner = address(0);
        identity.ownerEpoch += 1;
        // Nonces are a single monotonic sequence per identity shared across
        // owner and pending-owner signed commands; the epoch bump is what
        // actually revokes the previous owner's key, so the nonce sequence
        // does not need to reset.
        emit OwnerRotated(identityId, previousOwner, newOwner, identity.ownerEpoch);
    }

    function _requireIdentity(bytes32 identityId) private view returns (Identity storage identity) {
        identity = identities[identityId];
        if (!identity.exists) revert IdentityNotFound(identityId);
    }

    /// @dev Validates deadline, epoch, nonce and signature for a command
    /// signed by `signer`, then consumes the nonce. Reverts on any
    /// mismatch; the signature is checked last, but the nonce is only
    /// persisted if every check (including the signature) succeeds,
    /// since a revert unwinds the whole call.
    function _consumeOwnerCommand(
        Identity storage identity,
        address signer,
        bytes32 structHash,
        uint64 ownerEpoch,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) private {
        // deadlines here are expected in minutes/hours, not seconds; the
        // few seconds of validator manipulation tolerance is immaterial,
        // the same accepted pattern as EIP-2612 permit deadlines.
        // forge-lint: disable-next-line(block-timestamp)
        if (block.timestamp > deadline) revert CommandExpired(deadline, block.timestamp);
        if (ownerEpoch != identity.ownerEpoch) revert OwnerEpochMismatch(identity.ownerEpoch, ownerEpoch);
        if (nonce != identity.nonce) revert NonceMismatch(identity.nonce, nonce);

        bytes32 digest = _hashTypedDataV4(structHash);
        if (!SignatureChecker.isValidSignatureNow(signer, digest, signature)) revert InvalidSignature();

        identity.nonce += 1;
    }
}

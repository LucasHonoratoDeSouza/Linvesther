// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {SignatureChecker} from "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";

interface IIdentityOwnership {
    function identities(bytes32 identityId)
        external
        view
        returns (address owner, address pendingOwner, uint64 createdAt, uint64 ownerEpoch, uint64 nonce, uint8 identityType, bool exists);
}

/// @notice An optional public name and bio for an identity, changeable by
/// its owner at any time. Anyone can read the current text from this
/// contract, so a claim like "this account is mine" can be checked by
/// asking the owner to change the bio to a phrase of the checker's choice
/// and watching it change — without trusting any server.
///
/// The text is public and its history stays in the event log forever:
/// overwriting a bio replaces the current value but cannot erase earlier
/// ones, so it must never hold private information.
///
/// Like every other registry here, a change is authorized by an EIP-712
/// signature from the identity's current owner (ECDSA or ERC-1271), so any
/// relayer can submit it.
contract ProfileRegistry is EIP712 {
    uint256 public constant MAX_NAME_BYTES = 40;
    uint256 public constant MAX_BIO_BYTES = 280;

    struct Profile {
        string name;
        string bio;
        uint64 updatedAt;
    }

    bytes32 private constant SET_PROFILE_TYPEHASH =
        keccak256("SetProfile(bytes32 identityId,string name,string bio,uint64 nonce,uint256 deadline)");

    IIdentityOwnership public immutable identityRegistry;

    mapping(bytes32 identityId => Profile) private _profiles;
    mapping(bytes32 identityId => uint64) public nonces;

    event ProfileUpdated(bytes32 indexed identityId, address indexed owner, string name, string bio, uint64 updatedAt);

    error IdentityNotFound(bytes32 identityId);
    error CommandExpired(uint256 deadline, uint256 blockTimestamp);
    error NonceMismatch(uint64 expected, uint64 provided);
    error InvalidSignature();
    error NameTooLong(uint256 length);
    error BioTooLong(uint256 length);

    constructor(IIdentityOwnership identityRegistry_) EIP712("LinvestherZK-ProfileRegistry", "1") {
        identityRegistry = identityRegistry_;
    }

    function profileOf(bytes32 identityId) external view returns (string memory name, string memory bio, uint64 updatedAt) {
        Profile storage profile = _profiles[identityId];
        return (profile.name, profile.bio, profile.updatedAt);
    }

    /// @notice Sets both fields (an empty string clears one). Signed by the
    /// identity's current owner over exactly these values.
    function setProfile(
        bytes32 identityId,
        string calldata name,
        string calldata bio,
        uint64 nonce,
        uint256 deadline,
        bytes calldata signature
    ) external {
        (address owner,,,,, , bool exists) = identityRegistry.identities(identityId);
        if (!exists) revert IdentityNotFound(identityId);
        if (bytes(name).length > MAX_NAME_BYTES) revert NameTooLong(bytes(name).length);
        if (bytes(bio).length > MAX_BIO_BYTES) revert BioTooLong(bytes(bio).length);
        // forge-lint: disable-next-line(block-timestamp)
        if (block.timestamp > deadline) revert CommandExpired(deadline, block.timestamp);
        if (nonce != nonces[identityId]) revert NonceMismatch(nonces[identityId], nonce);

        bytes32 structHash = keccak256(
            abi.encode(SET_PROFILE_TYPEHASH, identityId, keccak256(bytes(name)), keccak256(bytes(bio)), nonce, deadline)
        );
        if (!SignatureChecker.isValidSignatureNow(owner, _hashTypedDataV4(structHash), signature)) revert InvalidSignature();

        nonces[identityId] = nonce + 1;
        // forge-lint: disable-next-line(unsafe-typecast)
        uint64 updatedAt = uint64(block.timestamp);
        _profiles[identityId] = Profile({name: name, bio: bio, updatedAt: updatedAt});
        emit ProfileUpdated(identityId, owner, name, bio, updatedAt);
    }
}

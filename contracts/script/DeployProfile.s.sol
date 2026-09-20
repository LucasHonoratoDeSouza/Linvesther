// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Script} from "forge-std/Script.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {ProfileRegistry, IIdentityOwnership} from "../src/ProfileRegistry.sol";

/// @notice Deploys only `ProfileRegistry` against the `IdentityRegistry`
/// already recorded in `deployments/<network>.json`, and adds its address
/// to that manifest — the contracts already deployed stay untouched
/// (they have no proxy, so they cannot be upgraded).
contract DeployProfileScript is Script {
    using stdJson for string;

    function run() external returns (address profileRegistry) {
        string memory network = vm.envOr("DEPLOY_NETWORK_NAME", string("local"));
        string memory manifestPath = string.concat("deployments/", network, ".json");
        string memory manifest = vm.readFile(manifestPath);

        vm.startBroadcast();
        ProfileRegistry registry = new ProfileRegistry(IIdentityOwnership(manifest.readAddress(".identityRegistry")));
        vm.stopBroadcast();

        profileRegistry = address(registry);
        vm.writeJson(vm.toString(profileRegistry), manifestPath, ".profileRegistry");
    }
}

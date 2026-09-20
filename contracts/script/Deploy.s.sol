// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Script} from "forge-std/Script.sol";
import {IdentityRegistry} from "../src/IdentityRegistry.sol";
import {AccountRegistry} from "../src/AccountRegistry.sol";
import {WebAuthnAccount} from "../src/WebAuthnAccount.sol";
import {P256VaultAccount} from "../src/P256VaultAccount.sol";
import {AccountFactory} from "../src/AccountFactory.sol";

/// @notice Deploys the registry pair and the account factory, in the
/// order `AccountRegistry`'s constructor requires (`IdentityRegistry`
/// first), then writes every address plus chainId and deploy block to
/// `contracts/deployments/<network>.json` — the "deployment manifest"
/// the EVM adapter specification requires ("Chain IDs, endereços, genesis e
/// code hashes são publicados no deployment manifest, nunca inferidos
/// do nome da rede"). The broadcaster key comes from whatever `forge
/// script` was invoked with (`--private-key`/`--account`), never read
/// or embedded by this script itself.
contract DeployScript is Script {
    function run() external returns (string memory manifestPath) {
        vm.startBroadcast();

        IdentityRegistry identityRegistry = new IdentityRegistry();
        AccountRegistry accountRegistry = new AccountRegistry(identityRegistry);
        WebAuthnAccount webAuthnImplementation = new WebAuthnAccount();
        P256VaultAccount p256VaultImplementation = new P256VaultAccount();
        AccountFactory accountFactory =
            new AccountFactory(address(webAuthnImplementation), address(p256VaultImplementation));

        vm.stopBroadcast();

        string memory network = vm.envOr("DEPLOY_NETWORK_NAME", string("local"));
        string memory json = "manifest";
        vm.serializeUint(json, "chainId", block.chainid);
        vm.serializeUint(json, "deployBlock", block.number);
        vm.serializeAddress(json, "identityRegistry", address(identityRegistry));
        vm.serializeAddress(json, "accountRegistry", address(accountRegistry));
        vm.serializeAddress(json, "webAuthnAccountImplementation", address(webAuthnImplementation));
        vm.serializeAddress(json, "p256VaultAccountImplementation", address(p256VaultImplementation));
        string memory finalJson = vm.serializeAddress(json, "accountFactory", address(accountFactory));

        manifestPath = string.concat("deployments/", network, ".json");
        vm.writeJson(finalJson, manifestPath);
    }
}

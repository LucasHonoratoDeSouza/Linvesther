// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {DeployScript} from "../script/Deploy.s.sol";
import {AccountRegistry} from "../src/AccountRegistry.sol";

contract DeployTest is Test {
    using stdJson for string;

    // A single test (not two) avoids a real race: `vm.setEnv` mutates
    // process-wide environment, and forge test may run test functions
    // concurrently — two functions each setting DEPLOY_NETWORK_NAME
    // could observe each other's value mid-run.
    function test_Run_WritesManifestWithAllFiveAddressesAndWiresAccountRegistry() public {
        DeployScript deployScript = new DeployScript();
        string memory manifestPath = deployScript.run();

        string memory json = vm.readFile(manifestPath);
        address identityRegistry = json.readAddress(".identityRegistry");
        address accountRegistry = json.readAddress(".accountRegistry");

        assertEq(json.readUint(".chainId"), block.chainid);
        assertTrue(identityRegistry != address(0));
        assertTrue(accountRegistry != address(0));
        assertTrue(json.readAddress(".webAuthnAccountImplementation") != address(0));
        assertTrue(json.readAddress(".p256VaultAccountImplementation") != address(0));
        assertTrue(json.readAddress(".accountFactory") != address(0));

        // AccountRegistry's `identityRegistry` is a public immutable set
        // in its constructor — reading it back confirms the script wired
        // the two contracts together, not two independent registries.
        assertEq(address(AccountRegistry(accountRegistry).identityRegistry()), identityRegistry);

        vm.removeFile(manifestPath);
    }
}

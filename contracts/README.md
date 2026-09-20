# Contracts

Foundry project for the EVM registry contracts (`IdentityRegistry`,
`AccountRegistry`, `CheckpointRegistry`, `PolicyRegistry`).
Solidity 0.8.26, pinned in `foundry.toml`.

`lib/forge-std` and `lib/openzeppelin-contracts` are git submodules;
`foundry.lock` records their exact pinned tags and commits. After cloning:

```sh
git submodule update --init --recursive
# or: make -C .. contracts-deps
```

Bump a pin deliberately with `forge install <org>/<repo>@<new-tag>
--no-commit`, then commit the updated submodule reference and
`foundry.lock`; never track a moving branch.

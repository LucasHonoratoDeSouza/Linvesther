# Workspace gates.
# "Gate Check Commands" table for the intended shape of each target; this
# file is what actually executes them. No secrets are required by any
# target here: bootstrap-binance uses fixtures only (no live Binance
# credentials), bootstrap-zk proves locally.
#
# Each of check-rust/check-contracts/check-integration/check-zk/check-e2e
# fails loudly when its expected suite does not exist yet, instead of
# silently reporting success for coverage that was never delivered. That
# is correct today: some of those suites may not exist yet. check-build only covers the workspace manifests
# (workspace manifests) and does not fail on the absence of crates/apps/
# contracts that later tasks are responsible for.

.PHONY: local-up local-down local-reset bootstrap-binance bootstrap-zk contracts-deps \
	check-build check-rust check-contracts check-integration check-zk check-e2e

bootstrap-binance:
	$(MAKE) -C experiments/binance-conformance check

bootstrap-zk:
	$(MAKE) -C experiments/risc0-benchmark check

contracts-deps:
	git submodule update --init --recursive contracts/lib/forge-std contracts/lib/openzeppelin-contracts

# --- build: manifests/compilation/lockfiles, per component present ---
check-build:
	pnpm install --frozen-lockfile
	pnpm -r --if-present run typecheck
	@if [ -d crates ] && [ -n "$$(find crates -mindepth 1 -maxdepth 1 -type d 2>/dev/null)" ]; then \
		cargo check --workspace --locked; \
	else \
		echo "check-build: crates/ has no members yet; nothing to typecheck"; \
	fi
	cd contracts && forge build
	@if [ -f services/Cargo.toml ]; then \
		cargo check --manifest-path services/Cargo.toml --locked; \
	else \
		echo "check-build: services/ has no workspace yet; nothing to typecheck"; \
	fi
	# TS codec conformance against the shared fixtures runs here (not a
	# dedicated bucket in the Gate Check Commands table); crates/commitments'
	# own vectors_test.rs is the authoritative unit-layer check (check-rust).
	pnpm --filter @linvestherzk/protocol --if-present run test

# --- rust: unit + property tests across crates/**/tests ---
check-rust:
	@if [ ! -d crates ] || [ -z "$$(find crates -mindepth 1 -maxdepth 1 -type d 2>/dev/null)" ]; then \
		echo "check-rust: no crates delivered yet; failing rather than reporting a pass with zero coverage" >&2; \
		exit 1; \
	fi
	# The zkVM guest crates are tested by check-zk, which builds the guest.
	cargo test --workspace --locked --exclude zkvm-methods --exclude zkvm-methods-tls-origin --exclude zkvm-methods-statistics

# --- contracts: Foundry unit/fuzz/invariant ---
check-contracts:
	@if [ -z "$$(find contracts/src -mindepth 1 -type f 2>/dev/null)" ]; then \
		echo "check-contracts: no contracts delivered yet; failing rather than reporting a pass with zero coverage" >&2; \
		exit 1; \
	fi
	cd contracts && forge test -vvv

# --- integration: database/storage, adapters and workers ---
# services/ is its own self-contained Cargo workspace (same pattern as
# experiments/*): Cargo's own "tests/" convention for a crate already
# means "integration test" (as opposed to a src/ unit test module), and
# the directory layout puts collector/compute under services/, not
# crates/ — so this gate targets that workspace directly rather than
# requiring a separate TS suite under tests/integration.
# chain-worker is TypeScript, so it is a pnpm
# workspace package (services/chain-worker), not part of the Cargo
# workspace above; its own integration tests (e.g. a real local Anvil
# node) run here too, in addition, not instead.
# apps/api/exports also names this gate; its suite (bundle hashing,
# private-export encryption) runs here too.
# crates/verifier is a root-workspace crate (like the domain
# crates check-rust already covers), but names this gate specifically —
# its own suite (minus the #[ignore]d real-proving test — see its
# README) runs here explicitly too, not only incidentally via check-rust.
# infra/recovery is its own standalone Cargo workspace (like
# experiments/*): restore/retention/mirror/purge-gate decision logic,
# independent of whether infra/docker-compose.yml's containers are
# actually running (see that crate's README for why this gate doesn't
# bring those up).
# services/local-agent is a services/ workspace member, so the
# `cargo test --manifest-path services/Cargo.toml` step above already
# covers it.
# packages/connector-conformance names this gate; its suite runs
# here explicitly.
# A TS-based cross-stack integration suite (e.g. API+DB+chain, once one
# exists) runs here too, in addition, not instead.
check-integration:
	@if [ ! -f services/Cargo.toml ]; then \
		echo "check-integration: no integration suite delivered yet; failing rather than reporting a pass with zero coverage" >&2; \
		exit 1; \
	fi
	cargo test --manifest-path services/Cargo.toml --locked
	@if [ -f services/chain-worker/package.json ]; then \
		pnpm --filter @linvestherzk/chain-worker test; \
	fi
	@if [ -f apps/api/package.json ]; then \
		pnpm --filter @linvestherzk/api test; \
	fi
	@if [ -d crates/verifier ]; then \
		cargo test -p verifier --locked; \
	fi
	@if [ -f infra/recovery/Cargo.toml ]; then \
		cargo test --manifest-path infra/recovery/Cargo.toml --locked; \
	fi
	@if [ -f packages/connector-conformance/package.json ]; then \
		pnpm --filter @linvestherzk/connector-conformance test; \
	fi
	@if [ -d tests/integration ] && [ -n "$$(find tests/integration -mindepth 1 -type f 2>/dev/null)" ]; then \
		pnpm --filter integration-tests test; \
	fi

# --- zk: guest execution, real receipt and verifier ---
check-zk:
	@if [ ! -d zkvm/methods ] || [ -z "$$(find zkvm/methods -mindepth 1 -type f 2>/dev/null)" ]; then \
		echo "check-zk: no guest methods delivered yet; failing rather than reporting a pass with zero coverage" >&2; \
		exit 1; \
	fi
	cargo test --workspace --locked -p zkvm-methods
	@if [ -d zkvm/methods/tls-origin ]; then \
		cargo test --workspace --locked -p zkvm-methods-tls-origin; \
	fi
	@if [ -d zkvm/methods/statistics ]; then \
		cargo test --workspace --locked -p zkvm-methods-statistics; \
	fi

# --- e2e: Vitest/API and Playwright per affected component ---
# apps/api's own unit tests (auth/lifecycle/claims/public) run here too:
# The API and web suites name check-e2e as their gate, and apps/api is exactly what
# tests/e2e exercises, so its unit-level suite belongs in the same gate
# rather than going unrun.
# apps/web is driven by a real browser (Playwright) against a real
# apps/api instance and a real Next.js dev server — see
# tests/e2e/playwright.config.ts's webServer entries.
check-e2e:
	@if [ ! -d tests/e2e ] || [ -z "$$(find tests/e2e -mindepth 1 -type f 2>/dev/null)" ]; then \
		echo "check-e2e: no e2e suite delivered yet; failing rather than reporting a pass with zero coverage" >&2; \
		exit 1; \
	fi
	@if [ -f apps/api/package.json ]; then \
		pnpm --filter @linvestherzk/api test; \
	fi
	pnpm --filter e2e-tests test
	@if [ -f apps/web/package.json ]; then \
		pnpm --filter e2e-tests test:web; \
	fi

# --- local environment: database, chain and .env.local (see docs, "Run it yourself") ---
local-up:
	infra/local/up.sh

local-down:
	infra/local/down.sh

local-reset:
	infra/local/up.sh --reset

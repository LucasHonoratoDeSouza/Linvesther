# Contributing

Thanks for helping. Linvesther makes claims about people's money, so changes are
held to a high bar for correctness and privacy. This page keeps that bar clear.

## Ground rules

- **Never publish private data.** No balance, position, trade, credential or
  exchange identifier belongs in code, tests, fixtures, logs or issues. Fixtures
  must be synthetic.
- **Keep guarantees separate.** A signature is consent, a zero-knowledge proof is
  a correct calculation, and neither proves the source data was honest. Do not
  label one as the other in code or in the interface.
- **No silent downgrades.** If a metric cannot be computed, report it as
  unavailable. Never substitute zero or an estimate.
- **Read-only, always.** Connections must reject credentials that can trade,
  transfer or withdraw.

## Setup

See [Getting started](README.md#getting-started). Copy `.env.example` to `.env`
and never commit it.

## Checks

Run what your change touches before opening a pull request:

| Area | Command |
| --- | --- |
| API | `pnpm --filter @linvestherzk/api test` and `typecheck` |
| Web | `pnpm --filter @linvestherzk/web typecheck` |
| Contracts | `make check-contracts` |
| Rust | `make check-rust` |
| Everything | `make check-build check-rust check-contracts check-integration check-zk check-e2e` |

Add or update tests with the change. A test should fail for the defect it guards
against; prefer real behavior over mocks in production paths.

## Commits and pull requests

- One logical change per commit, with its tests.
- Commit messages follow `type(scope): concrete change`, for example
  `fix(ledger): reject duplicate economic events`. Types: `feat`, `fix`,
  `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore`.
- A pull request explains the problem, the resulting behavior, any migration or
  compatibility impact, and how you verified it.

## Reporting bugs and ideas

Use the issue templates. For security problems, do not open an issue: follow
[SECURITY.md](SECURITY.md).

## Licensing

By contributing you agree that your work is released under the
[Apache-2.0 license](LICENSE).

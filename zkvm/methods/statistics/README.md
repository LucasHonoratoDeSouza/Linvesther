# zkvm/methods/statistics

A RISC Zero guest that verifies a Sharpe, Sortino or CAGR claim
against a *certified* fixed-point interval — never a floating-point
point estimate, per the protocol specification: "Implementação da
aproximação deve ter vetores independentes e não usar `f64`."

## The threshold rule

Per metrics.md: "Claims usam valores internos/intervalos
certificados... Para `metric > k`, aprovar apenas se limite inferior >
k; para `< k`, apenas se limite superior < k. Intervalo que cruza o
limiar produz `INDETERMINATE_PRECISION`." `guest/src/stats.rs`'s
`evaluate_claim` implements exactly this: a threshold that falls
inside the certified `[lower, upper]` bracket is neither approved nor
rejected — it's reported as indeterminate, rather than the guest
guessing.

## Certified bracket arithmetic (`guest/src/bracket.rs`)

No `f64` anywhere. Every irrational step (a square root) is bracketed
by an exact integer floor/ceil pair via Newton's-method integer square
root, and every division is bracketed the same way; multiplying or
combining two brackets always rounds outward (floor for the new lower
bound, ceil for the new upper bound), so the result is guaranteed to
still contain the true value.

**Sharpe and Sortino** need exactly one square root each (the sample
or downside standard deviation), so their brackets are directly
certified this way.

**CAGR** is the hard case: `CAGR = ratio^(365/days_elapsed) - 1`. The
naive way to verify this — clearing the fraction and comparing
`(1+CAGR)^days_elapsed` against `ratio^365` — requires raising one
side to an exponent in the hundreds or thousands, which overflows any
fixed-width integer for realistic multi-year returns long before it
says anything about the bounded, modest CAGR value itself. Instead,
`cagr_bracket` expands the exponent `365/days_elapsed` (always in
`(0, 1]`, since `days_elapsed >= 365` is required) into its binary
fraction and multiplies together the iterated-square-root terms of
`ratio` selected by each set bit: `ratio^(b_1/2 + b_2/4 + ...)`. Every
factor in that product is between `min(1, ratio)` and `max(1, ratio)`,
so the computation stays bounded regardless of how extreme `ratio`
is — confirmed by a test with a 1000x return over a decade. 40 bits of
expansion converges well past `SCALE`'s own precision floor.

## Sample size and precision,

- Sharpe and Sortino require at least 30 daily returns.
- CAGR requires at least 365 complete, gap-free days and a strictly
  positive index at both ends.
- Sharpe refuses (rather than publishing infinity) when the sample has
  zero variance; Sortino refuses when the sample has no downside
  relative to the MAR.

These are the same minimums and refusal conditions
`crates/domain/metrics` already enforces for the plain (non-proven)
point-value computation (see that crate's `stats.rs`/`cagr.rs`); this
guest re-implements them independently in fixed-point (`i128`, scale
`10^12`) because the guest is its own isolated Cargo workspace and
cannot depend on `rust_decimal`-based crates across that boundary —
the same cross-workspace constraint documented in the performance and TLS composition guests.

## Tests

- `guest/tests/run_test.rs` (native, no proving): sample-size and
  precision refusals for all three metrics; a clean approval and a
  clean rejection; a threshold that straddles the bracket reports
  `IndeterminatePrecision`; a 1000x-over-a-decade CAGR resolves without
  overflow.
- `tests/guest_test.rs` (host, real zkVM):
  `a_real_proof_of_an_approved_sharpe_claim_earns_the_verdict_for_real`
  is `#[ignore]`d (real proving needs more free RAM than this machine
  reliably has alongside a normal desktop session); run manually with
  `cargo test -p zkvm-methods-statistics --test guest_test -- --ignored --test-threads=1`
  when the machine has headroom. It has been run manually and passed.
  The remaining `execution_*` tests cover the same guest logic via the
  fast executor, without proving.

## Out of scope

This delivery does not implement genuine certified-interval CAGR for
every conceivable input at the spec's stated `<= 10^-10` width bound —
the 40-bit expansion is empirically well past that floor for realistic
inputs, but is not separately proven analytically tight in every case.
This mirrors `crates/domain/metrics::cagr`'s own documented limitation,
extended here with a real (not `f64`, not black-box) bracket rather
than a bare point value.

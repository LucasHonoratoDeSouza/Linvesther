# Risk and capital metrics

Per the protocol specification's metrics table:

- `drawdown.rs`: max drawdown (`1 - I_t/max_(u<=t) I_u`) and recovery
  time. Ties resolve to the earliest peak, then earliest trough, by
  construction (the running peak only advances on a strictly greater
  index, and the best magnitude only replaces on a strictly greater one)
  — not a secondary comparison bolted on afterward. The named vector
  (index `100, 120, 90, 108` → MDD 25%, unrecovered) is a test, asserted
  exactly.
- `stats.rs`: mean, sample variance, annualized volatility, Sharpe,
  Sortino. Volatility/Sharpe/Sortino all enforce the spec's 30-day
  minimum sample; Sharpe on a zero-variance series and Sortino with no
  downside both return a typed reason (`ZeroVariance`/`NoDownside`), never
  an infinite or fabricated value. `rf`/MAR are explicit parameters (zero
  in the reference profile, but never hardcoded as zero inside the
  function).
- `cagr.rs`: `(I_end/I_start)^(365/d)-1`, enforcing the 365-day minimum
  and strictly-positive index endpoints. **Known limitation**: computes a
  single point value via `rust_decimal`'s `checked_powd`, not the spec's
  required certified interval of width `<= 10^-10` from a rigorous
  deterministic root-finding method — implementing genuine certified
  interval arithmetic for fractional exponentiation is separate,
  substantial work not included in this delivery. Documented in the
  module itself, not silently presented as spec-complete.
- `capital.rs`: current capital (NAV + age) and sustained capital (the
  minimum across daily closes **and the baseline**, never an intraday
  minimum). Consistency counts only strictly-positive periods ("zero não
  é positivo") and returns `None` rather than a fabricated ratio when
  there are zero complete periods.

No `f64` anywhere; all arithmetic is `rust_decimal`, matching the
protocol's precision rules.

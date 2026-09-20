# Time-weighted return index

`r_j = Vend_j/Vstart_j - 1; I_0 = 1; I_j = I_(j-1) × (1+r_j)` per
the protocol specification. `compute_twr` consumes a chronological
stream of `Valuation`/`Flow` events for **one continuous segment** — a
gap or a total loss (NAV hitting zero) is represented by starting a fresh
call, never by a parameter that could chain a previous segment's index
in. See `src/twr.rs`'s module doc for exactly how each event kind affects
the running NAV and when a subperiod is (and is not) counted — the full
vs. partial withdrawal distinction and the total-loss terminal subperiod
both hinge on this.

Every one of the protocol specification's named vectors (aporte sem
lucro, lucro e aporte, retirada sem lucro, taxa, perda total intradiária,
retirada integral) is a test in `tests/twr_test.rs`, asserting the exact
value the spec states, not just "some plausible number."

# RISC Zero feasibility benchmark — measured result

Real proving run, not a projection. Command: `make check` (`BENCH_EVENTS=10000
BENCH_REPS=3`, the spec's defaults).

## Hardware

- CPU: AMD Ryzen 7 7735HS (16 logical cores), no GPU acceleration used
  (`default_prover()` on the CPU backend; no CUDA/Metal, no Bonsai remote
  proving).
- OS: Linux.
- `cargo-risczero` / `r0vm` 3.0.6, `risc0-zkvm` 3.0.6.

## Result

| Metric | Value |
| --- | --- |
| Events | 10,000 |
| Repetitions | 3 |
| Execution cycles (unproven run) | 5,974,234 |
| Proving `total_cycles` (last rep) | 6,422,528 |
| Proving `user_cycles` / `paging_cycles` / `reserved_cycles` (last rep) | 5,974,234 / 224,546 / 223,748 |
| Segments (last rep) | 7 |
| Receipt size | 1,932,174 bytes (~1.84 MiB) |
| Proving wall time — min / p50 / p95(≈max) / max | 816.9 s / 862.1 s / 967.0 s / 967.0 s |
| Peak RSS (`VmHWM`) | 15,340 kB |
| Real receipt verification | passes (`receipt.verify(METHODS_ID)`) |
| Fake receipt (genuine journal, no real proof) verification | **rejected** (`VerificationError`, non-dev-mode `VerifierContext`) |

Raw per-rep wall times: 966.96 s, 816.86 s, 862.14 s.

## What this does and does not establish

- **Established:** a 10,000-event guest workload of this shape is provable
  on ordinary CPU hardware in well under 20 minutes per proof, produces a
  receipt under 2 MiB, and a receipt that skipped real proving is reliably
  rejected — not a placeholder or a dev-mode artifact accepted by mistake.
- **Not established:** cost/throughput on GPU or Bonsai-hosted proving
  (neither was exercised); behavior of the actual reconciliation guest, which will differ in cycle cost from this synthetic fold; multi-
  tenant or concurrent proving load; cost in cloud pricing terms.
- **Sample size caveat:** `p95` here is the observed max of 3 repetitions,
  not a statistically meaningful 95th percentile. Wall time varied by about
  ±9% across reps on otherwise identical input, most plausibly from OS
  scheduling noise on a 16-core shared machine rather than from the
  workload itself (cycle counts are deterministic given fixed input).
- **Memory caveat:** the reported peak RSS (15 MB) is far lower than
  typically expected for zkVM proving at this scale and should be treated
  as unreliable rather than as evidence proving is memory-light. `/proc/
  self/status` `VmHWM` on the host process may not capture memory used by
  worker threads spawned during segment proving on this risc0-zkvm version/
  configuration; a proper measurement needs an external sampler
  (e.g. periodic `/proc/<pid>/status` polling across all threads, or a
  dedicated profiler) rather than a single end-of-run read. This is not
  fixed in this delivery — recorded here as an open gap rather than a
  number this report leans on.

## Comparison against the operations target

the operations design sets: "Prova padrão: meta
p95 ≤ 10 min para 10k eventos + estado acumulado; gate, ainda não medido."
This delivery is that first measurement, on CPU only, for a synthetic
workload rather than the actual reconciliation-plus-accumulated-state guest:

- **Measured p95 (967.0 s / 16.1 min) exceeds the 10-minute target**, by
  about 1.6x, on this CPU-only hardware.
- This does not mean the target is unreachable: the guest workload here is
  a bounded fold, not the real reconciliation guest with accumulated state
  (heavier per-event cost is plausible, but so is further optimization);
  GPU or Bonsai-hosted proving were not tried and typically reduce wall
  time substantially versus CPU; and cycle count (hence proving time)
  scales with guest instruction complexity, which the actual performance guest may
  differ from in either direction.
- **What this means for later work:** the performance claims guest and the
  TLS origin composition guest should re-measure against this same
  10-minute target once they exist, and the reference implementation
  should not assume CPU-only proving meets the operational target without
  that re-measurement. This is flagged here as an open risk carried
  forward, not resolved by this experiment.

## Cost

No cloud/hosted proving cost is measured or claimed; only local wall-clock
CPU time on the hardware above.

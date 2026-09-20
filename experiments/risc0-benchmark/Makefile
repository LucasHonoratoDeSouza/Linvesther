.PHONY: check

# BENCH_EVENTS/BENCH_REPS are forwarded to the host binary; defaults match
# the spec's 10k-event gate. Override for a fast smoke run, e.g.
# `make check BENCH_EVENTS=200 BENCH_REPS=1`.
BENCH_EVENTS ?= 10000
BENCH_REPS ?= 3

check:
	cargo test --release --manifest-path Cargo.toml -p host --test guest_fold_test
	BENCH_EVENTS=$(BENCH_EVENTS) BENCH_REPS=$(BENCH_REPS) cargo run --release --manifest-path Cargo.toml -p host

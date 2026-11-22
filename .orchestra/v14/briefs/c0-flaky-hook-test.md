# C0 — a test that fails under load is a broken test (small)
OWN: the plugin hook tests in fc-app (`looping_hooks_are_still_switched_off`, both the mem and `sqlite_file`
variants) and the code path they exercise; `crates/fc-storage/src/tests/search.rs` bench bounds.
`looping_hooks_are_still_switched_off` failed in four different workers' full runs on a loaded 4-core machine
and passes alone: it depends on wall-clock timing (200 ms / 10 s limits). `bench_search_20k_scaled` failed once
under load too (`front:kılıç*` 66 ms). Do: find what the hook test really asserts (a looping hook gets
switched off by a budget) and make it deterministic: drive the budget with the injected Clock / a fuel or
step counter instead of real time, WITHOUT weakening what it proves; if production code uses wall time where
an injected clock belongs (CLAUDE.md rule 2 spirit), fix that. For the bench: keep it as a perf guard but make
it robust (e.g. best-of-N, or compare against a calibrated baseline measured in the same process) and check
whether `front:kılıç*` got slower since v1.3.0 (`git stash`-free: build the v1.3.0 commit f5287c0 in a
separate worktree dir under /tmp if needed, or reason from the query plan) — report numbers. Prove: run the
hook tests 20× while `stress`-like load runs (e.g. 8 busy loops) — all pass. fmt, clippy, tests. Commit.

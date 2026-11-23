# I2: CI is red on the 1.4.1 merge (a059b18). Two failures to fix.

First, in your worktree: `git merge claude/festive-einstein-8y0kld` (it only adds the merge commit).

## 1. Clippy fails on fc-desktop (Linux CI job "Rust (fmt, clippy, test)")

`cargo clippy --workspace --all-targets --locked -- -D warnings` (CI includes fc-desktop, which this container cannot compile):

```
error: items after a test module
  --> apps/desktop/src-tauri/src/maintenance.rs:288:1
288 | mod tests {
364 | pub fn offer_rescue(db: &std::path::Path, error: &str) -> Result<(), String> {
408 | pub fn rescue_text(detail: &str) -> String {
```

Move the test module to the end of the file (no `#[allow]`). Then read every file under `apps/desktop/src-tauri/src` changed since tag-less commit `1e9217e` (`git diff 1e9217e -- apps/desktop/src-tauri`) with a reviewer's eye for anything else clippy or the compiler would reject there (unused imports, items after test modules, needless borrows, dead code, wrong types), since it cannot be compiled here. This was the first error clippy stopped at; there may be more behind it.

## 2. Windows job "Windows (build + test)": `perf_tests::perf_suite_scaled` fails

```
| capture (build, 1000 sent)      | p50 4.6 / p95 41.0 / max 11895.7 ms | budget p95 300 ms |
| answer while capturing (40)    | p50 27.3 / p95 156.6 / max 2314.4 ms | budget p95 400 ms |
| capture while answering (23)   | p50 54.9 / p95 831.0 / max 2360.4 ms | budget p95 600 ms |
capture p95 while answering: 831.0 ms >= budget 600 ms   (crates/fc-app/src/perf_tests.rs:873)
```

The previous commit's CI (df8c62c, after H1-H4) was red too; the last green one was 1e9217e, before the robustness round. So first find out what it is, do not just raise the budget:

- Is it a real slowdown the robustness round added to the capture or answer write path (disk-full checks, extra fsync, mirror backup work, media cleanup, WAL checkpoint changes, `.verify-*` sweeps, anything now running on the write path or holding the write lock)? Compare the write path at 1e9217e and now. Note the maxima: one capture took 11.9 s during the build and 2.3 s while answering, which looks like something long holding the writer (a backup, a checkpoint, an index rebuild), not general slowness.
- Or is it measurement noise from only 23 samples on a shared Windows runner (p95 of 23 is the second worst sample)?

If it is a real cause, fix the cause and show it with a measurement here (the scaled suite, several runs, before and after). If the product is fine and the measure is too thin, make the measure sound (more samples, or a statistic that one stall on a shared runner cannot fail) and say exactly why in `docs/12-performans.md`; the budget itself stays unless you can justify a change with numbers. Either way, say plainly in the report which of the two it was and what evidence you have.

## Checks

fmt, clippy (`--workspace --exclude fc-desktop --all-targets`), `cargo test -p fc-app --lib perf` and whatever tests your changes touch, gen-ts check if the API changed. Commit on `v14/i1`, no push. Report in Turkish: what each failure was, what changed, what you ran, and what only Windows CI can confirm.

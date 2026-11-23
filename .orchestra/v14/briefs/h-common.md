# H-workers — sağlamlık turu (v1.4.1): "veri asla kaybolmaz" kanıtı

Read `.orchestra/v14/COMMON.md` first; it binds you. An independent reviewer (GPT) read the v1.4.0 code and
narrowed the plan to this: prove that the owner's cards and study history come back after a failure, on
Windows. The scheduler is already FSRS-6 (`fsrs-rs`) with per-preset optimisation: do not replace it.
Rules for this round:
- Tests must pass on Windows CI too (no Unix-only paths, no wall-clock assumptions, files closed before delete).
- A fuzz/property test that finds a bug: fix the bug, keep the minimal failing case as a regression test.
- Bounded run time in the default suite (each new property test ≤ ~10 s in debug); a longer soak behind an env var.
- No UI redesign. If a capability needs a setting or a line of UI text, add it in the existing page (TR/EN).
- Do not bump the version. CHANGELOG: lines under "Unreleased (v1.4.1)". Three siblings share the 4-core machine: `-j 2`.
- Usual full checks before the final commit; commit on your branch, no push. Report in Turkish, short, table of findings (what broke → fix).

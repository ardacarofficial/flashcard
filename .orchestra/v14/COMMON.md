# v1.4 — common rules for every worker

Read `CLAUDE.md`, then `.orchestra/v13/COMMON.md` (its Hard rules, Environment and Report sections all still
apply, with "v1.3" read as "v1.4"), then this file, then your brief. `.orchestra/v14/gpt-backend-review.md` is
an independent review of the backend with file references: leads to verify, not truth.

## Why v1.4 exists
The owner (only user; learns English from films/series, cards come from his Subtitle browser extension) used
v1.3: "the core seems solid, but using it still confuses me". v1.4 has two tracks running in parallel:
- BACKEND (A-workers): make the core the best foundation for a vocabulary app: data that cannot be lost,
  one truth for "which words does he know", measured speed. No UI work beyond what a brief names.
- DESIGN (D-workers): clickable prototypes FIRST (stand-alone HTML, no app code), which the owner approves
  before anything is built in the app.

## Deltas from v1.3 rules
- Migration range for v1.4: 1400–1499; your brief names your sub-range.
- Every new capability is behind an existing or new module/setting so it can be switched off (CLAUDE.md rule 7).
- Acceptance for backend work = Rust tests + (where the app's API is the surface) a check through `fc call`
  or the dev bridge that reads the PERSISTED result back. State what you ran.
- Commits end with exactly:
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
  `Claude-Session: https://claude.ai/code/session_015HsE62NZHWhisCVzcargRj`
- Docs: update the docs section that describes what you changed (docs/02, 03, 06, 10) and add a line under an
  "Unreleased (v1.4)" heading at the top of CHANGELOG.md (create the heading if missing; one line per worker,
  append only, to keep merges trivial). Do not bump the version.
- Time: you have about 110 minutes. Commit working increments as you go; if you run out, a committed, tested
  part plus an honest "unfinished" list beats an uncommitted whole.

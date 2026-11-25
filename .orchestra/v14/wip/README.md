# Unfinished v1.4.2 work (J round)

The four workers were cut by the second account's weekly limit on 2026-11-25 before they finished or reported.
Each patch is the worker's whole diff against `9550fee` (committed and uncommitted work together), kept here so it
survives the container. Nothing in them is verified or merged. Briefs: `../briefs/j*.md`.

| Patch | Brief | State when cut |
|---|---|---|
| `j1.patch` | j1-independent-review | no commit; changes in fc-storage rescue and content writes, new rescue tests |
| `j2.patch` | j2-windows-e2e-and-ci-gates | 2 commits: Windows journeys J4–J7, Subtitle check; no report, never run on CI |
| `j3.patch` | j3-anki-compat-and-speed | 1 commit: manual reschedules, wider fixtures; speed bench script half written |
| `j4.patch` | j4-windows-offline-tts | no commit; provider work in progress |

To continue: `git worktree add <dir> 9550fee && git -C <dir> apply --index ../path/jN.patch`.

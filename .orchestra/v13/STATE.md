# Orchestra state — v1.3 "usable"

Goal (user, /goal): orchestrator only manages; coding is done by agents on the SECOND Claude account
(`ACC2_OAUTH_TOKEN`), in parallel, until done. Plan agreed with user after a two-round debate with GPT 6.2 Astra
(`gpt-review-1.md`, `gpt-review-2.md`). Rules: COMMON.md. Wire contract: CONTRACT.md. Briefs: briefs/.

Workers are headless `claude -p --model opus --effort high --permission-mode auto` processes started by
`<scratchpad>/v13/run-worker.sh <name> <brief>` (scratchpad = /tmp/claude-0/-home-user/9995e59c-…/scratchpad/v13;
logs in `logs/<name>.jsonl`, session id inside; resume with args 4–5). Each has worktree
`.claude/worktrees/v13-<name>` on local branch `v13/<name>` (never pushed). `--dangerously-skip-permissions` is
refused by the safety layer: use `--permission-mode auto`.

| Worker | Area | Status |
|---|---|---|
| b0 | dev bridge (real Rust core in browser) + journeys | running |
| r1 | study backend: study.options, scopes, truthful counts, resume | running |
| r2 | vocabulary: field roles, answer_field, context hint, card backs, context kept | running |
| f1 | study launcher dialog + one-key-one-job keyboard model + plain words | MERGED (ed3209b). After r1 merge: bind `features/study/contract.ts` to generated types, drop `as never` casts. Leftovers: key repeat in BrowseScreen/TermList; 'zor kart/unutma' wording in stats/browse; S3 'Şimdi çalış' → launcher |
| s1 | AI provider presets (OpenRouter…), model list, test connection | MERGED (23d8f99). Open: `plugins/redact.rs` classifies new `ai.test` job as default IO (closed) — move to AI area; external links in Tauri may need opener; real keys never tested |
| s2 | settings kit that explains, note types, MCP, automation recipes | running |
| s3 | add cards in one place: single/bulk/paste/file; first-run guide | MERGED (4d2105f). Notes: "Şimdi çalış" goes to /study (wire to launcher after f1); S4 help should call openGuide() |
| s4 | stats three questions, planning forecast model, `?` help sheet | MERGED (6180fb2). Open: 'Bunları çalış' starts a session then /study only offers Continue → hand over to F1 launcher; term/meaning picked by fixed keys → switch to R2 roles; `T::start` test helper needs `scope` after R1; help not a module |

After they report: merge in order r2 → r1 → f1 → b0 → s1 → s2 → s3 → s4 (regenerate TS types after Rust merges),
run full checks, then a journeys worker (launcher/keyboard/vocab journeys on the bridge; J5/J6 must turn green),
independent review + polish, design consistency pass (bounded), docs/CHANGELOG/version 1.3.0, Windows installer
via `.github/workflows/windows-build.yml`, try Windows WebDriver pilot (tauri-driver) as a non-gating job.

Note: first runs of r1/r2/b0/s2/s4 hit the 7000 s wall-clock (machine load 30); all resumed with `followup-timeout.md` (logs `<name>-HHMMSS.jsonl`). Free disk by deleting `target/` of merged, clean worktrees.

## Integration (2026-11-22, main checkout, branch claude/festive-einstein-8y0kld)
All eight workers merged (s2 last: b73b98a). Integration commits after it: study launcher bound to the
generated types (contract.ts re-exports, mock = r1 semantics, test drafts), Study now / Study these open the
launcher, keyboard safety (`$lib/keyboard`, J7 green), plugin redaction + reminder queue counts, field roles
in the UI, stats term/meaning by role, Esc = pause (Continue resumes the same session), launcher reason
"today's card limit", journeys S1–S6 (+ S7–S12 by a journeys sub-agent), screens.mjs `state-missing`.
Known: the core's 20-min learn-ahead means a 1 m learning step returns at once (S6 uses a 25 m step).

## After integration (14:55)
All 8 workers merged; integration worker (session e0efb3ea-3ff1-4131-937c-e3a646169af7) wired everything and ran it
on the real core: cargo test 1529 pass, pnpm test 504/504, e2e 22 journeys pass (S7c red on purpose), screens
--strict 672 PNG 0 findings. Windows CI build of 9dac3e4 succeeded (ipc.rs refactor compiles on Windows).
Decisions taken by orchestrator: (1) collection queue must honour per-deck presets; (2) hint rule = blank exactly
the answer if it occurs in the example, else show the example as context; forward vocab/subtitle front shows the
word only; (3) learn-ahead stays but truthful + a setting.
Final wave running (worktrees v13-x1/x2/x3): x1 core fixes (above three), x2 owner's-eyes review + polish (frontend
only; writes `.orchestra/v13/owner-walkthrough.md`), x3 docs + version 1.3.0 + non-gating Windows WebDriver pilot.
Then: merge x1 → x2 → x3, full checks + e2e myself, look at screens, push, fetch Windows installer artifact, final
report to user (open: real keys for AI never tested; Windows window manual run; fc.exe not in installer for MCP).

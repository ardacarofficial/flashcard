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

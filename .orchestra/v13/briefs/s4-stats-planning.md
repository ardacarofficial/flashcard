# S4 — statistics that tell you what to do, planning you can reason about, and help
Owner: "Statistics look fine but could be presented better; planning could be arranged better." Reviewer:
Statistics offers forecasts and difficult words whose actions are suspend/tag; Planning mixes exams, balancing
and optimizer controls; its current estimate varies retention — a new-cards-per-day forecast needs its own Rust
model (`apps/desktop/src/lib/features/stats/PlanningScreen.svelte` ~112).

OWN: `apps/desktop/src/lib/features/stats/**` + `stats.json`, `crates/fc-app/src/modules/stats/**`, `planning.rs`,
`goals.rs`, `optimizer.rs`, `crates/fc-api/src/v1/{stats,planning}.rs` (+ additive method.rs), generated TS types,
a new `apps/desktop/src/lib/features/help/**` (shortcuts sheet), migrations 1360–1369 if needed.
LEAVE-ALONE: study/home/decks screens and the shortcut definitions themselves (F1 is changing the key model right
now: your help sheet must READ the shortcut table at runtime from the same source the Shortcuts screen uses, not
copy it), settings (S2), add/import/onboarding (S3), ai (S1), Rust study session/queue (R1) — you may call
`study.start`/`study.options` per CONTRACT.md §1 but not change them.

Deliver
1. **Statistics → three questions, in this order** on the overview:
   (a) "Bugün nasılım?" — today's answered / correct rate / time, streak, goal progress; one line of plain
       interpretation ("Dün ile aynı tempodasın").
   (b) "Önümde ne var?" — next 7 days of due load as a small bar chart with today highlighted and the heaviest
       day called out; 30-day view one click away.
   (c) "Neyi unutuyorum?" — the words you got wrong most recently/often as a dense list (term, meaning, misses,
       last seen) whose PRIMARY action is "Bunları çalış" (starts a practice session with those cards — use the
       `struggled_today` scope when it is about today, or a custom filter for the longer window; if the launcher
       dialog from F1 exists use it, otherwise call `study.start` directly with a practice session) — suspend/tag
       stay as secondary row actions.
   Everything the current overview shows (heatmap, memory states, success rate by maturity, breakdowns by
   classification/deck, time-of-day, deck reports, leeches) remains, below, under clear headings or tabs, with a
   one-line "bu ne anlama geliyor" per chart. Charts follow sound practice: direct labels, mono axes, colour not
   the only signal, accessible table alternative.
2. **Planning you can reason about**: lead with two controls and a live consequence — "Günde kaç yeni kart?" and
   optional "Sınav tarihim" — and a forecast of daily workload (cards/day and minutes/day) for the next weeks that
   updates as the controls move, plus a sentence ("Bu tempoyla 3 hafta sonra günde ~85 kart, ~14 dk"). This needs
   a real forecast model in Rust (pure, in the planning module, driven by the FSRS parameters and current card
   states; deterministic via `Clock`/`Rng`); write it with tests (monotonic in new-cards/day; conserves cards;
   matches a hand-computed tiny case) and expose it as a query. Applying a change is an undoable command and says
   which decks/presets it will touch. Existing exam plans, load balancing and optimizer controls stay, explained,
   the technical ones under "Gelişmiş".
3. **Goals**: creating a goal is one sentence with blanks ("Her gün __ kart" / "__ tarihine kadar __ desteyi
   bitir"), progress shown the same way on Home's existing goal tile data (do not edit Home; keep the API shape).
4. **Help**: `?` (when not typing) opens a shortcuts sheet grouped by context (everywhere / studying / lists /
   editor), generated from the live shortcut table, searchable, with a link to the shortcut editor; also reachable
   from the palette and a small "?" in the top bar or sidebar footer (smallest shell edit, listed in your report).
Verify in a browser (mock) in TR/EN, dark/light, 1280/900. Tests for the forecast model (Rust), the overview's
primary actions, and the help sheet reflecting a changed shortcut.

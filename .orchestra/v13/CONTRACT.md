# v1.3 cross-worker contract (wire shapes workers code against in parallel)

Rust workers implement these; frontend workers consume them (and extend the mock transport to match). Names are
binding. If something here turns out to be impossible, implement the closest thing and say so loudly in your
report — do not silently change a shape another worker depends on.

## 1. Study launcher  (owner: R1; consumer: F1)

`study.options` — query, no side effects.
Request `StudyOptionsRequest { deck_id: DeckId | null, include_subdecks: bool | null }`
Response `StudyOptions {
  deck_id: DeckId | null,
  deck_name: string | null,
  resumable: ResumableSession[],          // unfinished sessions relevant to this deck (all kinds), newest first
  scopes: StudyScopeOption[],             // always all five scopes, in the order below
  modes: StudyKind[]                      // kinds available now: subset of normal, learn, test, match (module switches)
}`
`ResumableSession { session: SessionDto, deck_name: string | null, answered: number, remaining: number }`
`StudyScopeOption {
  scope: StudyScope,
  count: number,                          // EXACTLY how many cards a session started now with this scope would contain
  mode: StudyMode,                        // scheduled for due/new; trial (practice, schedule untouched) for the rest
  empty_reason: StudyEmptyReason | null,  // set when count == 0
  next_available_at: Timestamp | null     // when empty because cards are waiting / limits reset
}`
`StudyScope` (snake_case): `due` (today's queue exactly as a normal session would build it: reviews, learning
and new cards within limits) · `new` (unseen cards only, within the new-card limit) · `studied_today` (distinct
cards answered during the current local study day, any mode) · `struggled_today` (distinct cards rated Again /
answered wrong during the current local study day) · `all` (every card in the deck).
`StudyEmptyReason` (snake_case): `no_cards` · `nothing_due` · `daily_limit_reached` · `waiting` (learning cards
not yet due; see next_available_at) · `none_studied_today` · `none_struggled_today`.

`study.start` (existing `StartSession`) gains `scope: StudyScope | null` (null = today's behaviour for that
kind) and works for kinds normal, learn, test, match. Rules:
- For `studied_today` / `struggled_today` / `all` the session is practice (`mode = trial`) regardless of the
  request, and the card set is frozen at start.
- Starting when the resolved set is empty returns error code `CONFLICT_NOTHING_TO_STUDY` with details
  `{ reason: StudyEmptyReason, next_available_at: Timestamp | null }`, creates no session and leaves any
  existing unfinished session untouched.
- A session whose remaining cards are all waiting (learning steps) stays open; `SessionDto`/the current-card
  response exposes `next_due` so the UI can say "next card in N min".
- Invariant (must have a Rust test): for the same clock and deck, `study.options.scopes[due].count` equals the
  number of cards `study.start { kind: normal, scope: due }` then serves; and every "to study today" number the
  UI shows (Home headline and rows, Library, deck page) comes from this same queue logic. If `decks.tree` /
  `decks.list` currently report inventory counts under names the UI reads as "today", either make them agree
  with the queue logic or add queue-based fields named `today_due`, `today_learning`, `today_new`,
  `today_total` next to them and document it in the report.

## 2. Vocabulary fields  (owner: R2; consumers: F1 for card back + hint display, S3 for import mapping)

Note-type field gains optional `role: FieldRole | null` — what the field CONTAINS: `term` (dictionary form),
`surface` (the form as seen: "went"), `meaning`, `example` (a sentence containing the term), `example_translation`,
`source`, `note`. A card template gains optional `answer_field: string | null` — the stable field key this card
ASKS for (forward vocabulary card: the `meaning` field; reverse card: the `term` field). Both are optional
metadata in the stored JSON; old records load unchanged.
- Built-in note types (classic/basic, reversed, subtitle, …) get roles and answer fields via an idempotent data
  migration (range 1300–1329) that only touches known built-in definitions the user has not customised.
- Typed-answer / test grading uses the template's `answer_field` when present (so the expected answer for a
  subtitle card is the meaning only); otherwise existing behaviour. Cloze / choice engines keep authority.
- `notes.terms` (the deck page term list projection) returns, per row, `term_field` and `definition_field` keys
  chosen from roles (`term` → term; `meaning` → definition) and inline edits write to those keys. For types without
  roles: existing behaviour plus the response flag `ambiguous: true`, on which the UI opens the full editor instead
  of inline-editing a guess.
- Hint: when the note has an `example` field containing an occurrence of the term/surface, the first hint level is
  the example sentence with that occurrence masked (`I ___ home.`), returned as `HintDto { kind: "context", html }`;
  later levels and notes without a usable example use the existing letter hints (`kind: "letters"`). The masked
  sentence must never contain the answer elsewhere (if it does, fall back to letters).
- Built-in card backs render three labelled parts in this order when present: meaning (largest), example
  (sentence + its translation, visually separate, labelled via i18n key `cards.part.example`), source (smallest,
  `cards.part.source`). Custom user templates are never rewritten.
- Captured context is not lost: inbox acceptance and CSV/paste import can map a sentence/context column into the
  `example` role field of the chosen note type (API: import mapping accepts role names as targets).

## 3. Dev bridge  (owner: B0; consumers: everyone's verification, later the journeys worker)

`fc dev serve --db <file> [--port N] [--token T]` (CLI subcommand, compiled only with the `dev-bridge` cargo
feature or `debug_assertions`; never in release builds): loopback-only HTTP that forwards
`POST /rpc { method, params }` to the SAME dispatch the desktop shell uses (envelope, error serialization, lock
gating) and streams events (SSE `GET /events`). Frontend: a third transport selected by `vite --mode bridge`
(env `VITE_FC_BRIDGE_URL`, `VITE_FC_BRIDGE_TOKEN`), next to Tauri IPC and mock. Journeys live in
`apps/desktop/e2e/` (Playwright), run with `pnpm -C apps/desktop e2e` which builds `fc`, creates a temp profile,
starts bridge + vite and runs the journeys; assertions are on persisted results (rows via `fc`/RPC), not pixels.

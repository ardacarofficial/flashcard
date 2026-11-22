# A6 — release acceptance with the REAL Subtitle client: interrupted delivery and restore
OWN: `tools/subtitle-contract/**`, new integration tests under `crates/fc-http/tests` or `crates/fc-app` test
modules for the subtitle HTTP surface, the CI step that runs the contract check. LEAVE ALONE: backup internals
(A1 is rewriting them: use only the public `io.backup.*` API, and expect its streaming rewrite to land),
the word index (A3), `subtitle/capture.rs` logic unless you find a real bug (then fix it minimally and say so).

Today `tools/subtitle-contract/check.mjs` drives the extension's own `api.ts` (read-only repo
`/home/user/subtitle/src/flashcards/`) against a real `fc api serve`. Missing scenarios — add them, using the
extension's real client and its real outbox/retry code (`outbox.ts`) where possible, not a fake:
1. media uploaded, then the connection drops before the capture response is read → client retries →
   exactly one card, media attached once;
2. the response is lost after the server committed (kill the socket server-side) → retry → `duplicate`, same card;
3. the server process is killed mid-batch and restarted → the outbox drains → no duplicates, no lost captures;
4. undo of a capture, then the same capture id again → documented result, tested;
5. after all that: backup (`io.backup.create`), restore into a fresh profile, counts of notes/cards/media equal
   and one audio file byte-identical;
6. `subtitle.captures.recent` (new, see docs/06) tells the truth about each attempt above.
Fix real bugs you find in the core (smallest change, with a Rust test). Report every bug found.

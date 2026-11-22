# B4 — the word index on every surface, and the Subtitle known-words switch (wave 2; word index merged)
OWN: `crates/fc-mcp/**` and `crates/fc-app/src/modules/mcp/**` (tools), plugin host calls + `plugins-sdk`
(permissions, examples), automation (`crates/fc-app/src/modules/automation/**`), `crates/fc-cli` help for
`fc call words.*`, the known-words gate in `crates/fc-http/src/subtitle.rs` + its setting. LEAVE ALONE: query
language (B3), frequency data (B2), words module internals.

Do:
1. Owner's decision: Subtitle is only a capture tool. Sending known words back to it is a FEATURE THAT CAN BE
   SWITCHED: setting `subtitle.share_known_words`, default OFF. Off → `GET /subtitle/v1/known-words` answers
   with a documented, contract-compatible "not shared" response (check `/home/user/subtitle/docs/flashcards-api.md`
   and the extension's `api.ts` for what its client tolerates; an empty list with a flag is fine if the client
   tolerates it — prove with `node tools/subtitle-contract/check.mjs`, extend that check). `subtitle.status`
   reports the switch. TR/EN strings for the setting with a one-line plain explanation.
2. MCP tools: read-only `words_lookup`, `words_list`, `words_stats` ("which words in this text are new to
   me?", "what am I learning from Breaking Bad?"), respecting the same deck/permission scoping as other tools.
3. Plugin host: read-only words calls behind an explicit permission; a plugin limited to some decks must not
   learn words from other decks (test it). SDK example + versioned input/output samples.
4. Automation: a trigger/condition on word status change where it fits the engine (e.g. "kelime biliniyor
   olunca etiketle") with preview, only if it maps cleanly onto existing building blocks; otherwise document why not.

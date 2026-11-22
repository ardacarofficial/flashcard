# S1 — AI providers people can actually set up
Owner: "you only support Claude; you didn't add OpenRouter". Reality (gpt-review-1.md §A.2): Rust supports
Anthropic, OpenAI, OpenAI-compatible and Ollama (`crates/fc-ai/src/lib.rs`), but the UI preselects Claude, never
names OpenRouter, and a fresh OpenAI-compatible provider has `needs_key=false`, so the key field it needs is hidden
(`apps/desktop/src/lib/features/ai/ProvidersPanel.svelte` ~209).

OWN: `apps/desktop/src/lib/features/ai/**` + locale `ai.json`, `crates/fc-ai/**`, `crates/fc-app/src/modules/ai/**`,
`crates/fc-api/src/v1/ai.rs` (+ additive `method.rs`), generated TS types, `docs/06-ai-ve-entegrasyonlar.md` (AI
provider section). Migrations if needed: 1350–1359.
LEAVE-ALONE: everything else (settings kit `features/core/kit` is S2's: use it as is).

Deliver
1. **Provider presets as a building block in Rust**: a registry of named presets — Claude (Anthropic), OpenAI,
   OpenRouter, Google Gemini (OpenAI-compatible endpoint), Groq, Mistral, DeepSeek, Ollama (local), LM Studio
   (local), "Özel (OpenAI uyumlu)" — each with display name, kind (reuse the four existing adapters; do NOT add a
   separate engine per vendor), default base URL, whether a key is required, where to get a key (URL shown as a
   link), whether it is local/free, and suggested models. Exposed through the API so the UI draws from it.
   Verify base URLs / API shapes against current official docs where you can reach them; mark any you could not
   verify as unverified in the code comment and your report.
2. `needs_key` correct per preset (OpenRouter and other hosted compatible endpoints require a key; local ones don't,
   but allow an optional one). Keys stay in the OS keychain only (CLAUDE.md rule 10).
3. **Model list**: fetch available models from the provider (`/models` for OpenAI-compatible, Anthropic's models
   endpoint, Ollama tags) with a typed, cached command; searchable picker with free-text fallback.
4. **Test connection**: a command that makes one minimal request and returns a plain-language result (ok / wrong
   key / no credit / model not found / cannot reach / CORS-or-TLS) with the provider's message; shown inline.
5. **UI**: the page starts with one sentence on what AI is used for in this app and that it is optional and off
   until set up. "Sağlayıcı ekle" opens a gallery of preset cards (logo-less, name + one-line description + "ücretsiz/yerel"
   tags). Choosing one shows exactly three steps: paste key (link to where to get it) → pick model → "Bağlantıyı dene".
   Existing provider list as dense rows with status (ready / needs key / failed test), default-provider choice,
   per-feature routing and price/usage panels kept, under clear headings; advanced fields (base URL override,
   headers, timeouts) under "Gelişmiş". Mock transport updated so the flow is visible in mock mode.
6. Tests: Rust tests with the existing mock HTTP layer for model listing and test-connection outcomes per adapter;
   UI tests for the three-step add flow and the key field being shown for OpenRouter.
You cannot reach real providers with real keys here; say so, and make failures legible rather than silent.

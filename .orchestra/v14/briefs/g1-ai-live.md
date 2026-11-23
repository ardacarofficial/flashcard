# G1 — Yapay zekâyı gerçek bir sağlayıcıyla dene (OpenRouter), bulunanı düzelt

Read `.orchestra/v14/COMMON.md` first. AI in this app has never been run against a real provider.
In this container `openrouter.ai` is reachable and the outbound proxy injects a real credential for it into
every request: give the app ANY dummy key in OpenRouter's format (e.g. `sk-or-v1-` + 64 hex zeros); never look
for, print or store a real key. Use a cheap model (list models through the app, pick a small inexpensive one;
keep total spend tiny: short prompts, a handful of calls).

Do, through the real core (`fc` CLI / dev bridge, a fresh temp profile, real HTTP, no mocks):
1. Add OpenRouter as a provider the way a user would (the ready-provider path), connection test, model list.
   If the OS keychain is unavailable in the container, use whatever test/dev key store the code already has;
   do not weaken the "keys only in the OS keychain" rule in the product.
2. Each AI capability once, reading the PERSISTED result back: card generation from a text, preview/apply,
   `ai.meaning_in_context` on a Subtitle-style note with its sentence + `ai.meaning.apply`, `ai.meaning.suggest`,
   study-time AI help, usage/cost panel numbers, and the UI path for "Bu cümledeki anlam" via one journey run
   against the live provider (not added to the default suite).
3. Failure paths against the real API: wrong model name, cancelled/timeout, malformed JSON from the model
   (does the app recover and say something a person understands, TR/EN?), rate limit.
Fix every real bug you find (Rust first; tests that reproduce with a recorded/synthetic response so CI needs no
network). Add an opt-in live check (`FC_AI_LIVE=1`, ignored by default) that repeats step 1–2 so it can be rerun.
Write `docs/13-ai-canli-deneme.md`: what was run, model, what worked, what broke and was fixed, what is still
untested (other providers). Usual checks (fmt, clippy, cargo test workspace excl. fc-desktop, gen-ts --check,
pnpm check/test, e2e). Commit on your branch (no push). Report in Turkish, short, with a table.

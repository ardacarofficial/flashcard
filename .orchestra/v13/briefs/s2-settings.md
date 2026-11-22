# S2 — settings that explain themselves: kit, note types, MCP, automation
Owner: "Settings are very confusing. Note types are extremely confusing, hard to read. MCP and Automation are not
fit for user experience. Settings should teach how to use things and what will happen, in a friendly way."
Reviewer (gpt-review-1.md §A, gpt-review-2.md §6): 16 destinations with equal weight; descriptions are tiny
(`text-xs`); note types surround a preview with field flags, template syntax and CSS; MCP mixes "connect an
assistant" with "launch external servers" and shows `/path/to/collection.db`; automation needs a trigger, a query
and actions composed by hand before anything can be previewed.

OWN: `apps/desktop/src/lib/features/core/**` (settings kit + General page), `src/lib/shell/SettingsLayout.svelte`
and the settings grouping in `src/lib/shell/navigation.ts` (settings placements only; S3 moves import entries —
coordinate by keeping your edit to grouping/labels and not touching `/io`, `/starters` placements),
`features/cards/NoteTypesScreen.svelte` (+ its helper files, not CardView/NoteEditor), `features/mcp/**`,
`features/automation/**`, `features/presets/**`, `features/reminders/**`, `features/profiles/**`,
`features/plugins/**`, `features/subtitle/**`, `features/inbox/LocalApiScreen.svelte`, their locale files, `kit.json`.
Rust only if a recipe needs a tiny additive API: `crates/fc-app/src/modules/automation/**`, `mcp/**` (additive).
LEAVE-ALONE: `features/ai` (S1), `features/io`, `notes`, `quickadd`, `onboarding` (S3), `features/stats` (S4),
study/home/decks (F1), `features/cards` other files and Rust note types (R2 adds field `role` + template
`answer_field` to the API in parallel: design the note-type editor so a "Bu alan ne içeriyor?" role picker can
drop in; if the fields are already in the generated types when you get there, wire it, otherwise leave a clearly
named TODO-free extension point and tell me).

Deliver
1. **Kit**: every settings page = title + one plain sentence saying what this is for and when you would touch it;
   sections with headings; each row's description at body size (13px) answering "what happens if I change this",
   with a concrete example where it helps ("Günde 20 yeni kart → her gün en fazla 20 yeni kelime sorulur");
   long controls stack under the label earlier (≤ 900px and for wide controls); an "Gelişmiş" disclosure per page
   that holds rarely used rows, closed by default, remembering its state; defaults marked ("varsayılan"), and a
   per-row "varsayılana dön" where the API supports it. Apply to every page you own and check the pages owned by
   others still look right (they use the kit).
2. **Information architecture**: regroup the settings sub-navigation by what the user is trying to do, most used
   first, with rarely used/technical destinations (MCP, local API, automation, plugins, maintenance, logs) under
   a clearly separate "Gelişmiş" group. Fewer, clearer top-level items; nothing removed. A search field at the top
   of settings that filters rows/pages by words (labels + descriptions) if it can be done as presentation only.
3. **Note types**: default view is for a learner, not a template author: pick a type → "Alanlar" as a simple
   reorderable list (name, what it contains, required?) on the left and a LIVE card preview (front/back with sample
   data, flip) on the right; "Yeni not türü" starts from ready-made templates (Kelime, Kelime + örnek cümle,
   Ters yönlü, Boşluk doldurma, Subtitle) with a one-line explanation each. Template HTML and CSS editors move
   under "Gelişmiş: şablon kodu", unchanged in capability. Destructive changes say exactly what will happen to
   existing cards before confirming.
4. **MCP**: two separate tasks on two tabs/sections: (a) "Asistanını bağla" — pick the client (Claude Desktop,
   Claude Code, other), get the exact snippet with THIS profile's real database path and binary path filled in
   (ask the backend for them; add a tiny query if none exists), copy button, "what the assistant will be able to
   do" list with read-only toggle, how to test it; (b) "Dış MCP sunucuları" (the app as a client) kept but
   explained and under its own heading.
5. **Automation**: start from recipes ("Sürekli unuttuğum kartları etiketle", "Biten kartları arşiv destesine
   taşı", "Yeni eklenen Subtitle kartlarına etiket ekle", … 5–6 that the existing triggers/actions can express),
   each previewing how many cards it would affect right now; a rule reads as one sentence ("… olduğunda, şu
   kartlar için, şunu yap") with each part editable in place; the raw trigger/query/action editor stays as
   "Gelişmiş". Run history readable.
6. Other pages you own: same standard (presets: explain what a preset is and which decks use it; reminders;
   profiles/lock; plugins; Subtitle; local API).
Look at every page in a browser (mock) in TR and EN at 1280 and 900 and read it as someone who has never seen the
app. Tests updated/added for the kit disclosure, the note-type default view and the recipe flow.

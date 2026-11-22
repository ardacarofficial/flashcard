# S3 — one place to add cards: single, bulk, paste, file
Owner: "Why do I go to import/export for this? There is an Add card section… it could be better organised."
Reviewer: file import exists under both Create and Settings, paste under Create, Quick Add points to bulk entry;
CSV mapping offers only front/back so the example sentence is lost.

OWN: `apps/desktop/src/lib/features/notes/**`, `features/quickadd/**`, `features/io/**`, `features/onboarding/**`
(starters + first-run guide), `features/inbox/**` EXCEPT `LocalApiScreen.svelte` (S2), their locale files, and in
`src/lib/shell/navigation.ts` ONLY the placements of create/import/export/starters/backup entries (S2 edits the
settings grouping in the same file: keep your edit minimal and list it).
LEAVE-ALONE: Rust (R2 owns import mapping in Rust and adds role-based targets per CONTRACT.md §2 — code the
mapping UI against the contract: targets are the note type's fields, shown with their role names; if the generated
types don't have it yet, keep the current mapping working and leave the role display behind a capability check),
study/home/decks (F1), settings pages (S2), ai (S1), stats (S4).

Deliver
1. **"Kart ekle" is one destination with four ways in**, presented as tabs or a clear chooser at the top of the
   Create page and reachable from the top bar button menu / palette: "Tek kart" (opens Quick Add), "Toplu"
   (the grid), "Yapıştır" (paste text → detected separator → preview → add), "Dosyadan" (CSV/TSV, Anki .apkg,
   and whatever else the importer supports today). All four share the same destination picker (deck, note type)
   and end with the same confirmation ("12 kart eklendi · Geri al · Desteyi aç · Şimdi çalış").
2. **File import is a guided 3-step flow** inside that page: choose file (drag-drop or picker) → map columns to
   fields with a live preview of the first cards as they will look (front/back), duplicates policy explained in
   plain words → import with progress and a result summary (added / updated / skipped with reasons, undo).
   Mapping pre-fills sensibly from header names and remembers the last mapping per note type.
3. **Settings → Veri** keeps export, print, backup/restore and starter decks only, each with a sentence on what it
   is for; the import entry there becomes a link to the add-cards page. AI generation stays reachable from the
   add page ("AI ile üret") when the module is on.
4. **Quick Add**: keep the fast path (N, Tab, Ctrl+Enter, stays open); add a visible way to switch to Toplu /
   Yapıştır / Dosyadan carrying the chosen deck; when the note type has an example field, show it as an optional
   third input so a sentence can be captured with the word; duplicates explained inline.
5. **Inbox** (suggested cards from outside): each item shows where it came from and its captured sentence; accept
   keeps the sentence (R2 makes the backend keep it); when no deck is set, the row asks for one instead of failing.
6. **First run** (`?mock=fresh`): a short, skippable, friendly 4-step guide — add your first words (or load a
   starter deck), study them, see where today's cards and progress live, optional: connect the browser extension
   — each step actually performs/opens the thing, not a slideshow. Re-openable from Help. Empty states across your
   screens: one sentence + one action + a tiny example.
Verify in a browser (mock): paste 5 lines "word<TAB>meaning<TAB>sentence", import a small CSV, undo it. Tests for
the paste parser preview, mapping pre-fill and the shared confirmation.

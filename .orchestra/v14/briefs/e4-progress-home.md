# E4 — İlerleme (istatistik) + Ana sayfa (prototype: olustur-ara-istatistik.html, İstatistik part)
Read e-common.md first.
Own: `features/stats/**`, `features/home/**`, `features/decks/**`, `features/maintenance/**`, `features/reminders/**` (non-settings).
Build what the prototype shows for İstatistik (route id stays `stats`, shown as the prototype names it):
- The first view answers three questions in plain words (how am I doing, what is waiting, what is hard); charts and breakdowns behind "Ayrıntı"/Gelişmiş. Numbers from the single retrievability truth (stats API), never computed in Svelte.
- Hard cards: leech repair as a guided action (`stats.leech_repairs.plan/apply/list`, undoable), using typed-answer evidence the core now keeps.
- Words progress summary (from `words.stats`) linking to Kelimelerim (route id `words`, owned by E3).
- Home: the single study starter stays; show today's counts exactly as the core gives them; backup "son yedek" hint when `io.backup.status` says it is stale (link to settings; E2 owns that page).
- Library/deck page: deck language hint and unassigned-notes report where the prototype or docs/11 puts them.

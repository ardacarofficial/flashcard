# E3 — Ekle + Ara + Kelimelerim (prototype: olustur-ara-istatistik.html, Oluştur and Ara parts)
Read e-common.md first.
Own: `features/quickadd/**`, `features/notes/**`, `features/inbox/**`, `features/browse/**`, `features/subtitle/**` (non-settings), new `features/words/**`, `features/cards/NoteEdit*`/`NoteEditor*`.
Build what the prototype shows for Oluştur (Ekle) and Ara, plus the word surfaces the backend now has:
- Ekle: the one place to add cards, as the prototype lays it out; empty state teaches.
- Ara: plain search first, filters as chips that write the query for you (the new predicates: durum, dil, kelime, karşılaşma, nereden, kaynak, ses, görsel, sıklık, cefr); the query language stays for those who want it (help sheet).
- Kelimelerim (route id `words`; E2 wires the nav entry): `words.list/stats/lookup`, status (new/learning/known), frequency and CEFR band, manual mark, per-word encounters.
- On a note/card: its encounters ("bu kelimeyi nerede gördün": list, set primary, move, merge with `force` confirmation; `subtitle.encounters.*`), best example suggestion (`subtitle.encounters.rank/suggest_primary`), missing audio fill (`media.audio.fill_preview/fill`, origin badge özgün/klip/tts/dosya), accepted answers editor.
- Browser paging: if you can replace offset paging by keyset paging in the API without breaking the contract (versioned addition), do it; perf budget "browser page" is 100 ms at full scale and sits at ~97 (docs/12).

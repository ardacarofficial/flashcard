# v1.3 owner walkthrough (X2)

The ten jobs of `briefs/x2-owner-review.md`, done in the REAL app: `fc dev serve` (debug `fc` from
the integration build) on temp profiles (one empty, one seeded with `e2e/seed.mjs`: İngilizce 30
Classic, Subtitle 10 captures), `vite --mode bridge`, headless Chromium driven by Playwright,
Turkish, dark theme, 1280 and 900 wide (light theme spot checks). Screenshots were looked at
after every step. "Before" is the integrated v1.3 tree at `34477a3`; "after" is branch `v13/x2`.

Keys are written as pressed. `→ hesitation` marks where the owner would stop and guess.

## 1. First run: from nothing to studying the first 5 words

Before: open → guide dialog (focus on "İngilizce") → `Enter` ("Kendi kelimelerimi ekleyeceğim")
→ Create page, **focus on `<body>`** → 21 × `Tab` to reach the first term cell → hesitation.
The checklist already ticked step 1 "İlk kelimelerini ekle" (only an empty deck existed), and
the checklist panel sat over the page's save button (`Oluştur Ctrl↵`) at 1280×820. When Create
was opened from the sidebar instead, the destination was **"Yeni deste"** although a deck
existed: `Ctrl+Enter` → "Yeni desteye bir başlık ver."; switching to "Mevcut deste" showed
"Bir deste seç" with one deck in the collection, and `Ctrl+Enter` then silently did nothing.

After: `Enter` → Create page, **cursor in term 1**, checklist folded to its small button →
`apple Tab elma Tab book Tab kitap … Enter` (new row) … `Ctrl+Enter` → "5 kart eklendi · Geri al
· Desteyi aç · Şimdi çalış"; the checklist shows step 1 done (1/4) only now → "Şimdi çalış" (or
the checklist's "Çalış") → launcher → `Enter` → first card. Create opened any other way goes to
the selected deck (else the first one); "Yeni deste" is an explicit choice. 1 + 10 + 1 key
presses to the first card, no mouse.

## 2. Add one word with a sentence (N); 10 by paste; a CSV

Before: `N` → Quick Add on Classic (Terim / Tanım): nowhere to put the sentence → hesitation
(which of 12 note types has a sentence box?).
After: Quick Add on a type without an example field says "Örnek cümle de yazmak mı istiyorsun?
“Kelime kartı” türünde kelime, anlam ve cümle ayrı kutulardır." with a one-click
"“Kelime kartı” kullan". Paste (`word⇥meaning⇥sentence`, S9) and CSV (J4) were already fine;
they start on the same destination as the grid, so they too land in the deck you are in.

## 3. "I want to study English" — Cards, 10-question Test, Learn, Match; leave and come back

Before:
- Home / Library deck rows: **"Çalış" only on hover** (keyboard focus showed it, the eye did not).
- Deck page → `Test` → launcher → `Tab Tab 10 Enter` → paper. Answering by keyboard: focus stays
  on the option just picked, no A–D keys, 2 Tabs per question; true/false said
  **"Cevap: para"** as if it were the key.
- Deck page → `Öğren` with an unfinished Test of the deck: the launcher focused **"Devam et ·
  Test"** → `Enter` resumed the Test instead of starting Learn → hesitation.
- Cards → `Esc` (pause) → summary focus on **"Tamam"**, not on the amber "Devam et"; `Enter` =
  "Tamam" → an empty "Çalış / Çalışmayı seç" page with focus on `<body>` (keyboard dead end).
- Match: works with the mouse/Tab; Subtitle tiles read "bring He brought his sister to the
  party." (see Rust note below).
After:
- Deck rows on Home and in the Library carry a visible "Çalış" button.
- Test: `A`–`D` (or `1`–`4`) pick and move to the next question, `Enter` in a written answer
  moves on, the last answer lands on "Teslim et", `Ctrl+Enter` hands it in; arrows move between
  options; the footer says so; true/false reads "Önerilen cevap: para".
- The launcher focuses an unfinished session only of the mode it was opened for (Home's Study
  still puts any "Continue" first).
- Pause: focus on "Devam et" (`Enter` resumes the same session); "Tamam" goes back to the deck
  page (deck session) or Home (all decks) and leaves the paused session open for "Continue";
  `Esc` before any answer goes back the same way. An empty mode page focuses its one button.
- Found by journey O2: `Esc` pressed right after a grade (while the answer was still being
  saved) was silently dropped — the owner presses Esc and nothing happens. It now pauses as soon
  as the save is done.

## 4. Study again what you got wrong today; practise what Statistics says you forget

Cards session, rate three cards `1` → launcher "Bugün zorlandıklarım 3 · pratik · takvimi
değiştirmez" → `Enter`. Statistics › "Nelerde zorlanıyorum?" → "Bunları çalış · 3". Both were
already right (S4). Only wording: "Bugün “Tekrar” dediğin…" stays (it quotes the button).

## 5. Find a card with a typo and fix it

`Ctrl+K`/Ara → type → `↓ Enter` → edit → `Ctrl+Enter` (J3); deck page term list inline edit
writes the meaning field (S8). No hesitation. Column "Vade" is now "Tekrar tarihi".

## 6. Progress and pace

Statistics opens with "Bugün nasılım? / Önümde ne var? / Nelerde zorlanıyorum?" and an action
each. Planning: "Günde kaç yeni kart?" chips with the forecast. Hesitation (not fixed, Rust
model): on the seeded profile Planning says "Bugün ~52 kart" while Home says 20 — the forecast
does not apply the daily limits (its footnote says so), so the two numbers look contradictory.

## 7. Connect an AI provider; where is AI used

Before: Oluştur › Yapay zeka opened on **"Önce bir deste seç."** under tabs mixing "Kart üret",
"Sağlayıcılar", "Kullanım"; the same page (with a deck picker) was Settings › Yapay zeka.
After: Oluştur › Yapay zeka = "Yapay zekayla kart üret" only: the deck is a field of the form
("Kartlar hangi desteye?", preselected), and with no provider a note "Kart üretmek için önce bir
yapay zeka sağlayıcısı bağla" + "Sağlayıcı bağla" opens Settings › Yapay zeka. Settings ›
Yapay zeka = providers + usage, with a subtitle saying where AI is used and a "Kart üret" link
back. Provider: "Sağlayıcı ekle" → OpenRouter → key `sk-or-v1-fake…` → "Devam" → model list:
"Liste alınamadı: Hizmet kullanılamıyor. (… invalid peer certificate …)" here (the container's
proxy; on Windows this is the provider's 401) — legible, and "Model adını elle yazabilirsin" →
"Bağlantıyı dene" → "Bitti".

## 8. Note type from a template; what each field is for; which field is the answer

Settings › Not türleri: built-ins as tiles, "Yeni not türü" from a starting point, each field
has "Ne içeriyor?" (role) and each card "asks for" a field (S10). Understandable; not changed.

## 9. MCP; automation from a recipe

Asistanlar (MCP): pick Claude Desktop / Claude Code / other, three numbered steps, copy block
(outside the desktop app the collection path is a placeholder and the page says so).
Otomasyon: click "Sürekli unuttuğum kartları etiketle" → filled sentence editor with a live
preview ("Şu an hiçbir kart değişmez") → "Kuralı kaydet" → rule stored (`automation.rules.list`
read back). Wording now says "kenara koy" for suspend (see below).

## 10. Five minutes keyboard only; `?`

Before: `?` listed the global table only (Ctrl+K, Ctrl+N, study keys…) on every page; Home's
`Enter`, the Library's arrows, Create's `Ctrl+Enter`, the test paper's keys were nowhere.
After: `?` starts with **"Bu sayfada"** — the keys the page itself handles (Home: Enter start,
N add; Library: ↑↓, →←, Enter, N; deck page: Enter study, Space/←/→ preview, N; Create grid:
Tab, Enter new row, Ctrl+Enter save; test paper: A–D, Enter, Ctrl+Enter, Esc). Focus was
visible everywhere I went (amber ring); the Create page now starts in its first cell.

## One word per concept (TR / EN)

| Concept | Before (mixed) | After |
|---|---|---|
| card due for review | Vadeli · vadesi gelen · Sırası gelen · tekrarı gelecek | **Sırası gelen** (date column: Tekrar tarihi) / Due |
| suspend | Durdur · Askıya al · Askıda · durduruldu; EN Pause = also session pause | **Kenara koy / Kenarda** / **Set aside** |
| bury | Yarına bırak · Göm · Gömülü · bugünlük göm | **Yarına bırak / Yarına bırakıldı** / Leave for tomorrow |
| session pause | Ara ver / Pause | unchanged (now the only "Pause" in EN) |

## Not fixed here (Rust / other lanes) — reproductions

1. **Match tiles of Subtitle cards show the sentence** ("bring He brought his sister to the
   party."): `crates/fc-app/src/modules/modes/matching.rs` builds the term from
   `html_to_text(question)`; it should use the note's `term` role field (as `notes.terms` does).
   Repro: seeded profile → Library › Subtitle → Eşleştir → Başla.
2. **Planning's "today" ≠ Home's "today"** (52 vs 20 on the seeded profile after a few answers):
   the forecast model ignores daily limits; at least say it next to the number, or cap day 1.
3. The built-in Subtitle back shows the part of speech ("verb") in the `fc-pron` div right
   under the meaning; the study face now styles it small, but the template could mark it.

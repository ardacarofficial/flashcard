# D1 — study modes prototype → `prototypes/v14/modlar.html`
Read `.orchestra/v14/briefs/d-common.md` first.
Scope: the launcher ("Çalış") and the four ways of studying, clickable end to end with ~8 sample cards.
- **Kartlar**: flip + grade (as v1.3; keep).
- **Test** = what the owner expects: one word, FOUR options, pick with 1–4 or click; the right answer is shown
  at once (green/red, the correct one marked), then next. End: score + the ones he missed + "Yanlışları tekrar çalış".
- **Öğren**: today it is "multiple choice first, then typing until learned". Decide how it differs clearly from
  Test in one sentence the user reads in the launcher (e.g. Öğren = adım adım: önce seç, sonra yaz; kart
  öğrenilene kadar geri gelir). If you think it should merge into something else, show that as variant B.
- **Sınav** (today's "Test": a paper with mixed question types and a score at the end): question types are
  chosen by the user with checkboxes (çoktan seçmeli, yazarak, eşleştirme, doğru/yanlış); doğru/yanlış is OFF by
  default; number of questions.
- **Eşleştir**: tiles word ↔ meaning.
The launcher must make the choice obvious: each mode = icon, name, ONE line "ne yapar", and whether it changes
the schedule ("takvimi etkiler" / "pratik"). Scope choices from v1.3 stay (Bugünün kartları, Yeni kartlar,
Bugün çalıştıklarım, Bugün zorlandıklarım, Tüm kartlar) but consider progressive disclosure: default is
"Bugünün kartları", the rest one click away.

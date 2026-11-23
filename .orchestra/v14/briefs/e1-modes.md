# E1 — Çalışma modları ekranları (prototype: modlar.html)
Read e-common.md first.
Own: `apps/desktop/src/lib/features/modes/**`, `features/study/**`, `features/cards/CardView*` only where a mode needs it; journeys for study/modes.
Build what `prototypes/v14/modlar.html` shows:
- Mode picker with names that mean what people expect: **Test** = four options, pick one, the right answer shows at once (backend kind `quiz`, `modes.quiz.answer/result`); **Sınav** = the old exam with selectable question types (true/false off by default, matching available); Kartlar, Öğren, the rest as the prototype places them. One line under each mode says what it does.
- Test and Sınav screens, results screens, "does this change my schedule?" shown plainly (`study.options.modes[].affects_schedule`, `quiz.affects_schedule`).
- One key = one job (Space flips only; digits pick/grade); typed answers go to the API (kept by the core), accepted answers count.
- AI "meaning in this sentence" in study where the prototype shows it (`ai.meaning_in_context`, `ai.meaning.apply`), only when the AI module is on and a provider is set.

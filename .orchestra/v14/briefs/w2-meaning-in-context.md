# W2 — AI where it is worth it: the meaning IN THIS SENTENCE, and accepted answers
OWN: `crates/fc-ai/**` (a new narrow task), `crates/fc-app/src/modules/ai/**` for it, the written-answer
checker's accepted-answers data (migrations 1450–1459), docs/06. LEAVE ALONE: subtitle capture mapping (W1 is
rewriting it: integrate only through a public function you add under `modules/ai`, do not edit
`subtitle/**`), study modes (C1), providers list/presets.

Why: "pick up the kids" — which Turkish meaning fits? That is a real need and a language model is good at it.
What must NOT use AI: scheduling, exact answer checks, deciding a word is "known".
Do:
1. Task `ai.meaning_in_context`: input = sentence, term (lemma + surface + position), target language, and
   candidate meanings if a dictionary/the capture gave any; output (strict JSON, validated, retried once on
   bad JSON) = the meaning that fits this sentence, the phrase in the sentence it rests on, up to 3 other
   meanings of the word marked as "not this sentence", confidence, and whether it is an expression. Works with
   every provider preset through the existing client, incl. local (Ollama/LM Studio); budget/preview/apply
   through the existing `ai.preview/run` and suggestion-apply paths (nothing is written without the user
   applying it, or an explicit opt-in setting, default off).
2. The original definition and the AI suggestion are stored separately (never overwrite the source's meaning
   silently); applying is an undoable command.
3. Accepted answers: when the user accepts/edits a meaning, alternative correct translations become a per-note
   list the OFFLINE written-answer checker uses (so "vazgeçmek" and "bırakmak" both pass for "give up")
   without any model call at study time. Commands to add/remove them; shown through the API.
4. Tests against a fake provider (existing test server pattern): good JSON, bad JSON then good, refusal,
   timeout, provider off (clear error, no partial write), budget exceeded. No real key is available here: say so.

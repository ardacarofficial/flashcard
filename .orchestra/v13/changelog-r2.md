# v1.3 changelog draft — R2 (vocabulary correctness)

## Kullanıcıya görünenler (TR)

- **Kelime kartı artık kelime kartı gibi davranıyor.** Altyazıdan gelen "went" kartında (go → gitmek)
  yazılı cevapta ve Test modunda beklenen cevap sadece anlam: "gitmek" yazan doğru sayılır. Önceden
  örnek cümle, çevirisi ve kaynak da cevaba karışıyordu.
- **Ters kart terimi sorar.** "Klasik + ters" ve Kelime kartının "Üretme" kartı, yazılı cevapta terimi
  bekler.
- **İpucu önce cümleyi gösterir.** Notun örnek cümlesi varsa ilk ipucu, kelimenin gizlendiği cümledir
  (`I ___ home.`); sonraki ipuçları harfleri açar. Cümle zaten kartın önünde görünüyorsa ya da cevap
  cümlenin başka bir yerinde de geçiyorsa harf ipucuyla başlanır.
- **Kart arkası üç etiketli parça:** anlam (en büyük), örnek (cümle + çevirisi, ayrı ve "Örnek"
  etiketli), kaynak (en küçük, "Kaynak" etiketli). Kelime, Cümle ve Altyazı kartlarında. Kendi
  düzenlediğin şablonlara dokunulmadı.
- **Deste sayfasındaki terim listesi doğru alanı düzenler.** "Tanım" sütunu artık anlamdır (önceden
  altyazı kartlarında "geçtiği hâli" alanını değiştiriyordu). Hangi alanın terim/tanım olduğu
  bilinmeyen not tiplerinde kalem, satır içi tahmin yerine tam editörü açar.
- **Yakalanan cümle kaybolmuyor.** Tarayıcı eklentisinden gelen öğeyi kabul ederken cümle notun örnek
  alanına, sayfa kaynak alanına yazılır. Not tipi seçilmemişse ve öğe bir cümle taşıyorsa öğe, cümleyi
  tutabilen "Altyazı kartı" olur (cümle yoksa eskisi gibi Klasik).
- **İçe aktarmada cümle sütunu:** "kelime / anlam / cümle" sütunları rolü olan not tiplerine doğru
  alanlara gider; "sentence", "örnek", "definition" gibi başlıklar tanınır.

## Teknik (EN)

- `fc-core`: `FieldDef.role: Option<FieldRole>` (`term`, `surface`, `meaning`, `example`,
  `example_translation`, `source`, `note`) and `CardTemplate.answer_field: Option<FieldKey>`; both
  serde-optional (old JSON loads, unset values are not written). Validation: a role at most once per
  note type; `answer_field` must name a field. Built-ins carry roles/answer fields; vocabulary,
  sentence and subtitle backs use `.fc-part-meaning|answer`, `.fc-part-example` (+
  `.fc-example-translation`), `.fc-part-source` with empty `span.fc-part-label[data-fc-part]` labels and
  `PARTS_CSS`.
- **Migration 1300 `1300_vocabulary_roles`** (data only, no DDL; v1.3 range 1300–1399 registered in
  `RELEASE_RANGES`). For rows with a `builtin` marker: a field gets its role when key and kind are the
  shipped ones and it has no role; a template gets its v1.3 back and `answer_field` only when front,
  back and card type are exactly the shipped v1.2 values and it has no answer field; CSS is set only
  where it is still `''`. Changed rows: `version + 1`, `updated_at = now`. Idempotent; notes and cards
  untouched. Tests: `fc-storage/src/tests/vocab_roles.rs` against a fixture of every v1.2 built-in in
  both languages (`tests/fixtures/builtin_notetypes_v1_2.json`).
- Grading (`study::engine::render_for_study`): self-graded cards expect the `answer_field` text;
  typing/cloze/choice engines unchanged. This also fixes Test, Learn and Match answer keys.
- `study.hint`: `HintDto` gains `kind: "context" | "letters"` and `html` (additive; `level`/`text`
  unchanged). Pure logic in `fc-study::hint` (`context_hint`, `leveled_hint`, `find_word`).
- `notes.terms`: `NoteTerm` gains `term_field`, `definition_field`, `ambiguous`; `front`/`back` are
  chosen by role.
- `inbox.accept`: role-based shorthand, context kept (`example`/`source`), default note type rule
  (`inbox::default_notetype`, also used by the automation inbox filter).
- `io`: `ColumnTarget::Role { role }`; role-ordered default mapping for note types with a `term`
  field; header suggestions by role words; JSON field names may be role ids.
- Fix (pre-existing): `fc-study::text::html_to_text` panicked on a bare `&` followed by multi-byte
  letters within 12 bytes (e.g. `Tom & ğğğğğğ`).

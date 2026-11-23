# 03 · Mimari

## 1. Genel bakış

```
┌──────────────────────────────────────────────────────────────┐
│  Frontend (Svelte + TS)   ← tamamen değiştirilebilir          │
│  sadece gösterir · iş mantığı YOK                             │
└───────────────▲───────────────────────────────┬──────────────┘
                │ olaylar (event stream)        │ komut / sorgu
┌───────────────┴───────────────────────────────▼──────────────┐
│  API katmanı (sürümlü sözleşme: v1, v2 …)                     │
│  Kapılar: Tauri IPC · Yerel HTTP · MCP sunucusu · Eklenti ABI │
└───────────────▲───────────────────────────────┬──────────────┘
                │                               │
┌───────────────┴───────────────────────────────▼──────────────┐
│  Servis katmanı (uygulama)                                    │
│  komut işleyici · sorgu işleyici · oturum · iş kuyruğu         │
│  olay yolu · modül kaydı · eklenti çalışma zamanı              │
└──────▲─────────────────▲─────────────────────▲───────────────┘
       │                 │                     │
┌──────┴──────┐  ┌───────┴────────┐  ┌─────────┴──────────────┐
│ Saf çekirdek│  │ Depolama        │  │ Adaptörler              │
│ (core)      │  │ SQLite, medya,  │  │ AI, TTS, STT, çeviri,   │
│ I/O YOK     │  │ migration       │  │ keychain, dosya, ağ     │
└─────────────┘  └────────────────┘  └────────────────────────┘
```

**Bağımlılık yönü tek yönlüdür:** `core` hiçbir şeye bağımlı değildir. `storage` ve `adapters` `core`'a, `app` hepsine, `api` `app`'e, kabuklar (`tauri`, `http`, `mcp`) `api`'ye bağlıdır. Frontend sadece `api`'nin ürettiği TypeScript tiplerini bilir.

## 2. Rust workspace yapısı

```
crates/
  fc-core/          Saf alan mantığı: modeller, zamanlayıcı arayüzü, FSRS sarmalayıcı,
                    şablon dili, sorgu dili (parse → AST), içe aktarma ayrıştırıcıları,
                    cevap kontrolcüleri, kart türü motoru, ipucu stratejileri. I/O yok.
  fc-storage/       SQLite (rusqlite), migration'lar, repository'ler, AST → SQL derleyici,
                    FTS5, medya deposu (içerik hash'li dosyalar).
  fc-app/           Servis katmanı: komut/sorgu işleyiciler, olay kaydı, geri al,
                    oturum motoru, iş kuyruğu, olay yolu, modül kaydı, ayarlar.
  fc-api/           Sürümlü API sözleşmesi: DTO'lar, komut/sorgu tanımları, hata modeli,
                    TypeScript tip üretimi. Taşıma katmanından bağımsız.
  fc-study/         Saf çalışma mantığı: zamanlayıcılar (FSRS-6, SM-2), kuyruk kurma,
                    cevap kontrolcüleri, ipuçları (v1.3: bağlam ipucu), çalışma günü,
                    planlama (v1.3: iş yükü tahmini `planning::forecast`). I/O yok.
  fc-providers/     Sağlayıcı ve strateji arayüzleri (AI, TTS, STT, çeviri, sözlük,
                    lemmatizasyon; zamanlayıcı, kart türü, cevap kontrolcü, ipucu).
  fc-ai/            Yerleşik AI sağlayıcıları (HTTP adaptörleri), v1.3 ön ayarlar
                    (`presets`) ve bağlantı testi sınıflandırması (`diagnose`).
  fc-plugins/       Eklenti çalışma zamanı (WASM), host fonksiyonları, izinler, manifest.
  fc-interop/       Anki .apkg, deste paketi, yedek formatları.
  fc-http/          Yerel HTTP API sunucusu (axum).
  fc-mcp/           MCP sunucusu ve istemcisi.
  fc-cli/           Geliştirme/otomasyon için CLI (çekirdeğin Tauri'siz çalıştığının kanıtı);
                    `fc mcp serve`, `fc api serve`, v1.3 `fc dev serve` (yalnız geliştirme
                    derlemesi: dev köprüsü, docs/07 §3.1).
apps/
  desktop/
    src-tauri/      Tauri kabuğu: IPC köprüsü, pencere, tepsi, derin bağlantı,
                    global kısayol, pano, updater.
    src/            Svelte frontend.
  browser-extension/ Tarayıcı eklentisi (v0.9).
plugins-sdk/        Eklenti geliştirme kiti (Rust + ileride JS/TS).
```

## 3. Saf çekirdek (fc-core)

- **Kurallar:** `std::fs`, ağ, `SystemTime::now()`, global durum, rastgele sayı üreteci doğrudan kullanılmaz.
- Zaman `Clock` arayüzüyle, rastgelelik `Rng` tohumu ile dışarıdan gelir.
- Fonksiyon imzası örnekleri:
  - `schedule(card_state, rating, now, params) -> (new_state, review_log)`
  - `render_card(template, fields, ctx) -> RenderedCard`
  - `parse_query(text) -> Result<QueryAst, QueryError>`
  - `parse_delimited(text, options) -> Vec<ParsedRow>` + `detect_delimiters(text) -> Suggestion`
  - `check_answer(expected, given, checker_cfg) -> AnswerVerdict`
  - `generate_cards(note, notetype) -> Vec<CardSpec>`
- Yüksek test kapsamı (hedef ≥ %90). Özellik tabanlı testler (proptest): ayrıştırıcılar, sorgu dili, zamanlayıcı değişmezleri.

## 4. Veri modeli

Tüm kimlikler **UUIDv7** (zamana göre sıralanabilir). Tüm tablolarda `created_at`, `updated_at` (UTC, milisaniye). Silme **yumuşaktır** (`deleted_at`), çöp kutusu süresi dolunca kalıcı silinir.

| Tablo | Temel alanlar | Not |
|---|---|---|
| `decks` | id, parent_id, name, description, icon, color, preset_id, position | Hiyerarşi `parent_id` ile |
| `presets` | id, name, config (JSON, şemalı) | Algoritma parametreleri, hedef hatırlama, gömme, sıralama… |
| `notetypes` | id, name, kind, fields (JSON şema), templates (JSON), css, version | Alan şeması ve kart şablonları. v1.3: alan JSON'unda isteğe bağlı `role`, şablon JSON'unda isteğe bağlı `answer_field` (§4.1) |
| `notes` | id, notetype_id, fields (JSON: alan_id → değer), sort_field, checksum, tags | `checksum` tekrar tespiti için (normalize edilmiş benzersiz alan) |
| `cards` | id, note_id, deck_id, template_key, card_type, state (new/learning/review/relearning), due, stability, difficulty, reps, lapses, last_review, flags, suspended, buried_until, user_note | Zamanlama durumu burada |
| `review_log` | id, card_id, session_id, rating, state_before, state_after, scheduled_days, elapsed_days, duration_ms, answer_duration_ms, hints_used, mode, created_at | **Değişmez.** İstatistik ve FSRS optimizasyonunun kaynağı |
| `oplog` | id, command, payload, inverse, actor, batch_id, created_at | Geri al ve denetim izi (bkz. §5) |
| `tags` | id, name (hiyerarşik), color | Notlarla çoka-çok: `note_tags` |
| `attribute_defs` | id, scope_deck_id (NULL = koleksiyon), key, name, kind (ordered_enum/enum/multi_enum/number/text/date/bool), options (JSON: [{id, label, color, order}]), target (note/card), required, default_value, position, template_id | Kullanıcı tanımlı sınıflandırmalar (seviye, sınav…). Kapsam deste ağacında miras kalır. Bkz. [02 §8.1](02-ozellik-katalogu.md) |
| `attribute_values` | attr_id, entity_kind (note/card), entity_id, value (seçenek id'si, sayı, metin…) | `(attr_id, entity_kind, entity_id)` birincil anahtar; çoklu seçimde birden çok satır. Sorgu dili buradan derlenir |
| `media` | hash (sha256), mime, size, original_name, ref_count | Dosyalar `media/<ab>/<hash>` altında |
| `links` | from_note, to_note, label | `[[bağlantı]]` ve geri bağlantılar |
| `saved_queries` | id, name, query, icon, position | Kayıtlı filtreler |
| `sessions` | id, kind, query, config, state (JSON), started_at, ended_at | Çökme kurtarma için kalıcı |
| `settings` | scope, key, value | Şemalı ayarlar |
| `jobs` | id, kind, status, progress, payload, result, error | Arka plan işleri |
| `inbox` | id, source, payload, status, created_at | Dış kaynaklardan gelen kartlar |
| `automations` | id, name, trigger, condition (sorgu), actions, enabled | Otomasyon kuralları |
| `plugins` | id, version, manifest, enabled, granted_permissions | Kurulu eklentiler |
| `plugin_kv` | plugin_id, key, value | Eklentilere özel depolama |
| `goals` | id, kind, target, deadline, query | Hedefler |
| `ai_usage` | id, provider, model, feature, input_tokens, output_tokens, cost_estimate, created_at | Harcama takibi |
| `notes_fts` | FTS5 sanal tablo | Tam metin arama |
| v1.4 kelime dizini | `word_index`, `word_index_notes`, `word_note_language`, `word_index_dirty(_keys)` | Kalıcı kelime dizini (§4.2, migration 1410) |
| v1.4 kelime verisi | `word_data_packs`, `word_data`, `word_family`; görünüm `word_frequency` | Sıklık/CEFR paketi, yedeğe girmez (§4.3, 1440/1441) |
| v1.4 Subtitle | `subtitle_capture_log` (karşılaşmalar + tanı), görünüm `subtitle_encounter_counts`, `note_origins` | Yakalama = karşılaşma (docs/06 §5.1, 1430/1460/1490) |
| v1.4 medya kökeni | `media_origins` | `original`/`clip`/`tts`/`user` + TTS önbelleği (§13, 1420) |
| v1.4 anlam ve cevaplar | `note_meanings`, `note_accepted_answers`, `leech_repairs` | Bağlamdaki anlam, kabul edilen cevaplar, zor kart onarımları (1450/1470) |
| v1.4 yazılan cevaplar | `typed_answers` | Kart başına son 20 yazılı cevap, tekrarıyla; ayar `study.keep_typed_answers` (1421) |

### 4.1 Alan rolleri ve cevap alanı (v1.3)

Kelime kartı gibi davranması gereken notlar için not tipinde iki **isteğe bağlı** üst veri vardır (JSON'da yoksa eski kayıt aynen yüklenir; `fc-core::notetype`):

- **`FieldDef.role`** — alanın *ne içerdiği*: `term` (sözlük hâli: *go*), `surface` (görüldüğü hâli: *went*), `meaning`, `example` (terimi içeren cümle), `example_translation`, `source`, `note`. Bir rol bir not tipinde en fazla bir alanda olur; alan adından **asla** tahmin edilmez.
- **`CardTemplate.answer_field`** — bu kartın *neyi sorduğu* (alan anahtarı). İleri kelime kartı `meaning`, ters kart `term` sorar.

Kullananlar (hepsi Rust'ta):

| Yapı taşı | Davranış |
|---|---|
| Yazılı cevap, Test, Öğren, Eşleştir (`study::engine::render_for_study`) | Kendi kendine notlanan kartta beklenen cevap `answer_field` alanının metnidir; yoksa arka yüzün cevap kısmı (eski davranış). Yazma, boşluk doldurma ve çoktan seçmeli motorlar kendi cevaplarını belirlemeye devam eder. |
| Eşleştir kutucukları (`modes::matching`, `support::content::term_and_definition`) | Rolü olan tiplerde kutucuklar `term` (yoksa `surface`) alanı ↔ şablonun cevap alanı (kelimenin kendisi değilse; yoksa `meaning`); cümle, çeviri, kaynak asla. Rolsüz tiplerde çizilen soru ve cevap. |
| AI ipucu, kontrol, açıklama (`modules/ai/context.rs`) | "Cevap" yazılan kartta beklenen cevap, kendi notlananda `answer_field` alanının metnidir (örnek ve kaynak değil); yoksa arka yüzün cevap kısmı. |
| İpucu (`fc-study::hint`) | Kural (v1.3 X1): **ipucu bağlam gösterebilir, cevabı asla.** Notun `example` cümlesi 1. seviyedir (`HintDto.kind = context`), sonraki seviyeler cevabın harf ipuçlarıdır. Kelimeyi soran kart (ters/üretme: `answer_field` rolü `term`/`surface`) cümledeki geçişi `___` ile gizler; geçiş yoksa cümle olduğu gibi bağlamdır. Anlamı soran kart (ileri) kelimeyi vurgular (`<b class="fc-context-word">`) ve yalnız cevap (anlam ya da `, ; / \|` ile ayrılmış seçeneklerinden biri) cümlede geçiyorsa onu gizler. Geçiş: önce kalın işaretli kısım (yakalamanın UTF-16 konumundan), yoksa `surface` sonra `term` için tam kelime, büyük/küçük harf duyarsız. Cümle soru yüzünde zaten görünüyorsa, gizlendikten sonra cevap başka yerde de geçiyorsa (kelimeyi soran kartta çekimli hâli de: *goes*, *going*) bağlam ipucu verilmez, harflere düşülür. |
| Deste sayfası terim listesi (`notes.terms`) | Sütunlar `term` ve `meaning` alanlarıdır; satırda `term_field` / `definition_field` döner ve satır içi düzenleme bunlara yazar. İkisi birden olmayan tiplerde ilk iki alan gösterilir ve `ambiguous: true` gelir: arayüz tahmin etmek yerine tam editörü açar. |
| Gelen kutusu kabulü (`inbox.accept`) | `front`/`back` kısayolu `term`/`meaning` alanlarına gider; yakalanan cümle `example`, sayfa `source` alanına yazılır (boşsa). Not tipi seçilmemiş ve cümle taşıyan öğe, cümleyi tutabilen yerleşik Altyazı kartı olur. |
| İçe aktarma (`io.import.*`) | Sütun hedefi `{kind: "role", role}` olabilir; rolü olan tiplerde varsayılan sıra term, meaning, example… olur; başlıklar ("sentence", "örnek", "definition"…) role göre eşlenir; JSON alan adı olarak rol adı kabul edilir. |
| Yerleşik arka yüzler | Kelime/Cümle/Altyazı kartlarında üç etiketli parça: anlam (`.fc-part-meaning`, en büyük), örnek + çevirisi (`.fc-part-example`), kaynak (`.fc-part-source`, en küçük). Etiketler `span.fc-part-label[data-fc-part]` boş gelir, arayüz `cards.part.*` ile doldurur. Kullanıcı şablonları değiştirilmez. |

Yerleşik Altyazı kartının ön yüzü (v1.3 X1, **migration 1301**) yalnız kelimedir: geçtiği hâli büyük, sözlük hâli farklıysa küçük yanında (şablon süzgeci `{{unless_same:surface:word}}`: alan, metni öteki alanınkiyle aynıysa boş). Cümle ön yüzde olmadığı için ilk ipucu olabilir. 1301 yalnız ön yüzü, arka yüzü ve kart türü gönderildiği gibi kalmış yerleşik `recognition` şablonunu değiştirir (1300 ile aynı tespit); tekrar çalıştırmak bir şey değiştirmez.

Var olan koleksiyonlar **migration 1300** ile güncellenir: yerleşik bir not tipinin alanı, yerleşik anahtar ve türünü koruyorsa rolünü alır; şablon ön/arka/kart türü v1.2'de gönderildiği gibiyse yeni arka yüzü ve `answer_field`'ı alır; CSS hâlâ boşsa parça CSS'i eklenir. Düzenlenmiş her şey (şablon metni, alan türü, CSS) olduğu gibi kalır; adlar karşılaştırılmaz. Tekrar çalıştırmak hiçbir şeyi değiştirmez.

### 4.2 Kelime dizini (v1.4)

"Hangi kelimeleri biliyor?" sorusunun **tek** cevabı: okuma modu (`reading.analyze`, `reading.words.lookup`), Subtitle'ın `GET /subtitle/v1/known-words` ucu ve `words.*` API'si aynı kalıcı dizini okur (migration `1410_word_index`, modül `words`, kod `fc-app/src/modules/words/`, saf kurallar `fc_core::words`).

- **Anahtar:** (dil, anahtar). Dil birincil alt etikettir (`en-GB` → `en`); anahtar bir lemma ya da en çok dört kelimelik bir ifade (`go`, `pick up`). Yalnız **çekim** birleşir (`went` → `go`, dilin lemmatizer'ı); **türetme** birleşmez (`actor` ≠ `act`).
- **Kaynak:** not türünde `term` rolü olan her not — elle, Anki'den, Subtitle'dan, okuma modundan; hiç rolü olmayan not türü (Anki içe aktarımı, v1.3 öncesi türler) sıralama alanına düşer, cloze türleri hariç — ve okuma modunun elle işaretleri (`word_marks`).
- **Dil:** notun Subtitle yakalamasının dili, yoksa okuma modunda yapıldığı metnin dili (`word_note_language`), yoksa ev destesinin `words.language` ayarı (deste, sonra uygulama kapsamı). Gönderilen varsayılan **boş**: dili bilinmeyen not hiçbir kelimeye yazılmaz, `words.stats.unassigned_notes` ile raporlanır; tahmin edilmez.
- **Kanıt ayrı tutulur:** `word_index` satırında elle işaret (+zamanı) ile notların toplamı (not sayısı, canlı kart sayısı, en uzun tekrar aralığı, son tekrar, ilk not) ayrı sütunlardır; `word_index_notes` her notun katkısını tutar, not/kart kimlikleri buradan ve `cards_note` dizininden okunur.
- **Durum okurken türetilir:** elle işaret kazanır (known/ignored); yoksa aralığı `words.known_interval_days` (varsayılan 21, istatistiklerin "olgun" tanımı) eşiğine ulaşmış bir kart → `known`; başka kart → `learning`; yoksa `new`. Eşik değişince yeniden kurmak gerekmez.
- **Artımlı güncelleme:** `notes`, `cards`, `notetypes`, `subtitle_captures`, `word_note_language`, `word_marks` ve `words.language` ayarındaki her değişiklik tetikleyicilerle kirli kuyruğa (`word_index_dirty`, `word_index_dirty_keys`) girer; dağıtıcı (`App::run_command`) her yazma transaction'ının sonunda, oplog kaydından önce kuyruğu boşaltır (`words::index::drain`). Geri al/yinele de komut olduğu için dizin onlarla birlikte değişir; dizin türetilmiş veridir, oplog'a yazılmaz. Açılışta (`load_state`) da boşaltılır: migration mevcut her notu kuyruğa koyar.
- **Tam yeniden kurma:** `words.index.rebuild` (arka plan işi → iç komut `words.index.rebuild_now`) dizini boşaltıp her şeyi kuyruğa koyar ve aynı yoldan boşaltır; artımlı sonuçla eşitliği tohumlu rastgele bir geçmişle test edilir (SQLite ve bellek deposu).
- **Performans:** okuma tarafı yalnız birincil anahtar/dizin aramaları yapar (500 anahtarlık parçalar); 50 000 notta 500 biçimlik `words.lookup` debug derlemede ~35 ms. `reading.vocabulary_query` dolu ise yalnız ilgili kelimelerin kartları kapsamla süzülür.
- **Yüzeyler ve izin (v1.4 B4):** `words.lookup/list/stats` kendi iznini ister (`words:read`); aynı metotlar MCP araçları (`words_lookup`, `words_list`, `words_stats`), eklenti host çağrıları (kullanıcı destelere sınırlayabilir: `plugins.word_decks.set`, kesme `plugins::word_scope`'ta) ve `fc call` ile okunur. Subtitle uç noktası dizini yalnız `subtitle.share_known_words` açıkken okur (varsayılan kapalı; docs/06 §5.1).

- **Sorgular (v1.4 B3):** `durum:`, `dil:`, `kelime:`, `karşılaşma` koşulları bu dizini okur (`word_index_notes` + `word_index`, docs/02 §9.1); notun kökeni/kaynağı `note_origins` tablosunda (migration `1460_language_queries`), üreticiler (Subtitle yakalaması, okuma modu) notla birlikte yazar — `word_note_language` gibi türetilmiş üst veri.

### 4.3 Kelime verisi: sıklık, seviye, aile (v1.4)

Sahibi sık duyacağı kelimelere önce zaman ayırabilsin diye kelime dizini (§4.2) **sürümlü, yerel bir kelime verisi paketiyle** zenginleşir. Modül `word_data` (kapatılabilir), kurallar `fc_core::word_data` (saf), paket biçimi ve üretimi `tools/word-data/README.md`.

- **Paket:** dil başına bir JSON (`format: fc-word-data`, `format_version: 1`): sürüm, lisans, kaynaklar (ad, adres, lisans, istenen atıf), `words` (en sık duyulan önce; sıra = konum + 1), `profile` (CEFR profil listesinin kelimeyi öğrettiği en düşük seviye), `forms` (çekimli biçim → lemma), `family` (türemiş lemma → ailenin başı). İngilizce paket uygulamayla gelir (`crates/fc-providers/data/word-data/en.json`, ikiliye gömülü; çalışırken ağ ya da dosya yok): OpenSubtitles 2018 sayımları (hermitdave/FrequencyWords, CC BY-SA 4.0), Wiktionary çekim biçimleri (Subtitle eklentisinin sıklık derlemesinden, CC BY-SA 4.0), CEFR-J Wordlist 1.5 (atıfla serbest) ve Octanove C1/C2 profili (CC BY-SA 4.0). Paketin kendisi CC BY-SA 4.0; atıflar `words.data.sources` ile uygulamadan okunur. 38 475 lemma, 11 068 biçim, 8 841 profil girişi, 1 026 aile bağı, 762 KB.
- **Saklama:** migration `1440_word_data`: `word_data_packs` (dil başına kurulu paket, kökeni `bundled`/`imported`, sağlama toplamı), `word_data` (dil, biçim → lemma, sıra, bant, bant kökeni; her lemma, her çekimli biçim ve sıklık listesinde olmayan profil başlıkları), `word_family` (lemma → baş). Dizin anahtarı `word_data.form` ile tek dizinli aramayla birleşir. Başlangıçta (`load_state`) gelen paket yoksa ya da kurulu gelen paketin sağlama toplamı değiştiyse kurulur; sağlama toplamı derleme anındaki bir sabittir (`fc_providers::word_data::ShippedPack::checksum`, bir test baytlarla tutarlılığını denetler), paket güncelse açılış yalnız paket tablosunun küçük bir okumasıdır (ölçülen ~10 µs, test < 5 ms ister); kurulum tablo başına tek toplu ifadedir (`json_each`). Testler (`TestApp`) küçük bir sabit paket kurar; gerçek paketi isteyen test `TestApp::with_shipped_word_data` kullanır; içe aktarılmış pakete gelen paket asla yazılmaz. `words.data.import` (dosya ya da metin) bir dilin paketini değiştirir, `words.data.reset` gelen pakete döner. Paket kullanıcı verisi değil başvuru verisidir: bu komutların geri alması yok (yeniden kurma gibi), geri dönüş `reset`.
- **Yedek:** `.fcbackup` yalnız **içe aktarılmış** paketleri taşır (`word_data_packs.origin = 'imported'` ve o dillerin satırları); gelen paket ikiliden yeniden kurulur — açılışta ve bir geri yüklemenin hemen ardından (`CollectionReplaced`).
- **Sorgular:** `1441_word_frequency_view` B3'ün `sıklık`/`freq` ve `cefr`/`level` koşullarına `word_frequency(language, key, rank, band)` görünümünü verir (`word_data` üzerinde, birincil anahtar aramasıyla); bellek içi depo da aynı satırları okur.
- **Sıra:** lemmanın sıklık listesindeki yeri (1 = en sık). Çekim lemmaya katılır (`went` `go`'nun sırasını alır); türetme ayrı kalır (`actor` kendi sırasında).
- **Bant ve kökeni:** profil listesinde varsa profilin seviyesi (`profile`); yoksa sıradan tahmin (`rank`: ≤1000 A1, ≤2000 A2, ≤3500 B1, ≤6000 B2, ≤10 000 C1, ötesi C2; Subtitle eklentisinin `CEFR_LEVELS` sınırları); `estimated` profil dışı her bant için doğrudur. Paketteki gibi bir bant kesinlik iddiası değildir: sıra altyazı sayımıdır, profil öğretim listesidir.
- **İfadeler** (`expression` kökenli, hep tahmin; Subtitle `expressionLevel` kuralı): ifadenin kendi profil satırı varsa o (`according to`); yoksa İngilizce **öbek fiil** (ilk kelime işlev kelimesi değil, gerisi yalnız edat/parçacık: `give up`, `get out of`) fiili kadar zor ve en az A2; değilse işlev kelimeleri dışarıda **en nadir içerik kelimesi** kadar (`in front of` = `front`). Deyimi bir seviye yükselten kural dizinde uygulanmaz (deyimi set ifadeden ayıramıyoruz). İfadelerin sırası yoktur.
- **Aile:** `family` bağları yalnız gösterilir (`words.lookup` → `family`, her üyenin kendi durumu); hiçbir üyenin durumunu değiştirmez: `act` biliniyorsa `actor` yine `new`. İngilizce bağlar profil sözcük türleriyle kapılanmış ek kurallarından (‑er/‑or/‑ion/‑ment/‑ness/‑ity/‑ly/‑ful/‑less/‑able/‑ive…) ve elle gözden geçirilmiş bir durdurma listesinden gelir.
- **API:** `words.lookup` her biçime `level` (lemma, sıra, bant, bant kökeni, tahmin mi) ve `family` ekler (dizinde olmayan kelimeye de); `words.list` satırlarında `level`, yeni `sort` (`key`/`rank`/`band`), `bands` ve `max_rank` süzgeçleri (sayfalama sıralama değerlerini taşıyan opak imleçle); `words.stats` dil başına bant × durum sayıları (`bands`). Bant süzgeci ve sıralama paket satırı olan anahtarlara bakar: kendi satırı olmayan ifadeler sıralamanın sonunda kalır ve bant süzgecine girmez (bantları yine kuralla gösterilir). Modül kapalıyken sonuçlarda `level`/`family` yoktur ve `words.list` sıra/bant istenirse `CONFLICT_MODULE_DISABLED` döner.
- **Performans:** 50 000 kelimelik bir paketin içe aktarılması ve 500 biçimlik arama test edilir (`word_data::tests::a_50k_pack_loads_and_500_forms_look_up_fast`, süreler çıktıda).

### 4.4 En öğretici sahne ve zor kart onarımı (v1.4)

- **Puanlama saftır** (`fc_core::encounter_score`): çağıran cümleleri dil sağlayıcısıyla kelimelere ayırır, her kelimenin dizin durumunu (§4.2) ve sıklık sırasını (§4.3; `word_data` modülü açık ve dil için paket varsa) verir; `rank_scenes` her sahneye açıklanmış puanlar (`SceneReason {code, points, count, words}`) toplar ve en iyiden sıralar (eşitlikte giriş sırası: birincil önce, yani eşitlik onu yerinden etmez). Puanlar: hedef var +10 / yok −40 (ifade parçaları en çok 3 kelime arayla sırayla; kaynağın verdiği konum denetlenir, güvenilmez); başka bilinmeyen yok +10, her bilinmeyen −10, her öğrenilen −3 (işlev kelimeleri ve sırası ≤ 1000 olanlar bilinir sayılır; isimler — cümle ortasında büyük harf, paket varken paketin bilmediği ve dizinde olmayan kelime — 0 puanla yalnız bildirilir; paketin bilmediği kesmeler `I'll` temel sayılır); kesik başlangıç −10, kesik son −10, sarkan tire −15, kapanış noktalaması yok −4, iki konuşmacı −5; < 4 kelime −15, > 24 kelime −10, 6–16 kelime +4; ses +8, resim +4; çevre kelimelerin medyan sırası ≤ 2000 +5, > 6000 −5. Neredeyse kopya: aynı kelimeler, biri ötekinin içinde (≥ 3 kelime) ya da Jaccard ≥ 0.8; kümeler geçişlidir.
- **Modül `examples`** (kapatılabilir; açıkken yalnız sorgu): `subtitle.encounters.rank`, `subtitle.encounters.suggest_primary` (fark ≥ `SUGGEST_MARGIN` = 5), `examples.rank_sentences {language, target, sentences[≤200]}` (okuma modu adayları). Ayar `subtitle.auto_best_example` (ayar önekleri modül kimliği olduğu için Subtitle modülünde tanımlı).
- **Zor kart onarımı** (`stats` modülü): `stats.leech_repairs.plan {card_id}` kanıt + öneriler (her öneri: `repair`, çalıştıracağı var olan komut `method`, `why`, alan önizlemesi; AI yeniden yazma için `ai_preview` isteği — kullanıcı önizler, çalıştırır, gözden geçirdiği alanları `ai_rewrite` ile uygular). `stats.leech_repairs.apply {card_id, repair}` var olan komutu iç içe çalıştırır (modülü kapalıysa `CONFLICT_MODULE_DISABLED`) ve `leech_repairs` tablosuna (migration `1470_leech_repairs`: kart, not, tür, onarımın JSON'u, o anki unutma sayısı, zaman) yazar; tek geri al adımı, tersi iç `stats.leech_repairs.restore {remove, put}` (yinele aynı kimliği geri koyar). `stats.leech_repairs.list` her onarım için kartın önceki son 10 planlı tekrarının ve sonrakilerin (kartın bir sonraki onarımına kadar) geçme oranını, ilk 20 tekrarı ve 10'dan az tekrarda `small_sample` uyarısını verir; pratik tekrarlar sayılmaz. Tablo yedeğe girer.
- **Yazılan cevaplar** (v1.4 Z1, `study` modülü, `modules/study/typed.rs`): yazılarak verilen cevap tekrarla birlikte `typed_answers` tablosuna yazılır (migration `1421_typed_answers`: kart, `review_log_id` (boş olabilir), tekrarın modu, metin (kırpılmış, en çok 1000 karakter), `correct` (Öğren/Sınav'ın kararı: doğru ya da yazım hatası düzeyi = 1, yanlış = 0; Kartlar'da kullanıcı kendisi puanladığı için `NULL`), zaman). Üç yol: Kartlar (`study.answer` isteğinin `typed_answer` alanı; arayüz kart yazarak cevaplandıysa gönderir), Öğren'in yazma aşaması (`modes.learn.answer`'ın `given`'ı) ve Sınav'ın yazılı soruları (`modes.test.submit`). Satır cevap komutunun kendi transaction'ında yazılır; kart başına en yeni 20 (`KEEP_PER_CARD`) kalır, eskisi aynı transaction'da silinir. Geri al: tersi `study.restore`'un `remove_typed_answers` / `add_typed_answers` alanları (cevabın satırı gider, budanan geri gelir; yinele aynı kimliklerle geri koyar). Ayar `study.keep_typed_answers` (app, varsayılan açık): kapalıyken hiçbir şey yazılmaz, eski satırlar kalır. Tablo yedeğe girer; metin günlüğe, oplog özetine ve günlük paketine girmez (redaksiyon `typed_answer` anahtarını da maskeler). `stats.leech_repairs.plan`: `failed_reviews[].typed_answer` tekrarın satırından, `typed_answers_recorded` = ayarın değeri.

## 5. Olay kaydı ve geri al (hibrit yaklaşım)

Saf "event sourcing" (durumu her seferinde olaylardan yeniden üretmek) yerine **hibrit** bir model kullanılır: güncel durum normal tablolarda tutulur, her değişiklik **aynı transaction** içinde değişmez kayıtlara yazılır. Bu hem hızlı okuma hem tam geçmiş sağlar.

- **`review_log`:** Her tekrar değişmez bir kayıttır. İstatistikler ve FSRS optimizasyonu buradan hesaplanır. Bir kartın zamanlama durumu teorik olarak bu kayıttan yeniden hesaplanabilir (veritabanı kontrolü bunu doğrular).
- **`oplog`:** Her komut, kendisi ve **tersi (inverse)** ile kaydedilir.
  - Geri al = son işlemin tersini yeni bir komut olarak uygula (geçmiş silinmez).
  - Toplu işlem = aynı `batch_id` ile birden çok kayıt → tek geri al.
  - Not düzenleme geçmişi `oplog`'dan okunur.
- **Senkronizasyon için hazırlık:** `oplog` + UUID'ler + zaman damgaları ileride senkronizasyon altyapısının temelidir (kapsam dışı ama yolu kapatılmadı).
- `oplog` kayıtları belirli süre sonra (ayarlanabilir) sıkıştırılabilir. `review_log` asla silinmez — kart kalıcı silinse (çöp temizliği) bile: `card_id` düz bir sütundur, yabancı anahtar/CASCADE yoktur (migration `0501`), böylece ısı haritası, seri, toplamlar ve FSRS optimizasyon verisi korunur. Yalnızca bir cevabın geri alınması kendi satırını siler.

## 6. Komutlar ve sorgular (CQRS)

```rust
// Kavramsal
trait Command { type Output; fn validate(&self, ctx) -> Result<()>; fn execute(self, tx) -> Result<(Self::Output, Inverse)>; }
trait Query   { type Output; fn run(&self, conn) -> Result<Self::Output>; }
```

- **Komut:** Doğrulanır → transaction açılır → durum değişir → `oplog` (+ gerekirse `review_log`) yazılır → commit → olay yolu üzerinden **alan olayı** yayınlanır.
- **Sorgu:** Yan etkisizdir, okuma bağlantısı kullanır (SQLite WAL modu ile okuma ve yazma birbirini bloklamaz).
- **Toplu komut (`Batch`):** Komut listesi tek transaction'da çalışır, biri başarısız olursa hepsi geri alınır.
- Komut ve sorgular **kayıt defterinde (registry)** tutulur: ad, sürüm, girdi/çıktı şeması, gereken izin. Bu kayıt; komut paleti, klavye kısayolları, yerel HTTP API, MCP araçları ve eklenti izinleri için tek kaynaktır.

### Örnek komutlar
`deck.create` · `deck.rename` · `deck.move` · `note.create` · `note.update` · `note.delete` · `card.suspend` · `card.bury` · `card.flag` · `card.setDeck` · `card.reschedule` · `card.resetProgress` · `study.start` · `study.answer` · `study.undo` · `study.end` · `tag.rename` · `tag.merge` · `import.start` · `export.start` · `backup.create` · `preset.apply` · `settings.set` · `plugin.install` …

### Örnek sorgular
`deck.tree` · `deck.summary` · `note.get` · `card.search` · `card.info` · `study.next` · `stats.heatmap` · `stats.forecast` · `import.preview` · `settings.schema` · `modules.list` · `commands.list` …

(Yukarıdakiler kavramsal adlardır; gerçek adlar `fc-api`'nin `v1/methods/*.rs` dosyalarındadır, ör. `decks.tree`, `decks.summaries`, `notes.create`.)

### v1.3'te eklenen metotlar

| Metot | Tür | Ne yapar |
|---|---|---|
| `study.options` | sorgu | Başlatıcının verisi: beş kapsamın tam sayısı, boşsa nedeni ve `next_available_at`, devam edilebilir oturumlar, açık modların türleri (§9.1) |
| `modes.test.save_draft` | komut (geri al adımı değil) | Yarım test kâğıdının cevaplarını taslak olarak saklar; kâğıt (`TestPaperDto.draft`) devam edilince geri gelir |
| `stats.today` | sorgu | "Bugün nasılım?": bugün / dün, 7 günlük ortalama, tempo yargısı (`ahead`/`same`/`behind`…), seri |
| `stats.struggles` | sorgu | "Nelerde zorlanıyorum?": pencerede yanlış cevaplanan kartlar (her mod), terim/anlam rollerden, pratik için `cid:` sorgusu |
| `planning.forecast` | sorgu | Bir yeni kart/gün temposunun gün gün iş yükü tahmini (`fc_study::planning::forecast`) |
| `planning.apply_pace` | komut (geri alınabilir) | Tempoyu destenin şablonuna yazar (şablonu yoksa oluşturup atar), isteğe bağlı sınav tarihi; tek geri al adımı |
| `ai.providers.test` | komut → iş | `ai.test` işi: tek küçük istek, sonuç `AiConnectionTest` (docs/06 §1.1) |
| `mcp.setup` | sorgu | MCP istemci parçacığı için bu profilin `db_path`, `fc_path`, `platform` değerleri (yalnız kullanıcı/CLI) |

Var olan metotlarda v1.3 değişiklikleri (eklemeli; eski istemciler kırılmaz): `study.start` isteğe bağlı `scope` alır; `SessionDto.next_due`; `decks.tree` / `decks.summaries` `today_due`, `today_learning`, `today_new`, `today_total` alanlarını taşır (eski `due`/`learning`/`new` aynı değerde); `study.hint` yanıtı `kind` (`context`/`letters`) ve `html` taşır; `notes.terms` satırı `term_field`, `definition_field`, `ambiguous` taşır; içe aktarma sütun hedefi `{kind: "role", role}` olabilir; `ai.providers.list` → `presets`, sağlayıcı DTO'su `preset`, `status`, `last_test`; `ai.models` işinin sonucu tipli `AiModelList`; hedef türü `daily_cards`. Ayar kitinin "Varsayılana dön"ü ve `?` kısayol sayfası var olan `settings.reset` ve `shortcuts.list` metotlarını kullanır.

Yeni hata kodu: **`CONFLICT_NOTHING_TO_STUDY`**: `study.start` seçilen kapsamda kart bulamadı (`details.reason`: bir `StudyEmptyReason`; `details.next_available_at`: ms zaman damgası ya da `null`). Oturum oluşturulmaz, açık oturumlara dokunulmaz.

## 7. API sözleşmesi

- `fc-api` tüm DTO'ları, komut/sorgu adlarını ve hata kodlarını tanımlar.
- **TypeScript tipleri otomatik üretilir** (`ts-rs`; `fc-api`'nin `gen-ts` aracı `apps/desktop/src/lib/api/generated/`'a yazar, CI `gen-ts --check` ile eskimiş tipi yakalar). Frontend bu tiplerle derlenir. Backend'de bir tip değişirse frontend **derleme zamanında** kırılır, sessizce değil.
- **Sürümleme:** Komutlar `v1.note.create` biçiminde ad alanlıdır. Kırıcı değişiklik yeni sürüm ister. Eski sürüm bir geçiş süresi boyunca adaptörle desteklenir.
- **Taşıma bağımsızlığı:** Aynı sözleşme Tauri IPC, yerel HTTP (JSON), MCP (araç olarak) ve eklenti ABI üzerinden sunulur. Taşıma katmanları ince adaptörlerdir.
- **Kullanıcı dağıtıcısı** (v1.3, `fc_app::transport::dispatch`): kullanıcı adına çağrının tek yolu. Kilit kapısı (kilitliyken yalnız kilit ekranının metotları, `PERMISSION_APP_LOCKED`), kullanıcı aktörü, `null` parametre = `{}`, iç metotların reddi burada. Masaüstü IPC komutu (`src-tauri/src/ipc.rs`) ve geliştirme köprüsü (`fc dev serve`, `crates/fc-cli/src/dev/`) bunu çağırır; ikisi uygulamaya aynı yoldan ulaşır, hata JSON'u ve commit sonrası olaylar aynıdır.
- **Frontend'in üç taşıması** (`apps/desktop/src/lib/api/index.ts`): Tauri IPC (üretim); mock (`dev`, yalnız geliştirme: ekran görüntüleri ve birim testleri); **köprü** (v1.3, `vite --mode bridge`, `VITE_FC_BRIDGE_URL` + `VITE_FC_BRIDGE_TOKEN`: tarayıcıdaki arayüz `POST /rpc` + SSE `/events` ile gerçek çekirdeğe bağlanır). Mock ve köprü `import.meta.env.DEV` arkasındadır, üretim paketine girmez. Köprü, eklentilere açık kısıtlı yerel HTTP API'si (`fc-http`) **değildir**; ayrıntı docs/07 §3.1.
- **Sözleşme testleri:** Her komut/sorgu için girdi/çıktı örnekleri (snapshot) tutulur. Beklenmeyen değişiklik CI'da yakalanır.

### Hata modeli

```json
{ "code": "NOTE_DUPLICATE", "message_key": "errors.note.duplicate", "message": "…", "details": { "existing_note_id": "…" }, "retryable": false }
```

- `code` sabit ve belgelenmiştir. Frontend davranışını koda göre belirler, mesaja göre değil.
- Kategoriler: `VALIDATION_*`, `NOT_FOUND_*`, `CONFLICT_*`, `PERMISSION_*`, `PROVIDER_*`, `IO_*`, `INTERNAL`.

## 8. Olay yolu (event bus) ve kancalar

- Komutlar commit sonrası **alan olayları** yayınlar: `NoteCreated`, `CardAnswered`, `CardFlagged`, `DeckMoved`, `JobProgress`, `InboxItemReceived` …
- Aboneler: frontend (canlı güncelleme), otomasyon motoru, eklentiler, istatistik önbelleği.
- **Kancalar (hooks):** Komutlardan önce ve sonra çalışan, kayıtlı işleyiciler.
  - `before_*` kancaları girdiyi **değiştirebilir** veya işlemi **reddedebilir** (ör. eklenti kart oluşturulurken alanı otomatik doldurur).
  - `after_*` kancaları sadece tepki verir.
  - Kanca zinciri sıralı, zaman sınırlı ve hata izole edilmiştir: bir eklentinin hatası çekirdeği durdurmaz.

## 9. Oturum motoru

- Çalışma oturumu backend'de bir durum makinesidir: `start(kind, query, config) → next() → reveal() → answer(rating) → … → end()`.
- Kuyruk oluşturma: sırası gelenler, öğrenme adımları, yeniler; karıştırma stratejisi; kardeşleri yarına bırakma (kodda `bury_siblings`).
- Oturum durumu `sessions` tablosunda kalıcıdır (çökme kurtarma).
- Takvimi etkilemeyen oturumlar (deneme, test) `review_log`'a `mode` alanıyla yazar ama `cards` durumunu değiştirmez.
- Zamanlı mod, ipucu sayacı, cevap süresi ölçümü oturum motorundadır. Frontend sadece "gösterildi / çevrildi" zamanlarını bildirir (backend kendi zamanıyla doğrular).
- v1.3: **her türden bir açık oturum** (`study.start` yalnızca aynı türün açık oturumunu kapatır; test kâğıdı bir Kartlar oturumundan sağ çıkar). Kalan kartların hepsi öğrenme adımını bekleyen oturum açık kalır; `SessionDto.next_due` kartın gösterilebileceği anı verir. Başka kart kalmadığında öğrenme kartı vadesinden en çok önden öğrenme penceresi kadar önce gösterilir: `study.learn_ahead_mins` ayarı (varsayılan 20 = `fc_study::session::LEARN_AHEAD_MS`, 0–1440; `plan.rs` `learn_ahead_ms`). `next_due` / `next_available_at` kartın gerçekten sunulacağı andır (vade − pencere).
- Ayar değişikliği ekrandaki kartı almaz: oturum, gösterilen kartı seçtiği pencereyi `SessionState.learn_ahead_ms`'te tutar; `Session::shown_window_ms` bu değerle şimdiki ayarın büyüğüdür. Büyütülen pencere hemen, küçültülen bir sonraki cevaptan (yeni kart seçilince, `reset_shown_window`) geçerli olur; kart "artık oturumda değil" olmaz.

### 9.1 Kuyruk planlayıcısı (v1.3)

"Bugün çalışılacak" her sayı **tek yerde** hesaplanır: `crates/fc-app/src/modules/study/plan.rs` (`Planner`, `PlanRequest`, `today_counts`). Bir istek (deste + alt deste ayarı, oturum türü, `StudyScope`, filtre/sorgu) bir kuyruğa çözülür: istemcinin saat dilimindeki çalışma günü (`study.day_start_hour`), deste şablonunun günlük sınırları eksi bugün çalışılanlar (yalnız takvimi değiştiren cevaplar, her mod), `study.new_per_session`, kardeşleri yarına bırakma (`bury_siblings`), yarına bırakılmış/kenardaki kartlar ve boş kartlar (kuyruktaki her kart denetlenir).

| Kullanan | Ne alır |
|---|---|
| `study.start` | Oturumun kuyruğu; boşsa `CONFLICT_NOTHING_TO_STUDY` (hiçbir şey yazılmaz) |
| `study.options` | Beş kapsamın sayısı (`Planner::count`: kart şimdi gösterilemiyorsa 0), boşluk nedeni ve `next_available_at` (`Planner::empty_reason`) |
| `decks.tree` / `decks.summaries` | `today_*` alanları (`today_counts`, desteye ve koleksiyona) |
| Hatırlatıcılar (`reminders`) | Bildirimdeki sayı (koleksiyonun bugünkü kuyruğu) |
| Sorgu dili | `added:N` / `edited:N` / `reviewed:N` için çalışma gününün başı (`today_start`) |

- Planlayıcı bir anda birden çok planı paylaşılan önbellekle (deste dizini, bugünün cevapları, boş kart denetimi) kurar; deste ağacının bütün sayıları tek planlayıcıya mal olur. v1.4 Z2: süren kartlar (öğrenme/tekrar, gün sonundan önce vadeli) planlayıcı başına **bir kez** okunur ve her plan deste kümesine göre bellekte süzer (`Planner::candidates`; kart listesi, sınıflandırma koşulu ya da askıdakiler istendiğinde doğrudan depolama okuması); yeni kartlar her desteden `cards_deck_new_order` indeksiyle ilk n kadar okunup birleştirilir; boş kart denetimi kuyruğun notlarını tek sorguyla okur (`NoteRead::get_notes`). Sonuç depolama okumasıyla aynıdır (aynı kartlar, aynı sıra; rastgele dünyalarda `shared_candidates_equal_the_storage_read` ve `study_cards_per_deck_first_new_equals_reference` testleri).
- Saat dilimi verilmeyen sorgular (`decks.tree`, hatırlatıcı, sorgu dili) son `study.start`'ın kaydettiği istemci farkını (`ux_state`) kullanır, böylece aynı çalışma gününü paylaşırlar.
- Pratik kapsamları (`studied_today`, `struggled_today`, `all`) sınırsızdır, kümeyi başta sabitler, yarına bırakılmışları da alır ve öğrenme adımı beklemez; oturum `trial` olur.
- **Değişmez (Rust testi, `launcher_tests.rs`):** aynı saat ve deste için `study.options.scopes[due].count` = `study.start { kind: normal, scope: due }`'nun sunduğu kart sayısı.
- **Koleksiyon çapındaki kuyruk** ("Tüm desteler", `deck_id: null`; `plan_collection`): `collection_roots`'un (her üst düzey deste ve alt destelerini dahil etmeyen bir destenin alt desteleri; her canlı deste tam bir kez) kendi planlarının birleşimi, **her biri kendi şablonuyla** (günlük yeni/tekrar sınırı, sıra, kardeşleri yarına bırakma) ve alt deste ayarıyla. Harmanlama: öğrenme kartları vade sırasıyla önde, sonra tekrarlar, sonra yeniler; tekrar ve yeniler desteler arasında sırayla alınır. Koleksiyon üst sınırı yalnızca istenirse (`new_limit`) ya da `study.new_per_session` uygulama genelinde ayarlıysa. **Kardeşler desteler arasında da** yarına bırakılır: notunun bir kartı kuyruktaysa, destesi kardeşleri bırakan tekrar/yeni kart dışarıda kalır; öğrenme kartları hep kalır (C4). Değişmez (`launcher_tests.rs`): bir notun kartları farklı destelere dağılmamışsa deste başına `today_*` toplamı = koleksiyonun `today_*`'ı; dağılmışsa koleksiyonun sayısı toplamdan küçük olabilir.
- **Planlama tahmini** (`fc_study::planning::forecast`, `modules/planning.rs`): 0. gün bugünün gerçek kuyruğudur (`TodayQueue`, Ana sayfa ve başlatıcının sayısı); sonraki günler şablonların günlük yeni ve tekrar sınırlarına uyar, sınırı aşan tekrarlar ertesi güne kayar. Günün "kart" sayısı kart sayar (başlatıcı gibi), dakikalar cevap sayar.

## 10. Takılabilir arayüzler (strateji deseni)

| Arayüz | Yerleşik uygulamalar | Eklentiyle |
|---|---|---|
| `Scheduler` | FSRS (varsayılan), SM-2 | ✅ |
| `CardTypeEngine` | klasik, ters, cloze, çoktan seçmeli, yazarak, resim gizleme, ses | ✅ |
| `AnswerChecker` | birebir, normalize, toleranslı, çoklu, AI | ✅ |
| `HintStrategy` | ilk harf, kelime başları, kısmi, alan, AI | ✅ |
| `Importer` / `Exporter` | metin/CSV, JSON, Quizlet, .apkg, deste paketi, altyazı | ✅ |
| `AiProvider` | Claude, OpenAI, OpenAI uyumlu, Ollama | ✅ |
| `TtsProvider` | sistem TTS (Windows SAPI/OneCore), yerel model (ör. Piper) | ✅ |
| `SttProvider` | Whisper (yerel, whisper.cpp), Whisper API | ✅ |
| `TranslationProvider` | AI, DeepL vb. | ✅ |
| `DictionaryProvider` | — | ✅ |
| `Lemmatizer` | basit kural tabanlı (İngilizce) | ✅ |
| `PluginRegistrySource` | dosyadan kurulum | ✅ (mağaza kaynağı) |

Her arayüzün bir **kimliği**, **yetenek bildirimi** (capabilities) ve **ayar şeması** vardır. Seçim ayarlardan yapılır.

## 11. Modül kaydı

- Her özellik bir modüldür: `{ id, name_key, enabled_by_default, settings_schema, commands, queries, ui_contributions }`.
- `modules.list` sorgusu frontend'e açık modülleri ve arayüz katkılarını (menü öğeleri, ekranlar, ayar sayfaları) verir. Frontend menüyü **buna göre** çizer, sabit kodlamaz.
- Eklentiler de modül olarak kaydolur.

## 12. Arka plan iş kuyruğu

- Uzun işler (içe/dışa aktarma, AI ile toplu üretim, medya işleme, FSRS optimizasyonu, veritabanı kontrolü) `jobs` tablosuna yazılır ve işçi havuzunda (tokio) çalışır.
- İlerleme olayları (`JobProgress`), iptal (cooperative cancellation), hata ve sonuç kaydı.
- Uygulama kapanırsa yarım kalan işler açılışta "devam et / iptal" olarak sunulur.

## 13. Medya deposu

- Dosyalar içerik hash'i (SHA-256) ile saklanır → aynı dosya bir kez tutulur.
- Referans sayımı. Sahipsiz medya veritabanı kontrolünde temizlenir.
- Resim küçük önizlemeleri (thumbnail) önbelleklenir.
- Frontend medyaya özel bir protokolle erişir (`fcmedia://<hash>`), dosya yollarını bilmez.
- `[v1.4 W4]` **Köken:** her ses/görsel dosyasının nereden geldiği `media_origins` tablosunda (migration `1420_media_origins`): `original` (Subtitle eklentisinin filmden yakaladığı), `clip` (yerel video dosyasından ffmpeg ile kesilen; `source` = dosya adı), `tts` (sağlayıcı, ses, hız × 1000, metnin SHA-256'sı), `user`. Satırı olmayan dosya `user`dır; adı `subtitle-…` ise `original` (B3'ün eski kuralı). Üreticiler dosyanın yanına yazar (Subtitle yüklemesi, `reading.clips.extract`, `media.audio.fill`; iç komut `media.origin.record`), türetilmiş üst veri olarak geri alınmaz (aynı bayt = aynı dosya = aynı köken). Okuma: `media.origins { hashes | note_id }`, karşılaşma satırlarında `facts.audio_origin`/`screenshot_origin`, sorguda `ses:özgün|klip|tts|dosya|video` (docs/02 §9.1). Eskiler: `subtitle_uploads` ve `subtitle-…` adlı dosyalar `original`, gerisi tahmin edilmez.
- `[v1.4 W4]` **Eksik sesi tamamla** (modül `audio_fill`, kapatılabilir): `media.audio.fill_preview` (yazmaz: kaç not, her birine hangi kaynak, TTS önbellekte mi, hangi dil için ses yok) ve iptal edilebilir iş `media.audio.fill { scope: {query | note_ids}, video?, offset_ms?, tts?, tts_text: term|sentence, provider?, voice?, rate? }`. Not türünün ilk ses alanı boş olan her not için sıra: birincil karşılaşmanın kendi sesi → verilen yerel videodan, karşılaşmanın başlangıç–bitişi varsa (±`audio_fill.clip_offset_ms`, ±250 ms pay; süre yoksa **asla** uydurulmaz; `video.source` verilirse yalnız o video kimliği/başlığı) → kayıtlı `TtsProvider` ile terimin (ya da cümlenin) okunuşu. TTS önbelleği = (sağlayıcı, dil, ses, hız, metin hash'i) → dosya; aynı kelime bir kez sentezlenir. Ses, Altyazı notlarında birincil karşılaşmaya da yazılır (`audio_hash` + `example_fields.audio`; birincil değişse de kaybolmaz). Bütün iş **tek geri alma adımı** (içe aktarmadaki gibi çökmeye dayanıklı geri alma grubu, `media.audio.attach` + `media.audio.fill_rollback` iç komutları); iptal yazılanı geri alır. Dil için ses yoksa önizleme ve rapor dil başına `no_voice` ("<dil> için kurulu çevrimdışı ses yok") ya da `no_tts_provider` der, sessizce düşmez.
- `[v1.4 W4]` **Windows çevrimdışı ses:** çekirdekte henüz yerleşik bir OS ses sağlayıcısı yok (masaüstü arayüzü WebView konuşmasına düşer). Yeri hazır: `TtsProvider` + `Registry::register_tts`; Windows'ta `Windows.Media.SpeechSynthesis` (WinRT, `windows` crate'i, `cfg(windows)`) ile `voices()` = kurulu sesler, `synthesize()` = WAV akışı yazan bir sağlayıcı eklenecek; `voices()` dili listelemezse iş bunu `no_voice` olarak raporlar. Bu ortamda (Linux konteyner) derlenemediği/denenemediği için eklenmedi.

## 14. Ayarlar

- Ayarlar **şemalıdır**: `{ key, type, default, min, max, enum, label_key, description_key, advanced, scope }`.
- Kapsamlar: `app`, `profile`, `preset`, `deck`, `plugin:<id>`. Değer çözümleme daha özelden daha genele.
- `settings.schema` sorgusu frontend'e formu çizmek için gerekeni verir. Ayar araması şemadaki etiket ve açıklamalar üzerinde çalışır.

## 15. Migration'lar

- Her şema değişikliği numaralı ve **geri dönüşsüz ama test edilmiş** bir migration'dır.
- Numaralar sürüm başına ayrılmış aralıklardadır (`fc-storage` `migrate::RELEASE_RANGES`; aralık dışı migration testleri kırar): v0.1 `1`–`99`, v0.2 `100`–`199` … v1.0 `1000`–`1099` (izlere bölünmüş, docs/07 §5.1), v1.1 `1100`–`1199` (migration yok), v1.2 `1200`–`1299` (`1200_subtitle`), **v1.3 `1300`–`1399`** (`1300_vocabulary_roles` ve `1301_subtitle_word_front`: yalnız veri; 1301 yerleşik Altyazı kartının ön yüzünü yalnız kelimeye indirir, yalnızca şablon hâlâ gönderildiği gibiyse), **v1.4 `1400`–`1499`** (`1410_word_index`: kelime dizini, §4.2; `1420_media_origins`: medya kökeni + TTS önbelleği, §13; `1421_typed_answers`: yazılan cevaplar; `1422_speed_indexes`: deste başına yeni kart sırası + canlı kart sayımı indeksleri, docs/12; `1430_subtitle_capture_log`: yakalama tanısı; `1440_word_data` + `1441_word_frequency_view`: kelime verisi, §4.3; `1450_meaning_in_context`: bağlamdaki anlam + kabul edilen cevaplar; `1460_language_queries`: not kökeni; `1470_leech_repairs`: zor kart onarımları, §4.4; `1490_subtitle_encounters`: karşılaşmalar, docs/06 §5.1). Dosyalar `crates/fc-storage/migrations/NNNN_ad.sql`; birleştikten sonra **asla düzenlenmez** (sağlama toplamı tutmazsa veritabanı açılmaz).
- Migration öncesi veritabanının otomatik anlık kopyası alınır (sadece migration sırasında, periyodik yedek değil).
- CI'da: her migration boş veritabanında ve önceki sürümlerden üretilmiş örnek veritabanlarında çalıştırılır.
- Veri formatları (deste paketi, yedek, eklenti manifesti) de sürüm numarası taşır.
- `[v1.4]` `.fcbackup` sürüm 2: zip içinde `collection.json` (deflate; v1.4 Z2'den beri depo onu tablo tablo, imleçle satır satır doğrudan girdiye yazar: `InteropRead::dump_collection_json`, bellekte döküm yok), `media/<sha256>` (sıkıştırmasız) ve **en son yazılan** `manifest.json` (koleksiyon girdisinin ve her medyanın boyutu + SHA-256'sı, yedek anında eksik medyalar). `fc-interop` yazıcı/okuyucusu `Write + Seek` / `Read + Seek` üzerinde girdi girdi çalışır (dosya sistemi yok). Yabancı paketlerin (`.apkg`, `.colpkg`, `.fcpack`) 1 GiB toplam açma bütçesi kalır; `.fcbackup`'ta toplam tavan yok, korumalar girdi başına: başlık boyutu = manifest, deflate oranı ≤ 1100:1, üretilen bayt = başlıktaki, CRC, hash. Sürüm 1 yedekler okunmaya devam eder. Uygulama tarafı (`fc-app` `modules/io/backup_file.rs`): `.part` → `fsync` → doğrulama (geçici SQLite profiline `load_collection_json` ile arşivden akarken 512 satırlık parçalarla yükleme — eski şemalı döküm bütün okunur, `upgrade_dump` + `load_collection` — + medya, geri okuyup karşılaştırma; geri yükleme de aynı akışla, temizleyiciler her parçaya uygulanarak yazar) → `rename`; kesilen/iptal edilen yazım önceki dosyaya dokunmaz. Otomatik yedek (`backup_auto.rs`): iş kuyruğunun ana şeridi boştayken en çok 10 dakikada bir enjekte edilen `Clock` ile vadesine bakar; "son doğrulanmış yedek" ve son hata koleksiyonda değil `backups/status.json`'da tutulur (geri yükleme onu geri sarmasın). Migration gerekmedi.

## 16. Güvenlik

- API anahtarları işletim sistemi anahtar zincirinde (`keyring` crate: Windows Credential Manager, macOS Keychain, Linux Secret Service).
- Kart içeriğindeki HTML temizlenir (sanitize). Kart şablonları betik çalıştıramaz.
- Tauri yetenekleri (capabilities) en az izin ilkesiyle tanımlanır. CSP sıkı.
- Yerel HTTP API sadece `127.0.0.1`'e bağlanır, token ister, CORS sadece izin verilen tarayıcı eklentisi kökenlerine açıktır.
- Eklentiler WASM sandbox'ında, sadece izin verilen host fonksiyonlarına erişir (bkz. [05](05-eklenti-sistemi.md)).
- Uygulama kilidi parolası Argon2 ile hash'lenir.

## 17. Performans

- SQLite: WAL modu, uygun indeksler (`cards(deck_id, due)`, `cards(note_id)`, `review_log(card_id, created_at)` …), hazırlanmış ifadeler.
- Büyük listeler sayfalıdır (cursor tabanlı). Frontend sanal kaydırma kullanır.
- İstatistik özetleri önbelleklenir, olaylarla geçersiz kılınır.
- Kıyaslama (benchmark) testleri: 100k not / 200k kart / 2M tekrar kaydı üzerinde, CI'da regresyon kontrolü.
- `[v1.4]` Film izleme yükü (20k Subtitle karşılaşması / 6k kelime, binlerce küçük WAV/JPEG, ~1 GiB medya, İngilizce kelime paketi) aynı takımda; ölçümler, makine ve debug/release farkı [12-performans.md](12-performans.md)'de.

## 18. Gözlemlenebilirlik

- `tracing` ile yapılandırılmış loglar, yerel dosyaya döner (rotation).
- Telemetri **yok.** Loglar sadece kullanıcı isterse dışa aktarılır.

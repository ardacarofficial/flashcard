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
  fc-providers/     Sağlayıcı arayüzleri ve yerleşik uygulamalar: AI, TTS, STT, çeviri,
                    sözlük, lemmatizasyon.
  fc-plugins/       Eklenti çalışma zamanı (WASM), host fonksiyonları, izinler, manifest.
  fc-interop/       Anki .apkg, deste paketi, yedek formatları.
  fc-http/          Yerel HTTP API sunucusu (axum).
  fc-mcp/           MCP sunucusu ve istemcisi.
  fc-cli/           Geliştirme/otomasyon için CLI (çekirdeğin Tauri'siz çalıştığının kanıtı).
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
| `notetypes` | id, name, kind, fields (JSON şema), templates (JSON), css, version | Alan şeması ve kart şablonları |
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

## 5. Olay kaydı ve geri al (hibrit yaklaşım)

Saf "event sourcing" (durumu her seferinde olaylardan yeniden üretmek) yerine **hibrit** bir model kullanılır: güncel durum normal tablolarda tutulur, her değişiklik **aynı transaction** içinde değişmez kayıtlara yazılır. Bu hem hızlı okuma hem tam geçmiş sağlar.

- **`review_log`:** Her tekrar değişmez bir kayıttır. İstatistikler ve FSRS optimizasyonu buradan hesaplanır. Bir kartın zamanlama durumu teorik olarak bu kayıttan yeniden hesaplanabilir (veritabanı kontrolü bunu doğrular).
- **`oplog`:** Her komut, kendisi ve **tersi (inverse)** ile kaydedilir.
  - Geri al = son işlemin tersini yeni bir komut olarak uygula (geçmiş silinmez).
  - Toplu işlem = aynı `batch_id` ile birden çok kayıt → tek geri al.
  - Not düzenleme geçmişi `oplog`'dan okunur.
- **Senkronizasyon için hazırlık:** `oplog` + UUID'ler + zaman damgaları ileride senkronizasyon altyapısının temelidir (kapsam dışı ama yolu kapatılmadı).
- `oplog` kayıtları belirli süre sonra (ayarlanabilir) sıkıştırılabilir. `review_log` asla silinmez (kart kalıcı silinmedikçe).

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

## 7. API sözleşmesi

- `fc-api` tüm DTO'ları, komut/sorgu adlarını ve hata kodlarını tanımlar.
- **TypeScript tipleri otomatik üretilir** (`specta` / `tauri-specta`). Frontend bu tiplerle derlenir. Backend'de bir tip değişirse frontend **derleme zamanında** kırılır, sessizce değil.
- **Sürümleme:** Komutlar `v1.note.create` biçiminde ad alanlıdır. Kırıcı değişiklik yeni sürüm ister. Eski sürüm bir geçiş süresi boyunca adaptörle desteklenir.
- **Taşıma bağımsızlığı:** Aynı sözleşme Tauri IPC, yerel HTTP (JSON), MCP (araç olarak) ve eklenti ABI üzerinden sunulur. Taşıma katmanları ince adaptörlerdir.
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
- Kuyruk oluşturma: vadesi gelenler, öğrenme adımları, yeniler; karıştırma stratejisi; kardeş gömme.
- Oturum durumu `sessions` tablosunda kalıcıdır (çökme kurtarma).
- Takvimi etkilemeyen oturumlar (deneme, test) `review_log`'a `mode` alanıyla yazar ama `cards` durumunu değiştirmez.
- Zamanlı mod, ipucu sayacı, cevap süresi ölçümü oturum motorundadır. Frontend sadece "gösterildi / çevrildi" zamanlarını bildirir (backend kendi zamanıyla doğrular).

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

## 14. Ayarlar

- Ayarlar **şemalıdır**: `{ key, type, default, min, max, enum, label_key, description_key, advanced, scope }`.
- Kapsamlar: `app`, `profile`, `preset`, `deck`, `plugin:<id>`. Değer çözümleme daha özelden daha genele.
- `settings.schema` sorgusu frontend'e formu çizmek için gerekeni verir. Ayar araması şemadaki etiket ve açıklamalar üzerinde çalışır.

## 15. Migration'lar

- Her şema değişikliği numaralı ve **geri dönüşsüz ama test edilmiş** bir migration'dır.
- Migration öncesi veritabanının otomatik anlık kopyası alınır (sadece migration sırasında, periyodik yedek değil).
- CI'da: her migration boş veritabanında ve önceki sürümlerden üretilmiş örnek veritabanlarında çalıştırılır.
- Veri formatları (deste paketi, yedek, eklenti manifesti) de sürüm numarası taşır.

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

## 18. Gözlemlenebilirlik

- `tracing` ile yapılandırılmış loglar, yerel dosyaya döner (rotation).
- Telemetri **yok.** Loglar sadece kullanıcı isterse dışa aktarılır.

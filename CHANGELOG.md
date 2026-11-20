# Changelog

Biçim: [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/), sürümleme: [SemVer](https://semver.org/lang/tr/).

## [Unreleased] — v0.7 eklenti altyapısı (iz F, `feat/v0.7-plugins`)

### Eklendi
- **`fc-plugins` crate'i:** WASM çalışma zamanı (wasmtime 50): çağrı başına **yakıt**, çağrının tamamına duvar saati **son tarihi** (epoch + host çağrısı giriş/çıkışı; host süresi sayılır), örnek başına **bellek sınırı**; her çağrı taze örnek; tuzak/panik/sonsuz döngü hata olur, uygulama çökmez. Host çağrıları çağıran iş parçacığında servis edilir (kapsamlı işçi iş parçacığı + kanallar), böylece host tarafı yazma işlemi dahil her şeyi ödünç alabilir. **ABI v1** (`fc_plugins::abi`: `fc_abi_version`/`fc_alloc`/`fc_handle` + `fc.host_call`/`fc.host_reply`, JSON zarflar) ve dondurulmuş elle yazılmış v1 fikstürü (`tests/fixtures/abi_v1.wat`). `plugin.toml` manifesti (izinler, katkılar, `api_version`, `min_app_version`), izin sözlüğü (API izinleri, `hooks:before|after:<metot>`, `hooks:event:<olay>`, `network:<host>`, `ui:*`, sağlayıcı izinleri; hassas izinler işaretli), `.fcplugin` paketi (boyut sınırlı okuma), `PluginRegistrySource` + `FileSource` (yalnız dosyadan kurulum, 08 K3), model indirme yöneticisi (`file://` getirici, devam ettirilebilir, SHA-256 doğrulamalı, disk kullanımı, silme).
- **`plugins` modülü:** `plugins.list/inspect/install/uninstall/set_enabled/set_permissions/settings.get/settings.set/locales/logs/run/ui.open/ui.act/ui.close/models.*` (+ iç `plugins.restore/kv.set/record_failure/rollback`). Kurma, güncelleme, kaldırma (isteğe bağlı veri silme), aç/kapa, izinler, ayarlar **geri alınabilir**. Eklenti API'si = public API'nin izinlerle süzülmüş hali (`ensure_invocable`, eklenti aktörü için izin kaydı `Registry::permits`). Kancalar: `before:` (parametreyi değiştir/reddet; hata = eklentisiz devam, ayarla `reject`), `after:`, `event:`; eklentinin yaptığı değişiklikler kanca tetiklemez. Eklenti işlemi (komut, görünüm, çalışma modu adımı) **tek geri al adımı**; eklenti hata verirse yazdıkları geri sarılır. Art arda `plugins.max_failures` hata → otomatik kapatma + `plugins_changed{auto_disabled}`.
- **Katkılar:** kart türü (`CardTypeEngine`), TTS sağlayıcı (`TtsProvider`), komutlar (çalışma zamanında kayıtlı `v1.plugin.<id>.<komut>`; palette ve `commands.list`'te), bildirimsel arayüzlü görünümler ve çalışma modları, ana menü / kenar paneli / kart altı düğme katkıları (`modules.list`'te eklenti modülleri), şemalı eklenti ayarları, eklentiye özel KV, eklentinin kendi çevirileri.
- **`tts` modülü:** `tts.providers/voices/synthesize` (yerleşik ya da eklenti sağlayıcıları; ayar `tts.provider`).
- **Kayıt defteri çalışma zamanı kaydı:** `Registry::register_dynamic` (`Arc<str>` anahtarlı metotlar), `register_card_type`/`register_tts` (`&self`), eklenti izinleri; statik katalog aynen. Yeni işleyici türü `Registry::action` (işlem dışı çalışan metotlar).
- **SDK:** `plugins-sdk/rust` (`fc-plugin-sdk`: `Plugin` trait'i, `export_plugin!`, tipli `host::call::<M>()` (tipler fc-api'den), `kv`, `settings`, `log`, `notify`, arayüz kurucuları, yerel testler için sahte host) ve örnek eklentiler: `autofill` (kanca ile alan doldurma), `scramble-card` (kart türü), `tone-tts` (TTS), `reverse-drill` (çalışma modu), `misbehave` (test: döngü/panik/bellek/izinsiz çağrı), `escape` (test: sanal alandan kaçış denemeleri).
- **Masaüstü:** Eklentiler ekranı (dosyadan kur + izin onayı, aç/kapa, izinler, ayarlar, günlük, kaldır, indirmeler), bildirimsel arayüz çizicisi, `/plugins/view/<id>/<görünüm>` rotası, kenar paneli ve kart altı düğmeleri; eklenti metinleri `plugins.locales`'tan.
- Migration **`0600_plugins`** (`plugins` satırına çalışma durumu sütunları, `plugin_packages`).

- **`fc plugin` CLI:** `list`, `inspect`, `install [--grant …|--grant-all]`, `uninstall [--delete-data]`, `enable`, `disable`, `grant`, `logs`, `run`, `pack`.
- **`fs:pick`:** host fonksiyonları `fs.open`/`fs.save`; yerel dosya penceresini host gösterir (`fc_app::FilePicker`, masaüstünde `rfd`), eklenti yalnız dosya adı + bayt alır. Eklentinin kendi ayarı için `settings.set` host fonksiyonu. SDK: `host::fs`, `host::set_setting`.
- Saldırgan test eklentisi `plugins-sdk/examples/escape` ve kaçış testleri (kanca, serbest yol, ayar, depo geri sarma).

### Güvenlik (bağımsız inceleme bulguları)
- `before:`/`after:` kancaları artık `plugins.*`, `oplog.*`, `modules.*`, `settings.*`, `shortcuts.*`, `io.*`, `batch.*`, `jobs.*`, `mcp.*` ve AI sağlayıcı/anahtar metotlarına takılamaz; her kanca hedef metodun kendi iznini de ister (önceden bir eklenti kullanıcının izin değişikliğini ya da dışa aktarma yolunu yeniden yazabiliyordu).
- `io:*` (serbest dosya yolu), `settings:write`, `modules:write`, `oplog:write`, `mcp:*`, `ai_keys:*`, `ai_requests:*`, `ai:write` eklentilere verilemez; `Registry::permits` eklenti için bunları kayıtlı izin olsa bile reddeder (eklenti kendi yakıt/bellek sınırını yükseltemez).
- Eklenti deposu ve ayar yazımları işlemin geri al grubuna girer: eklenti hata verirse geri sarılır.
- **Kanca zinciri (yeniden inceleme):** hata veren eklenti çağrısının geri sarılması artık eklentinin aktörüyle çalışır (önceden `System`: geri sarmanın olayları `event:` kancalarını yeniden tetikliyor, her işlemde katlanarak büyüyordu). Ek olarak iş parçacığı başına **yeniden giriş koruması** (kanca çalışırken olan hiçbir şey, aktörü ne olursa olsun, `event:` kuyruğuna girmez; kanca kanca içinde başlamaz) ve **işlem başına kanca bütçesi** (`plugins.max_hooks_per_action`, varsayılan 200): aşılınca işlemin kalan kancaları atlanır, eklenti kapatılır, kullanıcıya i18n bildirim (`plugins_changed.reason = plugins.notice.hook_budget`). Test: `failed_hook_rollback_does_not_cascade`, `hook_budget_per_action` (WAT fixture'ları).
- **Bellek sınırı (yeniden inceleme):** multi-memory, threads/paylaşımlı bellek, memory64, özel sayfa boyutu, GC, istisnalar, stack switching, component model, wide arithmetic, relaxed SIMD kapalı; `Limiter` tüm bellekleri ve tabloları **toplam** sayar, örnek başına 1 bellek, en çok 4 tablo. 99 × 64 MB'lık modül derlenmez (`many_memories.wat`).
- **Eklenti deposu (yeniden inceleme):** `kv.set` artık manifestte istenip verilmiş **`storage:kv`** izni ister (silme her zaman serbest); eklenti başına kota `plugins.kv_max_keys` (10 000) ve `plugins.kv_max_mb` (16) — uygulama ayarı, eklenti değiştiremez; büyütmeyen yazım ve geri yükleme (`SetPluginKv.restore`) kotaya takılmaz. `PluginRead::plugin_kv_usage`.

- **Kaynak sınırları (üçüncü inceleme):** (1) *Son tarih atlatma:* son tarih artık host çağrılarında geçen süreyi de sayar (her host çağrısının girişinde ve çıkışında + epoch; yalnız dosya penceresinde kullanıcıyı bekleme hariç, `HostCalls::paused`); çağrı başına en çok `plugins.max_host_calls` (10 000) host çağrısı ve `plugins.host_mb_per_call` (64) MiB sınırdan geçen veri (`HOST_LIMIT`, arıza); her host çağrısı yakıt tüketir (50 000 + işlevin kendi bedeli + bayt başına 4). Bir işlemin kancaları toplam `plugins.hook_ms_per_action` (10 000 ms) ile sınırlı, kancası arıza veren eklenti o işlemde atlanır: 1 000 000 `kv.get` döngüsü komut olarak da kanca olarak da kısa sürede durur. (2) *Host belleği atlatma:* her host fonksiyonu için istek/yanıt boyutu, çağrı başına ve dakikalık sayı sınırı (`fc_plugins::limits::HOST_FNS`, docs/05 §12 tablosu); istek boyutu ayrıştırılmadan denetlenir, aşan çağrı `VALIDATION_PLUGIN_HOST_LIMIT` alır. `ui.notify` artık **`ui:notify`** izni ister (3/çağrı, 10/dakika, ≤ 4 KiB): 400 × 1 MiB bildirim reddedilir, tepe bellek ~2 MiB (entegrasyon testi `plugin_host_memory`). `kv.list` sayfalı (`after`, `limit`; sayfa ≤ 1000 kayıt / ≈ 4 MiB; `PluginRead::list_plugin_kv_page`). İşlem yanıtı ≤ 4 MiB (TTS 16 MiB) ayrıştırılmadan denetlenir; girdi > 16 MiB ise eklenti çağrılmaz (`INPUT_TOO_LARGE`). Arayüz oturumu `state` ≤ 1 MiB, eklenti başına 8 oturum; `event:` kuyruğu ≤ kanca bütçesi + 1; manifest katkı listeleri ≤ 64, izin ≤ 128, ayar şeması ≤ 100. Günlük satırı kesilirken çok baytlı karakterde panik düzeltildi.
### Değişti
- Workspace sürümü `0.7.0`; `fc_plugins::APP_VERSION` artık `CARGO_PKG_VERSION`. Eklentilere açık v1 metot anlık görüntüsünden (`api_v1.json`) eklentiye verilemeyen metotlar çıktı. `ErasedHandler::validate` (kapsamlı `match` yerine).
- `ensure_invocable` artık kayıt defterini alır; eklenti aktörleri yalnızca verilen izinlerle çağırabilir (önceden her şey serbestti). `Registry::card_type_engine`/`card_types` artık `Arc` döndürür.
- İki yedek testi (`upgrade_dump_runs_later_data_migrations_on_restored_rows`, `restoring_an_older_schema_backup_migrates_it_forward`) sonraki sürümlerin migration'larına dayanıklı hale getirildi.
- `rust-toolchain.toml` ve CI `wasm32-unknown-unknown` hedefini kurar (örnek eklentiler testlerde derlenir).

## [Unreleased] — Faz 2 entegrasyonu (v0.2–v0.5 birleşik, son inceleme bekliyor)

### Faz 2 entegrasyonu — tur 3 (+ v0.4, `feat/phase2-integration`)
- **Tek preset okuma yolu:** çalışma deste ayarlarını yalnız `presets::effective` ile okur (deste → üst deste → varsayılanlar); tur 1'in `engine::preset_of`'u (yalnız destenin kendi şablonu) kalktı. Tek şema (`PresetConfig`): limitler, (yeniden) öğrenme adımları, hedef hatırlama, **`fsrs_weights`** (önceden `extra.fsrs_weights`), yeni **`maximum_interval_days`** (önceden `extra`; aynı anahtar, eski satırlar kayıpsız okunur), kardeş gömme, yeni/tekrar sırası (`retrievability` = en düşük hatırlama önce, `fc_study::session::Order::Preset`). `study.bury_siblings` ve `study.order` ayarları kalktı (preset'te); kardeş gömme varsayılanı **açık** (docs/02). Masaüstü preset düzenleyicisine "En uzun aralık".
- **Özel bayrak adları sorguda:** `browse::flag_names` ← v0.3 `card_flag_defs` (`flag:önemli`, boşluklu ad `_` ile ya da tırnakla); renk adları TR/EN her zaman; bayrak *n* = bit *n − 1*. `bulk.set_flag` geçersiz bayrakta `VALIDATION_FLAG` (v0.3 ile aynı). Kart tarayıcısının bayrak listesi `flags.list` adlarını gösterir.
- **Özel çalışma oturumları sorgu dilinden:** `StudyFilter.query` (`seviye:A2 is:due`) ve `StudyFilter.saved_query_id` (kayıtlı sorgu = filtreli deste; kanonik saklandığı için seçenek adı değişse de çalışır); oturum `sessions.query`'ye yazılır. Çalışma ekranında sorgu kutusu ve kayıtlı sorgu seçimi.
- **`bulk.change_notetype`:** seçili notları (tek kaynak türünden) başka bir not türüne taşır; alan eşleme (hedef ← kaynak; varsayılan: aynı anahtar, sonra sıra), şablon eşleme (eşlenen şablonun kartı kimliği, zamanlaması ve geçmişiyle kalır; cloze/maske anahtarı `c<n>`/`m<n>` korunur), eşlenmeyen kartlar çöpe, eksikler üretilir. Tek komut = tek geri al (`bulk.restore_notes` artık not türünü, `bulk.restore_cards` şablon anahtarı ve motoru da geri yazar; üretilen kartlar geri alınca çöpe gider). Kart tarayıcısında "Not türünü değiştir".
- **Arama dizini düz metin:** not yazımı ve içe aktarma dizine cloze cevaplarını (`{{c1::Ankara::şehir}}` → `Ankara`), bağlantı etiketlerini, HTML'siz metni yazar; resim/ses/video alanları ve çıplak `media:<hash>` dizine girmez (`fc_core::note::index_text`). Migration **`0302_search_plain_text`** dizini notlardan yeniden kurar (SQL fonksiyonları `fc_index_text`/`fc_index_sort`); yedekten geri yükleme dizini her zaman notlardan yeniden kurar.
- **Komut paleti ve kısayollar:** palet dört izin tüm public komutlarını TR/EN etiketle listeler (`locales/*/commands.json`); kısayol düzenleyici çalışma tuşlarını kapsar (`study.reveal/again/hard/good/easy/hint/suspend/bury/undo/end`, bağlam `study`). Ctrl/⌘+Z çalışmada artık kabuğun genel `ui.undo`'su (önceden ikisi birden geri alıyordu); çalışma ekranı dışarıdan gelen geri al/yinele'de kendini yeniler.
- `browse::PurgeTrashJob` v0.5 iş havuzuna kaydedildi (`JobContext::invoke`); açılışta çöp temizliği yarım kalmış geri al gruplarının geri sarılmasından sonra koşar.
- **Testler:** `phase2_manage_tests.rs` × 3 depo (preset → kuyruk/aralık + miras + geri al; özel bayrak adları; sorgudan/kayıtlı sorgudan özel oturum; not türü değiştirme + birebir geri al/yinele; düz metin dizini, içe aktarılan notlar dahil; palet + çalışma kısayolları); `migration_0302_rebuilds_the_index_as_folded_plain_text`; eski şemalı yedek bayat dizinle geri yüklenir → birebir; `preset_orders`; `index_text_strips_markup`; `keys.test.ts` yeniden bağlanan tuşlar. v0.4'ün `negation_is_complement` proptest'i `or/and/not` kelimesinde yanlış alarm veriyordu (operatör), düzeltildi.

### Faz 2 entegrasyonu — tur 2 (+ v0.5, `feat/phase2-integration`)
- **Medya tek depoda:** içe/dışa aktarma ve yedek v0.3'ün içerik adresli dosya deposunu kullanır (`io::media::FileMedia`); geçici `BlobMedia` ve `media_blobs` kalktı — `0402_drop_media_blobs`, açılışta kalan baytları önce dosyalara taşır. Aynı içerik diskte tek dosya. Yedekler medya dosyalarını içerir, geri yükleme onları yeniden yazar.
- **Anki → v0.3:** Anki'nin hazır not türleri (Basic, ters, isteğe bağlı ters, yazarak, Cloze, Image Occlusion) yapılarından tanınıp yerleşik türlere eşlenir; Image Occlusion şekilleri v0.3 maskelerine çevrilir. `[sound:x]` ↔ `<audio src="media:…">`, resim/ses alanlarında çıplak `media:` başvurusu. İçe aktarılan kartlar kart türü motorlarıyla çizilir.
- **Anki bayrakları** numara ↔ bit doğru çevrilir (7 = mor = `card_flag_defs` 7; önceden numara bit maskesi gibi yazılıyordu).
- **İçe aktarılan geçmiş** `review_log.mode = "imported"`: kart geçmişinde görünür, günlük çalışma limitlerine sayılmaz.
- **Satır içe aktarma** (metin/CSV/JSON) kartları v0.3 motorlarından üretir: boşluk doldurma türüne de satır aktarılır.
- Yarıda atılan içe aktarma, eklediği ve kimsenin kullanmadığı medya satırlarını da siler (`ImportedIds.media`).
- **Dokümantasyon:** v0.5 ayırıcı algılama kriteri **kısmen** karşılandı olarak işaretlendi (tutulmuş sette %92,9, 65/70); v1.0'a iyileştirme maddesi eklendi.
- **Testler:** `phase2_io_tests.rs` uçtan uca (CSV → sınıflandırma, Anki cloze/resim/ses/geçmiş → diskte tekil medya → çizim → çalışma, limit → yedek → koleksiyon klasörünü sil → geri yükle → birebir); Anki yerleşik eşleme ve resim gizleme dönüşümü; 0402 taşıma migration testi.

### Faz 2 entegrasyonu — tur 1 (v0.2 + v0.3, `feat/phase2-integration`)
- **Çalışma kartları kart türü motoruyla çizilir:** `StudyCardDto` artık `sides` (temizlenmiş HTML, CSS, etkileşim, TTS) ve `flags` taşır; düz metin `question/answer` ve `fc_study::render` kalktı (`fc_study::text::html_to_text` kaldı). Çalışma ekranı `CardView` ile medya, KaTeX, kod ve not türü CSS'ini gösterir.
- **Boş kartlar** (silinen cloze/maske) kuyruğa girmez, oturumda atlanır, puanlanamaz.
- **Cevap kontrolü motor üzerinden:** yazarak türler v0.2 kontrolcüleriyle, çoktan seçmeli motorla (`CheckAnswer.choice`); soruda 1–9 seçenek seçer.
- **Preset günlük limitleri:** normal oturum `new_per_day`/`reviews_per_day`'den bugün çalışılanı düşer (`StudyRead::review_logs_since`, `BuildOptions.review_limit`).
- **Bayraklar çalışmada:** Anki numaralı adlar/renkler (`flags.list`), Ctrl/⌘+1–7 aç/kapa.
- **Testler:** `phase2_tests.rs` uçtan uca (cloze + klasik + Seviye → çalış → puanla → geri al birebir; deneme modu `cards`'ı değiştirmez; günlük limitler) × 3 depo; `review_logs_since` depolama testi; `review_limit` kuyruk testi; tuş haritası ve bayrak bileşen testi.

## [0.5.0] — Yayımlanmadı · İçe / dışa aktarma (iz D, `feat/v0.5-io`)

### Eklendi
- **`fc-interop` crate'i** (saf, bayt girer bayt çıkar): esnek ayırıcılı metin/CSV/TSV (tırnak, özel ayırıcı, `max_fields`) + ayırıcı otomatik algılama; kendi JSON not biçimimiz; Quizlet metni; Anki `.apkg`/`.colpkg` okuma (eski `collection.anki2`/`.anki21` ve yeni zstd `collection.anki21b` + protobuf) ve yazma (şema 11) — cloze, medya, zamanlama ve tekrar geçmişi korunur; `.fcpack` deste paketi (sınıflandırma tanım/seçenek/değerleri kimlikleriyle); `.fcbackup` tam yedek; yazdırma HTML'i (çift taraflı baskı için aynalı arka yüzler).
- **İş kuyruğu çalışanı:** `fc_app::jobs::spawn_worker`, `App::run_pending_jobs`, modül başına `Registry::job`; `jobs.cancel` işin iptal işaretini yazıcıyı beklemeden verir; işler yazmayı yalnızca komutlarla yapar.
- **`io` modülü:** `io.import.preview/start`, `io.export.start`, `io.backup.create/inspect/restore`, `io.print.render`. İçe aktarma tek işlem + **tek geri al** adımıdır, yazarken iptal edilirse tamamen geri sarılır; tekrar edenler için atla/güncelle/yine de ekle; sütunlar alan/etiket/sınıflandırmaya eşlenir (istenirse eksik seçenekler oluşturulur). Geri yükleme önce mevcut durumun yedeğini alır.
- **Depolama:** `0400_interop` (medya içeriği `media_blobs`), medya/tekrar kaydı/koleksiyon döküm-yükleme repo alanı.
- **Masaüstü:** İçe/dışa aktar ekranı (önizleme, sütun eşleme, ilerleme + iptal, geri al), dışa aktarma, yedek/geri yükleme, yazdırma; Tauri dosya diyaloğu (yalnız aç/kaydet izni); TR/EN.
- **CLI:** `fc call` kuyruğa aldığı işi hemen çalıştırıp bitmiş işi yazar.

### Değişti
- `JobDto.result` ve `imported` / `collection_replaced` olayları eklendi (eklemeli).
- **v0.5 inceleme düzeltmeleri:** büyük içe aktarma ~500 satırlık parçalarla kendi işlemlerinde yazılır (çalışırken çalışma/düzenleme komutları zaman aşımına uğramaz), yine tek geri al adımıdır; iptal işlenmiş parçaları iz bırakmadan kaldırır (`io.import.discard`). Zip/zstd bombalarına toplam bütçe. Windows-1254 ve UTF-16 metin dosyaları. Eski şemalı yedek geri yüklenirken sonraki migration'lardan geçirilir; güvenlik yedeği uygulama veri klasörüne yazılır. İş sonu durumu yazımı yeniden denenir. Anki gömülü kartları korunur. Medya `MediaStore` trait'i arkasında (v0.3 deposuna bağlanmaya hazır), alan referansı `media:<sha256>`. Ayırıcı algılama için ayrı tutulmuş örnek seti (40/46). Ardından: algılama sayı/saat/URL parçalarını maskeler, gerekirse `max_fields` önerir, `::`/`–`/`—`/`→` adayları, Anki `#separator:` başlıkları (2. tutulmuş set: ilk ölçüm 63/70, şimdi 65/70). **Çökme güvenliği:** içe aktarma parçaları kalıcı bir işaretle (`0401_unfinished_batches`) izlenir; uygulama parçalar arasında kapanırsa açılışta işlenmiş parçalar geri sarılır.

## [0.4.0] — Yayımlanmadı · Kart yönetimi (iz C, `feat/v0.4-manage`)

### Eklendi
- **Sorgu dili (`fc-core::query`, saf):** ayrıştırıcı → sözdizimsel AST (karakter konumlu hatalar) → katalogla çözümleme → `Filter`; bellek içi referans değerlendirici. Serbest metin (FTS5 önek/ifade, `*` joker), `deck:`/`deste:`, `tag:`/`etiket:`, `notetype:`, `type:`, `card:`, `is:`, `flag:`, `stability/difficulty/lapses/reps/retrievability` karşılaştırmaları, `added/edited/reviewed/due:N`, `re:`, alan joker/regex, `nid:`/`cid:`, `-`/`NOT`/`OR`/parantez. **Sınıflandırmalar** anahtar ya da (yerelleştirilmiş) adla: `seviye:B1`, `seviye>=B1` (sıralı seçimde seçenek sırasıyla), `sınav:TYT ders:fizik`, sayı/tarih/evet-hayır/metin, `x:*`. Türkçe katlama (ı/İ/ş/ç/ğ/ö/ü).
- **fc-storage `search` alanı:** `Filter` → SQL + FTS5 (alan jokerlerinde FTS ön süzgeci), SQL fonksiyonları (`fc_fold`, `fc_glob`, `fc_html_glob`, `fc_regex`…), kayıtlı sorgular, tekrar grupları, çöp listesi, kart tekrar geçmişi, bütünlük raporu. Migration `0300_search` (mevcut FTS satırlarını katlar, sıralama indeksleri).
- **API (eklemeli):** `search.cards/ids/check`, `saved_queries.*`, `bulk.move_cards/set_suspended/set_buried/set_flag/reset_progress/add_tags/remove_tags/find_replace/set_attribute` (+ iç `bulk.restore_*`), `browse.card_info/duplicates/trash/check_database/repair_database`, `palette.list`, `shortcuts.list/set/reset`; hata kodları `VALIDATION_QUERY` (+`kind/start/end`), `VALIDATION_SAVED_QUERY_NAME`, `VALIDATION_SHORTCUT`, `VALIDATION_FIND_REPLACE`, `NOT_FOUND_SAVED_QUERY`, `NOT_FOUND_SHORTCUT_ACTION`, `CONFLICT_SHORTCUT`; olaylar `cards_changed`, `saved_queries_changed`.
- **fc-app modülleri `browse` ve `palette`** (ayrı ayrı kapatılabilir). Her toplu işlem tek komut = tek geri al (birebir anlık görüntü tersi).
- **Masaüstü:** kart tarayıcısı (canlı konumlu sorgu hatası, kayıtlı sorgular, sıralanabilir tablo, not modu, "eşleşenlerin hepsini seç", toplu işlem çubuğu, kart bilgisi), bakım ekranı (çöp kutusu, tekrarlar, veritabanı denetimi + onarım), kısayol düzenleyici (kaydet/kaldır/varsayılan, çakışmalar), Ctrl+K komut paleti (tüm public metotlar + ekranlar).

- **Deste ayar şablonları (`presets` modülü):** `presets.list/get/for_deck/create/update/clone/delete/assign` (+ iç `presets.restore`), Rust'ta doğrulanan şemalı ayar (`PresetConfig`: yeni/gün, tekrar/gün, (yeniden) öğrenme adımları, hedef hatırlama, FSRS parametreleri, kardeş gömme, yeni/tekrar sırası; şema docs/10 §16.1), destede yoksa en yakın üst destenin şablonu, o da yoksa varsayılanlar (`modules::presets::effective`, v0.2'nin okuyacağı tek yer). Hepsi geri alınabilir. Hata kodları `VALIDATION_PRESET_NAME`, `VALIDATION_PRESET_CONFIG` (`field`), `NOT_FOUND_PRESET`, `CONFLICT_PRESET_NAME`; olay `presets_changed`. Masaüstü: "Deste ayarları" düzenleyicisi.
- **Çöp saklama süresi:** ayar `browse.trash_retention_days` (varsayılan 30, 1–3650), `browse.purge_trash` (kalıcı silme; `older_than_days: 0` = çöpü boşalt), açılışta otomatik temizlik ve v0.5 işçi havuzu için `PurgeTrashJob`. Bakım ekranında "Çöpü boşalt".
- **Veritabanı denetimi:** alanı eksik notlar (`broken_fields`, onarım: boş alan eklenir, geri alınabilir) ve sahipsiz sınıflandırma değerlerinin silinmesi.
- Migration `0301_search_covering` (arama birleşimi için kapsayan indeksler + çöpteki notların kısmi indeksi).

### Değişti
- SQLite bağlantı başına sayfa önbelleği 64 MiB (`CACHE_SIZE_KIB`); FTS metni katlanmış yazılır.
- **Bayrak numaraları Anki'ninki:** 1 kırmızı, 2 turuncu, 3 yeşil, 4 mavi, 5 pembe, 6 turkuaz, 7 mor (önceden 5 = mor); arayüz 7 bayrağın hepsini sunar. Özel bayrak adları (v0.3 `card_flag_defs`) için tek bağlama noktası `browse::flag_names`.
- **Kayıtlı sorgular kanonik saklanır:** sınıflandırma anahtarları `attr_<id>`, seçenek etiketleri seçenek kimliği olarak; gösterilirken güncel adlarla yazılır. Seçenek/anahtar yeniden adlandırılınca sorgu bozulmaz, `seviye>=B1` güncel sıralamayı izler.
- **Arama hızı:** sayım ile sayfa ayrıldı (pencere sayımı her eşleşmeyi sıralatıyordu); not sütunu gerekmeyen sorgular `notes` birleşimi olmadan, büyük sonuçlarda sayfa sıralama indeksinden okunur. Yaygın etiket sorgusu ~90 → ~50 ms (100k).
- Geri al, toplu işlemlerde `updated_at`'i eski haline getirir (geri alınan değişiklik `edited:N`'de görünmez).

### Testler
- Proptest: yazdır→ayrıştır gidiş-dönüş, ayrıştırıcı toplamlığı + sınır içi hata konumları, olumsuzlama tümleyeni; SQL ↔ referans değerlendirici (her atom + 300 rastgele bileşik sorgu); 1000 kartlık toplu işlemlerin tek geri alla birebir dönüşü; palet tamlığı; 100k not benchmark'ı (`#[ignore]`, docs/04) ve CI'da koşan 20k benchmark'ı (`bench_search_20k_scaled`, bütçe 60 ms); sıralı tarama = tam sıralama testi; kayıtlı sorgunun yeniden adlandırma/sıralama sonrası çalışması; şablon CRUD/miras/geri al; çöp temizliği; denetimde eksik alan ve sahipsiz değer onarımı.

## [0.3.0] — Yayımlanmadı · Kart türleri ve içerik (iz B, `feat/v0.3-cards`)

### Eklendi
- **Şablon dili** (`fc-core::content::template`, saf): `{{alan}}`, `{{#alan}}…{{/alan}}`, `{{^alan}}`, `{{text:}}`, `{{tts:[dil:]}}`, `{{type:[cloze:]}}`, `{{hint:}}`, `{{cloze:}}`, `{{FrontSide}}`/`{{Deck}}`/`{{Tags}}`/`{{Card}}`/`{{Type}}`; şablon doğrulama ve anahtar üretimi.
- **İçerik:** Markdown (tablo, üstü çizili, görev listesi, `$…$`/`$$…$$` → `span.math`), cloze `{{c1::cevap::ipucu}}`, `[[bağlantı|etiket]]`, `media:<sha256>` başvuruları. **HTML temizleme Rust'ta** (ammonia, izin listesi; yalnız `media:`/`note:`/http(s)/mailto URL'leri, sınırlı sınıf ve stil) + CSS temizleme; 36 XSS vektörü testi.
- **`CardTypeEngine`** üretir / gösterir / değerlendirir; motorlar `Registry::card_type` ile kaydolur. Yerleşik motorlar: klasik, yazarak, cloze, çoktan seçmeli (deterministik karıştırma, desteden otomatik çeldirici), resim üzerinde gizleme (sabit numaralı maskeler, iki mod).
- **Hazır not türleri** (ilk açılışta tohumlanır, TR/EN adlarla): Klasik, Klasik + ters, İsteğe bağlı ters, Yazarak cevaplama, Boşluk doldurma, Çoktan seçmeli, Resim üzerinde gizleme, Resimli quiz, Sesli kart, Kelime kartı.
- **Not türü editörü** (`notetypes.create/update/delete/restore/usage`): alan ekle/çıkar/yeniden adlandır/sırala, şablonlar, CSS; kartlı şablon silinemez, yeni şablon mevcut notlara kart üretir, alan çıkarma geri alınabilir (`notetypes.restore_version`).
- **Kart gösterimi ve önizleme** (`cards.render`, `cards.preview`): temizlenmiş HTML, etkileşim (kendin puanla / yaz / seç), TTS istekleri; not düzenlenince yeni kartlar (yeni cloze, dolan isteğe bağlı ters) üretilir, mevcut kartlar asla silinmez.
- **Medya deposu** (`media.add/get/list/read`): içerik hash'iyle tekilleştirme (`media/<ab>/<sha256>`), MIME algılama, başvuru sayımı; masaüstünde `fcmedia://` protokolü.
- **`[[Bağlantılar]]` ve geri bağlantılar** (`links.backlinks`, `links.suggest`): okuma anında çözülür (sonradan yaratılan nota bağlantı da çalışır).
- **Bayraklar** (yedi renk, özel adlar; `flags.list/update`, `cards.set_flag/set_flags`) ve **kart notu** (`cards.set_user_note`, `cards.meta`).
- **Modüller:** `cardtypes` (omurga), `media` (omurga), `links`, `flags`, `card_notes`, `math`, `code_highlight` (son ikisi yalnız arayüz anahtarı).
- **Migration `0200_card_content`:** `note_media`, `note_link_refs`, `note_keys`, `card_flag_defs` (+ test).
- **Masaüstü:** not türü ekranı (canlı önizleme, sınıflandırmalar alanların yanında), her not türü için not editörü (Markdown araç çubuğu, yapıştır/sürükle-bırak medya, `[[` tamamlama, maske çizici, kart önizleme), not listesinden düzenleme (kart bayrakları, kart notu, geri bağlantılar), KaTeX ve kod renklendirme (tembel yükleme); TR/EN metinler.

### Değişti
- `notes.create` kartları not türü şablonlarının motorlarından üretir (`standard_card_specs` yerine); `notes.update` eksik kartları ekler (tersi `cards.trash_generated`).
- CSP: `style-src 'unsafe-inline'` (kart CSS'i, KaTeX), `media-src fcmedia:`, `font-src data:`.

### Düzeltildi (inceleme)
- Not türü CSS'i kapsamdan kaçamaz: `sanitize_css` kaçışları normalleştirir, parantezleri dengeler; frontend dengesiz CSS'i basmaz.
- Silinen cloze numarasının kartı `empty: true` (`cards.render`, `cards.meta`); çalışma atlar.
- Çeldirici havuzu tek sınırlı, tohumlu sorgu (deste boyunca not okuma yok).
- `media.ref_count` okunurken sayılır (kalıcı silmede bayatlamaz); geri alınan `media.add`'in dosyası açılışta temizlenir.
- Bayrak numaralaması Anki'ninki (1 kırmızı … 5 pembe, 7 mor), `card_flag_defs` tek kaynak.
- Kullanılmayan `NoteAddForm` kaldırıldı.

## [0.2.0] — Yayımlanmadı · Çalışma çekirdeği (iz A, `feat/v0.2-study`)

### Eklendi
- **`fc-study` (yeni saf crate):** FSRS zamanlayıcı (`fsrs-rs` 7, **FSRS-6** modeli, varsayılan ağırlıklar; ağırlıklar preset'ten değiştirilebilir) ve SM-2 yedeği aynı `Scheduler` arayüzünün arkasında; Anki uyumlu öğrenme/yeniden öğrenme adımları (adım `due - last_review`'dan çıkarılır, şema değişmez); cevap kontrolcüleri (birebir, normalize — Türkçe harf eşdeğerliği ayarlı —, yazım toleranslı, çoklu cevap; `doğru/neredeyse/yanlış` + fark); ipucu stratejileri (ilk harfler, kelime başları, kısmi); çalışma kuyruğu (öğrenme → vadesi gelen → yeni, kardeş gömme, sıralama, tohumlu karıştırma); çalışma günü sınırı (yerel saat + devir saati; FSRS'in `elapsed_days`'i takvim günüdür, vade gün başına hizalanır). G/Ç yok; saflık betiği artık bu crate'i de denetler.
- **`study` modülü (fc-app):** kalıcı oturum motoru (`sessions`, çökme kurtarma: `study.active`), `study.start/session/answer/hint/check_answer/end/summary`, `study.suspend/bury/postpone`, `study.card_history` (geçmiş + hatırlama olasılığı), iç ters metot `study.restore`. Puanlama tek adımda geri alınır: kart sütunları, gömülen kardeşler, oturum durumu ve `review_log` satırı birebir döner. Deneme modu `review_log`'a `mode = "trial"` yazar, `cards`'a hiç dokunmaz. Yanlışları tekrar (mistakes) oturumu, sınıflandırmaya göre özel oturum (yapılı filtre; v0.4 sorgu dili gelince bağlanacak), cevap süresi (istemci bildirir, arka uç kendi saatiyle sınırlar). Ayarlar: zamanlayıcı, oturum başına yeni kart, sıra, kardeş gömme, yazarak cevap, cevap kontrolcüsü, Türkçe eşdeğerlik, otomatik puanlama, ipucu türü, gün başlangıç saati.
- **Depolama:** `0100_study.sql` (oturum ve açık oturum indeksleri), `StudyRead/StudyWrite` (oturumlar, `review_log`, sınıflandırma koşullu aday kartlar), SQLite + `MemStore` uygulamaları.
- **API:** `v1.study.*` DTO'ları ve 13 metot, hata kodları `NOT_FOUND_SESSION`, `CONFLICT_SESSION_ENDED`, `CONFLICT_NOT_CURRENT_CARD`, `VALIDATION_SCHEDULER_PARAMS`, olay `study_cards_changed`; TS tipleri üretildi.
- **Masaüstü:** Çalış ekranı (`features/study/`): tamamen klavyeyle (Boşluk göster, 1–4 puanla, H ipucu, S askıya al, B göm, Z geri al, Esc bitir, R yanlışları tekrar), sonraki aralık butonların üstünde, deneme modu, sınıflandırma filtresi, erteleme, oturum sonu ekranı (sayı, doğruluk, süre, yanlışlar), yazarak cevap + fark; TR/EN metinler; geliştirme mock'u.
- **CLI:** study komutlarının geri al özetleri (TR/EN).

### Testler
- `fc-study`: 35+ test — FSRS-6 formüllerinin bağımsız referans uygulamasıyla 365 günlük deterministik simülasyon (adımlı/adımsız, 4 tohum); günün rastgele saatlerinde (sabah erken, gece geç, devir saati çevresi, 8 saat dilimi) 2 yıllık simülasyon, bağımsız takvimle (`elapsed_days`, aralık, vade) karşılaştırma; ilk tekrar referans değerleri, adım makinesi, SM-2, kontrolcüler, ipuçları, kuyruk.
- `study` servis testleri: 17 senaryo × 3 depo (MemStore, SQLite bellek, SQLite dosya) — puanla → geri al → kart birebir (yeni kart ve review kartta Again/lapse), geri alma biten oturumu açmaz, deneme modunda `cards` değişmez, uygulama düzeyinde 365 günlük simülasyon + `review_log`'dan yeniden hesaplama, yeniden başlatmada oturum sürer, …
- Depolama: migration 0100'ün v0.1 veritabanını yükseltmesi, oturum/`review_log`/aday kart sorguları.
- Arayüz: tüm oturumun yalnızca klavyeyle yürütüldüğü bileşen testi, tuş haritası ve aralık biçimi testleri.

## [0.1.0] — Yayımlanmadı (inceleme bekliyor)

v0.1 · Temel: her şeyin üstüne kurulacağı iskelet ve API sözleşmesinin ilk hali (docs/04, docs/10).

### Değişti
- **Faz 2 hazırlığı:** arayüz ekranları ve mock işleyiciler `lib/features/<alan>/` altında dosya başına kaydedilir (`import.meta.glob`); `App.svelte` ekranı genel çizer. `.gitattributes` (LF). CLI tüm insan çıktısını `--lang` kataloğundan üretir. docs/10 §10 tarifi genişletildi.

### Eklendi
- **Workspace:** `fc-core`, `fc-api`, `fc-storage`, `fc-providers`, `fc-app`, `fc-cli` ve Tauri 2 kabuğu `fc-desktop`.
- **fc-core:** saf alan modeli (UUIDv7 kimlikler, `Clock`/`Rng`, deste/not/not türü/kart/etiket), sınıflandırma tanımları + miras çözümü + hazır şablonlar (CEFR, YKS…). G/Ç yok (CI saflık betiği).
- **fc-storage:** SQLite (WAL, tek yazıcı + okuma havuzu), checksum'lı migration'lar (`0001_init`, migrate öncesi yedek), tüm repository'ler, FTS5, deste alt ağacı çöpü, tam geri alınabilir seçenek silme/birleştirme.
- **fc-api:** v1 metot kataloğu (public/internal), hata modeli, olaylar, `gen-ts` ile TypeScript tipleri (`--check` CI'da).
- **fc-app:** komut/sorgu dağıtıcı, registry, oplog + geri al/yinele (iç ters metotlarla birebir), olay yolu, modüller, şemalı ayarlar, iş kuyruğu arayüzü; deste/not/etiket/sınıflandırma/ayar/modül handler'ları.
- **fc-cli (`fc`):** `deck create/list/rename/move/delete/restore`, `note add/list/edit/delete/restore` (`--attr anahtar-veya-ad=değer`), `attribute define/apply-template/set/list/option add|rename|reorder|delete`, `undo`/`redo` (TR/EN özet), genel `fc call <metot> [<json>|-]` (yalnız public metotlar).
- **Masaüstü:** Svelte 5 arayüz (deste ağacı, klasik not ekleme + sınıflandırma alanları, not listesi, ayarlar), TR/EN i18n, tema token'ları; tek `invoke` IPC + `fc://event` olay akışı; menüler `modules.list`'ten.
- **Faz 2 hazırlığı:** metot kataloğu, hata kodları, repo trait'leri, `MemStore`, modül listesi ve i18n sözlükleri alan/özellik başına dosyalara bölündü; migration'lar klasörden otomatik toplanır ve numara aralıkları testle zorunlu (docs/10 §10).
- **CI:** fmt, clippy `-D warnings`, testler, TS tip tutarlılığı, frontend check/test/build, Windows derlemesi, `fc-core` saflık kontrolü, dependabot.

### Değişti
- `decks.create` isteğe bağlı `parent_id` alır; deste yolunu artık Rust kurar (arayüz birleştirmiyor).
- Masaüstü başlangıç hatası yerel hata penceresinde gösterilir.
- `rust-toolchain.toml` 1.100.0'a sabitlendi.

### Testler
- `store_suite!` (28 servis testi) MemStore, SQLite bellek ve SQLite dosya üzerinde.
- Uçtan uca kabul testi (`fc-app/src/acceptance.rs`) ve gerçek `fc` ikilisiyle CLI testi.

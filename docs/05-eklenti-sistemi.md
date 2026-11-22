# 05 · Eklenti Sistemi

## 1. Amaç

Çekirdeğe dokunmadan yeni yetenek eklemek. Yerleşik özellikler de aynı arayüzleri kullandığı için eklentiler "ikinci sınıf vatandaş" değildir. Tek fark: eklentiler izinlerle sınırlanır.

## 2. Teknoloji

- **WASM** tabanlı, izole çalışma (öneri: **Extism** üzerinden `wasmtime`). Sebep: dilden bağımsız (Rust, Go, AssemblyScript, JS/TS → WASM), güvenli sandbox, yakıt (fuel) ve zaman sınırı.
- Eklenti ile host arası veri JSON (ileride gerekirse daha verimli bir ikili format).
- Eklentinin arayüz kısmı (UI) iki yolla gelir:
  1. **Bildirimsel arayüz (öncelikli):** Eklenti bir arayüz tanımı (form, liste, kart görünümü, buton, metin, ses kaydı bileşeni) döner, frontend bunu kendi bileşenleriyle çizer. **Frontend değişse bile eklenti çalışır.**
  2. **İzole web görünümü (ileri seviye):** Eklenti kendi HTML/JS paketini getirir, ayrı ve kısıtlı bir iframe/webview'da çalışır, sadece mesajlaşma köprüsüyle host API'ye erişir.

## 3. Manifest

```toml
id = "com.ornek.cumle-kurma"
name = "Cümle Kurma Antrenörü"
version = "1.2.0"
api_version = "1"            # host eklenti API sürümü
authors = ["…"]
description = "Bildiğin kelimelerle Türkçe cümle üretir, İngilizceye çevirmeni ister."
entry = "plugin.wasm"
min_app_version = "0.7.0"

permissions = [
  "cards:read",
  "review_log:read",
  "notes:write",
  "ai:invoke",
  "ui:study_mode",
]

[contributes]
study_modes = ["sentence-builder"]
commands = ["cumle.yeniOturum"]
settings = "settings.schema.json"
locales = "locales/"
```

## 4. İzinler

Telefon uygulamaları gibi: kurulumda listelenir, kullanıcı onaylar, sonradan ayarlardan geri alınabilir. Hassas izinler (mikrofon, ağ) ilk kullanımda ayrıca sorulur.

| İzin | Anlamı |
|---|---|
| `decks:read` / `decks:write` | Desteleri okuma / değiştirme |
| `notes:read` / `notes:write` | Notları okuma / oluşturma / düzenleme |
| `cards:read` / `cards:write` | Kart durumunu okuma / değiştirme (askıya alma, bayrak…) |
| `review_log:read` | Çalışma geçmişini okuma |
| `scheduler:provide` | Zamanlama algoritması sağlama |
| `study:record` | Kendi çalışma modunda sonuç kaydetme (`review_log`'a yazma) |
| `media:read` / `media:write` | Medya okuma / ekleme |
| `ai:invoke` | Kullanıcının ayarladığı AI sağlayıcısını çağırma (harcama takibine yazılır) |
| `tts:invoke` / `stt:invoke` / `translate:invoke` | İlgili sağlayıcıları çağırma |
| `audio:record` | Mikrofon (ilk kullanımda ayrıca onay) |
| `network:<host>` | Belirli bir hosta HTTP isteği (her host ayrı izin) |
| `fs:pick` | Kullanıcının seçtiği dosyayı okuma (serbest dosya sistemi erişimi yok) |
| `hooks:<olay>` | Belirli kancalara abone olma |
| `ui:*` | Arayüz katkı noktaları (aşağıda) |
| `models:manage` | Model indirme yöneticisini kullanma |
| `words:read` | v1.4: kelime dizinini okuma (`words.lookup/list/stats`): hangi kelimeleri bildiğin, durumları, arkalarındaki not/kart kimlikleri. Kendi iznidir (`reading:read` yetmez). Kullanıcı eklentiyi **bazı destelerle sınırlayabilir** (`plugins.word_decks.set`, aşağıda §13) |

İzin kontrolü host fonksiyonu seviyesindedir. Kayıt defterindeki her komut/sorgu, gerektirdiği izni bildirir. Yani eklenti API'si, frontend API'sinin izinlerle filtrelenmiş halidir.

## 5. Host API (eklentinin çağırabildikleri)

- **Komut/sorgu çağırma:** `host.query("v1.card.search", { query: "deck:İngilizce stability>21" })`, `host.command("v1.note.create", {...})`. İzin kontrollü.
- **Sağlayıcılar:** `host.ai.complete(...)`, `host.tts.speak(...)`, `host.stt.transcribe(audio)`, `host.translate(...)`.
- **Depolama:** `host.kv.get/set` (eklentiye özel), `host.settings.get` (eklenti ayarları).
- **Olaylar:** `host.subscribe("CardAnswered")`.
- **Arayüz:** `host.ui.notify(...)`, `host.ui.open_view(view_id, data)`.
- **Ağ:** `host.http.fetch(...)` (sadece izinli hostlar).
- **Loglama:** `host.log(...)` (eklentinin kendi log kanalı).

## 6. Eklentinin sağlayabildikleri (katkı noktaları)

| Katkı | Açıklama |
|---|---|
| Kart türü | `CardTypeEngine` uygulaması |
| Çalışma modu | Kendi oturum akışı (soru üret → cevap al → değerlendir → kaydet) |
| Sağlayıcı | Scheduler, AnswerChecker, HintStrategy, Importer, Exporter, AI, TTS, STT, çeviri, sözlük, lemmatizasyon |
| Komut | Komut paletinde ve kısayollarda görünür |
| Kanca | `before_note_create`, `after_card_answered` … |
| Arayüz parçası | Menü öğesi, kenar çubuğu paneli, kart altı araç çubuğu düğmesi, ayar sayfası, tam ekran görünüm |
| Ayarlar | Şemalı ayarlar (ana ayar ekranında görünür, aranabilir) |
| Tema | CSS token setleri |
| Not türü / şablon | Hazır not türleri |
| Çeviriler | Kendi i18n dosyaları |

## 7. Yaşam döngüsü ve güvenilirlik

- Kur → izinleri onayla → etkinleştir → (güncelle) → devre dışı bırak → kaldır.
- Kaldırırken: eklentiye özel veri silinsin mi sorulur. Eklentinin oluşturduğu notlar kullanıcıya aittir, silinmez.
- **Yakıt/zaman sınırı:** Her çağrı sınırlıdır. Aşan eklenti sonlandırılır, hata kaydedilir, art arda hata veren eklenti otomatik devre dışı kalır ve kullanıcı bilgilendirilir.
- **Bellek sınırı** WASM örneği başına.
- **Hata izolasyonu:** Kanca zincirinde bir eklentinin hatası diğerlerini ve çekirdek işlemini durdurmaz (`before_*` kancasında hata = işlem eklentisiz devam eder, ayarlanabilir).
- **İmza (ileride):** Mağaza kaynaklı eklentiler imzalı olabilir.

## 8. Model indirme yöneticisi

- Yerel TTS sesleri, Whisper modelleri, lemmatizasyon sözlükleri gibi büyük dosyalar için.
- Model kataloğu sağlayıcılar/eklentiler tarafından bildirilir: `{ id, name, size, url, sha256, license }`.
- İndirme arka plan işidir (devam ettirilebilir), SHA-256 doğrulanır, disk kullanımı gösterilir, silinebilir.

## 9. Eklenti mağazası (altyapı)

- `PluginRegistrySource` arayüzü: `search`, `get_manifest`, `download`, `check_updates`.
- Yerleşik kaynak: **yerel dosyadan kurulum** (`.fcplugin` paketi).
- Uzak kaynak (kendi sunucu, GitHub tabanlı liste vb.) **sonra karar verilecek.** Arayüz hazır olduğu için eklemek çekirdeği etkilemez.

## 10. SDK

- `plugins-sdk/rust`: tipler (fc-api'den üretilir), host fonksiyon bağlamaları, makrolar, test yardımcıları (sahte host).
- İleride: JS/TS SDK (WASM'e derleme ile).
- `fc-cli plugin new|build|test|pack` komutları.
- **Sürümlü örnekler (v1.4):** `plugins-sdk/samples/v1/<metot>.json` her metot için `input`/`output`, tür ve gereken izni verir (şimdilik `words.lookup`, `words.list`, `words.stats`). Uygulamanın `plugin_api_samples_match_the_api` testi her örneği API'nin kendi istek/yanıt tipine çözüp aynen geri yazar: örnek bozulursa v1 sözleşmesi değişmiş demektir.
- **Örnek `new-words` (v1.4):** salt okunur görünüm; bir altyazı satırı yapıştırırsın, `words.lookup` (`words:read`) hangilerinin senin için yeni, hangilerini öğrenmekte olduğunu söyler. Hiçbir şey yazmaz (`example_new_words_view`).

## 11. Örnek eklenti senaryoları

### 🧩 Cümle kurma antrenörü
1. `host.query("v1.card.search", { query: "deck:İngilizce stability>21" })` → iyi bilinen kelimeler.
2. `host.ai.complete(...)` → "Bu kelimelerden 3-5 tanesini içeren, seviyeye uygun bir Türkçe cümle üret."
3. Bildirimsel arayüz: Türkçe cümle + metin kutusu.
4. Kullanıcının çevirisi → `host.ai.complete(...)` ile değerlendirme (doğru mu, hatalar, doğal alternatif).
5. `host.command("v1.study.record", ...)` → sonuç kaydedilir; kullanılan kelimelerin kartlarına "pratik" kaydı düşer (zamanlamayı etkileyip etkilemeyeceği eklenti ayarı).

### 🎤 Telaffuz / sesli cevaplama
1. Türkçe kelime veya cümle gösterilir.
2. Bildirimsel ses kayıt bileşeni → `audio:record`.
3. `host.stt.transcribe(audio, lang="en")` (yerel Whisper modeli, model yöneticisinden indirilmiş).
4. `AnswerChecker` (toleranslı + isteğe bağlı AI) ile karşılaştırma.
5. Sonuç ve isteğe bağlı TTS ile doğru telaffuzun dinletilmesi.

### 🔊 Tarayıcı / sistem sesi TTS sağlayıcısı
- `TtsProvider` uygular. Windows'ta sistem sesleri, ya da yerel model (Piper), ya da bulut API (`network:<host>` izniyle).

### 🌐 Çeviri sağlayıcısı
- `TranslationProvider` uygular; okuma modu ve kart oluşturmada otomatik doldurma için kullanılır.

## 12. Uygulama notları (v0.7, `feat/v0.7-plugins`)

Kararların özeti; ayrıntı kodun `///` belgelerinde (`crates/fc-plugins`, `crates/fc-app/src/modules/plugins`).

| Konu | Karar |
|---|---|
| Çalışma zamanı | Extism yerine doğrudan **wasmtime 50** (ince ABI, daha az bağımlılık). Yakıt (`plugins.fuel_millions`), çağrının tamamına duvar saati son tarihi (epoch + her host çağrısının giriş/çıkışında denetim, `plugins.timeout_ms`; host çağrılarında geçen süre **sayılır**), host çağrısı sayısı ve sınırı geçen bayt bütçesi, host fonksiyonu başına boyut/sayı/hız sınırları (aşağıdaki tablo), bellek sınırı (`plugins.memory_mb`; örneğin **tüm** bellek ve tabloları toplam sayılır, tek bellek; multi-memory, threads, memory64, GC, istisnalar, component model vb. kapalı). Her çağrı taze örnek; durum `kv`'de ya da arayüz oturumunun `state`'inde. Derlenmiş modül SHA-256 ile önbellekte |
| ABI v1 | Dışa aktarımlar `memory`, `fc_abi_version() -> 1`, `fc_alloc(len)`, `fc_handle(ptr,len) -> (ptr<<32)\|len`; içe aktarımlar `fc.host_call(ptr,len) -> len`, `fc.host_reply(ptr)`. JSON zarflar: `{op, input}` → `{ok}`/`{err:{code,message,details}}`; host `{fn, args}` → aynı biçim. İşlemler ve host fonksiyonları `fc_plugins::abi::{ops, host_fns}` (yalnız eklenir). WASI yok: başka içe aktarım isteyen modül kurulamaz |
| Sürüm | `api_version = "1"` = ABI v1 + `v1` metot kataloğu. Desteklenmeyen sürüm `VALIDATION_PLUGIN_API_VERSION`. Sözleşme: dondurulmuş `crates/fc-plugins/tests/fixtures/abi_v1.wat` ve eklentilere açık metotların anlık görüntüsü `crates/fc-app/src/modules/plugins/api_v1.json` (yalnız büyüyebilir). Uygulamanın eklentilere gösterdiği sürüm `fc_plugins::APP_VERSION` = workspace paket sürümü (`Cargo.toml` `[workspace.package] version`, kilometre taşını izler: `0.7.0`) |
| İzinler | API izinleri = metotların kendi izinleri (`notes:write`, `tts:read`…; §4'teki `tts:invoke` yerine `tts.*` metotları `tts:read` ister — katalog kuralı: sorgu `:read`). Host izinleri: `ai:invoke`, `stt:invoke`, `translate:invoke`, `audio:record`, `fs:pick`, `models:manage`, `tts:provide`, `card_types:provide`, `storage:kv` (kendi deposuna yazma; okuma ve silme izinsiz; eklenti başına kota `plugins.kv_max_keys` / `plugins.kv_max_mb`, uygulama ayarı). Kancalar `hooks:before:<metot>`, `hooks:after:<metot>`, `hooks:event:<olay>` (izin = abonelik bildirimi). `ui:study_mode/view/menu/sidebar/card_toolbar/command/settings/notify` (`ui:notify` = `host.ui.notify`). **Hiçbir eklentiye verilemez** (`fc_plugins::permission`): `plugins:*`, `io:*` (dışa/içe aktarma ve yedek metotları serbest dosya yolu alır — okuma dahil), `mcp:*`, `ai_keys:*`, `ai_requests:*` (ücretli AI için host izni `ai:invoke`), `settings:write`, `modules:write` (uygulama ayarları, eklentinin kendi sınırları dahil), `oplog:write`, `oplog:read` (geçmiş her alanın verisini taşır), `ai:write`, `batch:write` (sonuç süzmeyi ve iç çağrıların kancalarını atlatır), `jobs:write`; `Registry::permits` eklenti aktörü için bunları kayıtlı izin olsa bile reddeder. Dosyalar yalnız **`fs:pick`** ile: host yerel aç/kaydet penceresini gösterir (`App::set_file_picker`, masaüstünde `rfd`), dosyayı kendisi okur/yazar; eklenti yalnız dosya adını ve baytları görür (`host_fns::FS_OPEN/FS_SAVE`, en çok 8 MiB, yalnız kullanıcının başlattığı işlemde; önerilen ad klasörsüz). Eklentinin kendi ayarları `host_fns::SETTINGS_SET` (yalnız kendi şemasındaki anahtarlar). **Kancalar:** `before:`/`after:` yalnız public komutlara; `plugins.`, `oplog.`, `modules.`, `settings.`, `shortcuts.`, `io.`, `batch.`, `jobs.`, `mcp.`, `ai.providers./prices./routes.` ile başlayan ya da izni eklentiye verilemeyen metotlara **kanca takılamaz** (kurulumda `VALIDATION_PLUGIN_MANIFEST`; saklanmış satırda olsa bile dizine alınmaz). Kanca ayrıca hedef metodun kendi iznini **ve gördüğü verinin okuma iznini** ister (`before:notes.create` → `notes:write` + `notes:read`; alanlar `plugins::redact::exposure`): manifest hepsini istemeli, kanca yalnız hepsi verilmişken çalışır (kanca, metodu doğrudan çağırmaktan fazlasını vermez). Hassas: `network:*`, `audio:record`, `fs:pick`, `ai:invoke` (kurulumda işaretsiz gelir). Her katkı izni ister (manifest doğrulaması); izin geri alınınca katkı kapanır |
| Nerede çalışır | `before:` kancası: yazma işlemi açılmadan, yalnız sorgu + kendi deposu/ayarları; komut çağıramaz, parametreyi değiştirir ya da reddeder. `after:`/`event:` kancası: commit sonrası, yazdıkları tek geri al adımı, **kanca tetiklemez**. İşlem (`plugins.run`, `v1.plugin.<id>.<komut>`, `plugins.ui.*`): yazdıkları tek geri al adımı (`oplog.summary.plugins.run`), eklenti hata verirse geri sarılır; eklentinin kendi deposu (`kv`) ve ayarları da aynı gruba yazılır: hata olunca onlar da geri sarılır, kullanıcı verisiyle birlikte geri alınır, yalnız depo/ayar değiştiren çağrı geri al adımı bırakmaz (her bağlamda, `before:` kancası dahil); diğer eklentilerin `before:` kancaları uygulanır. Kart türü/TTS motoru: host erişimi yok (yalnız `log`). Eklentilerin yaptığı değişiklikler ve hata veren eklenti çağrısının geri sarılması (eklenti aktörüyle) `after:`/`event:` kancası tetiklemez; kanca çalışırken oluşan hiçbir olay kuyruğa girmez, kanca kanca içinde başlamaz (zincirleme yok). Kanca bütçesi **eklenti başınadır**: bir kullanıcı işleminde her eklenti en çok `plugins.max_hooks_per_action` (200) kanca, toplam `plugins.hook_ms_per_action` (10 000 ms) çalıştırır; kancası arıza veren eklenti o işlemde atlanır; bütçesini aşan eklentinin yalnız **kendi** kalan kancaları o işlemde atlanır, diğerleri çalışmaya devam eder, kullanıcıya işlem başına **tek** bildirim gider (`plugin_hooks_skipped`, `plugins.notice.hooks_skipped`). Bütçe aşımı **kimseyi kapatmaz** (1000 nota toplu etiket zararsız eklentiyi kapatmaz); kapatma yalnız eklentinin kendi arızalarıyladır (tuzak, zaman aşımı, yakıt, bellek, ABI; art arda `plugins.max_failures`). Eklentinin işlemi içindeki kancalar **zinciri başlatanın** bütçesine yazılır; bütçesini aşan çağrısı reddedilir. Eklentiler geri al/yinele çağıramaz, eklenti işlemi başlatamaz |
| Hata | Hata türleri `OUT_OF_FUEL`, `TIMEOUT`, `HOST_LIMIT`, `MEMORY_LIMIT`, `TRAP`, `ABI` (arıza) ve `PLUGIN` (eklentinin kendi hatası), `INPUT_TOO_LARGE` (hostun girdisi çok büyük) — bu ikisi arıza sayılmaz. Arızalar art arda `plugins.max_failures` (3) olunca eklenti kapanır (`auto_disabled`, `plugins_changed{auto_disabled: true}`); açmak sayacı sıfırlar. `before:` hatası: varsayılan eklentisiz devam, `plugins.before_hook_failure = reject` ile `PROVIDER_PLUGIN_FAILED`. Eklenti günlüğü bellekte (son 200 satır, `plugins.logs`) |
| Depolama | `plugins` satırı (0600: `package_sha256`, `failures`, `last_error`, `auto_disabled`, `deleted_at`), paketler `plugin_packages` (her sürüm, içerik adresli; geri al/yinele bayt taşımaz). Kaldırma yumuşak; "verileri sil" KV ve ayarları da siler (geri al hepsini getirir). Ayarlar `settings` tablosunda `plugin:<id>` kapsamında |
| Arayüz | Bildirimsel düğümler `UiNode` (`column/row/heading/text/html/input/choice/audio_recorder/audio/button/feedback/progress`); HTML arka uçta temizlenir; metinler eklentinin çeviri anahtarı (`plugin.<id>.<anahtar>`) ya da düz veri. Eklentiler `modules.list`'te `plugin:<id>` modülü olarak görünür: ana menü `/plugins/view/<id>/<görünüm>`, kenar paneli, kart altı düğmesi (`v1.plugin.<id>.<komut>` + `{card_id}`), ayar sayfası. İzole web görünümü (§2.2) sonraya |
| Mağaza / modeller | `PluginRegistrySource` + `FileSource` (kurulum bu arayüzden geçer). Modeller `<koleksiyon klasörü>/models/<eklenti>/<model>`; indirme arka plan işi (`plugins.model_download`), şimdilik `file://` getirici (HTTP getirici aynı `Fetcher` arayüzüne takılır) |
| CLI | `fc plugin list | inspect <dosya> | install <dosya> [--grant <izin>…|--grant-all] | uninstall <id> [--delete-data] | enable | disable | grant <id> [izin…] | logs | run <id> <komut> [json] | pack <klasör> [--wasm] [--out]` (`pack` yerel geliştirici aracı: `pack_dir` + kurulumdaki manifest denetimi) |
| Henüz yok | `host.http.fetch` ve `host.ai.complete` izin denetler ama `PROVIDER_UNAVAILABLE` döner (HTTP istemcisi / AI sağlayıcısı v0.8); ses kaydı bileşeni çizilmez; `fc plugin new|build|test` (iskele/derleme: `cargo build --target wasm32-unknown-unknown` + `fc plugin pack`); **izole web görünümü** (§2.2: sandbox'lı iframe, sıkı CSP, izinle süzülmüş host API'ye `postMessage` köprüsü — ertelendi; bildirimsel arayüz şimdilik tek yol, böylece eklentinin tarayıcıda kod çalıştırma yüzeyi yok); paket imzası |

### Host fonksiyonu sınırları

Kaynak: `fc_plugins::limits::HOST_FNS` (test `docs_table_lists_every_host_function` bu tabloyu kodla karşılaştırır). İstek/yanıt = tüm `{fn, args}` / `{ok}`·`{err}` JSON'u. İstek boyutu, işlev adı dışında hiçbir şey ayrıştırılmadan denetlenir (eklentinin baytları `Value`'ya dönüşmez). Sınırı aşan istek, çağrı başına ya da dakikalık sayıyı aşan çağrı ve büyük yanıt eklentiye `VALIDATION_PLUGIN_HOST_LIMIT` (`details`: `function`, `limit` = `request_bytes`/`reply_bytes`/`per_call`/`per_minute`, `max`) olarak döner; eklenti devam edebilir. Dakikalık pencere eklenti başınadır (60 sn kayan). Her host çağrısı ayrıca **50 000 yakıt + bayt başına 4** (istek + yanıt) tüketir.

| Host fonksiyonu | İzin | En büyük istek | En büyük yanıt | Çağrı başına / dakikada | Yakıt | Not |
|---|---|---|---|---|---|---|
| `invoke` | `<method's own>` | 1 MiB | 16 MiB | 1000 / — | 1000000 | Komutlar `before:` kancalarından geçer; `before:` kancasında komut yok |
| `kv.get` | — | 1 KiB | 2 MiB | 10000 / — | 200000 | Değer en çok 1 MiB |
| `kv.set` | `storage:kv` | 1,00 MiB | 1 KiB | 1000 / — | 1000000 | Anahtar 1–256 bayt, değer ≤ 1 MiB; kota `plugins.kv_max_keys` / `plugins.kv_max_mb` |
| `kv.delete` | — | 1 KiB | 1 KiB | 1000 / — | 500000 |  |
| `kv.list` | — | 1 KiB | 4,06 MiB | 100 / — | 1000000 | Sayfalı: `{prefix, after?, limit?}`, sayfa ≤ 1000 kayıt / ≈ 4 MiB |
| `settings.get` | — | 1 KiB | 1 MiB | 100 / — | 200000 | Şema ≤ 100 ayar |
| `settings.set` | — | 64 KiB | 1 MiB | 100 / — | 1000000 | Yalnız kendi şemasındaki anahtarlar |
| `fs.open` | `fs:pick` | 4 KiB | 12 MiB | 2 / 10 | 1000000 | Yalnız kullanıcının başlattığı işlemde; dosya ≤ 8 MiB; pencerede geçen süre son tarihe sayılmaz |
| `fs.save` | `fs:pick` | 12 MiB | 4 KiB | 2 / 10 | 1000000 | Aynı; dosya ≤ 8 MiB |
| `log` | — | 8 KiB | 1 KiB | 100 / 600 | 50000 | Satır ≤ 2000 bayt (UTF-8 sınırında kesilir), eklenti başına son 200 satır |
| `ui.notify` | `ui:notify` | 4 KiB | 1 KiB | 3 / 10 | 500000 | Bildirim olayı + işlem yanıtı |
| `tts.synthesize` | `tts:read` | 64 KiB | 16 MiB | 50 / — | 1000000 | `invoke v1.tts.synthesize` kısayolu |
| `tts.voices` | `tts:read` | 1 KiB | 1 MiB | 10 / — | 200000 | `invoke v1.tts.voices` kısayolu |
| `http.fetch` | `network:<host>` | 1 MiB | 16 MiB | 50 / 120 | 1000000 | Şimdilik `PROVIDER_UNAVAILABLE` |
| `ai.complete` | `ai:invoke` | 1 MiB | 1 MiB | 10 / 60 | 1000000 | Şimdilik `PROVIDER_UNAVAILABLE` |
| `models.list` | `models:manage` | 1 KiB | 64 KiB | 10 / — | 200000 | Manifest ≤ 16 model; yol yok, `ready` |

**Çağrı bütçeleri** (uygulama ayarı; eklentiler ayarlara erişemez, aşan çağrı durdurulur = arıza): duvar saati `plugins.timeout_ms` (3000; host çağrıları **dahil**, her host çağrısının girişinde ve çıkışında + epoch ile denetlenir; yalnız dosya penceresinde kullanıcıyı bekleme süresi eklenir) → `TIMEOUT`; host çağrısı sayısı `plugins.max_host_calls` (10 000) ve sınırı geçen bayt (istek + yanıt) `plugins.host_mb_per_call` (64) → `HOST_LIMIT`; yakıt `plugins.fuel_millions` → `OUT_OF_FUEL`. **İşlem bütçesi (eklenti başına):** bir kullanıcı işleminde her eklentinin kancaları en çok `plugins.max_hooks_per_action` (200) kanca, toplam `plugins.hook_ms_per_action` (10 000 ms); aşınca o eklentinin kalan kancaları atlanır (kapatılmaz); kancası arıza veren eklenti o işlemde başka kanca çalıştırmaz. Böylece bir işlem her eklenti için en çok kanca süresi bütçesi + bir çağrının son tarihi kadar bekler.

**Hostta eklenti başına tutulan** (hepsi sınırlı): günlük (200 satır × 2000 bayt), dakikalık sayaçlar (işlev başına en çok kendi sınırı kadar zaman damgası), depo (kota, diskte), arayüz oturumları (oturum `state` ≤ 1 MiB, eklenti başına 8 açık oturum; fazlası en eskisini kapatır), `event:` kuyruğu (işlem başına en çok kanca bütçesi + 1 olay). İşlem girdisi > 16 MiB ise eklenti çağrılmaz (`INPUT_TOO_LARGE`, arıza sayılmaz); işlem yanıtı > 4 MiB (TTS sentezi 16 MiB) ayrıştırılmadan reddedilir (`ABI`). Manifest: katkı listesi başına ≤ 64 (model ≤ 16), izin ≤ 128, yazar ≤ 16, açıklama ≤ 500 karakter; ayar şeması ≤ 100.

## 13. Tehdit modeli (threat model)

Varsayım: eklenti **düşmandır** (kodu ve girdileri saldırgan seçer); kullanıcı ve uygulama güvenilir. Dördüncü güvenlik incelemesinden sonra yapılan sistematik veri akışı denetiminin özeti; her madde testli.

| Garanti | Nasıl | Test |
|---|---|---|
| Eklenti uygulamayı çökertemez, dondurmaz | Yakıt, çağrının tamamına son tarih (host süresi dahil), bellek, host çağrısı sayısı/baytı, işlev başına boyut/sayı/hız (§12) | `misbehaving_plugins_cannot_crash_the_app`, `host_call_loops_*` |
| Tek uzun host çağrısı son tarihi aşamaz | Host çağrısının veritabanı ifadeleri eklentinin son tarihinde kesilir (`fc_storage::interrupt`, SQLite ilerleme işleyicisi, iç içe son tarih yalnız kısalır); tüm koleksiyonu tarayan metotlar (`browse.duplicates/check_database/repair_database/purge_trash`) eklentiye kapalı; listeler sayfalı | `host_call_database_work_stops_at_the_deadline`, `long_statements_stop_at_the_deadline` |
| Eklenti okuyamadığını göremez | **Tek süzme katmanı** `plugins::redact`: eklentiye giden her değer çağıran eklentinin o alandaki `<alan>:read` izniyle süzülür — metot sonucu (sorgu reddedilir, komut yanıtı kimliklere iner), hata (mesaj ve ayrıntılar kimliklere iner; başka eklentinin mesajı hiç geçmez), `before:`/`after:` kanca girdisi (kanca okuma iznini ister), `event:` girdisi (kimliklere iner), iş sonucu (işin türüne göre), başka eklentinin ayar kapsamı (`plugins` alanı, asla). Çapraz alanlı sonuçlar her alanı ister (arama satırı: `search`+`cards`+`notes`; çalışma oturumu: `study`+`notes`). Kâhin komutlar (`bulk.find_replace`) okuma izni olmadan çalışmaz. Yazma izni okumayı kapsamaz | `redact::tests::domain_*` (alan başına), `every_grantable_method_is_classified`, `results_are_filtered_by_read_permissions`, `hooks_see_only_what_they_may_read` |
| Geçmiş üzerinden sızma yok | `oplog:read` verilemez (kayıtlar her alanın verisini, başka eklentilerin ayar/depo değerlerini taşır); `batch:write` verilemez | `oplog_cannot_be_read_by_plugins` |
| Bir eklenti diğerini kapatamaz, bütçesini harcayamaz | Kanca bütçesi **eklenti başına, işlem başına**: her eklentinin kendi sayısı/süresi var; aşan eklentinin yalnız kendi kalan kancaları atlanır, kullanıcıya tek bildirim gider, bütçe aşımı **hiç kimseyi kapatmaz** (kapatma yalnız kendi arızalarıyla). Eklentinin işlemindeki her bütçe **zinciri başlatana** yazılır (`host::initiator`, iş parçacığı yerel zincir): kancaların dakikalık host çağrısı pencereleri, işlemin kanca bütçesi; A'nın çağrısı içinde B'nin kancası/motoru arıza verirse B'ye sayılmaz (B'nin günlüğüne yazılır); bütçeyi aşan A çağrısı reddedilir (B'nin `before:` kancası olmadan devam etmez) | `bulk_edit_cannot_switch_one_plugin_off_for_another`, `hook_budget_is_charged_to_the_initiator`, `faults_in_another_plugins_chain_are_not_counted`, `looping_hooks_are_still_switched_off` (döngüyü yakıt bitirir: yükten bağımsız), `looping_hooks_hit_the_deadline` (döngüyü süre sınırı bitirir) |
| Eklenti yetki yükseltemez | Yönetim, uygulama ayarları/modüller (kendi sınırları dahil), geri al/yinele, serbest dosya yolu (`io.*`), MCP, AI anahtarları verilemez; bu metotlara kanca takılamaz; kayıtlı izin olsa bile `Registry::permits` reddeder | `hooks_cannot_reach_management_io_or_settings`, `plugins_get_no_free_paths_or_app_settings` |
| Dosya sistemi yok | Yalnız `fs:pick`: kullanıcı seçer, host okur/yazar; eklenti ad + bayt görür, yol asla (`models.list` de yol döndürmez); okuma sınırı akış sırasında uygulanır (cihaz/boru boyutu yalan söyleyebilir) | `fs_open_limit_holds_while_reading` |
| Faz 3 alanları (istatistik, planlama, hedefler, AI, MCP) yalnız okuma izniyle | `plugins::redact` bu alanları da sınıflar (`stats:read` + kapsam sorgusu kâhin olduğu için `search:read`; `planning:read`; `goals:read`; `ai:read`, önizleme ve akış için + `notes:read`); `mcp` hiç okunamaz. Bu alanların olayları (`exams_changed`, `goals_changed`, `ai_config_changed`, `ai_partial`, AI/optimizer işlerinin `job_progress`'i) okuma izni yoksa kimliklere inmiş halde bile **gelmez** (`redact::event_delivered`). `stats.refresh_cache`, `optimizer.run` (tüm geçmiş), `ai.notes.add/update` (AI taslağını uygulamak kullanıcının) eklentiye kapalı; `stats:write` verilemez; `Registry` bunları `Actor::Plugin` için doğrudan çağrıda da reddeder (`ensure_access`) | `redact::tests::domain_{stats,planning,goals,ai,mcp}`, `phase3_events_need_their_read_permission`, `phase3_plugins_tests` |
| Faz 4 alanları (öğrenme modları, okuma) yalnız okuma izniyle | `modes.*` oturum/kart verisidir: `study:read` + `notes:read`; öğren cevabı ve test teslimi kartın cevabına göre **yargılar** → kâhin (izin olmadan çalışmaz); `modes.inline.create`'in yinelenen sayısı var olan notları söyler → kâhin (`notes:read`). `reading.*` → `reading:read`; renk/sözlük kelimenin **kartlarından** gelir → `reading.analyze`/`reading.words.lookup` + `cards:read` + `notes:read`; okuma öğesinden kart/cloze metni nota kopyalar → kâhin (`notes` + `reading`). Okuma olayları (`reading_items_changed`, `word_marks_changed`) okuma izni yoksa **gelmez** | `redact::tests::domain_{modes,reading}`, `phase4_tests` |
| Okuma: serbest yol ve program yok | `reading.items.import` (her dosya/web sayfası) ve `reading.clips.extract` (ffmpeg çalıştırır) eklentiye kapalı (`Exposure::Refused`), **kancalanamaz** (genel kural: eklentinin çağıramadığı metoda kanca takılamaz — `before:` kancası kullanıcının yetkisiyle parametreyi değiştirirdi). Altyazının video yolu `media_path` (ffmpeg girdisi) `redact::USER_ONLY_FIELDS`: eklentiye giden hiçbir değerde yok (`filter` her derinlikte siler), eklenti ayarlayamaz (`io:read` olmayan çağırana `PERMISSION_DENIED`; güncellemesi var olan yolu korur), `before:` kancasının döndürdüğü parametrede kullanıcının değeri geri konur (`keep_user_only`). ffmpeg yolu ayarı (`reading.ffmpeg_path`) `settings:exec` ister: yalnız kullanıcı/CLI/sistem, eklentiye verilemez (`FORBIDDEN_API_PERMISSIONS`), işleyicide de aktöre göre denetlenir (güvenilir iç yollar) | `media_paths_are_user_only`, `exec_settings_are_user_only`, `hooks_cannot_reach_management_io_or_settings`, `permits_is_right_for_every_actor` |
| Diğer aktörler | `Registry::permits` tek yer: kullanıcı/CLI/sistem her şey; MCP `MCP_PERMISSIONS` (+ yalnız araçların çağırdığı metotlar); eklenti verilen izinler; **otomasyon** yalnız kural eylemleri (`AUTOMATION_PERMISSIONS` + `ACTIONS` listesi), **yerel API** yalnız onaylı kapsamlar ve HTTP listesi (v0.9); kullanıcıya özel izinler (`settings:exec`, `mcp:write`) başka hiçbir aktörde | `phase4_permits_tests::every_actor_against_every_sensitive_domain` (her aktör × her hassas alan), `phase3_plugins_tests` |
| v0.9 alanları (gelen kutusu, otomasyon, yerel API) | `inbox.add/list/count/reject` → `inbox:*` (öneri yapar ve görür; **kabul** kullanıcının ya da kullanıcının kuralının işi → `inbox.accept` kapalı), `deep_links.open` kapalı, `clipboard.check` → `clipboard:read`. `automation.*` ve `local_api.*` hiç (alanlar verilemez, kancalanamaz): kural sonradan otomasyon yetkisiyle çalışır, istemci onayı kullanıcının kararıdır. `automation_changed`, `local_api_*` olayları hiç gitmez | `redact::tests::domain_{inbox,automation,local_api}` |
| Kelime dizini yalnız açık izinle ve verilen destelerden (v1.4) | `words.lookup/list/stats` → `words:read` (kendi alanı; `reading:read` yetmez). Kullanıcı eklentiyi destelere sınırlarsa (`plugins.word_decks.set`, eklenti ayar kapsamında host'a ait `host:word_decks` anahtarı: eklenti şeması bu adı alamaz, `plugins:write` verilemez → eklenti kendi sınırını kaldıramaz) host yanıtı eklenti görmeden keser (`plugins::word_scope`): kelime yalnız **bütün** notları izinli destelerde (ve alt destelerinde) ise görünür, yoksa `new` + `word: null`; `words.list` satırları aynı kuralla düşer; tüm dizine dair cevaplar (`words.stats`, okumanın kelime renkleri `reading.analyze`/`reading.words.lookup`) reddedilir. Yalnız işaretle bilinen kelime desteye ait değildir, sınırlı eklentiye gösterilmez. Dizini yeniden kurmak (`words.index.rebuild*`) tüm koleksiyon işidir, eklentiye kapalı. Sınır yalnız kelime dizinini kapsar: `notes:read`/`search:read` verilen eklenti her desteyi görür | `words_need_their_permission_and_stay_in_the_plugins_decks` |
| Zincirleme yok | Eklenti yazımları ve geri sarmaları kanca tetiklemez; kanca kanca içinde başlamaz | `failed_hook_rollback_does_not_cascade`, `hooks_do_not_nest_and_their_effects_do_not_queue` |

**Bilinçli sınırlar (garanti dışı):** kart türü ve TTS motorları, işledikleri notun/metnin içeriğini görür (işleri bu; host erişimleri yok, yalnız çıktıları döner). Bir hata *kodu* (ör. `CONFLICT_DECK_EXISTS`) okuma izni olmayan eklentiye bir şeyin varlığını söyleyebilir (ayrıntısı söylemez). Kullanıcının kendi işleminde arıza veren kanca kendi eklentisine sayılır; saklanmış veriyle (ör. bir eklentinin yazdığı not) başka eklentinin motorunu sonradan bozmak engellenmez, yalnız doğrudan çağrı zinciri ayrılır. Ağ ve AI host fonksiyonları henüz yok (`PROVIDER_UNAVAILABLE`); geldiklerinde aynı süzme ve bütçe kuralları geçerli.

**Yerel eşleştirme akını ve odak çalma (v1.0 entegrasyonu, tur 1; bilinçli ödünleşim).** Bu bölümün saldırganı eklentidir, ama aynı makinedeki **herhangi bir yerel süreç** (ya da bağlantı açan bir web sayfası) iki kapıyı çalabilir: `POST /v1/pair` (docs/06 §3) ve `flashcard://` bağlantısı. İkisi de ana pencereyi öne getirir (eşleştirme onay penceresi, bağlantının hedefi ya da kilit ekranı) — kullanıcı bir isteği **görmeden** onaylamasın ve kimin bağlanmak istediğini fark etsin diye. Bedeli: kötü niyetli yerel bir program pencereyi tekrar tekrar öne getirip kullanıcıyı rahatsız edebilir ya da yazdığı sırada odağı çalabilir. Sınırlar: eşleştirme sunucu genelinde dakikada 5, aynı ad + köken 10 dakikada 3 (`429`), aynı anda en çok 5 bekleyen istek (en eskisi düşer); kilitliyken yerel API hiç dinlemez ve bağlantılar yalnız kuyruğa girer (en çok 10, tekil; kilit ekranı onları **çalışmadan önce** listeler ve bırakabilir). Odak çalma eşleştirme penceresinden kendiliğinden **onay** alamaz: pencere bir form değildir (Enter bir şey göndermez), hiçbir düğme odakla açılmaz, sekme sırasında önce **Reddet** gelir ve izinler onay kutularıyla daraltılabilir. Onaylı bir istemcinin önerileri ayrıca **istemci başına günlük sınırla** (`local_api.inbox_daily_max`, varsayılan 200/24 saat, reddedilen/kabul edilen dahil) ve bekleyen kotasıyla sınırlıdır; kullanıcı önerileri inceleme olmadan kabul eden bir gelen kutusu kuralı yazdıysa kural düzenleyicisi ve kural listesi bunu uyarıyla gösterir. Bu bir korumanın değil bir **ödünleşimin** kaydıdır: aynı kullanıcı hesabında çalışan bir program zaten koleksiyon dosyasını okuyabilir (docs/02 §29 tehdit modeli).

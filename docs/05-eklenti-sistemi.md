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
| Çalışma zamanı | Extism yerine doğrudan **wasmtime 50** (ince ABI, daha az bağımlılık). Yakıt (`plugins.fuel_millions`), eklentinin içinde geçen süreye son tarih (epoch, `plugins.timeout_ms`; host çağrısı sırasında geçen süre sayılmaz), bellek sınırı (`plugins.memory_mb`; örneğin **tüm** bellek ve tabloları toplam sayılır, tek bellek; multi-memory, threads, memory64, GC, istisnalar, component model vb. kapalı). Her çağrı taze örnek; durum `kv`'de ya da arayüz oturumunun `state`'inde. Derlenmiş modül SHA-256 ile önbellekte |
| ABI v1 | Dışa aktarımlar `memory`, `fc_abi_version() -> 1`, `fc_alloc(len)`, `fc_handle(ptr,len) -> (ptr<<32)\|len`; içe aktarımlar `fc.host_call(ptr,len) -> len`, `fc.host_reply(ptr)`. JSON zarflar: `{op, input}` → `{ok}`/`{err:{code,message,details}}`; host `{fn, args}` → aynı biçim. İşlemler ve host fonksiyonları `fc_plugins::abi::{ops, host_fns}` (yalnız eklenir). WASI yok: başka içe aktarım isteyen modül kurulamaz |
| Sürüm | `api_version = "1"` = ABI v1 + `v1` metot kataloğu. Desteklenmeyen sürüm `VALIDATION_PLUGIN_API_VERSION`. Sözleşme: dondurulmuş `crates/fc-plugins/tests/fixtures/abi_v1.wat` ve eklentilere açık metotların anlık görüntüsü `crates/fc-app/src/modules/plugins/api_v1.json` (yalnız büyüyebilir). Uygulamanın eklentilere gösterdiği sürüm `fc_plugins::APP_VERSION` = workspace paket sürümü (`Cargo.toml` `[workspace.package] version`, kilometre taşını izler: `0.7.0`) |
| İzinler | API izinleri = metotların kendi izinleri (`notes:write`, `tts:read`…; §4'teki `tts:invoke` yerine `tts.*` metotları `tts:read` ister — katalog kuralı: sorgu `:read`). Host izinleri: `ai:invoke`, `stt:invoke`, `translate:invoke`, `audio:record`, `fs:pick`, `models:manage`, `tts:provide`, `card_types:provide`, `storage:kv` (kendi deposuna yazma; okuma ve silme izinsiz; eklenti başına kota `plugins.kv_max_keys` / `plugins.kv_max_mb`, uygulama ayarı). Kancalar `hooks:before:<metot>`, `hooks:after:<metot>`, `hooks:event:<olay>` (izin = abonelik bildirimi). `ui:study_mode/view/menu/sidebar/card_toolbar/command/settings`. **Hiçbir eklentiye verilemez** (`fc_plugins::permission`): `plugins:*`, `io:*` (dışa/içe aktarma ve yedek metotları serbest dosya yolu alır — okuma dahil), `mcp:*`, `ai_keys:*`, `ai_requests:*` (ücretli AI için host izni `ai:invoke`), `settings:write`, `modules:write` (uygulama ayarları, eklentinin kendi sınırları dahil), `oplog:write`, `ai:write`; `Registry::permits` eklenti aktörü için bunları kayıtlı izin olsa bile reddeder. Dosyalar yalnız **`fs:pick`** ile: host yerel aç/kaydet penceresini gösterir (`App::set_file_picker`, masaüstünde `rfd`), dosyayı kendisi okur/yazar; eklenti yalnız dosya adını ve baytları görür (`host_fns::FS_OPEN/FS_SAVE`, en çok 8 MiB, yalnız kullanıcının başlattığı işlemde; önerilen ad klasörsüz). Eklentinin kendi ayarları `host_fns::SETTINGS_SET` (yalnız kendi şemasındaki anahtarlar). **Kancalar:** `before:`/`after:` yalnız public komutlara; `plugins.`, `oplog.`, `modules.`, `settings.`, `shortcuts.`, `io.`, `batch.`, `jobs.`, `mcp.`, `ai.providers./prices./routes.` ile başlayan ya da izni eklentiye verilemeyen metotlara **kanca takılamaz** (kurulumda `VALIDATION_PLUGIN_MANIFEST`; saklanmış satırda olsa bile dizine alınmaz). Kanca ayrıca hedef metodun kendi iznini ister: manifest onu da istemeli, kanca yalnız ikisi de verilmişken çalışır (kanca, metodu doğrudan çağırmaktan fazlasını vermez). Hassas: `network:*`, `audio:record`, `fs:pick`, `ai:invoke` (kurulumda işaretsiz gelir). Her katkı izni ister (manifest doğrulaması); izin geri alınınca katkı kapanır |
| Nerede çalışır | `before:` kancası: yazma işlemi açılmadan, yalnız sorgu + kendi deposu/ayarları; komut çağıramaz, parametreyi değiştirir ya da reddeder. `after:`/`event:` kancası: commit sonrası, yazdıkları tek geri al adımı, **kanca tetiklemez**. İşlem (`plugins.run`, `v1.plugin.<id>.<komut>`, `plugins.ui.*`): yazdıkları tek geri al adımı (`oplog.summary.plugins.run`), eklenti hata verirse geri sarılır; eklentinin kendi deposu (`kv`) ve ayarları da aynı gruba yazılır: hata olunca onlar da geri sarılır, kullanıcı verisiyle birlikte geri alınır, yalnız depo/ayar değiştiren çağrı geri al adımı bırakmaz (her bağlamda, `before:` kancası dahil); diğer eklentilerin `before:` kancaları uygulanır. Kart türü/TTS motoru: host erişimi yok (yalnız `log`). Eklentilerin yaptığı değişiklikler ve hata veren eklenti çağrısının geri sarılması (eklenti aktörüyle) `after:`/`event:` kancası tetiklemez; kanca çalışırken oluşan hiçbir olay kuyruğa girmez, kanca kanca içinde başlamaz (zincirleme yok). Kanca bütçesi: bir kullanıcı işlemi en çok `plugins.max_hooks_per_action` (200) kanca çalıştırır; aşan eklenti kapatılır ve kullanıcıya bildirilir (`plugins.notice.hook_budget`). Eklentiler geri al/yinele çağıramaz, eklenti işlemi başlatamaz |
| Hata | Hata türleri `OUT_OF_FUEL`, `TIMEOUT`, `MEMORY_LIMIT`, `TRAP`, `ABI` (arıza) ve `PLUGIN` (eklentinin kendi hatası; arıza sayılmaz). Arızalar art arda `plugins.max_failures` (3) olunca eklenti kapanır (`auto_disabled`, `plugins_changed{auto_disabled: true}`); açmak sayacı sıfırlar. `before:` hatası: varsayılan eklentisiz devam, `plugins.before_hook_failure = reject` ile `PROVIDER_PLUGIN_FAILED`. Eklenti günlüğü bellekte (son 200 satır, `plugins.logs`) |
| Depolama | `plugins` satırı (0600: `package_sha256`, `failures`, `last_error`, `auto_disabled`, `deleted_at`), paketler `plugin_packages` (her sürüm, içerik adresli; geri al/yinele bayt taşımaz). Kaldırma yumuşak; "verileri sil" KV ve ayarları da siler (geri al hepsini getirir). Ayarlar `settings` tablosunda `plugin:<id>` kapsamında |
| Arayüz | Bildirimsel düğümler `UiNode` (`column/row/heading/text/html/input/choice/audio_recorder/audio/button/feedback/progress`); HTML arka uçta temizlenir; metinler eklentinin çeviri anahtarı (`plugin.<id>.<anahtar>`) ya da düz veri. Eklentiler `modules.list`'te `plugin:<id>` modülü olarak görünür: ana menü `/plugins/view/<id>/<görünüm>`, kenar paneli, kart altı düğmesi (`v1.plugin.<id>.<komut>` + `{card_id}`), ayar sayfası. İzole web görünümü (§2.2) sonraya |
| Mağaza / modeller | `PluginRegistrySource` + `FileSource` (kurulum bu arayüzden geçer). Modeller `<koleksiyon klasörü>/models/<eklenti>/<model>`; indirme arka plan işi (`plugins.model_download`), şimdilik `file://` getirici (HTTP getirici aynı `Fetcher` arayüzüne takılır) |
| CLI | `fc plugin list | inspect <dosya> | install <dosya> [--grant <izin>…|--grant-all] | uninstall <id> [--delete-data] | enable | disable | grant <id> [izin…] | logs | run <id> <komut> [json] | pack <klasör> [--wasm] [--out]` (`pack` yerel geliştirici aracı: `pack_dir` + kurulumdaki manifest denetimi) |
| Henüz yok | `host.http.fetch` ve `host.ai.complete` izin denetler ama `PROVIDER_UNAVAILABLE` döner (HTTP istemcisi / AI sağlayıcısı v0.8); ses kaydı bileşeni çizilmez; `fc plugin new|build|test` (iskele/derleme: `cargo build --target wasm32-unknown-unknown` + `fc plugin pack`); **izole web görünümü** (§2.2: sandbox'lı iframe, sıkı CSP, izinle süzülmüş host API'ye `postMessage` köprüsü — ertelendi; bildirimsel arayüz şimdilik tek yol, böylece eklentinin tarayıcıda kod çalıştırma yüzeyi yok); paket imzası |

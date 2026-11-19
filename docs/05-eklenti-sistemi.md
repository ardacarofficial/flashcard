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

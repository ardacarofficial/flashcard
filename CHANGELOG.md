# Changelog

Biçim: [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/), sürümleme: [SemVer](https://semver.org/lang/tr/).

## [Unreleased]

### Eklendi — v0.5 İçe / dışa aktarma (iz D)
- **`fc-interop` crate'i** (saf, bayt girer bayt çıkar): esnek ayırıcılı metin/CSV/TSV (tırnak, özel ayırıcı, `max_fields`) + ayırıcı otomatik algılama; kendi JSON not biçimimiz; Quizlet metni; Anki `.apkg`/`.colpkg` okuma (eski `collection.anki2`/`.anki21` ve yeni zstd `collection.anki21b` + protobuf) ve yazma (şema 11) — cloze, medya, zamanlama ve tekrar geçmişi korunur; `.fcpack` deste paketi (sınıflandırma tanım/seçenek/değerleri kimlikleriyle); `.fcbackup` tam yedek; yazdırma HTML'i (çift taraflı baskı için aynalı arka yüzler).
- **İş kuyruğu çalışanı:** `fc_app::jobs::spawn_worker`, `App::run_pending_jobs`, modül başına `Registry::job`; `jobs.cancel` işin iptal işaretini yazıcıyı beklemeden verir; işler yazmayı yalnızca komutlarla yapar.
- **`io` modülü:** `io.import.preview/start`, `io.export.start`, `io.backup.create/inspect/restore`, `io.print.render`. İçe aktarma tek işlem + **tek geri al** adımıdır, yazarken iptal edilirse tamamen geri sarılır; tekrar edenler için atla/güncelle/yine de ekle; sütunlar alan/etiket/sınıflandırmaya eşlenir (istenirse eksik seçenekler oluşturulur). Geri yükleme önce mevcut durumun yedeğini alır.
- **Depolama:** `0400_interop` (medya içeriği `media_blobs`), medya/tekrar kaydı/koleksiyon döküm-yükleme repo alanı.
- **Masaüstü:** İçe/dışa aktar ekranı (önizleme, sütun eşleme, ilerleme + iptal, geri al), dışa aktarma, yedek/geri yükleme, yazdırma; Tauri dosya diyaloğu (yalnız aç/kaydet izni); TR/EN.
- **CLI:** `fc call` kuyruğa aldığı işi hemen çalıştırıp bitmiş işi yazar.

### Değişti
- `JobDto.result` ve `imported` / `collection_replaced` olayları eklendi (eklemeli).

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

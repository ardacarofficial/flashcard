# Changelog

Biçim: [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/), sürümleme: [SemVer](https://semver.org/lang/tr/).

## [Unreleased]

### v0.4 · Kart yönetimi (iz C, `feat/v0.4-manage`)

#### Eklendi
- **Sorgu dili (`fc-core::query`, saf):** ayrıştırıcı → sözdizimsel AST (karakter konumlu hatalar) → katalogla çözümleme → `Filter`; bellek içi referans değerlendirici. Serbest metin (FTS5 önek/ifade, `*` joker), `deck:`/`deste:`, `tag:`/`etiket:`, `notetype:`, `type:`, `card:`, `is:`, `flag:`, `stability/difficulty/lapses/reps/retrievability` karşılaştırmaları, `added/edited/reviewed/due:N`, `re:`, alan joker/regex, `nid:`/`cid:`, `-`/`NOT`/`OR`/parantez. **Sınıflandırmalar** anahtar ya da (yerelleştirilmiş) adla: `seviye:B1`, `seviye>=B1` (sıralı seçimde seçenek sırasıyla), `sınav:TYT ders:fizik`, sayı/tarih/evet-hayır/metin, `x:*`. Türkçe katlama (ı/İ/ş/ç/ğ/ö/ü).
- **fc-storage `search` alanı:** `Filter` → SQL + FTS5 (alan jokerlerinde FTS ön süzgeci), SQL fonksiyonları (`fc_fold`, `fc_glob`, `fc_html_glob`, `fc_regex`…), kayıtlı sorgular, tekrar grupları, çöp listesi, kart tekrar geçmişi, bütünlük raporu. Migration `0300_search` (mevcut FTS satırlarını katlar, sıralama indeksleri).
- **API (eklemeli):** `search.cards/ids/check`, `saved_queries.*`, `bulk.move_cards/set_suspended/set_buried/set_flag/reset_progress/add_tags/remove_tags/find_replace/set_attribute` (+ iç `bulk.restore_*`), `browse.card_info/duplicates/trash/check_database/repair_database`, `palette.list`, `shortcuts.list/set/reset`; hata kodları `VALIDATION_QUERY` (+`kind/start/end`), `VALIDATION_SAVED_QUERY_NAME`, `VALIDATION_SHORTCUT`, `VALIDATION_FIND_REPLACE`, `NOT_FOUND_SAVED_QUERY`, `NOT_FOUND_SHORTCUT_ACTION`, `CONFLICT_SHORTCUT`; olaylar `cards_changed`, `saved_queries_changed`.
- **fc-app modülleri `browse` ve `palette`** (ayrı ayrı kapatılabilir). Her toplu işlem tek komut = tek geri al (birebir anlık görüntü tersi).
- **Masaüstü:** kart tarayıcısı (canlı konumlu sorgu hatası, kayıtlı sorgular, sıralanabilir tablo, not modu, "eşleşenlerin hepsini seç", toplu işlem çubuğu, kart bilgisi), bakım ekranı (çöp kutusu, tekrarlar, veritabanı denetimi + onarım), kısayol düzenleyici (kaydet/kaldır/varsayılan, çakışmalar), Ctrl+K komut paleti (tüm public metotlar + ekranlar).

#### Değişti
- SQLite bağlantı başına sayfa önbelleği 64 MiB (`CACHE_SIZE_KIB`); FTS metni katlanmış yazılır.

#### Testler
- Proptest: yazdır→ayrıştır gidiş-dönüş, ayrıştırıcı toplamlığı + sınır içi hata konumları, olumsuzlama tümleyeni; SQL ↔ referans değerlendirici (her atom + 300 rastgele bileşik sorgu); 1000 kartlık toplu işlemlerin tek geri alla birebir dönüşü; palet tamlığı; 100k not benchmark'ı (`#[ignore]`, docs/04).

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

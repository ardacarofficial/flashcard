# Changelog

Biçim: [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/), sürümleme: [SemVer](https://semver.org/lang/tr/).

## [Unreleased]

### v0.3 · Kart türleri ve içerik (iz B)

#### Eklendi
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

#### Değişti
- `notes.create` kartları not türü şablonlarının motorlarından üretir (`standard_card_specs` yerine); `notes.update` eksik kartları ekler (tersi `cards.trash_generated`).
- CSP: `style-src 'unsafe-inline'` (kart CSS'i, KaTeX), `media-src fcmedia:`, `font-src data:`.

#### Düzeltildi (inceleme)
- Not türü CSS'i kapsamdan kaçamaz: `sanitize_css` kaçışları normalleştirir, parantezleri dengeler; frontend dengesiz CSS'i basmaz.
- Silinen cloze numarasının kartı `empty: true` (`cards.render`, `cards.meta`); çalışma atlar.
- Çeldirici havuzu tek sınırlı, tohumlu sorgu (deste boyunca not okuma yok).
- `media.ref_count` okunurken sayılır (kalıcı silmede bayatlamaz); geri alınan `media.add`'in dosyası açılışta temizlenir.
- Bayrak numaralaması Anki'ninki (1 kırmızı … 5 pembe, 7 mor), `card_flag_defs` tek kaynak.
- Kullanılmayan `NoteAddForm` kaldırıldı.

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

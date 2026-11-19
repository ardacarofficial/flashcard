# Changelog

Biçim: [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/), sürümleme: [SemVer](https://semver.org/lang/tr/).

## [Unreleased]

### Eklendi — v0.2 Çalışma çekirdeği (İz A, `feat/v0.2-study`)
- **`fc-study` (yeni saf crate):** FSRS zamanlayıcı (`fsrs-rs` 7, **FSRS-6** modeli, varsayılan ağırlıklar; ağırlıklar preset'ten değiştirilebilir) ve SM-2 yedeği aynı `Scheduler` arayüzünün arkasında; Anki uyumlu öğrenme/yeniden öğrenme adımları (adım `due - last_review`'dan çıkarılır, şema değişmez); cevap kontrolcüleri (birebir, normalize — Türkçe harf eşdeğerliği ayarlı —, yazım toleranslı, çoklu cevap; `doğru/neredeyse/yanlış` + fark); ipucu stratejileri (ilk harfler, kelime başları, kısmi); çalışma kuyruğu (öğrenme → vadesi gelen → yeni, kardeş gömme, sıralama, tohumlu karıştırma); çalışma günü sınırı (yerel saat + devir saati). G/Ç yok; saflık betiği artık bu crate'i de denetler.
- **`study` modülü (fc-app):** kalıcı oturum motoru (`sessions`, çökme kurtarma: `study.active`), `study.start/session/answer/hint/check_answer/end/summary`, `study.suspend/bury/postpone`, `study.card_history` (geçmiş + hatırlama olasılığı), iç ters metot `study.restore`. Puanlama tek adımda geri alınır: kart sütunları, gömülen kardeşler, oturum durumu ve `review_log` satırı birebir döner. Deneme modu `review_log`'a `mode = "trial"` yazar, `cards`'a hiç dokunmaz. Yanlışları tekrar (mistakes) oturumu, sınıflandırmaya göre özel oturum (yapılı filtre; v0.4 sorgu dili gelince bağlanacak), cevap süresi (istemci bildirir, arka uç kendi saatiyle sınırlar). Ayarlar: zamanlayıcı, oturum başına yeni kart, sıra, kardeş gömme, yazarak cevap, cevap kontrolcüsü, Türkçe eşdeğerlik, otomatik puanlama, ipucu türü, gün başlangıç saati.
- **Depolama:** `0100_study.sql` (oturum ve açık oturum indeksleri), `StudyRead/StudyWrite` (oturumlar, `review_log`, sınıflandırma koşullu aday kartlar), SQLite + `MemStore` uygulamaları.
- **API:** `v1.study.*` DTO'ları ve 13 metot, hata kodları `NOT_FOUND_SESSION`, `CONFLICT_SESSION_ENDED`, `CONFLICT_NOT_CURRENT_CARD`, `VALIDATION_SCHEDULER_PARAMS`, olay `study_cards_changed`; TS tipleri üretildi.
- **Masaüstü:** Çalış ekranı (`features/study/`): tamamen klavyeyle (Boşluk göster, 1–4 puanla, H ipucu, S askıya al, B göm, Z geri al, Esc bitir, R yanlışları tekrar), sonraki aralık butonların üstünde, deneme modu, sınıflandırma filtresi, erteleme, oturum sonu ekranı (sayı, doğruluk, süre, yanlışlar), yazarak cevap + fark; TR/EN metinler; geliştirme mock'u.
- **CLI:** study komutlarının geri al özetleri (TR/EN).

### Testler
- `fc-study`: 30 test — FSRS-6 formüllerinin bağımsız referans uygulamasıyla 365 günlük deterministik simülasyon (adımlı/adımsız, 4 tohum), ilk tekrar referans değerleri, adım makinesi, SM-2, kontrolcüler, ipuçları, kuyruk.
- `study` servis testleri: 15 senaryo × 3 depo (MemStore, SQLite bellek, SQLite dosya) — puanla → geri al → kart birebir, deneme modunda `cards` değişmez, uygulama düzeyinde 365 günlük simülasyon + `review_log`'dan yeniden hesaplama, yeniden başlatmada oturum sürer, …
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

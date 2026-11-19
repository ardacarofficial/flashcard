# Changelog

Biçim: [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/), sürümleme: [SemVer](https://semver.org/lang/tr/).

## [Unreleased]

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

### Eklendi — v0.2 Çalışma çekirdeği (İz A, `feat/v0.2-study`)
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
### Eklendi — v0.5 İçe / dışa aktarma (iz D)
- **`fc-interop` crate'i** (saf, bayt girer bayt çıkar): esnek ayırıcılı metin/CSV/TSV (tırnak, özel ayırıcı, `max_fields`) + ayırıcı otomatik algılama; kendi JSON not biçimimiz; Quizlet metni; Anki `.apkg`/`.colpkg` okuma (eski `collection.anki2`/`.anki21` ve yeni zstd `collection.anki21b` + protobuf) ve yazma (şema 11) — cloze, medya, zamanlama ve tekrar geçmişi korunur; `.fcpack` deste paketi (sınıflandırma tanım/seçenek/değerleri kimlikleriyle); `.fcbackup` tam yedek; yazdırma HTML'i (çift taraflı baskı için aynalı arka yüzler).
- **İş kuyruğu çalışanı:** `fc_app::jobs::spawn_worker`, `App::run_pending_jobs`, modül başına `Registry::job`; `jobs.cancel` işin iptal işaretini yazıcıyı beklemeden verir; işler yazmayı yalnızca komutlarla yapar.
- **`io` modülü:** `io.import.preview/start`, `io.export.start`, `io.backup.create/inspect/restore`, `io.print.render`. İçe aktarma tek işlem + **tek geri al** adımıdır, yazarken iptal edilirse tamamen geri sarılır; tekrar edenler için atla/güncelle/yine de ekle; sütunlar alan/etiket/sınıflandırmaya eşlenir (istenirse eksik seçenekler oluşturulur). Geri yükleme önce mevcut durumun yedeğini alır.
- **Depolama:** `0400_interop` (medya içeriği `media_blobs`), medya/tekrar kaydı/koleksiyon döküm-yükleme repo alanı.
- **Masaüstü:** İçe/dışa aktar ekranı (önizleme, sütun eşleme, ilerleme + iptal, geri al), dışa aktarma, yedek/geri yükleme, yazdırma; Tauri dosya diyaloğu (yalnız aç/kaydet izni); TR/EN.
- **CLI:** `fc call` kuyruğa aldığı işi hemen çalıştırıp bitmiş işi yazar.

### Değişti
- `JobDto.result` ve `imported` / `collection_replaced` olayları eklendi (eklemeli).
- **v0.5 inceleme düzeltmeleri:** büyük içe aktarma ~500 satırlık parçalarla kendi işlemlerinde yazılır (çalışırken çalışma/düzenleme komutları zaman aşımına uğramaz), yine tek geri al adımıdır; iptal işlenmiş parçaları iz bırakmadan kaldırır (`io.import.discard`). Zip/zstd bombalarına toplam bütçe. Windows-1254 ve UTF-16 metin dosyaları. Eski şemalı yedek geri yüklenirken sonraki migration'lardan geçirilir; güvenlik yedeği uygulama veri klasörüne yazılır. İş sonu durumu yazımı yeniden denenir. Anki gömülü kartları korunur. Medya `MediaStore` trait'i arkasında (v0.3 deposuna bağlanmaya hazır), alan referansı `media:<sha256>`. Ayırıcı algılama için ayrı tutulmuş örnek seti (40/46). Ardından: algılama sayı/saat/URL parçalarını maskeler, gerekirse `max_fields` önerir, `::`/`–`/`—`/`→` adayları, Anki `#separator:` başlıkları (2. tutulmuş set: ilk ölçüm 63/70, şimdi 65/70). **Çökme güvenliği:** içe aktarma parçaları kalıcı bir işaretle (`0401_unfinished_batches`) izlenir; uygulama parçalar arasında kapanırsa açılışta işlenmiş parçalar geri sarılır.

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

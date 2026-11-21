# 04 · Yol Haritası

Her sürümün bir **amacı**, **kapsamı** ve **kabul kriterleri** vardır. Bir sürüm, kabul kriterlerinin tamamı sağlanmadan bitmiş sayılmaz.

Süre tahminleri **duvar saati (wall-clock)** cinsindendir ve AI ajanlarıyla geliştirme varsayar. İki sütun: 4 çekirdekli bulut ortamı ve 8 çekirdek / 16 GB VDS (Hetzner CPX42) üzerinde 4-5 paralel ajan.

## 📅 Faz özeti

| Faz | Sürümler | Paralellik | 4 çekirdek | 8 çekirdek VDS |
|---|---|---|---|---|
| 1. Temel | v0.1 | Sıralı | 5-7 saat | 4-6 saat |
| 2. Çekirdek özellikler | v0.2 · v0.3 · v0.4 · v0.5 | 4 iz | 8-12 saat | 7-10 saat |
| 3. Akıllı katman | v0.6 · v0.7 · v0.8 | 3 iz | 10-14 saat | 8-12 saat |
| 4. Dış dünya ve modlar | v0.9 · v0.10 | 2-3 iz | 6-9 saat | 5-7 saat |
| 5. Cila ve yayın | v1.0 | Kısmen | 6-9 saat | 5-7 saat |
| **Toplam** | | | **~35-51 saat** | **~30-42 saat** |

> Bu tahminler kullanıcı geri bildirimi ve Windows testleri için geçen süreyi **içermez.** En riskli kalemler: eklenti sistemi (v0.7) ve .apkg formatı (v0.5).

### Bağımlılık grafiği

```
v0.1 ──┬── v0.2 (çalışma) ──────┬── v0.6 (istatistik/planlama)
       ├── v0.3 (kart türleri) ──┤
       ├── v0.4 (yönetim) ───────┼── v0.7 (eklentiler) ──┬── v0.8 (AI) ──┐
       └── v0.5 (içe/dışa) ──────┘                       │               ├── v0.10 (öğrenme modları) ── v1.0
                                                          └── v0.9 (dış dünya) ─┘
```

- v0.2–v0.5 sadece v0.1'e bağlıdır, paralel geliştirilir. (v0.4'ün sorgu dili v0.2 özel oturumlarına lazım: v0.2 önce basit filtreyle başlar, sorgu dili gelince ona bağlanır.)
- v0.8'in AI sağlayıcı arayüzü v0.7'deki sağlayıcı altyapısını kullanır. Sağlayıcı **arayüzleri** v0.1'de tanımlanırsa v0.7 ve v0.8 daha paralel ilerler.

---

## 🏗️ v0.1 · Temel

**Amaç:** Her şeyin üstüne kurulacağı sağlam iskelet. API sözleşmesinin ilk hali.

**Kapsam**
- Cargo workspace ve crate yapısı ([03 §2](03-mimari.md)), pnpm ile frontend, Tauri 2 kabuğu.
- `fc-core`: temel modeller (Deck, Note, NoteType, Card, Tag), `Clock`/`Rng` soyutlamaları, sağlayıcı/strateji arayüzlerinin **imzaları** (gövdeler sonraki sürümlerde).
- `fc-storage`: SQLite bağlantı havuzu (WAL), migration altyapısı, ilk şema, repository'ler.
- `fc-app`: komut/sorgu altyapısı, kayıt defteri (registry), `oplog` + geri al, olay yolu, iş kuyruğu iskeleti, modül kaydı, şemalı ayarlar.
- `fc-api`: v1 ad alanı, hata modeli, TypeScript tip üretimi.
- Tauri IPC köprüsü (tek genel `invoke(command, payload)` + olay akışı).
- `fc-cli`: deste/not oluşturma ve listeleme (çekirdeğin Tauri'siz çalıştığının kanıtı).
- i18n altyapısı (TR/EN anahtarları), tema token altyapısı.
- Sınıflandırmalar ([02 §8.1](02-ozellik-katalogu.md)): `attribute_defs`/`attribute_values` şeması, tanımlama/düzenleme/seçenek komutları (geri alınabilir), deste ağacında miras çözümü, hazır şablonlar (CEFR, YKS…). Not ekleme formunda alan olarak görünür.
- Minimal arayüz: deste ağacı, not ekleme (klasik), not listesi.
- CI: fmt, clippy (`-D warnings`), testler, tip üretimi tutarlılık kontrolü, Windows derlemesi.

**Kabul kriterleri**
- [x] `cargo test --workspace` geçer, `fc-core` I/O içermez (lint/inceleme ile doğrulanır).
- [x] Deste/not oluşturma, düzenleme, silme komutları çalışır ve **geri alınabilir.**
- [x] Rust'ta bir DTO alanı değişince frontend `pnpm check` kırılır.
- [x] Aynı işlemler CLI ile de yapılabilir.
- [x] Boş veritabanından migration testi geçer.
- [x] Bir desteye "Seviye" (A1–C2) sınıflandırması tanımlanır, alt destede not eklerken seçilebilir, seçenek yeniden adlandırma/sıralama geri alınabilir.
- [ ] Windows'ta CI derlemesi başarılı. *(iş akışı hazır, henüz Windows'ta koşmadı)*

> **Doğrulama (entegrasyon, `feat/v0.1-integration`):** `fc-app/src/acceptance.rs` (gerçek SQLite dosyası: boş veritabanından migration, deste/not oluştur-düzenle-sil + birebir geri al/yinele, üst destede "Seviye" A1–C2 → alt deste notunda seçim, seçenek yeniden adlandırma/sıralama geri alma, yeniden açınca durum ve geri al korunur); `store_suite!` MemStore + SQLite (bellek ve dosya) üzerinde; `fc-cli/tests/cli_e2e.rs` gerçek `fc` ikilisiyle; `check-core-purity.sh`; DTO alanı yeniden adlandırılıp `gen-ts` çalıştırılınca `pnpm check` 7 hata verdi (geri alındı).

## 📖 v0.2 · Çalışma çekirdeği

**Amaç:** Uygulama günlük kullanılabilir hale gelir.

**Kapsam:** FSRS entegrasyonu (`fsrs-rs`), SM-2 yedek; oturum motoru; çalışma ekranı; puanlama ve sonraki aralık gösterimi; geri al; kart geçmişi ve `review_log`; cevap süresi; askıya al/göm; kardeş gömme; yazarak cevaplama + cevap kontrolcüleri (birebir, normalize, toleranslı, çoklu); ipucu motoru; deneme modu; yanlışları tekrar çalış; çalışmayı ertele; oturum sonu ekranı; çalışma kısayolları.

**Kabul kriterleri**
- [x] Zamanlayıcı testleri: deterministik saatle 365 günlük simülasyon, FSRS referans değerleriyle uyum.
- [x] Puanlama → geri al → kart eski durumuna birebir döner.
- [x] Deneme modunda `cards` tablosu değişmez.
- [x] Bir oturum tamamen klavyeyle yürütülebilir.

> **Doğrulama (`feat/v0.2-study`):** (1) `fc-study/src/scheduler/tests.rs`: FSRS-6 formüllerinin spesifikasyondan yazılmış bağımsız f64 uygulamasıyla 365 günlük simülasyonlar (adımlı/adımsız, 4 tohum) her geçişte kararlılık/zorluk/aralık uyumu + ilk tekrar referans değerleri; `fc-app/src/modules/study/tests.rs::simulation_365_days` uygulama düzeyinde sabit saatle bir yıl, kart durumu `review_log`'dan birebir yeniden hesaplanır, iki koşu aynı. (2) `answer_undo_redo_is_exact`, `sibling_burying_is_undone_exactly`: geri alınca kartın **tüm** sütunları (gömülen kardeş dahil) eşit, `review_log` satırı silinir, oturum kartı yeniden gösterir; yinele birebir. (3) `trial_mode_never_changes_cards`: deneme oturumu (ipucu, kontrol, adımlar, geri al dahil) sonrası `cards` tablosunun tamamı aynı, `review_log` `mode = "trial"`. Hepsi MemStore + SQLite bellek + SQLite dosyada. (4) `apps/desktop/src/lib/features/study/StudyScreen.test.ts`: başlat → ipucu → göster → puanla → geri al → … → özet → yanlışları tekrar → geri dön, yalnızca tuş basışlarıyla. **Not:** FSRS-7 yerine FSRS-6 (bkz. 08 K10).
>
> **Faz 2 entegrasyonu (v0.2 × v0.3):** çalışma kartları v0.3 kart türü motoruyla çizilir (temizlenmiş HTML), boş kartlar atlanır, yazarak türler v0.2 kontrolcüleriyle, seçmeli kartlar motorla değerlendirilir, preset günlük limitleri uygulanır, bayraklar Anki numaralamasıyla gösterilir; uçtan uca `fc-app/src/phase2_tests.rs` (bkz. 10 §13).
>
> **Faz 2 tur 3 (+ v0.4):** çalışma deste ayarlarını yalnız `presets::effective` ile okur (deste → üst deste → varsayılanlar; limitler, adımlar, hedef hatırlama, FSRS ağırlıkları, en uzun aralık, kardeş gömme, yeni/tekrar sırası); şablon değişince bir sonraki kuyruk ve aralıklar değişir (`phase2_manage_tests.rs::deck_preset_drives_queue_and_intervals`). Özel oturum sorgu dilinden (`seviye:A2 is:due`) ya da kayıtlı sorgudan ("filtreli deste") başlar (`custom_sessions_from_queries`). Çalışma kısayolları kısayol düzenleyicisinden değiştirilebilir (`palette_and_shortcuts_cover_every_track`, `keys.test.ts`).

## 🧩 v0.3 · Kart türleri ve içerik

**Kapsam:** Not türü şeması ve editörü; şablon dili; kart türü motoru; klasik, ters, isteğe bağlı ters, cloze, çoktan seçmeli, yazarak, resim üzerinde gizleme, resimli quiz, sesli kart; hazır not türleri (kelime kartı dahil); Markdown editör, KaTeX, kod blokları; medya deposu, sürükle-bırak, Ctrl+V; kart önizleme; `[[bağlantılar]]` ve geri bağlantılar; kart notu; bayraklar (özel adlarıyla).

**Kabul kriterleri**
- [x] Yeni bir kart türü sadece `CardTypeEngine` uygulanarak eklenebilir (testte örnek tür ile kanıt). → `fc-app/src/cards_tests.rs::a_new_card_type_is_just_an_engine` (test içi `ReverseWords` motoru, 3 depoda)
- [x] Not türüne alan eklemek/çıkarmak mevcut kartları ve geçmişi bozmaz. → `notetype_field_add_remove_keeps_cards_and_history` (kart kimlikleri/zamanlama aynı, geri al değerleri birebir getirir, eski not düzenlemesi hâlâ geri alınır)
- [x] Aynı medya iki kez eklenince tek dosya tutulur. → `same_media_twice_is_one_file_on_disk`, `same_media_twice_is_one_entry`, `fc-storage` `same_media_twice_is_one_file_and_one_row`
- [x] HTML temizleme testleri (XSS vektörleri) geçer. → `fc-core` `content::html::tests::xss_vectors_are_neutralized` (36 vektör) + uçtan uca `rendering_is_sanitized` (alan, kullanıcı şablonu, seçenek, CSS)

> **Faz 2 tur 3 (+ v0.4):** özel bayrak adları sorgu dilinde (`flag:önemli`, `flag:çok_zor`; TR/EN renk adları her zaman), bayrak *n* = bit *n − 1* (`phase2_manage_tests.rs::custom_flag_names_in_queries`). Arama dizini cloze/HTML/bağlantı işaretlemesinden arındırılmış, katlanmış düz metin tutar (`0302_search_plain_text`, `search_index_is_folded_plain_text`).

## 🛠️ v0.4 · Kart yönetimi

**Kapsam:** Sorgu dili (ayrıştırıcı + SQL derleyici + FTS5); arama; kayıtlı filtreler; özel çalışma oturumları; kart tarayıcısı; toplu işlemler; bul-değiştir; çöp kutusu; tekrar tespiti; kart bilgi ekranı; veritabanı kontrolü; deste ayar şablonları; komut paleti; kısayol düzenleyici.

**Kabul kriterleri**
- [x] Sorgu dili için ayrıştırıcı özellik tabanlı testleri; hatalı sorguda konumlu hata.
- [x] 100k notta arama < 100 ms (benchmark).
- [x] 1000 kartlık toplu işlem tek geri al ile tamamen geri döner.
- [x] Komut paleti kayıt defterindeki tüm komutları listeler.

> **Doğrulama (`feat/v0.4-manage`):**
> - *Özellik tabanlı testler + konumlu hata:* `fc-core/src/query/tests.rs` (proptest: `print_parse_roundtrip`, `parser_is_total_and_positions_are_in_bounds`, `query_alphabet_fuzz`, `term_spans_slice_the_source`, `negation_is_complement`; `errors_are_positioned_in_characters`, `classification_errors_are_positioned`). Uçtan uca: `fc-app/src/modules/browse/tests.rs::search_errors_are_positioned` (`VALIDATION_QUERY`, `details.start/end` karakter cinsinden). SQL derleyicisi referans değerlendiriciyle karşılaştırılır: `fc-storage/src/tests/search.rs` (`every_atom_agrees_…`, `random_queries_agree_…`: 300 rastgele bileşik sorgu).
> - *Benchmark:* `cargo test -p fc-storage --release -- --ignored --nocapture bench_search_100k` — 100k not / 100k kart, tam tarayıcı isteği (toplam sayı + ilk 50 satır + satır verisi), 5 denemenin en iyisi; test her sorgu < 100 ms'yi doğrular. 8 çekirdek, yük ~2.5 (başka izler derlerken): boş sorgu ~17 ms, `kilic` ~30, `"kılıç kalkan"` ~5, `deck:İngilizce is:due` ~25–38, `tag:fiil -is:suspended` (koleksiyonun ~%48'i) **~50** (önce 88–95), `seviye>=B1 sınav:TYT` ~71–73 (en pahalısı, önce 65–72), `stability>20 lapses>=2` ~21–25, `retrievability<0.8` ~37–40, `front:kılıç*` ~58, `(seviye:B1 OR seviye:B2) app` ~42–45. Nasıl: sayım ve sayfa ayrı (pencere sayımı her eşleşmeyi sıralatıyordu); not sütunu okumayan süzgeçler `notes` birleşimi olmadan kapsayan indekslerden (`0301_search_covering`); ≥20k eşleşmede sayfa sıralama indeksinden okunup ilk sayfada durulur (küme yüklemleri ilişkili `EXISTS`), daha azında eşleşen rowid'ler sıralanır. **CI'da koşan küçük benchmark:** `bench_search_20k_scaled` (yoksayılmaz, her `cargo test`'te; 20k not, bütçe 100 ms × 20k/100k × 3 gürültü payı = 60 ms; ölçülen en kötü ~14 ms). Sıralı tarama = tam sıralama: `ordered_scan_pages_match_a_full_sort`.
> - *1000 kart tek geri al:* `fc-app/src/modules/browse/tests.rs::bulk_ops_on_1000_cards_undo_in_one_step` (taşı, askı, göm, bayrak, sıfırla, etiket ekle/çıkar, bul-değiştir, sınıflandırma ata; her biri tek oplog girdisi, geri al → durum birebir, yinele → birebir) — MemStore, SQLite bellek ve SQLite dosya. Geri al `updated_at`'i de geri getirir: `undo_restores_modification_times`.
> - *Komut paleti:* `…/browse/tests.rs::palette_lists_every_registry_command` (`palette.list` = registry'deki tüm public metotlar) + `apps/desktop/src/lib/features/browse/palette.test.ts` (arayüz her metodu listeler, her metodun TR/EN etiketi var).
> - *Deste ayar şablonları:* `fc-app/src/modules/presets.rs` testleri (`crud_validation_and_undo`, `assignment_inheritance_delete_and_undo`, `module_can_be_disabled`; üç depoda) + `fc-core/src/preset.rs` (doğrulama, v0.1 satırlarının okunması) + `apps/desktop/src/lib/features/presets/PresetsScreen.test.ts`. Şema docs/10 §16.1.
> - *Çöp kutusu (30 gün, ayarlanabilir):* `…/browse/tests.rs::trash_is_purged_after_retention` (süresi dolan silinir, yenisi kalır, `0` = boşalt, ayar doğrulanır); açılışta `App::load_state` temizler.
> - *Kayıtlı sorgular yeniden adlandırma/sıralamaya dayanıklı:* `fc-core/src/query/tests.rs::stored_queries_follow_option_renames_and_reorders`, `…/browse/tests.rs::saved_queries_follow_option_renames_and_reorders`.
> - *Veritabanı denetimi:* `…/browse/tests.rs::database_check_and_repair_undo` (yetim kart, vadesiz kart, eksik alan, sahipsiz değer; onar → sağlıklı, geri al → birebir).
> - ~~**Kapsamdan kalanlar:**~~ **Faz 2 tur 3'te tamamlandı** (`fc-app/src/phase2_manage_tests.rs`, üç depoda; docs/10 §17): toplu "not türünü değiştir" (`bulk.change_notetype`: alan ve şablon eşleme, eşlenen şablonun kartı kimliği ve geçmişiyle kalır, tek geri al — `change_notetype_keeps_history_and_undoes`); özel çalışma oturumu (`StudyFilter.query` / `saved_query_id` → `search_card_ids`, kayıtlı sorgu = filtreli deste — `custom_sessions_from_queries`); özel bayrak adları (`browse::flag_names` ← `card_flag_defs` — `custom_flag_names_in_queries`). Komut paleti dört izin tüm komutlarını TR/EN etiketle listeler (`palette.test.ts`, `palette_and_shortcuts_cover_every_track`).

## 📥 v0.5 · İçe / dışa aktarma

**Kapsam:** Esnek ayırıcılı metin/CSV içe aktarma (önizleme, otomatik algılama, alan eşleme, tırnak işleme); JSON; Quizlet metni; Anki .apkg/.colpkg içe ve dışa (eski ve yeni şema sürümleri, medya, geçmiş); deste paketi (`.fcpack`); tam yedek (`.fcbackup`) ve geri yükleme; yazdırma/PDF.

**Kabul kriterleri**
- [x] **v1.0'da karşılandı** — `Arda, porsuk` ve `Arda: porsuk` ve özel ayırıcılı dosyalar doğru ayrışır ✅; otomatik algılama test kümesinde ≥ %95 doğru: **ayar setinde karşılandı, tutulmuş sette değil.** Ölçümler: ayar seti ≥ %95; ayardan sonra yazılan 2. tutulmuş sette ilk ölçüm %90,0 (63/70), Anki `#separator:` başlık desteğiyle **%92,9 (65/70)** (o iki örnek artık saf tutulmuş değil). Kalan kaçanlar: `~` ayırıcı, çift TAB / boşlukla hizalı sütunlar, her satırında virgül olan cümle listesi, tireli kelime listesi; önizleme her zaman elle düzeltmeye izin verir. **→ v1.0'da iyileştirilecek** (yeni bir tutulmuş setle yeniden ölçülerek). Ayrıntı docs/10 §14. **v1.0:** 2. set kaçanlarına göre ayarlandı (70/70, ayarlı); ayardan önce yazılan 3. tutulmuş sette tek ölçüm **%90,8 (99/109)**; 3. set kaçanlarına göre ayarlandı (109/109, ayarlı); ayardan önce kör yazılan 4. tutulmuş sette tek ölçüm **%96,2 (100/104)** ✅. Ayrıntı docs/10 §34.
- [x] **v1.0'da karşılandı** — Örnek Anki desteleri (cloze, resimli, sesli) kayıpsız içe aktarılır ✅ (yapı testli); dışa aktarılan .apkg'nin Anki'de açıldığı **doğrulanmadı**: paket yalnız yapı testinden geçer (şema 11 tabloları, JSON anahtarları, referans bütünlüğü), gerçek Anki'de elle açılmadı ve testlerde gerçek Anki'nin ürettiği bir dosya yok (fikstürler sentetik). **→ v1.0 öncesi** gerçek Anki ile elle deneme + Anki'nin ürettiği bir örnek paketle test. Faz-2 tur 2'den beri Anki'nin hazır not türleri v0.3 yerleşiklerine eşlenir ve kartlar v0.3 motorlarıyla çizilir — docs/10 §15. **v1.0:** gerçek Anki'nin (resmî `anki` 26.09.4) ürettiği paketler (temel, ters, yazmalı, cloze, resim maskesi, ses, tekrar geçmişi, FSRS) fikstür olarak testlerde; bizim .apkg/.colpkg'miz gerçek Anki'de açılıp karşılaştırılır (`our_exports_open_in_real_anki`, CI). Ayrıntı docs/10 §34.
- [x] Yedek → sil → geri yükle döngüsü veriyi birebir korur.
- [x] İçe aktarma iptal edilebilir ve tek geri al ile geri döner.

> **Doğrulama (`feat/v0.5-io`):**
> - Ayrıştırma/algılama: `fc-interop/src/text/tests.rs` (`acceptance_examples_parse`, 70 örneklik etiketli kümede `detection_is_at_least_95_percent_accurate`); uçtan uca `fc-app/src/modules/io/tests.rs::text_import_parses_acceptance_examples_with_auto_detection`.
> - Anki: fikstürler testte sentetik üretilir (`fc_interop::anki::fixtures`: cloze + resim + ses notu, tüm kart durumları, filtreli deste, geçmiş) ve üç paket düzeninde (eski `collection.anki2`, `collection.anki21`, yeni zstd/protobuf `collection.anki21b`) yazılır. `all_layouts_read_the_same_collection`, `converts_cloze_image_sound_scheduling_and_history`, `export_then_import_roundtrips`, `exported_package_is_a_valid_anki_collection` (şema 11 tablo/sütunları, `ver = 11`, model/deste/ayar JSON anahtarları, referans bütünlüğü, checksum, medya listesi); uygulama düzeyinde `anki_packages_import_losslessly_in_every_layout`, `exported_apkg_reimports_with_the_same_content`.
> - Yedek: `backup_delete_restore_preserves_everything` (Anki içe aktarma + sınıflandırma + oplog içeren koleksiyon → `.fcbackup` → deste sil → geri yükle → `jobs` ve `oplog` dışındaki **tüm tablolar** ve medya baytları birebir aynı (geri yükleme geçmişi yeniden başlatır: oplog boşalır, tek bir geri yükleme bariyeri yazılır — docs/10 §18); otomatik güvenlik yedeği silinmiş durumu geri getirir). Depolama düzeyinde `dump_then_load_is_bit_exact`.
> - İptal + tek geri al: `cancelling_in_the_middle_of_a_chunk_rolls_the_chunk_back` (64 satır yazıldıktan sonra iptal → parçanın işlemi geri sarılır, oplog değişmez), `cancel_after_committed_chunks_leaves_no_trace`, `jobs_cancel_stops_queued_and_running_imports`, `import_is_one_undo_step_and_redoable` (yeni desteler + not güncelleme + yeni sınıflandırma seçenekleri tek `oplog.undo` ile geri, `oplog.redo` ile aynen ileri).
> - Sınıflandırmalar: `text_columns_map_to_classifications_and_create_missing_options`, `fcpack_preserves_classifications_exactly`.
> - **Faz-2 tur 2 (v0.5 × v0.3 × v0.2), uçtan uca:** `fc-app/src/phase2_io_tests.rs::anki_import_study_backup_wipe_restore_is_identical` — CSV sütunu → Seviye (eksik seçenek oluşturulur) → cloze + resim + ses + mor bayrak + bugünden bir tekrar içeren `.apkg` → yerleşik not türleri, aynı resmi başka adla taşıyan ikinci paket yeni medya eklemez (diskte 2 dosya) → kartlar motorla çizilir (`media:` resim/ses, cloze) → içe aktarılan geçmiş `imported`, günlük tekrar limitini tüketmez → çalışma `normal` tekrar yazar → yedek → koleksiyon klasörü tamamen silinir → boş koleksiyona geri yükleme → `jobs` ve `oplog` dışındaki tüm tablolar, medya dosyaları ve sınıflandırmalar birebir.

## 📊 v0.6 · İstatistik ve akıllı planlama

**Kapsam:** [02 §17](02-ozellik-katalogu.md) istatistiklerinin tamamı; FSRS kişisel optimizasyonu; hedef hatırlama oranı (+ iş yükü tahmini); yük dengeleme; kolay günler; sınav tarihi modu; hedefler; deste karnesi.

**Kabul kriterleri**
- [x] İstatistikler `review_log`'dan yeniden üretilebilir ve önbellekle aynı sonucu verir.
- [x] Optimizasyon arka plan işi olarak çalışır, iptal edilebilir.
- [x] Yük dengeleme simülasyonda günlük yük varyansını düşürür (test).

> **Doğrulama (`feat/v0.6-stats`):** (1) `fc-app/src/modules/stats/tests.rs::cache_equals_recomputation` (MemStore + SQLite bellek + dosya): her adımda önbellekli `stats.overview` = `fresh` yeniden hesaplama (alanı alanına) ve sayılan tekrar = `review_log`'un deneme dışı satırları; günler boyu cevaplar (SQLite'ta önbellek + eklenen satırlar katlanır), cevabın geri alınması (satır silinir → önbellek geçersiz), yinele, içe aktarılan tekrarlar (sayılır, `imported`), deneme oturumu (sayılmaz), not çöpe + kalıcı temizlik (tekrar geçmişi kalır, toplamlar ve önbellek değişmez — 0501), ikinci takvim (UTC+3). Saf düzeyde `fc_core::stats::tests::aggregates_are_a_monoid` (proptest: parça parça toplanıp birleştirilen = tek seferde toplanan). (2) `optimizer_job_saves_weights` (iş → ağırlıklar preset'e, tek geri al), `optimizer_job_cancelled_before_it_runs`, `optimizer_job_cancelled_while_training` (gerçek arka plan çalışanı, SQLite dosyası: iş koşarken `jobs.cancel` → `cancelled`, preset değişmez); `fc_study::optimize::tests::abort_flag_interrupts_training` (eğitim başka iş parçacığından durdurulur). (3) `fc_study::planning::tests::load_balancing_lowers_daily_load_variance`: 365 gün, 120 gün boyunca günde 20 yeni kart, FSRS, olasılıkla hatırlama — 15 günlük hareketli ortalamaya göre günlük yük varyansı **29.5 → 8.4**, toplam iş ±%10 içinde, tepe yük daha düşük; `easy_days_get_fewer_reviews`, `exam_mode_maximizes_recall_on_exam_day`; uygulama düzeyinde `load_balancing_spreads_equal_cards`, `exam_mode_and_exam_commands`. Sınıflandırma kırılımı: `breakdown_by_classification`. Ayrıntı: [10 §20](10-v0.1-sozlesme.md).

## 🔌 v0.7 · Eklenti altyapısı

**Kapsam:** [05-eklenti-sistemi.md](05-eklenti-sistemi.md) tamamı: WASM çalışma zamanı, manifest, izinler, host API, kancalar, sağlayıcı kayıtları, arayüz katkı noktaları, eklenti ayarları, eklenti KV deposu, model indirme yöneticisi, mağaza kaynağı arayüzü, eklenti SDK'sı ve örnek eklentiler.

**Kabul kriterleri**
- [x] Örnek eklentiler çalışır: yeni kart türü, yeni çalışma modu, TTS sağlayıcı, kanca ile alan doldurma.
- [x] İzni olmayan host fonksiyonu çağrısı reddedilir (test).
- [x] Sonsuz döngüye giren / panik yapan eklenti uygulamayı çökertmez (yakıt/zaman sınırı testi).
- [x] Eklenti API'si sürümlüdür; v1 eklentisi sonraki çekirdek sürümünde çalışmaya devam eder (sözleşme testi).
- [x] İzin deposu `permits`'i eklenti aktörleri için gerçek yapar: eklentiye `ai:write` **hiç verilemez** (v0.7 güvenlik kararı; kayıtlı izin olsa da `PERMISSION_DENIED`, adres ve anahtar değişmez); `ai_keys:write` olmadan `keep_key: true` adres değişikliğinde anahtarı **korumaz**, izinliyse korur (test; tur 4 — bkz. [10 §25](10-v0.1-sozlesme.md)).

> **Doğrulama (`feat/v0.7-plugins`):** örnek eklentiler `plugins-sdk/examples` altında Rust ile yazılır, testte `wasm32-unknown-unknown`'a derlenip `.fcplugin` olarak paketlenir ve public API ile kurulur (`fc_plugins::testing`); hepsi MemStore + SQLite bellek + SQLite dosyada (`fc-app/src/modules/plugins/tests.rs`).
> - *Örnekler:* `example_card_type_plugin` (`scramble-card`: not türü şablonu `plugin.com.example.scramble.scramble`, kart üretimi/çizimi/yazarak cevap motordan, HTML temizlenir, izin geri alınınca tür kaybolur), `example_study_mode_plugin` (`reverse-drill`: menüden açılan bildirimsel arayüz, cevap `study.answer` ile kaydedilir, adım tek geri al), `example_tts_provider_plugin` (`tone-tts`: `tts.providers/voices/synthesize` geçerli WAV döner), `example_hook_fills_a_field` (`autofill`: `before:notes.create` boş alanı doldurur, ayarlarla yönlendirilir, izin geri alınınca durur, eklenti komutu çalışma zamanında kayıtlı metot olarak palette).
> - *İzinsiz çağrı:* `host_calls_without_permission_are_refused` (verilmemiş `decks:write` ile `decks.create` → `PERMISSION_DENIED`, hiçbir şey yazılmaz; aynı kontrol `Actor::Plugin` ile doğrudan çağrıda; verilmemiş kanca çalışmaz; istenmemiş izin verilemez). Çalışma zamanı düzeyinde `fc-plugins` `abi_v1_contract`.
> - *Çökmezlik:* `misbehaving_plugins_cannot_crash_the_app` (sonsuz döngü → `TIMEOUT`, yakıt az → `OUT_OF_FUEL`, panik → `TRAP`, bellek → `MEMORY_LIMIT`; çekirdek işlem kanca hatasında sürer; 3. ardışık hatada otomatik kapanır ve bildirilir; yeniden açılır), `failed_operations_roll_back` (yazdıktan sonra panikleyen eklentinin yazdığı geri sarılır, geri al adımı kalmaz); `fc-plugins` `runtime::tests` (WAT ile döngü: yakıt ve son tarih, tuzak, bellek, ABI ihlali, host çağrısında geçen süre sayılmaz).
> - *Sürümlü API:* `plugin_api_v1_contract` (dondurulmuş, elle yazılmış ABI v1 modülü `abi_v1.wat` paketlenip kurulur ve izinler üzerinden çalışır; `api_version = "2"` → `VALIDATION_PLUGIN_API_VERSION`; eklentilere açık v1 metotlarının ad/tür/izin anlık görüntüsü `api_v1.json` yalnız büyüyebilir). Ayrıntı docs/05 §12, docs/10 §24.
> - *Sanal alandan kaçış (bağımsız inceleme sonrası):* saldırgan `escape` eklentisiyle `hooks_cannot_reach_management_io_or_settings` (yönetim/`io`/ayar/modül/geri al/`batch` kancaları kurulumda reddedilir, saklanmış satırda olsa da çalışmaz; kanca hedef metodun iznini ister), `plugins_get_no_free_paths_or_app_settings` (`io:*`, `settings:write`… verilemez; kayıt defterine sızmış izin de işe yaramaz; kendi ayarı evet, `plugins.fuel_millions` hayır; dosya yalnız `fs:pick` penceresinden, yol görmeden), `plugin_storage_rolls_back_with_the_call` (hata olunca depo/ayar geri sarılır, kullanıcı verisiyle birlikte geri alınır). CLI: `fc-cli` `plugin_subcommands`.

## 🤖 v0.8 · Yapay zeka

**Kapsam:** [06 §1-2](06-ai-ve-entegrasyonlar.md): sağlayıcılar, anahtar zinciri, kullanım takibi, AI özellikleri, MCP istemcisi ve sunucusu.

**Kabul kriterleri**
- [x] Sağlayıcılar sahte (mock) sunucuya karşı test edilir; gerçek API testleri isteğe bağlı. *(`fc_ai::mock`: Claude/OpenAI/uyumlu/Ollama tel biçimi ve hatalar `fc-ai/src/tests.rs`; uçtan uca `fc-app/src/modules/ai/tests.rs::generate_cards_end_to_end_is_level_aware`; gerçek API: `FC_AI_LIVE_*`)*
- [x] API anahtarı veritabanında veya logda asla görünmez (test). *(`api_key_never_reaches_db_oplog_errors_events_or_jobs`: veritabanı dosyası + WAL, döküm, oplog, olaylar, işler, hatalar; `fc-ai`: `http_errors_map_to_kinds_and_never_carry_the_key`, `Secret` Debug'ı. Uygulamada log altyapısı yok; anahtar hiçbir `Debug`/hata metnine girmez)*
- [x] AI'a gönderilecek içerik kullanıcıya önizlenir. *(`ai.preview` → `ai.run` önizleme özetiyle; `nothing_is_sent_that_was_not_previewed`; arayüz `ai.test.ts`)*
- [x] Claude Desktop/Claude Code, MCP sunucusu üzerinden deste listeleyip kart ekleyebilir.

> **Doğrulama (MCP, iz H `feat/v0.8-mcp`):** `fc-cli/tests/mcp_stdio.rs::claude_lists_decks_and_adds_cards_over_stdio` gerçek `fc --db <dosya> mcp serve` ikilisini Claude Desktop/Code'un yaptığı gibi başlatır ve stdin/stdout'tan satır satır JSON-RPC konuşur: `initialize` (`2025-06-18`) → `notifications/initialized` → `tools/list` → `tools/call list_decks` → `list_classifications` → `add_notes` (Seviye=B1) → `search_cards seviye>=B1`; ardından ayrı `fc` süreçleri notları görür, oplog aktörü `{"kind":"mcp","id":"claude-ai"}`, `fc undo` tek adımda geri alır; `--read-only` oturum yazma araçlarını listelemez/reddeder. Aynı akış bellek içi borudan `fc-mcp/tests/stdio_protocol.rs`'te; araç anlamı (sınıflandırma, onaylı toplu atama, yazma anahtarı, izinler) `fc-app` `modules/mcp/tests.rs` × 3 depoda. İstemci iskeleti: `mcp_tools_connects_to_an_external_server` (`fc mcp tools` → kayıtlı dış sunucu = başka bir `fc mcp serve`). Kurulum: [06 §2.1](06-ai-ve-entegrasyonlar.md).

> **Kapsam (06 §1.4), iz G tur 2 (Faz 3 tur 3 ile birleşik):** PDF (sayfa aralığı) ve web sayfası kaynağı, açıklama/ipucu akışı ve çalışma ekranındaki açıkla/ipucu/AI kontrol düğmeleri eklendi — bkz. [10 §22](10-v0.1-sozlesme.md).

## 🌉 v0.9 · Dış dünya ile bağlantı

**Kapsam:** Yerel HTTP API, token ve izin onayı, gelen kutusu, tarayıcı eklentisi (Chromium tabanlı), derin bağlantılar, global hızlı ekleme, pano izleme, otomasyon kuralları.

**Kabul kriterleri**
- [x] Token'sız istek reddedilir; sadece localhost dinlenir.
- [x] Tarayıcı eklentisinden eklenen kelime gelen kutusuna düşer. *(gerçek uygulamaya karşı: `apps/browser-extension/tests/e2e/real-app.spec.ts`, CI `extension` işinde `FC_BIN` ile)*
- [x] `flashcard://` linkleri uygulama kapalıyken de açar (tek örnek). *(kabuk mantığı testli; gerçek Windows kurulumunda elle denenmedi)*
- [x] Otomasyon kuralı döngüye girmez (derinlik sınırı testi).

> **Doğrulama (otomasyon, iz J, `feat/v0.9-auto`):** `fc-app` `modules/automation/tests.rs` × 3 depo — `automation_rule_does_not_loop` (kendi eylemiyle kendini tetikleyen kural bir kez çalışır, ikincisi `skipped_cycle`; farklı kurallardan zincir `automation.max_depth`'te `skipped_depth` ile durur; birbirini sonsuza dek tetikleyen iki kural en derin sınırda bile döngü tespitiyle biter; `automation.max_runs_per_event` bir değişikliğin başlattığı çalışmaları sınırlar), `classified_note_rule_undoes_with_the_note` (`seviye:A1` → desteye taşı + etiket; kural çalışması aktör `automation:<id>` ile tetikleyen notla **aynı geri al adımı**, geri al/yinele birebir ve kural tetiklemez; cevap için `answer_and_its_rules_are_one_undo_step`), `flag_and_answer_triggers` (kırmızı bayrak → `#düzelt` + Seviye ata; "Tekrar" cevabı → askıya al + `#leech`), `preview_is_a_dry_run` (hiçbir şey yazılmaz, olay/geçmiş/kayıt yok), `module_switch_stops_rules`, `finished_import_triggers_rules`, `scheduled_rules_run_when_due`, `manual_runs_and_failures_are_logged`, `actions_are_undo_safe` + `automation_actor_reaches_only_its_actions` (her eylem public + geri alınabilir + replay edilebilir; otomasyon aktörü başka hiçbir public metoda ulaşamaz), `plugins_and_mcp_cannot_manage_rules`; `fc-core` `automation::tests` (döngü koruması), `fc-storage` `tests/automation.rs` (`0850` migration), masaüstü `automation.test.ts`. Faz 4 tur 2: gelen kutusu tetikleyicisi + "not olarak kabul et" eylemi (`inbox_rules_accept_classified_proposals`, `inbox_actions_fit_inbox_triggers_only`), mod cevapları (`mode_answers_trigger_answer_rules`), aktör × alan matrisi (`phase4_permits_tests`). Ayrıntı: docs/10 §30, §33.

> **Doğrulama (iz I, `feat/v0.9-api`):**
> - *Token'sız ret + yalnız localhost:* `crates/fc-http/tests/server.rs` gerçek TCP üzerinden — `token_less_requests_are_refused` (token'sız/yanlış/onay bekleyen token → `401 PERMISSION_UNAUTHENTICATED`, hiçbir şey yazılmaz; yalnız `ping` ve eşleştirme cevap verir), `only_loopback_is_listened_on` (bağlanan adres loopback; makinenin loopback dışı arayüzünden bağlantı kurulamaz; `Host: evil.example` → `403`; aynı port meşgulse başka porta kaçmaz) + birim `config_names_loopback_only` (`ServerConfig` loopback dışı adresi **ifade edemez**: `0.0.0.0`, LAN, `[::]`, alan adı reddedilir). İzin reddi: `permissions_are_denied_outside_scopes_and_allow_list` (verilmeyen kapsam → `403`, açık liste dışı metot → `404`, iptal → `401 revoked`, modül kapalı → `503`), servis düzeyinde `fc-app` `local_api::tests::local_api_reaches_only_its_allow_list_with_granted_scopes` (her public metot, verilebilen tüm kapsamlarla: liste dışı hepsi `PERMISSION_DENIED`, hiçbir şey yazılmaz) × 3 depo.
> - *Eklentiden kelime → gelen kutusu:* `fc-http` `extension_word_lands_in_the_inbox` (`chrome-extension://` kökeninden ön uçuş + `POST /v1/pair` → bekliyor → uygulamada onay → `POST /v1/pair/{id}` ile token **bir kez** (her istekte `X-Flashcard-Client` + JSON `Content-Type`) → desteler + `attributes.effective` → `POST /v1/inbox` (bağlam + `Seviye: B1` + `client_ref`) `201`, yeniden gönderim `200` aynı öğe → `inbox.list`'te kaynak `local_api` → kabul → `seviye:B1` not; token veritabanı dosyasında/WAL'da yok); gerçek ikililerle `fc-cli/tests/api_http.rs` (`fc api serve` + ham HTTP + ayrı `fc call` süreçleriyle onay ve `inbox.list`).
> - *`flashcard://` uygulama kapalıyken (tek örnek):* `apps/desktop/src-tauri/src/outside.rs` testleri — `cold_start_link_is_handled_and_kept_for_the_ui` (işletim sisteminin argüman olarak verdiği bağlantı açılışta işlenir, arayüz dinlemeye başlayana kadar kuyrukta bekler), `second_instance_links_reach_the_running_app` (ikinci başlatmanın argümanları çalışan örneğe gider; `add` bağlantısı gelen kutusuna düşer, hatalı bağlantı hata döner), `scheme_and_single_instance_are_configured` (`tauri.conf.json` `flashcard` şemasını kaydeder, `tauri-plugin-single-instance` ilk eklenti); saf ayrıştırıcı `fc_core::inbox::deeplink::tests`; uygulama anlamı `fc-app` `inbox::tests::deep_links_navigate_or_propose_never_write_notes` × 3 depo; arayüz yönlendirmesi `features/inbox/inbox.test.ts`. **Sınır:** işletim sistemi kaydı ve gerçek ikinci süreç (Windows kurulum dosyası) bu ortamda çalıştırılamadı.
> - Ayrıntı ve kararlar: [10 §31](10-v0.1-sozlesme.md), sözleşme [06 §3](06-ai-ve-entegrasyonlar.md).

> **Doğrulama (eklenti, iz K + Faz 4 tur 2):** `apps/browser-extension/tests/e2e/real-app.spec.ts` paketlenmemiş eklentiyi başsız Chromium'da gerçek `fc --db <geçici> api serve`'e karşı çalıştırır: sayfada "badger" seçilir → yakalanır → **Bağlan** (`POST /v1/pair`, `POST /v1/pair/{id}` yoklaması, her istekte `X-Flashcard-Client`) → onay `fc call local_api.clients.approve` (uygulamanın diyaloğunun çağırdığı komut) → deste İngilizce + Seviye B1 → **Kaydet** → `fc call inbox.list` öğeyi `pending`, kaynak `local_api` + istemci kimliği, deste, bağlam ve tek sınıflandırma değeriyle geri okur. Yerelde geçti (Faz 4 tur 2); CI `extension` işi `cargo build -p fc-cli` + `FC_BIN` ile her koşuda çalıştırır. Eklentinin geri kalanı sahte API'ye karşı (`extension.spec.ts`, vitest). Ayrıntı: [10 §32](10-v0.1-sozlesme.md), [06 §5](06-ai-ve-entegrasyonlar.md).

> **Doğrulama (tarayıcı eklentisi, iz K `feat/v0.9-ext`; kriter, `real-app.spec.ts` gerçek uygulamaya karşı geçene dek işaretsiz):** `apps/browser-extension/tests/e2e/extension.spec.ts::a word selected on a page lands in the inbox with deck, classification and context` — paketlenmemiş MV3 eklentisi başsız Chromium'da: 127.0.0.1'deki sayfada "badger" seçilir, sağ tık menüsünün çalıştırdığı yakalama koşar, açılır pencerede **Bağlan** (eşleştirme + onay), deste "İngilizce" + Seviye=B1 → **Gelen kutusuna ekle**; yerel API'ye `AddInboxItem {front, back, deck_id, attributes: {<Seviye kimliği>: <B1 kimliği>}, context: {url, title, sentence: "The badger dug a deep hole under the old oak."}, client_ref}` gelir, istekler eklentinin kökeninden. Aynı dosyada uygulama kapalıyken kuyruk → açılınca tek teslim, Türkçe arayüz. Yerel API iz I ile paralel yazıldığından bu testler docs/06 §3'e göre yazılmış sahte API'ye (`tests/mock-api`) karşı koşar; **`tests/e2e/real-app.spec.ts`** aynı akışı gerçek `fc --db <tmp> api serve`'e karşı koşar (onay `fc call local_api.clients.approve`, öğe `fc call inbox.list` ile geri okunur) — `FC_BIN` verilince çalışır, I + K birleştirmesinde koşturulmalı. Birim: `tests/unit/service.test.ts` (ekleme, kuyruk, `client_ref` ile tekrarsız yeniden gönderme, iptal edilen token), `api.test.ts`, `outbox.test.ts`, `attributes.test.ts`, `fields.test.ts`, `context.test.ts`, `i18n.test.ts`. Ayrıntı: [06 §5](06-ai-ve-entegrasyonlar.md).

## 🗣️ v0.10 · Öğrenme modları

**Kapsam:** Okuma modu + kelime renklendirme + lemmatizasyon; altyazıdan kart + cümle madenciliği; artımlı okuma; Quizlet tarzı öğren modu; test oluşturucu; güven bazlı puanlama; zamanlı mod; odak modu; satır içi kart oluşturma.

**Kabul kriterleri**
- [x] Bilinen kelime durumu FSRS verisiyle tutarlı. *(okuma izi: olgun kart = istatistikteki "olgun" — `modules::reading::tests::{mem,sqlite}::word_colours_follow_fsrs_maturity`, test istatistik modülünün kendi `card_fact`/`stats.overview` olgun sayımıyla karşılaştırır; 10 bin kelime renklendirme < 50 ms: `bench_colouring_10k` (`#[ignore]`, release; v1.0'da ölçülen **14,9 ms**) — CI'da her `cargo test` ile koşan ölçekli eşi `bench_colouring_10k_scaled` (aynı 10 bin kelime, 300 kart; bütçe debug 1 s / release 100 ms) yalnız regresyonu yakalar, 50 ms'yi kanıtlamaz; docs/10 §28, §36)*
- [x] .srt dosyasından cümle kartı, video varsa ses kesitiyle. *(okuma izi: `subtitle_file_to_sentence_card`, `subtitle_file_to_sentence_card_with_clip` — sahte ffmpeg ile; docs/10 §28)*
- [x] Öğren modu: kart önce çoktan seçmeli, sonra yazarak sorulur; her aşamada doğru bilinince "öğrenildi" olur (yanlış yazılı cevap çoktan seçmeliye döner); oturum bir sınıflandırmayı hedefleyebilir (`seviye:A2`).
- [x] Test oluşturucu: karışık soru tipli, yazdırılabilir (cevap anahtarı isteğe bağlı) kâğıt, puanlama; sınıflandırma hedefi (`sınav:TYT`); takvim (`cards`) asla değişmez; teslim geri alınabilir.
- [x] Güven bazlı puanlama: 1–5 seviye ayarlardaki tabloyla FSRS puanına eşlenir; sonuç, eşlenen puanla dört düğmeli cevabın birebir aynısıdır.
- [x] Zamanlı mod backend saatiyle uygulanır: geç cevap reddedilir, süre dolunca "Tekrar" ya da geç; ikisi de birebir geri alınır. Sayaç kart ekrana gelince başlar ve kart bazındadır: görülmemiş kart asla süresi dolmuş sayılmaz, sayaç yeniden başlatılamaz.
- [x] Her mod `review_log`'a kendi `mode`'uyla yazar; istatistik pratik modları saymaz; günlük limitler takvimi değiştiren her modu sayar.
- [x] Satır içi kart oluşturma (`soru :: cevap`) çalışma sırasında oturumu bozmadan, tek geri al adımıyla.
- [x] Odak modu ve diğer tüm modlar ayarlardan kapatılabilen modüllerdir.

> **Doğrulama (iz M, `feat/v0.10-modes`; okuma/altyazı/artımlı okuma iz L'de):** `fc-app/src/modules/modes/tests.rs`, MemStore + SQLite bellek + SQLite dosya: (1) `learn_mode_learns_cards_at_every_stage` (yalnız A2 kartları; aşama günlüğü seçmeli-yanlış → seçmeli → yazılı-yanlış → seçmeli → yazılı = öğrenildi; diğerleri seçmeli → yazılı; her cevap `learn` ve kartı zamanlar; `study.answer` reddedilir; geri al/yinele birebir). (2) `test_generator_mixes_types_scores_and_never_schedules` (`sınav:TYT` 6 soru, 2+2+2 tip, anahtar yalnız istenince/teslimde; 3 doğru + 1 az kaldı + 1 yanlış + 1 boş = 3,5 puan %58; 6 `test` satırı; `cards` aynı; istatistik 0; ikinci teslim `CONFLICT_TEST_SUBMITTED`; geri al/yinele). (3) `confidence_levels_map_onto_fsrs_ratings` (özel tablo 2 → Zor; deste ayarı ölçeği seçer; her seviyenin kartı, eşlenen puanla dört düğmede cevaplanan ikiz kartla aynı zamanlama durumunda; düğme aralıkları = önizleme). (4) `timed_mode_is_enforced_on_the_backend_clock` (sayaç `modes.timed.start` ile başlar; başlamamış kart ne cevaplanır ne süresi dolar; erken bitirme/geç cevap reddi, süresi dolmuş sayaç yeniden başlatılamaz, Tekrar ve Geç, birebir geri al sayacı sıfırlar, doğrulama hataları); `timed_countdown_belongs_to_the_card_on_screen` (askıya alma/gömme sonrası gelen kart, geri alınan kart ve "Tekrar"la dönen öğrenme kartı eski sayacı devralmaz); `learn_choice_answers_do_not_rate_reviewed_cards` (tekrar kartında seçmeli = `trial_learn`, takvim aynı, istatistik saymaz; yazılı doğru İyi, yanlış Tekrar). İçe aktarma: `io/tests.rs::imported_history_keeps_practice_modes`. (5) `modes_log_their_own_review_mode_and_respect_daily_limits` (preset günde 4 yeni: öğren 12 kartı alır, öğren/güven/zamanlı cevaplar 4→3→2→1 düşürür; pratik öğren/zamanlı/test düşürmez ve `cards`'ı değiştirmez; günlükte 8 ayrı mod; istatistik yalnız 3'ü sayar, önbellek = yeniden hesaplama). (6) `inline_cards_during_study_are_one_undo_step`. (7) `modes_are_switchable_modules`. Saf: `fc-study` `modes::{learn,test,confidence,timed}`, `fc-core` `inline_cards`. Arayüz: `features/modes/modes.test.ts` (öğren ekranı yalnız klavyeyle, test kâğıdı yazdır/puan, sayaç dolunca `modes.timed.expire`, 1–5 tuşlarıyla güven, F odak, N satır içi kart). Ayrıntı: [10 §27](10-v0.1-sozlesme.md).

*Okuma izi (`feat/v0.10-reading`) kapsamı: okuma modu + kelime renklendirme + lemmatizasyon, altyazıdan kart + cümle madenciliği, artımlı okuma. Öğren modu, test oluşturucu, güven bazlı puanlama, zamanlı/odak modu ve satır içi kart oluşturma diğer v0.10 izinde.*

## 🚀 v1.0 · Cila ve Windows sürümü

**Kapsam:** İlk açılış rehberi; hazır başlangıç desteleri; TR/EN çevirilerin tamamı; temalar; uygulama kilidi; profiller; hatırlatıcılar ve tepsi; otomatik güncelleme (imzalı); çökme kurtarma; log dışa aktarma; performans çalışması; Windows kurulum dosyası (MSI/NSIS); kullanıcı rehberi.

**Kabul kriterleri**
- [x] Performans hedefleri ([02 §34](02-ozellik-katalogu.md)) benchmark ile sağlanır. *(iz P: `fc-app/src/perf_tests.rs::perf_suite_100k` — `#[ignore]`, release, 100k not / 200k kart / 2M tekrar, ayrı süreçte ölçüm: %10 öğrenme kartıyla (kuyrukta ~19 300 kart): açılış 626 ms < 2 sn, sıradaki kart p95 7,2 ms < 20 ms (medyan 4,1 ms), oturum başlatma 156 ms < 500 ms, en yavaş arama 70 ms < 100 ms, tarayıcı sayfası 81 ms, 10 bin içe aktarma 1,2 sn, istatistik 643 ms / 1,06 sn / 2,6 sn, tepe bellek 301 MB < 400 MB; CI'da her `cargo test` ile `perf_suite_scaled` (1/10, debug, geniş bütçe) yalnız regresyonu yakalar; ayrıntı ve ölçüm koşulları docs/10 §36)*
- [ ] Temiz bir Windows'ta kurulum → ilk açılış → çalışma → güncelleme akışı sorunsuz. **Elle deneme betiği [07 §8](07-gelistirme-sureci.md)'de, gerçek bir Windows koşusu bekliyor.** **Otomatikleşen kısmı hazır (iz O), elle deneme bekliyor:** CI `windows-smoke` NSIS'i sessiz, kullanıcı başına kurar ve kurulu uygulamayı `--smoke` ile çalıştırır (koleksiyon oluştur/migrate → çalış + geri al → bütünlük → taslak → gizli bilgisiz log dışa aktarma); güncelleme akışı sahte güncelleyiciyle testli, gerçek imzalı A → B güncellemesi ve `flashcard://` soğuk başlatma [07 §8.5](07-gelistirme-sureci.md) betiğiyle **proje sahibince** denenecek. **İz N kısmı (ilk açılış → çalışma) ✅:** ilk açılış rehberi boş koleksiyonda ne çalışıldığını sorar ve uygun sınıflandırma şablonuyla (İngilizce → CEFR, YKS → TYT/AYT/YDT, Japonca → JLPT, Çince → HSK, okul → sınıf) deste + ön ayar + başlangıç destelerini **tek geri al adımında** kurar (`onboarding/tests.rs::english_setup_uses_cefr_and_is_one_undo_step`, `yks_setup_uses_the_exam_template`, `features/onboarding/onboarding.test.ts`); veri olan (yükseltilmiş) koleksiyonda rehber çıkmaz. Kurulum ve güncelleme iz O'da; temiz Windows'ta elle deneme yapılmadı.
  - [x] Başlangıç desteleri: küçük, bu uygulama için yazılmış (lisans temiz), TR/EN, notlar sınıflandırma değeriyle (`fc-core ux::starter` testleri, `starter_decks_install_in_both_languages`).
  - [x] Profiller = ayrı koleksiyonlar (ayrı veritabanı + medya klasörü; varsayılan profil eski yerinde kalır), geçiş uygulamayı yeni koleksiyonla yeniden başlatır (`profiles/tests.rs::profiles_are_separate_collections`, `switching_retires_the_session_…`).
  - [x] Uygulama kilidi: PIN/parola yalnız Argon2id özeti olarak (OS anahtar zinciri; kalıcı değilse profil listesi), hareketsizlikte otomatik kilit, yanlış denemede artan bekleme, kilitliyken IPC yalnız kilit ekranına açık (`only_an_argon2id_hash_is_kept`, `idle_app_locks_itself`, `locked_app_lets_only_the_lock_screen_through_and_backs_off`, masaüstü `ipc_is_gated_while_locked`). Veritabanı şifrelemesi değil (docs/08 K4).
  - [x] Kilitliyken / profil değişirken yerel HTTP API ve otomasyon zamanlayıcısı durur ya da reddeder (423 `PERMISSION_APP_LOCKED`), profil geçişinde sunucu yeni koleksiyona yeniden bağlanır (`background_callers_are_refused_while_locked`, fc-http `a_locked_app_answers_nothing_but_ping`, masaüstü `api_server_stops_while_locked_and_is_rebound_after_a_profile_switch`).
  - [x] Güvenlik incelemesi düzeltmeleri: kilitliyken `flashcard://` bağlantıları kuyrukta bekler, kilit açılınca işlenir, `take_deep_links` kapalı (`links_wait_while_locked_and_run_after_unlock`); yanlış deneme sayacı yeniden başlatmada korunur (`wrong_attempt_back_off_survives_a_restart`), kontrol + kayıt tek kilitte (`parallel_attempts_share_no_free_slot`); özeti kayıp kilit kapalı kalır, açık kurtarma akışı (`a_lock_without_its_hash_fails_closed_until_recovered`, `profiles.test.ts`); AI anahtarları profil başına (`ai_keys_are_namespaced_per_profile`, masaüstü `ai_keys_are_scoped_to_the_open_profile`); medya protokolü ve hızlı ekleme ön doldurması kilitliyken kapalı (`media_protocol_is_gated_while_locked`, `quick_add_prefill_is_empty_while_locked`). Tehdit modeli: docs/02 §29.
  - [x] Hatırlatıcılar: seçilen saat/günlerde günde bir OS bildirimi, o gün çalışıldıysa ya da kart yoksa yok; sistem tepsisi (Aç/Kilitle/Çık), "pencere kapanınca tepside kal" (`reminders_tests.rs`, `fc-core ux::reminder`). Windows'ta gerçek bildirim elle denenmedi.
- [x] Tüm arayüz metinleri çeviri anahtarlarından gelir (sabit metin yok, lint ile). *(iz P: masaüstü `tests/no-hardcoded-text.test.ts` — her Svelte bileşeninin AST'si: işaretleme metni, görünen öznitelik/prop'lar, betik cümleleri; kullanılan her anahtar TR+EN; tarayıcı eklentisi `tests/unit/i18n.test.ts` — sayfalar, manifest `__MSG_…__`, betiklerin metin yazdığı yerler; backend `fc-app/src/i18n_lint_tests.rs` — modüllerin ilan ettiği her `*_key` ve kodun gönderdiği her ileti anahtarı TR+EN; her `ErrorCode` metni ve CLI'nin çevrilmiş hataları `fc-cli/src/i18n.rs`; docs/10 §36)*
- [x] İçe aktarmada ayırıcı otomatik algılama **yeni bir tutulmuş sette** ≥ %95 doğru: 4. tutulmuş sette **%96,2 (100/104)**, `text::heldout4_tests` (3. sette ayar öncesi %90,8; bkz. v0.5).
- [x] Gerçek Anki doğrulaması: Anki'nin ürettiği paketler içe aktarılır (`anki::real_tests`, `io::real_anki_tests`), bizim paketlerimiz gerçek Anki'de açılır (`our_exports_open_in_real_anki`).
- [x] Kullanıcı rehberi TR/EN: [docs/guide/](guide/README.md).

> **Doğrulama (iz O, `feat/v1.0-release`: kurulum, imzalı güncelleme, çökme kurtarma, loglar):** `fc-app/src/modules/maintenance/tests.rs`, MemStore + SQLite bellek + SQLite dosya: (1) `after_an_unclean_shutdown_the_app_offers_session_drafts_crash_and_integrity` (bitmeyen çalıştırma + çökme raporu → `maintenance.recovery.get` sebep `crash`, açık oturum devam ettirilebilir, taslak geri gelir, açılışta hızlı bütünlük denetimi, rapor maskeli; kapatma geri al adımı değil). (2) `a_clean_previous_run_offers_nothing`. (3) `drafts_are_scratch_state_validated_and_bounded`. (4) `exported_logs_never_contain_keys_or_tokens` (anahtar zincirindeki şekilsiz sağlayıcı anahtarı, `sk-ant`, `ghp_`, JWT, uzun hex, parola, Bearer — paketteki hiçbir dosyada yok). (5) `updates_are_checked_downloaded_verified_and_applied_only_on_request`, (6) `a_badly_signed_update_is_refused` (`PROVIDER_UPDATE_SIGNATURE`, hiçbir şey kurulmaz). (7) `no_automated_actor_reaches_maintenance_or_updates`. Ayrıca `run.rs` (temiz/kirli çalıştırma işareti), `logging.rs` (dönen, maskeleyen log), `crash.rs`, `smoke.rs`, `fc-core::maintenance::redact`, depolama `tests/maintenance.rs` (migration 1050), masaüstü `src-tauri/src/maintenance.rs` (`--smoke`, çalıştırma işareti, imza hatası eşlemesi), `features/maintenance/maintenance.test.ts`. Kurulum: NSIS kullanıcı başına + MSI, WebView2 önyükleyici; yayın: `release.yml` (etiket `v*` → taslak Release + `latest.json`). Ayrıntı: [07 §8](07-gelistirme-sureci.md).

## 🎨 v1.2 · Arayüz yeniden tasarımı ve Subtitle bağlantısı

*v1.1 ilk arayüz tasarımıydı (Quizlet esinli, ferah; docs/10 §38). Proje sahibi kullanırken rahat edemedi, özellikle kart eklemeyi ağır buldu; v1.2 arayüzü akışlardan başlayarak yeniden kurar.*

**Kapsam:** Koyu öncelikli, sık, klavye öncelikli tasarım sistemi (tek kehribar vurgu, JetBrains Mono sayılar); her zaman görünen üst çubuk (arama/komut `Ctrl+K`, "Kart ekle" `N`); Hızlı Ekle penceresi; tablo benzeri toplu ekleme ızgarası; Ana sayfa → oturum (oturumda kabuk kaybolur); Kütüphane ağaç-tablosu → deste sayfası; İstatistik; tüm diğer ekranlar ve Ayarlar aynı sistemde. Ayrıca Subtitle tarayıcı eklentisinin sözleşmesini karşılayan `subtitle` modülü ve `/subtitle/v1` HTTP API'si. Davranış ve Rust API'si değişmez; hiçbir özellik kaldırılmaz.

**Kabul kriterleri** (tam liste: [11 §6](11-tasarim.md))
- [x] Hiçbir özellik, ekran, ayar, kısayol ya da modül anahtarı kaldırılmadı; taşınan her şey paletten bulunur (`navigation.test.ts`, `module-switches.test.ts`).
- [x] Kart eklemek tek tuş: `N` → alanlar → `Ctrl+Enter`, pencere açık kalır, Geri al (`quickadd.test.ts`).
- [x] Çalışma oturumu pencerenin tamamını alır; 2'li ve 4'lü cevap düzeni korunur.
- [x] Lint tarzı testler (ham kontrol, kontrast, sabit metin, sayı biçimi, ilk yükleme, paket bütçesi) yeşil; ilk JS 175,8 kB ≤ 320 kB.
- [x] Subtitle: eklentinin sözleşmesi değiştirilmeden karşılanır, migration `1200` testli, yakalama tek geri al adımı; sözleşme denetimi eklentinin kendi istemcisiyle (`tools/subtitle-contract/check.mjs`). Ayrıntı: [06 §5.1](06-ai-ve-entegrasyonlar.md).
- [ ] Gerçek Tauri penceresinde (Windows) elle gezinti.
- [ ] Subtitle eklentisi gerçek bir tarayıcıda denenecek.

## 🔮 v1.0 sonrası

macOS · Linux · senkronizasyon · mobil · erişilebilirlik · oyunlaştırma · eklenti mağazası kaynağının işletilmesi · isteğe bağlı veritabanı şifreleme.

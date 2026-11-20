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
- [ ] **Kısmen karşılandı** — `Arda, porsuk` ve `Arda: porsuk` ve özel ayırıcılı dosyalar doğru ayrışır ✅; otomatik algılama test kümesinde ≥ %95 doğru: **ayar setinde karşılandı, tutulmuş sette değil.** Ölçümler: ayar seti ≥ %95; ayardan sonra yazılan 2. tutulmuş sette ilk ölçüm %90,0 (63/70), Anki `#separator:` başlık desteğiyle **%92,9 (65/70)** (o iki örnek artık saf tutulmuş değil). Kalan kaçanlar: `~` ayırıcı, çift TAB / boşlukla hizalı sütunlar, her satırında virgül olan cümle listesi, tireli kelime listesi; önizleme her zaman elle düzeltmeye izin verir. **→ v1.0'da iyileştirilecek** (yeni bir tutulmuş setle yeniden ölçülerek). Ayrıntı docs/10 §14.
- [ ] **Kısmen karşılandı** — Örnek Anki desteleri (cloze, resimli, sesli) kayıpsız içe aktarılır ✅ (yapı testli); dışa aktarılan .apkg'nin Anki'de açıldığı **doğrulanmadı**: paket yalnız yapı testinden geçer (şema 11 tabloları, JSON anahtarları, referans bütünlüğü), gerçek Anki'de elle açılmadı ve testlerde gerçek Anki'nin ürettiği bir dosya yok (fikstürler sentetik). **→ v1.0 öncesi** gerçek Anki ile elle deneme + Anki'nin ürettiği bir örnek paketle test. Faz-2 tur 2'den beri Anki'nin hazır not türleri v0.3 yerleşiklerine eşlenir ve kartlar v0.3 motorlarıyla çizilir — docs/10 §15.
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
- [ ] Örnek eklentiler çalışır: yeni kart türü, yeni çalışma modu, TTS sağlayıcı, kanca ile alan doldurma.
- [ ] İzni olmayan host fonksiyonu çağrısı reddedilir (test).
- [ ] Sonsuz döngüye giren / panik yapan eklenti uygulamayı çökertmez (yakıt/zaman sınırı testi).
- [ ] Eklenti API'si sürümlüdür; v1 eklentisi sonraki çekirdek sürümünde çalışmaya devam eder (sözleşme testi).
- [ ] İzin deposu `permits`'i eklenti aktörleri için gerçek yapar: `ai:write` olup `ai_keys:write` olmayan eklenti sağlayıcı adresini `keep_key: true` ile değiştirirse anahtar **korunmaz**, izinliyse korunur (test; v0.7 birleşmesinde eklenir — bkz. [10 §23](10-v0.1-sozlesme.md)).

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
- [ ] Token'sız istek reddedilir; sadece localhost dinlenir.
- [ ] Tarayıcı eklentisinden eklenen kelime gelen kutusuna düşer.
- [ ] `flashcard://` linkleri uygulama kapalıyken de açar (tek örnek).
- [ ] Otomasyon kuralı döngüye girmez (derinlik sınırı testi).

## 🗣️ v0.10 · Öğrenme modları

**Kapsam:** Okuma modu + kelime renklendirme + lemmatizasyon; altyazıdan kart + cümle madenciliği; artımlı okuma; Quizlet tarzı öğren modu; test oluşturucu; güven bazlı puanlama; zamanlı mod; odak modu; satır içi kart oluşturma.

**Kabul kriterleri**
- [ ] Bilinen kelime durumu FSRS verisiyle tutarlı.
- [ ] .srt dosyasından cümle kartı, video varsa ses kesitiyle.
- [x] Öğren modu: kart önce çoktan seçmeli, sonra yazarak sorulur; her aşamada doğru bilinince "öğrenildi" olur (yanlış yazılı cevap çoktan seçmeliye döner); oturum bir sınıflandırmayı hedefleyebilir (`seviye:A2`).
- [x] Test oluşturucu: karışık soru tipli, yazdırılabilir (cevap anahtarı isteğe bağlı) kâğıt, puanlama; sınıflandırma hedefi (`sınav:TYT`); takvim (`cards`) asla değişmez; teslim geri alınabilir.
- [x] Güven bazlı puanlama: 1–5 seviye ayarlardaki tabloyla FSRS puanına eşlenir; sonuç, eşlenen puanla dört düğmeli cevabın birebir aynısıdır.
- [x] Zamanlı mod backend saatiyle uygulanır: geç cevap reddedilir, süre dolunca "Tekrar" ya da geç; ikisi de birebir geri alınır. Sayaç kart ekrana gelince başlar ve kart bazındadır: görülmemiş kart asla süresi dolmuş sayılmaz, sayaç yeniden başlatılamaz.
- [x] Her mod `review_log`'a kendi `mode`'uyla yazar; istatistik pratik modları saymaz; günlük limitler takvimi değiştiren her modu sayar.
- [x] Satır içi kart oluşturma (`soru :: cevap`) çalışma sırasında oturumu bozmadan, tek geri al adımıyla.
- [x] Odak modu ve diğer tüm modlar ayarlardan kapatılabilen modüllerdir.

> **Doğrulama (iz M, `feat/v0.10-modes`; okuma/altyazı/artımlı okuma iz L'de):** `fc-app/src/modules/modes/tests.rs`, MemStore + SQLite bellek + SQLite dosya: (1) `learn_mode_learns_cards_at_every_stage` (yalnız A2 kartları; aşama günlüğü seçmeli-yanlış → seçmeli → yazılı-yanlış → seçmeli → yazılı = öğrenildi; diğerleri seçmeli → yazılı; her cevap `learn` ve kartı zamanlar; `study.answer` reddedilir; geri al/yinele birebir). (2) `test_generator_mixes_types_scores_and_never_schedules` (`sınav:TYT` 6 soru, 2+2+2 tip, anahtar yalnız istenince/teslimde; 3 doğru + 1 az kaldı + 1 yanlış + 1 boş = 3,5 puan %58; 6 `test` satırı; `cards` aynı; istatistik 0; ikinci teslim `CONFLICT_TEST_SUBMITTED`; geri al/yinele). (3) `confidence_levels_map_onto_fsrs_ratings` (özel tablo 2 → Zor; deste ayarı ölçeği seçer; her seviyenin kartı, eşlenen puanla dört düğmede cevaplanan ikiz kartla aynı zamanlama durumunda; düğme aralıkları = önizleme). (4) `timed_mode_is_enforced_on_the_backend_clock` (sayaç `modes.timed.start` ile başlar; başlamamış kart ne cevaplanır ne süresi dolar; erken bitirme/geç cevap reddi, süresi dolmuş sayaç yeniden başlatılamaz, Tekrar ve Geç, birebir geri al sayacı sıfırlar, doğrulama hataları); `timed_countdown_belongs_to_the_card_on_screen` (askıya alma/gömme sonrası gelen kart, geri alınan kart ve "Tekrar"la dönen öğrenme kartı eski sayacı devralmaz); `learn_choice_answers_do_not_rate_reviewed_cards` (tekrar kartında seçmeli = `trial_learn`, takvim aynı, istatistik saymaz; yazılı doğru İyi, yanlış Tekrar). İçe aktarma: `io/tests.rs::imported_history_keeps_practice_modes`. (5) `modes_log_their_own_review_mode_and_respect_daily_limits` (preset günde 4 yeni: öğren 12 kartı alır, öğren/güven/zamanlı cevaplar 4→3→2→1 düşürür; pratik öğren/zamanlı/test düşürmez ve `cards`'ı değiştirmez; günlükte 8 ayrı mod; istatistik yalnız 3'ü sayar, önbellek = yeniden hesaplama). (6) `inline_cards_during_study_are_one_undo_step`. (7) `modes_are_switchable_modules`. Saf: `fc-study` `modes::{learn,test,confidence,timed}`, `fc-core` `inline_cards`. Arayüz: `features/modes/modes.test.ts` (öğren ekranı yalnız klavyeyle, test kâğıdı yazdır/puan, sayaç dolunca `modes.timed.expire`, 1–5 tuşlarıyla güven, F odak, N satır içi kart). Ayrıntı: [10 §24](10-v0.1-sozlesme.md).

## 🚀 v1.0 · Cila ve Windows sürümü

**Kapsam:** İlk açılış rehberi; hazır başlangıç desteleri; TR/EN çevirilerin tamamı; temalar; uygulama kilidi; profiller; hatırlatıcılar ve tepsi; otomatik güncelleme (imzalı); çökme kurtarma; log dışa aktarma; performans çalışması; Windows kurulum dosyası (MSI/NSIS); kullanıcı rehberi.

**Kabul kriterleri**
- [ ] Performans hedefleri ([02 §34](02-ozellik-katalogu.md)) benchmark ile sağlanır.
- [ ] Temiz bir Windows'ta kurulum → ilk açılış → çalışma → güncelleme akışı sorunsuz.
- [ ] Tüm arayüz metinleri çeviri anahtarlarından gelir (sabit metin yok, lint ile).
- [ ] İçe aktarmada ayırıcı otomatik algılama **yeni bir tutulmuş sette** ≥ %95 doğru (v0.5'ten devreden: şu an %92,9, 65/70; bkz. v0.5).

## 🔮 v1.0 sonrası

macOS · Linux · senkronizasyon · mobil · erişilebilirlik · oyunlaştırma · eklenti mağazası kaynağının işletilmesi · isteğe bağlı veritabanı şifreleme.

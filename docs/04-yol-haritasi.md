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

## 🧩 v0.3 · Kart türleri ve içerik

**Kapsam:** Not türü şeması ve editörü; şablon dili; kart türü motoru; klasik, ters, isteğe bağlı ters, cloze, çoktan seçmeli, yazarak, resim üzerinde gizleme, resimli quiz, sesli kart; hazır not türleri (kelime kartı dahil); Markdown editör, KaTeX, kod blokları; medya deposu, sürükle-bırak, Ctrl+V; kart önizleme; `[[bağlantılar]]` ve geri bağlantılar; kart notu; bayraklar (özel adlarıyla).

**Kabul kriterleri**
- [x] Yeni bir kart türü sadece `CardTypeEngine` uygulanarak eklenebilir (testte örnek tür ile kanıt). → `fc-app/src/cards_tests.rs::a_new_card_type_is_just_an_engine` (test içi `ReverseWords` motoru, 3 depoda)
- [x] Not türüne alan eklemek/çıkarmak mevcut kartları ve geçmişi bozmaz. → `notetype_field_add_remove_keeps_cards_and_history` (kart kimlikleri/zamanlama aynı, geri al değerleri birebir getirir, eski not düzenlemesi hâlâ geri alınır)
- [x] Aynı medya iki kez eklenince tek dosya tutulur. → `same_media_twice_is_one_file_on_disk`, `same_media_twice_is_one_entry`, `fc-storage` `same_media_twice_is_one_file_and_one_row`
- [x] HTML temizleme testleri (XSS vektörleri) geçer. → `fc-core` `content::html::tests::xss_vectors_are_neutralized` (36 vektör) + uçtan uca `rendering_is_sanitized` (alan, kullanıcı şablonu, seçenek, CSS)

## 🛠️ v0.4 · Kart yönetimi

**Kapsam:** Sorgu dili (ayrıştırıcı + SQL derleyici + FTS5); arama; kayıtlı filtreler; özel çalışma oturumları; kart tarayıcısı; toplu işlemler; bul-değiştir; çöp kutusu; tekrar tespiti; kart bilgi ekranı; veritabanı kontrolü; deste ayar şablonları; komut paleti; kısayol düzenleyici.

**Kabul kriterleri**
- [ ] Sorgu dili için ayrıştırıcı özellik tabanlı testleri; hatalı sorguda konumlu hata.
- [ ] 100k notta arama < 100 ms (benchmark).
- [ ] 1000 kartlık toplu işlem tek geri al ile tamamen geri döner.
- [ ] Komut paleti kayıt defterindeki tüm komutları listeler.

## 📥 v0.5 · İçe / dışa aktarma

**Kapsam:** Esnek ayırıcılı metin/CSV içe aktarma (önizleme, otomatik algılama, alan eşleme, tırnak işleme); JSON; Quizlet metni; Anki .apkg/.colpkg içe ve dışa (eski ve yeni şema sürümleri, medya, geçmiş); deste paketi (`.fcpack`); tam yedek (`.fcbackup`) ve geri yükleme; yazdırma/PDF.

**Kabul kriterleri**
- [ ] **Kısmen karşılandı** — `Arda, porsuk` ve `Arda: porsuk` ve özel ayırıcılı dosyalar doğru ayrışır ✅; otomatik algılama test kümesinde ≥ %95 doğru: **ayar setinde karşılandı, tutulmuş sette değil.** Ölçümler: ayar seti ≥ %95; ayardan sonra yazılan 2. tutulmuş sette ilk ölçüm %90,0 (63/70), Anki `#separator:` başlık desteğiyle **%92,9 (65/70)** (o iki örnek artık saf tutulmuş değil). Kalan kaçanlar: `~` ayırıcı, çift TAB / boşlukla hizalı sütunlar, her satırında virgül olan cümle listesi, tireli kelime listesi; önizleme her zaman elle düzeltmeye izin verir. **→ v1.0'da iyileştirilecek** (yeni bir tutulmuş setle yeniden ölçülerek). Ayrıntı docs/10 §14.
- [x] Örnek Anki desteleri (cloze, resimli, sesli) kayıpsız içe aktarılır; dışa aktarılan .apkg Anki'de açılır. *(Anki'nin kendisiyle elle açma denemesi henüz yapılmadı; yapı testle doğrulanıyor, aşağıya bkz. Faz-2 tur 2'den beri Anki'nin hazır not türleri v0.3 yerleşiklerine eşlenir ve kartlar v0.3 motorlarıyla çizilir — docs/10 §15.)*
- [x] Yedek → sil → geri yükle döngüsü veriyi birebir korur.
- [x] İçe aktarma iptal edilebilir ve tek geri al ile geri döner.

> **Doğrulama (`feat/v0.5-io`):**
> - Ayrıştırma/algılama: `fc-interop/src/text/tests.rs` (`acceptance_examples_parse`, 70 örneklik etiketli kümede `detection_is_at_least_95_percent_accurate`); uçtan uca `fc-app/src/modules/io/tests.rs::text_import_parses_acceptance_examples_with_auto_detection`.
> - Anki: fikstürler testte sentetik üretilir (`fc_interop::anki::fixtures`: cloze + resim + ses notu, tüm kart durumları, filtreli deste, geçmiş) ve üç paket düzeninde (eski `collection.anki2`, `collection.anki21`, yeni zstd/protobuf `collection.anki21b`) yazılır. `all_layouts_read_the_same_collection`, `converts_cloze_image_sound_scheduling_and_history`, `export_then_import_roundtrips`, `exported_package_is_a_valid_anki_collection` (şema 11 tablo/sütunları, `ver = 11`, model/deste/ayar JSON anahtarları, referans bütünlüğü, checksum, medya listesi); uygulama düzeyinde `anki_packages_import_losslessly_in_every_layout`, `exported_apkg_reimports_with_the_same_content`.
> - Yedek: `backup_delete_restore_preserves_everything` (Anki içe aktarma + sınıflandırma + oplog içeren koleksiyon → `.fcbackup` → deste sil → geri yükle → `jobs` dışındaki **tüm tablolar** ve medya baytları birebir aynı; otomatik güvenlik yedeği silinmiş durumu geri getirir). Depolama düzeyinde `dump_then_load_is_bit_exact`.
> - İptal + tek geri al: `cancelling_in_the_middle_of_writing_rolls_everything_back` (64 satır yazıldıktan sonra iptal → işlem geri sarılır, oplog değişmez), `jobs_cancel_stops_queued_and_running_imports`, `import_is_one_undo_step_and_redoable` (yeni desteler + not güncelleme + yeni sınıflandırma seçenekleri tek `oplog.undo` ile geri, `oplog.redo` ile aynen ileri).
> - Sınıflandırmalar: `text_columns_map_to_classifications_and_create_missing_options`, `fcpack_preserves_classifications_exactly`.
> - **Faz-2 tur 2 (v0.5 × v0.3 × v0.2), uçtan uca:** `fc-app/src/phase2_io_tests.rs::anki_import_study_backup_wipe_restore_is_identical` — CSV sütunu → Seviye (eksik seçenek oluşturulur) → cloze + resim + ses + mor bayrak + bugünden bir tekrar içeren `.apkg` → yerleşik not türleri, aynı resmi başka adla taşıyan ikinci paket yeni medya eklemez (diskte 2 dosya) → kartlar motorla çizilir (`media:` resim/ses, cloze) → içe aktarılan geçmiş `imported`, günlük tekrar limitini tüketmez → çalışma `normal` tekrar yazar → yedek → koleksiyon klasörü tamamen silinir → boş koleksiyona geri yükleme → `jobs` dışındaki tüm tablolar, medya dosyaları ve sınıflandırmalar birebir.

## 📊 v0.6 · İstatistik ve akıllı planlama

**Kapsam:** [02 §17](02-ozellik-katalogu.md) istatistiklerinin tamamı; FSRS kişisel optimizasyonu; hedef hatırlama oranı (+ iş yükü tahmini); yük dengeleme; kolay günler; sınav tarihi modu; hedefler; deste karnesi.

**Kabul kriterleri**
- [ ] İstatistikler `review_log`'dan yeniden üretilebilir ve önbellekle aynı sonucu verir.
- [ ] Optimizasyon arka plan işi olarak çalışır, iptal edilebilir.
- [ ] Yük dengeleme simülasyonda günlük yük varyansını düşürür (test).

## 🔌 v0.7 · Eklenti altyapısı

**Kapsam:** [05-eklenti-sistemi.md](05-eklenti-sistemi.md) tamamı: WASM çalışma zamanı, manifest, izinler, host API, kancalar, sağlayıcı kayıtları, arayüz katkı noktaları, eklenti ayarları, eklenti KV deposu, model indirme yöneticisi, mağaza kaynağı arayüzü, eklenti SDK'sı ve örnek eklentiler.

**Kabul kriterleri**
- [ ] Örnek eklentiler çalışır: yeni kart türü, yeni çalışma modu, TTS sağlayıcı, kanca ile alan doldurma.
- [ ] İzni olmayan host fonksiyonu çağrısı reddedilir (test).
- [ ] Sonsuz döngüye giren / panik yapan eklenti uygulamayı çökertmez (yakıt/zaman sınırı testi).
- [ ] Eklenti API'si sürümlüdür; v1 eklentisi sonraki çekirdek sürümünde çalışmaya devam eder (sözleşme testi).

## 🤖 v0.8 · Yapay zeka

**Kapsam:** [06 §1-2](06-ai-ve-entegrasyonlar.md): sağlayıcılar, anahtar zinciri, kullanım takibi, AI özellikleri, MCP istemcisi ve sunucusu.

**Kabul kriterleri**
- [ ] Sağlayıcılar sahte (mock) sunucuya karşı test edilir; gerçek API testleri isteğe bağlı.
- [ ] API anahtarı veritabanında veya logda asla görünmez (test).
- [ ] AI'a gönderilecek içerik kullanıcıya önizlenir.
- [ ] Claude Desktop/Claude Code, MCP sunucusu üzerinden deste listeleyip kart ekleyebilir.

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

## 🚀 v1.0 · Cila ve Windows sürümü

**Kapsam:** İlk açılış rehberi; hazır başlangıç desteleri; TR/EN çevirilerin tamamı; temalar; uygulama kilidi; profiller; hatırlatıcılar ve tepsi; otomatik güncelleme (imzalı); çökme kurtarma; log dışa aktarma; performans çalışması; Windows kurulum dosyası (MSI/NSIS); kullanıcı rehberi.

**Kabul kriterleri**
- [ ] Performans hedefleri ([02 §34](02-ozellik-katalogu.md)) benchmark ile sağlanır.
- [ ] Temiz bir Windows'ta kurulum → ilk açılış → çalışma → güncelleme akışı sorunsuz.
- [ ] Tüm arayüz metinleri çeviri anahtarlarından gelir (sabit metin yok, lint ile).
- [ ] İçe aktarmada ayırıcı otomatik algılama **yeni bir tutulmuş sette** ≥ %95 doğru (v0.5'ten devreden: şu an %92,9, 65/70; bkz. v0.5).

## 🔮 v1.0 sonrası

macOS · Linux · senkronizasyon · mobil · erişilebilirlik · oyunlaştırma · eklenti mağazası kaynağının işletilmesi · isteğe bağlı veritabanı şifreleme.

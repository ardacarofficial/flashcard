# 12 — Performans: ölçülmüş hız (v1.4)

Bu belge uygulamanın büyük bir koleksiyonda **ölçülmüş** hızını kaydeder. Hedefler docs/02 §34'te, takımın
tasarımı docs/03 §17'de, v1.0 ölçümleri docs/10 §36–37'de.

## Nasıl ölçülür

```bash
# Tam ölçek (release, ~8 dk + ilk derleme): 100k not / 200k kart / 2M tekrar + film katmanı
FC_PERF_DIR=<gerçek diskte bir klasör> cargo test -p fc-app --release --lib perf_suite_100k -- --ignored --nocapture
# CI ölçeği (her `cargo test`, debug): 10k not / 20k kart / 200k tekrar + 1k karşılaşma
cargo test -p fc-app --lib perf_suite_scaled -- --nocapture
```

Dünya bir süreçte kurulur, ölçümleri **ayrı bir süreç** (`perf_tests::probe`) yapar: masaüstü uygulamasının
yaptığı gibi koleksiyonu açar ve her şeyi `App::invoke` (JSON giriş/çıkış) üzerinden çağırır. Yedek ölçümleri
kendi tepe belleğini ölçer (Linux `clear_refs` ile süreç tepe değeri sıfırlanır). `FC_PERF_KEEP=<dosya>` kurulan
koleksiyonu saklar (profil için). Profil (v1.4 Z2): `FC_PERF_HOT_DB=<kopya> FC_PERF_HOT_OP=tree|summaries|start|options|breakdown|page cargo test -p fc-app --release --lib perf_tests::hot -- --ignored --nocapture` tek işi döngüde zamanlar; gdb ile örneklemek için (kopyanın eski `-wal`/`-shm` dosyalarını silin).

**Film izleme katmanı** (`fc-app/src/perf_film.rs`, v1.4 Z1): v1.0 dünyasının üstüne, gerçek yoldan
(`subtitle.media.upload` + `subtitle.captures.create`):

- 20 000 Subtitle karşılaşması, 6 000 kelimeye (her kelime en az bir kez, sonra az sayıda kelimeye yığılarak:
  bazı kelimeler onlarca sahnede);
- karışık diller: %85 İngilizce (uygulamayla gelen paketten, en sık 150 işlev kelimesi atlanarak; üçte birinde
  çekimli biçim, ör. *went*), %10 Almanca, %5 İspanyolca (uydurma kelimeler, paket yok);
- 60 "bölüm" (video kimliği), cümleler paketin en sık 1 000 kelimesinden;
- 3 000 yakalamada ekran görüntüsü + ses: **geçerli** 8×8 JPEG (Pillow ile üretilmiş, yorum bölümüyle tekilleşir)
  ve 16 kHz mono PCM WAV (2 000 örnek) → 6 000 küçük medya dosyası;
- **gerçek ~1 GiB** büyük medya: `media.add` ile 64 × 16 MiB WAV (yedek akışı için test düğmesi kullanılmadı).

## Makine

Linux konteyneri, **4 vCPU Intel Xeon @ 2.10 GHz**, 15 GiB bellek, sanal disk; Rust 1.100.0. v1.0 ölçümleri
(docs/10 §36–37) 8 çekirdekli bir makinedeydi: aynı sayılar burada ~1,2–1,5× yavaş beklenir.

## v1.4.1 kontrol koşusu (I1, 2026-11-23, release, aynı makine, sakin)

Sağlamlık turundan (H1–H4, I1) sonra aynı komutla baştan sona: **bütün bütçeler tutuyor**, gevşetilen yok
(kurulum: v1.0 dünyası 50 s, film katmanı 172 s). Z2'ye göre kayda değer oynamalar: açılış 705 → 870 ms (bütçe
2 000; açılış artık çöp boşaltmadan sonra ayrıca sahipsiz medya taraması ve `.verify-*` artığı denetimi yapar),
önbellekli istatistik 662 → 863 ms (bütçe 1 000, payı azaldı), `study.start` özel sorgu 684 → 952 ms (bütçesiz).
Gerisi Z2 ile aynı ya da daha iyi: `study.start` 459 ms, sıradaki kart p95 9,6 ms, en yavaş arama 64 ms, tarayıcı
sayfası 91 ms (imleçle 26), kırılım 1 342 ms, tepe bellek 326 MB, yakalama p95 12,3 ms, cevap sırasında yakalama
p95 42,1 ms, `io.backup.create` 74 s / tepe 386 MB, `io.backup.verify` 58 s / 357 MB.

## Sonuçlar — tam ölçek, **release**, aynı makine (2026-11-23, Z2 sonrası)

"Önce" sütunu Z1'in koşusu (sakin makine); "sonra" Z2'nin bütün değişikliklerden sonra baştan sona koştuğu son
koşu (kurulum: v1.0 dünyası 48 s, film katmanı 202 s). Z2'nin değişiklik öncesi kendi koşusu (aynı kod, başka
derlemelerle yüklü makine) Z1'inkine yakındı: açılış 1 772 ms, `study.start` 1 012 ms, tarayıcı sayfası 89 ms,
kırılım 2 472 ms, yedek tepe 2 676 / 2 673 MB. Bütçelerin hiçbiri gevşetilmedi; yedeğin bellek bütçesi
4 000 → **1 000 MB**'a sıkılaştırıldı.

| Ölçülen | Önce (Z1) | Sonra (Z2) | Bütçe | Durum |
|---|---|---|---|---|
| Açılış (`open_sqlite` + kabuğun ilk sorguları) | 2 235 ms | **705 ms** | 2 000 ms | tamam (önce aşıyordu) |
| `study.start` tüm koleksiyon | 1 140 ms | **441 ms** | 500 ms | tamam (önce aşıyordu) |
| `study.start` özel (`level>=B1 deck:İngilizce`) | — | 684 ms | — | (bütçesiz) |
| Sıradaki kart p50 / p95 | 7,3 / 15,9 ms | 6,2 / 17,6 ms | p95 20 ms | tamam |
| En yavaş arama | 66 ms | 68 ms (`retrievability<0.8`) | 100 ms | tamam |
| Tarayıcı sayfası (en kötü) | 108 ms | **97 ms** | 100 ms | tamam ama **sınırda** (aşağıya bkz.) |
| İstatistik: önbellekli / kırılım / deste kapsamı | 816 / 2 887 / 3 454 ms | 662 / **1 324** / 3 301 ms | 1 000 / 1 500 / 10 000 ms | tamam (kırılım önce aşıyordu) |
| 10 000 satır içe aktarma | 1 802 ms | 1 913 ms | 10 000 ms | tamam |
| Tepe bellek (yedek hariç) | 305 MB | 322 MB | 400 MB | tamam |
| Yakalama (kurulumda 20 000 tane) p50 / p95 / maks | 9,2 / 22,4 / 123 ms | 8,4 / 23,5 / 66 ms | p95 50 ms | tamam |
| **Cevap kaydedilirken yakalama**: cevap p50 / p95 / maks (40) | 7,8 / 14,1 / 31 ms | 8,4 / 12,6 / 31 ms | p95 60 ms | tamam |
| … aynı anda yakalama p50 / p95 / maks (20) | 11,2 / 54,5 / 55 ms | 4,5 / 93,1 / 107 ms | p95 100 ms | tamam, ama bu kısımda makinede debug derlemesi koşuyordu (aşağıya bkz.) |
| `study.options` (Subtitle destesi) / ilk kart | 726 / 12 ms | **343** / 114 ms | toplam 1 500 ms | tamam |
| `words.lookup` 500 biçim p50 / p95 / maks | 7,0 / 8,7 / 15 ms | 5,7 / 7,8 / 12 ms | p95 100 ms | tamam |
| `words.list` sıklık sırası (en kötü varyant) | 4,3 ms | 4,6 ms | 200 ms | tamam |
| Dil sorguları (en kötü `ses:yok`) | 48 ms | 62 ms | 300 ms | tamam |
| `subtitle.encounters.rank` p50 / p95 / maks | 0,4 / 0,8 / 0,9 ms | 0,4 / 0,9 / 1,0 ms | p95 50 ms | tamam |
| `words.index.rebuild_now` (5 694 kelime) | 1 814 ms | 1 937 ms | 5 000 ms | tamam |
| `io.backup.create` (1 122 MiB, 6 074 medya; yaz + geçici profile geri yükleyip doğrula) | 73 s, tepe 2 675 MB | 75 s, tepe **383 MB** | 150 s / 1 000 MB | tamam |
| `io.backup.verify` | 60 s, tepe 2 675 MB | 58 s, tepe **354 MB** | 120 s / 1 000 MB | tamam |

**Ölçüm notu:** son koşunun ilk bölümü (açılıştan içe aktarmaya, film katmanının kurulumu dahil) sakin makinede
koştu; film ölçümlerinin ortasından itibaren aynı makinede `cargo test` derlemesi başladı. "Aynı anda yakalama"nın
p95'i (93 ms; iki örnekte 100 ms'ye yakın) ve yedek süreleri bu yükü taşır; Z2 bu yolların koduna dokunmadı.

## Z2: ne değişti (profil → düzeltme)

Profil: `perf_tests::hot` (saklanan koleksiyonda tek işi döngüde zamanlar) + gdb örnekleme, release.

| Yavaş yol (profilde payı) | Düzeltme | Etki |
|---|---|---|
| `decks.tree` → `today_counts`: her deste düğümü kendi alt ağacının süren kartlarını yeniden okuyordu (~%45) | Planlayıcı süren kartları (gün sonundan önce vadeli öğrenme/tekrar) **bir kez** okur, her plan deste kümesine göre bellekte süzer (`Planner::candidates`); kart listesi, sınıflandırma koşulu ya da askıdakiler istendiğinde doğrudan depolama okuması | açılış 2 235 → 705 ms |
| Yeni kartlar: koleksiyon çapındaki `cards_new_order` küçük bir destenin ilk n kartı için diğer destelerin bütün yeni kartlarının üstünden yürüyordu (~%30) | migration **1422** `cards_deck_new_order (deck_id, created_at, id)`: her desteden ilk n, sonra birleştir + ilk n (≤ 64 deste; fazlası eski yol) | `study.start`, `study.options`, ağaç |
| Boş kart denetimi kuyruktaki her kartın notunu tek tek okuyordu (19 000 not × 2 sorgu) | `NoteRead::get_notes`: kuyruğun notları tek sorguda (10 000'lik parçalar) | `study.start` 1 140 → 441 ms (üçüyle birlikte) |
| Ağaçta `card_counts_by_deck` yalnız toplam için bütün kart tablosunu not birleşimiyle tarıyordu (~%33) | `DeckRead::card_totals_by_deck` + migration 1422 `cards_deck_live (deck_id, note_id) WHERE deleted_at IS NULL`: indeksten sayım, not canlılığı küçük `notes_trashed` kümesine bakış | ağaç, `decks.summaries`, boşluk nedeni |
| `study_cards` durum süzgecinin dışladığı dalı da koşuyordu | durumlar bir dalı dışlıyorsa o sorgu çalışmaz | — |
| Kırılım önbelleği film katmanının eklediği kartlarla geçersiz kalıyor, 2M satırı baştan katlıyordu (kartların hangi değerde olduğunun parmak izi her yeni kartta değişir) | Parmak izi kartların ekleme sırasını da saklar (`m<cards.rowid>/…`); sonradan eklenen kart parmak izine girmez (tekrarları önbellekten sonra geldiği için üstüne katlanır); önbellek sırasından önce tekrarı olan eklenmiş kart önbelleği yine geçersiz kılar. Gerçek kullanımda: her yakalanan kelime artık önbelleği atmıyor | kırılım 2 887 → 1 324 ms |
| Yedek dökümü bellekte bütünüyle (yazarken bir, doğrularken bir kez) | Akış: oluştururken SQLite imleci satır satır `collection.json`'a (aynı baytlar); doğrularken/geri yüklerken akışlı ayrıştırıcı 512 satırlık parçalarla geçici SQLite'a (geri yüklemede temizleyiciler parça başına). Eski şemalı dökümler eski yoldan (bütün döküm + `upgrade_dump`) | tepe 2,7 GB → 0,38 GB |

"Gösterilen sayı = verilen kart" bozulmadı: sayılar ve oturum aynı planlayıcıdan geçer (`every_scope_count_is_what_start_serves`,
`the_collection_queue_is_each_decks_own` vb. aynen geçiyor); yeni yol rastgele dünyalarda eski yola eşit
(`shared_candidates_equal_the_storage_read`: havuzlu aday okuması = depolama okuması ve her ağaç düğümü = yalnız o
deste için kurulan planlayıcı; `study_cards_per_deck_first_new_equals_reference`: deste deste ilk n = hepsini okuyup
ilk n; ikisi de mem + SQLite / rastgele tohumlarla), kırılım önbelleği + kuyruk = yeniden hesap (`breakdown_cache_equals_recomputation`,
eklenen kart ve onun çalışılması dahil).

## CI ölçeği — **debug** (aynı makine)

10k not / 20k kart / 200k tekrar + 1 000 karşılaşma / 300 kelime / 202 medya + 32 MiB:
açılış 346 ms · `study.start` 174 ms · sıradaki kart p95 7,5 ms · en yavaş arama 19 ms · yakalama p95 19 ms ·
cevap/yakalama aynı anda p95 28 / 47 ms · `study.options` 73 ms · `words.lookup` 500 p95 14 ms · dil sorguları
11 ms · `encounters.rank` p95 3,7 ms · dizin 346 ms · yedek oluştur / doğrula **16 / 10 s, tepe ~500 MB** (Z2 akışla: 17 / 12 s, tepe ~250 / ~225 MB).
Debug, release'ten 5–20× yavaştır (özellikle deflate ve serde); CI bütçeleri yalnız kaba regresyonu yakalar.

## Windows CI'deki yakalama takılmaları (v1.4 Z2 izleme)

Windows işinde (debug, ölçekli dünya) yakalama p50 5 ms iken p95 360 ms / maks 2,2 s, aynı anda cevap maks 1,5 s
ölçüldü. Linux'ta `FC_PERF_SLOW_MS=12` (yavaş yakalamaları sırasıyla yazdırır) ile aynı örüntü küçük ölçekte
görüldü: yavaşlar medyalı yakalamalar **değil**, her ~14–22 yakalamada bir (#14, 29, 42, 58, 73 …) ~17 ms — SQLite'ın
otomatik WAL checkpoint'i (1 000 sayfa): o commit'in içinde, yazar kilidi tutulurken sayfaları kopyalayıp veritabanı
dosyasını senkronluyordu; Windows'un CI diskinde bu saniyeler sürüyor ve arkasındaki her komut (cevaplar) bekliyordu.
Otomatik checkpoint kapatılınca periyodik yavaşlar kayboldu (p95 17,3 → 6,3 ms, cevap maks 27 → 11 ms). Düzeltme:
checkpoint deponun kendi iş parçacığında (docs/03 "SQLite"); sonrası Linux'ta yakalama p50 / p95 / maks 4,5 / 6,3 / 24 ms,
cevap 6,6 / 10,0 / 11 ms. Ölçekli CI bütçeleri **değişmedi** (Windows'ta kalan bir yavaşlık disk/virüs tarayıcısı
kaynaklıysa bir sonraki koşunun `FC_PERF_SLOW_MS` dökümü gösterir).

**İkinci adım:** ilk sürüm her commit patlamasından 250 ms sonra checkpoint ediyordu. strace: tam checkpoint'ten sonraki
ilk commit WAL'ı baştan başlatıp başlığını `fsync`'liyor (SQLite `walFrames`, `syncHeader`), yani duraklamadan sonraki
her ilk komut bir WAL senkronu ödüyordu (Linux'ta görünmez, Windows CI'de bir eklenti kancasının `kv.set` yazması 3 s'lik
süre sınırını aştı → `example_hook_fills_a_field` boş alan). Artık arka plan checkpoint'i yalnız WAL 1 000 sayfaya
ulaşınca (eski eşik, yazarın WAL kancası) tetikleniyor: baştan başlatma eskisi kadar seyrek, kopyalama + veritabanı
senkronu yine yazma yolunun dışında.

## Düzeltilenler (önce → sonra)

| Sorun | Düzeltme | Önce → sonra (CI ölçeği, debug) |
|---|---|---|
| Yedeğin `collection.json`'u `serde_json` ile **tamponsuz** zip + SHA-256 yazıcısına yazılıyor, okurken bayt bayt okunuyordu: her küçük parça ayrı hash + deflate çağrısı | `fc-interop::fcbackup`'ta 64 KiB `BufWriter`/`BufReader` (biçim aynı) | oluştur 46 → 16 s, doğrula 31 → 10 s |
| Doğrulama, geri yüklediği satırları saymak için koleksiyonu **ikinci kez döküyordu** | `InteropRead::dump_row_counts` (SQLite'ta tablo başına `COUNT(*)`) | doğrulamada bir tam döküm daha az |

## Bilinen sınırlar

- **Tarayıcı sayfası sınırda (97 / 100 ms), ofsetle.** En kötü ofset sayfası 200k kartın ortasındaki sayfa (sıralama
  indeksinde ~100k girdi yürüme, `OFFSET`). Önceki iki koşu 88 ve 108 ms: makine yüküyle bütçenin iki yanına düşebilir.
  **v1.4 (F1): keyset sayfalama.** `search.cards` artık isteğe bağlı `cursor` alır, `next_cursor` döner (docs/02 §9);
  imleçle sayfa `OFFSET` saymadan sıralama indeksinde son satırın ardından başlar. Ara ekranının "Daha fazla"sı imleci
  kullanır (ofset yalnız imleç yoksa). Perf takımı aynı sayfaları imleçle de ölçer: `… by keyset cursor` satırı
  (aynı 100 ms bütçesi; ofset satırı ve bütçesi değişmedi, ofsetle rastgele erişim isteyen çağıranlar için). Profil:
  `FC_PERF_HOT_OP=keyset`. Tam ölçek sonucu (release, 2026-11-23, aynı koşu,
  makinede başka derlemeler de koşuyordu): en kötü sayfa **ofsetle 74,2 ms → imleçle 29,9 ms** (üç sıralama ×
  dört konum, 200k kartın ortası ve sonu dahil; imleçli sayfanın kalan maliyeti çoğunlukla eşleşme sayımı `total`).
  Aynı koşuda `study.start` 663 ms ile bütçeyi aştı (yüklü makine; bu iz o yola dokunmadı), bu yüzden üst tablo
  güncellenmedi.
- **Eski şemalı yedekler** (bu derlemeden eski `schema_versions`) hâlâ bütün döküm bellekte okunur (`upgrade_dump`
  bütün dökümü ister); bugünkü şemayla alınan yedekler akışla.
- **Deste kapsamlı istatistik** 3,3 s (bütçe 10 s): önbellek bütün koleksiyon içindir, kapsamlı istek her seferinde
  hesaplanır.

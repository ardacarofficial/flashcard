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
koleksiyonu saklar (profil için).

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

## Sonuçlar — tam ölçek, **release**, sakin makine (2026-11-23)

Kurulum: v1.0 dünyası 52 s, film katmanı 211 s.

| Ölçülen | Sonuç | Bütçe | Durum |
|---|---|---|---|
| Açılış (`open_sqlite` + kabuğun ilk sorguları) | 2 235 ms | 2 000 ms | **aşıyor** (v1.0: 659 ms, 8 çekirdek) |
| `study.start` tüm koleksiyon | 1 140 ms | 500 ms | **aşıyor** (v1.0: 162 ms) |
| Sıradaki kart p50 / p95 | 7,3 / 15,9 ms | p95 20 ms | tamam |
| En yavaş arama (`deck:İngilizce is:due`) | 66 ms | 100 ms | tamam |
| Tarayıcı sayfası (en kötü) | 108 ms | 100 ms | **sınırda** (v1.0: 88 ms) |
| İstatistik: önbellekli / kırılım / deste kapsamı | 816 / 2 887 / 3 454 ms | 1 000 / 1 500 / 10 000 ms | kırılım **aşıyor** (v1.0: 1 040 ms) |
| 10 000 satır içe aktarma | 1 802 ms | 10 000 ms | tamam |
| Tepe bellek (yedek hariç) | 305 MB | 400 MB | tamam |
| Yakalama (kurulumda 20 000 tane) p50 / p95 / maks | 9,2 / 22,4 / 123 ms | p95 50 ms | tamam |
| **Cevap kaydedilirken yakalama**: cevap p50 / p95 / maks (40) | 7,8 / 14,1 / 31 ms | p95 60 ms | tamam |
| … aynı anda yakalama p50 / p95 / maks (20) | 11,2 / 54,5 / 55 ms | p95 100 ms | tamam |
| `study.options` (Subtitle destesi) / ilk kart | 726 / 12 ms | toplam 1 500 ms | tamam (ama yavaş; aşağıya bkz.) |
| `words.lookup` 500 biçim p50 / p95 / maks | 7,0 / 8,7 / 15 ms | p95 100 ms | tamam |
| `words.list` sıklık sırası (en kötü varyant) | 4,3 ms | 200 ms | tamam |
| Dil sorguları (en kötü `ses:yok`) | 48 ms | 300 ms | tamam |
| `subtitle.encounters.rank` p50 / p95 / maks | 0,4 / 0,8 / 0,9 ms | p95 50 ms | tamam |
| `words.index.rebuild_now` (5 694 kelime) | 1 814 ms | 5 000 ms | tamam |
| `io.backup.create` (1 122 MiB, 6 074 medya; yaz + geçici profile geri yükleyip doğrula) | 73 s, tepe 2 675 MB | 150 s / 4 000 MB | tamam, bellek yüksek |
| `io.backup.verify` | 60 s, tepe 2 675 MB | 120 s / 4 000 MB | tamam, bellek yüksek |

Aynı koşu sakin olmayan makinede (yanında başka bir derleme) neredeyse aynıydı (açılış 2 263 ms, `study.start`
955 ms); fark gürültü değil.

## CI ölçeği — **debug** (aynı makine)

10k not / 20k kart / 200k tekrar + 1 000 karşılaşma / 300 kelime / 202 medya + 32 MiB:
açılış 346 ms · `study.start` 174 ms · sıradaki kart p95 7,5 ms · en yavaş arama 19 ms · yakalama p95 19 ms ·
cevap/yakalama aynı anda p95 28 / 47 ms · `study.options` 73 ms · `words.lookup` 500 p95 14 ms · dil sorguları
11 ms · `encounters.rank` p95 3,7 ms · dizin 346 ms · yedek oluştur / doğrula **16 / 10 s, tepe ~500 MB**.
Debug, release'ten 5–20× yavaştır (özellikle deflate ve serde); CI bütçeleri yalnız kaba regresyonu yakalar.

## Düzeltilenler (önce → sonra)

| Sorun | Düzeltme | Önce → sonra (CI ölçeği, debug) |
|---|---|---|
| Yedeğin `collection.json`'u `serde_json` ile **tamponsuz** zip + SHA-256 yazıcısına yazılıyor, okurken bayt bayt okunuyordu: her küçük parça ayrı hash + deflate çağrısı | `fc-interop::fcbackup`'ta 64 KiB `BufWriter`/`BufReader` (biçim aynı) | oluştur 46 → 16 s, doğrula 31 → 10 s |
| Doğrulama, geri yüklediği satırları saymak için koleksiyonu **ikinci kez döküyordu** | `InteropRead::dump_row_counts` (SQLite'ta tablo başına `COUNT(*)`) | doğrulamada bir tam döküm daha az |

## Bilinen sınırlar (ölçüldü, düzeltilmedi)

- **Yedeğin belleği koleksiyonla büyür.** Döküm (`CollectionDump`) yazarken bir kez, doğrulamada bir kez
  bütünüyle bellekte: 2M tekrarda tepe ~2,7 GB. Medya zaten akışla gider; sıradaki adım tabloları satır satır
  akıtmak (yazarken SQL imleci → JSON, okurken akışlı ayrıştırıcı → `load_collection`).
- **Açılış ve `study.start`.** Örnekleyici profil (gdb, sakladığımız koleksiyonda): açılışın çoğu `decks.tree`
  → `study::plan::today_counts`. Her deste düğümü kendi alt ağacının adaylarını yeniden okur (`study_cards`;
  bu dünyada ~19 000 öğrenme kartı bir saat içinde vadeli, hepsi kuyrukta) ve v1.3'ten beri kuyruktaki her kartın
  "boş kart" denetimi notu tek tek okuyup şablonu çizer. v1.4'ün eklediği bir maliyet değil (v1.3 "gösterilen
  sayı = verilen kart" kuralı); düzeltmesi planlayıcıda düğümler arası aday önbelleği + toplu not okuması ister ve
  sayım = verilen kart garantisine dokunduğu için bu geçişte yapılmadı. `study.options` (726 ms) aynı yoldan geçer.
- **Kırılım istatistiği** 2,9 s (v1.0: 1,0 s, 8 çekirdek): makine farkının üstünde; incelenmedi.

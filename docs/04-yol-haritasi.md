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
- Minimal arayüz: deste ağacı, not ekleme (klasik), not listesi.
- CI: fmt, clippy (`-D warnings`), testler, tip üretimi tutarlılık kontrolü, Windows derlemesi.

**Kabul kriterleri**
- [ ] `cargo test --workspace` geçer, `fc-core` I/O içermez (lint/inceleme ile doğrulanır).
- [ ] Deste/not oluşturma, düzenleme, silme komutları çalışır ve **geri alınabilir.**
- [ ] Rust'ta bir DTO alanı değişince frontend `pnpm check` kırılır.
- [ ] Aynı işlemler CLI ile de yapılabilir.
- [ ] Boş veritabanından migration testi geçer.
- [ ] Windows'ta CI derlemesi başarılı.

## 📖 v0.2 · Çalışma çekirdeği

**Amaç:** Uygulama günlük kullanılabilir hale gelir.

**Kapsam:** FSRS entegrasyonu (`fsrs-rs`), SM-2 yedek; oturum motoru; çalışma ekranı; puanlama ve sonraki aralık gösterimi; geri al; kart geçmişi ve `review_log`; cevap süresi; askıya al/göm; kardeş gömme; yazarak cevaplama + cevap kontrolcüleri (birebir, normalize, toleranslı, çoklu); ipucu motoru; deneme modu; yanlışları tekrar çalış; çalışmayı ertele; oturum sonu ekranı; çalışma kısayolları.

**Kabul kriterleri**
- [ ] Zamanlayıcı testleri: deterministik saatle 365 günlük simülasyon, FSRS referans değerleriyle uyum.
- [ ] Puanlama → geri al → kart eski durumuna birebir döner.
- [ ] Deneme modunda `cards` tablosu değişmez.
- [ ] Bir oturum tamamen klavyeyle yürütülebilir.

## 🧩 v0.3 · Kart türleri ve içerik

**Kapsam:** Not türü şeması ve editörü; şablon dili; kart türü motoru; klasik, ters, isteğe bağlı ters, cloze, çoktan seçmeli, yazarak, resim üzerinde gizleme, resimli quiz, sesli kart; hazır not türleri (kelime kartı dahil); Markdown editör, KaTeX, kod blokları; medya deposu, sürükle-bırak, Ctrl+V; kart önizleme; `[[bağlantılar]]` ve geri bağlantılar; kart notu; bayraklar (özel adlarıyla).

**Kabul kriterleri**
- [ ] Yeni bir kart türü sadece `CardTypeEngine` uygulanarak eklenebilir (testte örnek tür ile kanıt).
- [ ] Not türüne alan eklemek/çıkarmak mevcut kartları ve geçmişi bozmaz.
- [ ] Aynı medya iki kez eklenince tek dosya tutulur.
- [ ] HTML temizleme testleri (XSS vektörleri) geçer.

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
- [ ] `Arda, porsuk` ve `Arda: porsuk` ve özel ayırıcılı dosyalar doğru ayrışır; otomatik algılama test kümesinde ≥ %95 doğru.
- [ ] Örnek Anki desteleri (cloze, resimli, sesli) kayıpsız içe aktarılır; dışa aktarılan .apkg Anki'de açılır.
- [ ] Yedek → sil → geri yükle döngüsü veriyi birebir korur.
- [ ] İçe aktarma iptal edilebilir ve tek geri al ile geri döner.

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

## 🔮 v1.0 sonrası

macOS · Linux · senkronizasyon · mobil · erişilebilirlik · oyunlaştırma · eklenti mağazası kaynağının işletilmesi · isteğe bağlı veritabanı şifreleme.

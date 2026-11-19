# 02 · Özellik Kataloğu

Onaylanan tüm özellikler. Her özelliğin yanında **hangi yapı taşlarına dayandığı** ve **hangi sürümde** geleceği yazar. Yapı taşlarının ayrıntıları için bkz. [03-mimari.md](03-mimari.md).

Gösterim: `[v0.2]` = hedef sürüm · 🧱 = dayandığı yapı taşları

---

## 1. 📚 Desteler

`[v0.1]` · 🧱 Veri modeli, komutlar, olay kaydı

- **Hiyerarşik desteler**, sınırsız derinlik: `Dil::İngilizce::Fiiller`.
- Bir üst deste çalışılınca alt destelerin kartları da dahil olur (ayar ile kapatılabilir).
- Deste oluşturma, yeniden adlandırma, taşıma (sürükle-bırak), silme (çöp kutusuna).
- Deste açıklaması, ikonu ve rengi.
- Her deste bir **ayar şablonuna (preset)** bağlıdır. `[v0.4]`
- Deste başına özet: yeni, öğreniliyor, bugün vadesi gelen kart sayıları.

## 2. 🃏 Notlar ve kartlar

`[v0.1–v0.3]` · 🧱 Veri modeli, not türü şeması, kart türü motoru

- **Not ≠ Kart.** Not, veriyi tutar (ör. kelime, anlam, örnek cümle, ses). Kart, o notun bir sorulma biçimidir.
- Bir nottan birden fazla kart türeyebilir (ör. İng→Tr ve Tr→İng). Bunlara **kardeş kart** denir.
- Not oluşturma, düzenleme, silme (çöp kutusuna), taşıma, kopyalama.
- Not düzenlendiğinde ondan türeyen kartlar otomatik güncellenir. Kartların öğrenme geçmişi korunur.
- Tüm notlar ve kartlar değişmez **UUID** ile tanımlanır.

## 3. 📝 Not türleri ve kart şablonları

`[v0.3]` · 🧱 Alan şeması, kart türü motoru

- **Not türü** = alanlar listesi + bu alanlardan kart üreten kart şablonları.
- Alan tipleri: metin, zengin metin (Markdown), resim, ses, video, sayı, etiket listesi, bağlantı.
- Alan seçenekleri: zorunlu mu, benzersiz mi (tekrar tespiti için), aranabilir mi, varsayılan değer.
- **Hazır not türleri:**
  - Klasik (Ön / Arka)
  - Klasik + ters kart
  - İsteğe bağlı ters kart
  - Yazarak cevaplama
  - Cloze (boşluk doldurma)
  - Çoktan seçmeli
  - Resim üzerinde gizleme
  - Kelime kartı (kelime, anlam, telaffuz, örnek cümle, ses, resim)
- Kullanıcı kendi not türünü oluşturabilir, alan ekleyip çıkarabilir.
- Kart şablonları alanları yerleştiren bir şablon dili kullanır: `{{kelime}}`, `{{#ornek}}...{{/ornek}}` (koşullu), `{{tts:kelime}}` (sesli okuma), `{{type:anlam}}` (yazarak cevaplama), `{{hint:ornek}}` (ipucu).
- Şablon dili saf bir fonksiyondur: `render(şablon, alanlar, bağlam) → içerik`. Frontend'den bağımsızdır.

## 4. 🧩 Kart türleri

`[v0.3]` · 🧱 Kart türü motoru

Kart türü motorunun üç işi vardır: **kart üret**, **soruyu sun**, **cevabı değerlendir**. Yeni bir kart türü bu üçünü sağlayarak eklenir (yerleşik ya da eklenti).

| Tür | Açıklama |
|---|---|
| Klasik | Ön yüz gösterilir, kullanıcı çevirir, kendini puanlar |
| Ters | Arka yüz soru olur |
| Cloze | `{{c1::Ankara}} Türkiye'nin başkentidir` → boşluk sorulur. Aynı notta c1, c2... ayrı kartlar olur |
| Çoktan seçmeli | Doğru cevap + çeldiriciler. Çeldiriciler elle girilir ya da aynı destedeki diğer notlardan otomatik seçilir (veya AI ile üretilir) |
| Yazarak cevaplama | Kullanıcı cevabı yazar, cevap kontrolcüsü değerlendirir, fark gösterilir (diff) |
| Resim üzerinde gizleme | Resmin bölgeleri maskelenir, her maske ayrı kart olur. "Birini gizle, diğerlerini göster" ve "hepsini gizle, birini sor" modları |
| Resimli quiz | Resim gösterilir, cevap metin, seçenek ya da yazarak verilir |
| Sesli kart | Ses çalınır, kullanıcı ne duyduğunu yazar/seçer (dinleme pratiği) |
| Sesle cevaplama | Kullanıcı cevabı söyler, STT sağlayıcısı metne çevirir, cevap kontrolcüsü değerlendirir (STT eklentisi gerekir) |

## 5. 🧠 Aralıklı tekrar (zamanlayıcı)

`[v0.2, v0.6]` · 🧱 Zamanlayıcı arayüzü, olay kaydı

- Varsayılan algoritma: **FSRS'in en güncel sürümü** (resmi Rust kütüphanesi `fsrs-rs` üzerinden).
- Zamanlayıcı bir arayüzdür. SM-2 yedek olarak gelir, eklentiyle başka algoritma takılabilir.
- Puanlama: **Tekrar (1) / Zor (2) / İyi (3) / Kolay (4).**
- Her butonun üstünde "bir sonraki tekrar" süresi gösterilir (ör. "10dk", "3g", "2ay").
- **Kişisel optimizasyon** `[v0.6]`: Kullanıcının geçmişinden FSRS parametreleri öğrenilir (deste/preset bazında).
- **Hedef hatırlama oranı** `[v0.6]`: Ör. %90. Yükseldikçe tekrar sıklaşır. Arayüz bunun iş yükü etkisini tahmin olarak gösterir.
- **Yük dengeleme** `[v0.6]`: Tekrarlar eşdeğer aralıktaki günlere dağıtılır, bir gün 300 bir gün 10 kart olmaz.
- **Kolay günler** `[v0.6]`: Haftanın belirli günleri (ör. hafta sonu) daha az kart planlanır.
- **Sınav tarihi modu** `[v0.6]`: Bir deste için sınav tarihi verilir. Algoritma kartların o tarihte hatırlanma olasılığını en üst düzeye çıkaracak şekilde planlar, sınav öncesi yoğunlaştırır.
- **Günlük limit yok.** Kullanıcı istediği kadar çalışır. (Yeni kartların "tanıtım sırası" ayarlanabilir: rastgele, ekleme sırası.)

## 6. ✅ Çalışma modu

`[v0.2]` · 🧱 Oturum motoru, zamanlayıcı, kart türü motoru, olay kaydı

- Akış: kart gösterilir → (cevap verilir) → çevrilir → puanlanır → sonraki kart.
- **Oturum motoru** backend'dedir: sıradaki kartı backend seçer, frontend sadece gösterir.
- Kart sırası: vadesi gelenler, öğrenilmekte olanlar, yeniler. Karıştırma seçenekleri: rastgele, yeniden eskiye, en zordan kolaya.
- **Kardeş kartlar** aynı gün gösterilmez (gömülür), ayarla kapatılabilir.
- **Cevap süresi** ölçülür ve olay olarak kaydedilir (cevabı gösterene kadar ve puanlayana kadar ayrı ayrı).
- **Geri al:** son puanlama geri alınır, kart tekrar gösterilir.
- **Oturum sonu ekranı:** çalışılan kart sayısı, doğruluk, süre, "**Yanlışları tekrar çalış**" butonu.
- **Çalışmayı ertele:** "Bugünün kartlarını N gün ileri kaydır" (seçili deste ya da tümü).
- **Çökme kurtarma:** Oturum durumu kalıcıdır. Uygulama kapanırsa kaldığı yerden devam eder. `[v1.0]`

### 6.1 Çalışma oturumu türleri

| Oturum | Açıklama | Takvimi etkiler mi? |
|---|---|---|
| Normal | Vadesi gelen kartlar | ✅ |
| Özel oturum | Bir sorguyla seçilen kartlar (ör. `tag:zor deck:İngilizce`) | Seçilebilir |
| Deneme modu | Kartlar çalışılır, puanlar zamanlamayı **etkilemez** (ör. sınav öncesi göz gezdirme) | ❌ |
| Yanlışları tekrar | Oturumda "Tekrar" denilen kartlar | Seçilebilir |
| Zamanlı mod | Her karta süre sınırı (ör. 10 sn), süre dolunca "Tekrar" sayılır ya da geçilir | Seçilebilir |
| Öğren modu (Quizlet tarzı) | Kart önce çoktan seçmeli, sonra yazarak sorulur. Kart her seviyede doğru cevaplanınca "öğrenildi" olur `[v0.10]` | Seçilebilir |
| Test | Desteden karışık soru tipleriyle sınav kâğıdı üretilir, sonunda puan verilir `[v0.10]` | ❌ |

### 6.2 Çalışma sırasında araçlar

- 💬 **İpucu motoru:** Ayarlanabilir ipucu stratejileri: ilk harf, her kelimenin ilk harfi, harflerin bir kısmı, belirli bir alan (ör. örnek cümle), AI ipucu. Kaç ipucu kullanıldığı kaydedilir.
- 📝 **Kart üstüne not:** Kullanıcının kişisel notu (ör. "Bunu şöyle hatırlıyorum"). Not türü alanlarından ayrıdır.
- 🚩 **Bayraklama:** Renkli bayraklar (kırmızı, turuncu, yeşil, mavi, mor...). Bayrak adları özelleştirilebilir (ör. kırmızı = "düzelt").
- ⏸️ **Askıya al:** Kart elle tekrar açılana kadar gösterilmez.
- ⛱️ **Göm:** Kart sadece bugün gösterilmez.
- ✏️ **Kartı düzenle:** Çalışmadan çıkmadan kartı düzenleme.
- ℹ️ **Kart bilgisi:** Bkz. §11.
- 🔊 **Sesli okuma:** TTS sağlayıcısı varsa alan seslendirilir (otomatik ya da butonla).
- 🧘 **Odak modu:** Tam ekran, dikkat dağıtıcı her şey gizli.

## 7. ✍️ Cevap kontrolü

`[v0.2]` · 🧱 Cevap kontrolcü arayüzü

Yazarak ve sesle cevaplamada cevabı değerlendiren takılabilir kontrolcüler:

| Kontrolcü | Açıklama |
|---|---|
| Birebir | Tam eşleşme |
| Normalize | Büyük/küçük harf, baştaki/sondaki boşluk, noktalama, Türkçe karakter eşdeğerliği (ı/i, ş/s: ayarlanabilir) yok sayılır |
| Yazım toleranslı | Levenshtein mesafesine göre "neredeyse doğru" |
| Çoklu doğru cevap | `elma; apple` gibi alternatifler |
| AI anlam kontrolü | Eş anlamlı/doğru anlamlı cevapları kabul eder `[v0.8]` |

Sonuç sadece doğru/yanlış değil, **derecelidir**: `doğru`, `neredeyse`, `yanlış` + fark (diff). Kullanıcı sonucu geçersiz kılıp kendi puanını verebilir. Kontrolcü bir puan **önerir**, varsayılan olarak puanlamayı kullanıcı yapar (ayarla otomatik puanlama açılabilir).

## 8. 🏷️ Etiketler

`[v0.1]` · 🧱 Veri modeli

- Notlara etiket verilir. Hiyerarşik etiketler: `dil::fiil::düzensiz`.
- Etiket yeniden adlandırma ve birleştirme (tüm notlarda).
- Etiket rengi.

## 9. 🔎 Arama ve sorgu dili

`[v0.4]` · 🧱 Sorgu dili (DSL), SQLite FTS5

- Tek bir **sorgu motoru** her yerde kullanılır: arama, özel oturum, toplu işlem, istatistik filtreleri, otomasyon kuralları, eklentiler.
- Örnek sorgular:
  - `deck:İngilizce tag:fiil` → destede ve etikette
  - `deck:"Dil::İngilizce"` → tırnakla boşluk
  - `is:due` · `is:new` · `is:suspended` · `is:buried` · `is:flagged` · `flag:red`
  - `stability>30` · `difficulty>7` · `lapses>=5` · `reps<3` · `retrievability<0.8`
  - `added:7` (son 7 gün) · `reviewed:1` · `due:0` · `due:3`
  - `type:cloze` · `notetype:"Kelime kartı"`
  - `kelime:app*` → alanda joker
  - `-tag:kolay` → değil · `(a OR b)` → veya
  - `"tam ifade"` · `re:^ab.*` → regex
- **Kayıtlı filtreler:** Sorguya isim verilir (ör. "Zor fiillerim"), kenar çubuğunda görünür, tek tıkla çalışılır.
- Tam metin arama için SQLite **FTS5**, Türkçe karakter normalizasyonuyla.
- Sorgu dili saf bir ayrıştırıcıdır: `parse(metin) → AST`, `compile(AST) → SQL`. Hata mesajları konumlu ve anlaşılırdır.

## 10. 🛠️ Kart yönetimi

`[v0.4]` · 🧱 Komutlar, sorgu dili, olay kaydı

- **Kart tarayıcısı:** Tablo görünümü, sütunlar seçilebilir (alanlar, deste, etiket, vade, stabilite, zorluk, unutma sayısı...), sıralama, sorgu çubuğu.
- **Toplu işlemler:** Seçili kartlara: taşı, etiket ekle/çıkar, askıya al, göm, bayrakla, sil, vadeyi sıfırla, not türünü değiştir (alan eşleme ile), bul-değiştir. Hepsi **tek transaction** ve **tek geri al**.
- **Çöp kutusu:** Silinen notlar 30 gün (ayarlanabilir) tutulur, geri yüklenebilir, sonra kalıcı silinir.
- **Tekrar eden kart tespiti:** Kart eklerken ve içe aktarırken, "benzersiz" işaretli alana göre (normalize edilmiş) uyarı. Ayrıca "Tekrarları bul" aracı.
- **Veritabanı kontrolü:** Sahipsiz kart, eksik medya, bozuk alan, tutarsız zamanlama tespiti ve onarımı. Önce rapor, sonra onayla onarım.
- **Deste ayar şablonları (preset):** Algoritma parametreleri, hedef hatırlama oranı, kardeş gömme, yeni kart sırası vb. ayarlar bir presette toplanır, birden çok desteye uygulanır.

## 11. ℹ️ Kart bilgisi ve geçmiş

`[v0.2, v0.4]` · 🧱 Olay kaydı

- Kartın tüm geçmişi: her tekrarın tarihi, puanı, süresi, o andaki stabilite/zorluk, kullanılan ipucu, oturum türü.
- Eklenme tarihi, son değişiklik, bir sonraki vade, mevcut hatırlama olasılığı.
- Hafıza grafiği (zamana göre stabilite).
- Notun düzenleme geçmişi (eski alan değerleri).

## 12. ↩️ Geri al / yinele

`[v0.2]` · 🧱 Olay kaydı, komutlar

- Her komut geri alınabilir: puanlama, düzenleme, silme, toplu işlem, içe aktarma.
- Sınırsız geri al yığını (oturum boyunca), Ctrl+Z / Ctrl+Shift+Z.
- Arayüz son işlemi gösterir: "Geri alındı: 50 kart taşındı".

## 13. ⌨️ Klavye kısayolları ve komut paleti

`[v0.2, v0.4]` · 🧱 Komutlar

- Çalışmada: `Space` çevir, `1-4` puanla, `Z` geri al, `E` düzenle, `H` ipucu, `F` bayrak, `S` askıya al, `B` göm, `R` sesi tekrar çal.
- Genel: `Ctrl+K` komut paleti, `Ctrl+N` yeni not, `Ctrl+F` ara, `Ctrl+Z` geri al.
- Tüm kısayollar ayarlardan değiştirilebilir. Çakışma tespiti.
- **Komut paleti:** Uygulamadaki her eylem (eklentilerinkiler dahil) aranabilir ve çalıştırılabilir.
- Kısayol tanımları backend'deki komut kaydından (registry) gelir. Frontend bunları sadece bağlar.

## 14. ✍️ İçerik ve editör

`[v0.3]` · 🧱 Alan şeması, medya deposu, şablon dili

- **Zengin editör:** Markdown tabanlı. Kalın, italik, renk, liste, tablo, alıntı, kod bloğu (sözdizimi renklendirme).
- **LaTeX / matematik:** `$x^2$` ve `$$...$$` (KaTeX ile).
- **Medya:** Resim, ses, video. Sürükle-bırak, dosyadan seç, **Ctrl+V ile ekran görüntüsü yapıştır.**
- **Kart önizleme:** Kaydetmeden önce kartın her türünün nasıl görüneceği.
- **Kartlar arası bağlantı:** `[[porsuk]]` yazınca ilgili not bulunur, link olur. Kartta tıklanınca o not açılır. Geri bağlantılar (bu nota kim link vermiş) görünür.
- **Satır içi kart oluşturma (RemNote tarzı)** `[v0.10]`: Uzun bir notta `soru :: cevap` satırları otomatik kart olur.
- İçerik saklama formatı Markdown + sınırlı uzantılardır (cloze, bağlantı, medya referansı). Frontend'e hem ham hem işlenmiş (HTML) hali verilir. HTML **temizlenir** (sanitize).

## 15. 📥 İçe aktarma

`[v0.5]` · 🧱 İçe aktarıcı arayüzü, iş kuyruğu, tekrar tespiti

### 15.1 Esnek metin içe aktarma (Quizlet tarzı)
- **Terim ↔ tanım ayırıcısı:** `,` · `:` · `-` · `=` · `Tab` · `;` · **özel** (ör. `>>`, ` - `).
- **Kartlar arası ayırıcı:** yeni satır · `;` · boş satır · **özel.**
- **Canlı önizleme:** Ayarlar değiştikçe ilk N kartın nasıl oluşacağı tablo halinde.
- **Otomatik algılama:** Dosya/metin analiz edilip en olası ayırıcılar önerilir.
- Alan eşleme: sütunlar hangi not türü alanına gidecek.
- Tırnak içi ayırıcıların doğru işlenmesi (`"Arda, Ali": porsuk`).
- Kaynak: dosya veya doğrudan yapıştırılan metin.

### 15.2 Formatlar
| Format | İçe | Dışa |
|---|---|---|
| Düz metin / CSV / TSV (esnek ayırıcı) | ✅ | ✅ |
| JSON (kendi şemamız) | ✅ | ✅ |
| Quizlet dışa aktarım metni | ✅ | — |
| Anki **.apkg** / **.colpkg** (notlar, not türleri, medya, zamanlama geçmişi) | ✅ | ✅ |
| Kendi deste paketimiz (bkz. §16) | ✅ | ✅ |
| Altyazı (.srt, .vtt) → bkz. §25 | ✅ | — |

### 15.3 Ortak davranış
- Tüm içe aktarmalar **arka plan işi** olarak çalışır: ilerleme çubuğu, iptal.
- İçe aktarma tek bir geri alınabilir işlemdir.
- Tekrar eden kartlar için seçenek: atla, güncelle, yine de ekle.
- İçe aktarma raporu: kaç eklendi, kaç atlandı, hatalar.

## 16. 📦 Dışa aktarma, deste paketi, yedek, yazdırma

`[v0.5]`

- **Deste paketi:** Tek dosya (`.fcpack`, zip tabanlı): notlar, kartlar, not türleri, medya, isteğe bağlı öğrenme geçmişi. Sürüm bilgisi taşır. Paylaşılabilir.
- **Tam koleksiyon yedeği:** Tüm koleksiyon tek dosya (`.fcbackup`). Elle alınır ("Yedek al" butonu). Geri yükleme önce mevcut durumun yedeğini alır.
- **Yazdırma / PDF:** Desteyi kâğıt kart düzeninde (ön yüzler bir sayfa, arka yüzler ayna sıralı) veya liste halinde yazdırma.

## 17. 📊 İstatistikler

`[v0.6]` · 🧱 Olay kaydı, sorgu dili

Her istatistik global, deste, etiket veya sorgu bazında görüntülenebilir; tarih aralığı seçilebilir.

| İstatistik | Açıklama |
|---|---|
| 🔥 Seri | Kaç gündür aralıksız çalışıldığı, en uzun seri |
| ✅ Başarı oranı | Doğru cevap yüzdesi (yeni / genç / olgun kartlar ayrı) |
| 📅 Aktivite ısı haritası | Gün gün çalışma yoğunluğu |
| 🐌 Zor kartlar (leech) | Unutma sayısı eşiği aşan kartlar. Eşik ayarlanabilir. Eylem: etiketle, askıya al, AI ile yeniden yaz |
| ⏱️ Cevap süreleri | Ortalama, dağılım, en yavaş kartlar |
| 🔮 Gelecek tahmini | Önümüzdeki günlerde vadesi gelecek kart sayısı |
| 🧠 Hafıza durumu | Yeni / öğreniliyor / genç / olgun / askıda dağılımı |
| 📈 Gelişim | Haftalık/aylık çalışma ve başarı trendi |
| 🕐 En verimli saat | Saatlere göre başarı oranı |
| 📉 Stabilite / zorluk dağılımı | FSRS parametrelerinin histogramları |
| 🎯 Gerçekleşen vs hedef hatırlama | Hedef %90 ise gerçekte kaç |
| 📈 Deste karnesi | Destenin ne kadarının "oturduğu" yüzde olarak |

- İstatistikler olaylardan hesaplanır. Yeni istatistik eklemek şema değişikliği gerektirmez.
- Büyük koleksiyonlar için ön hesaplanmış özet tabloları (olaylardan türetilir, her an yeniden üretilebilir).

## 18. 🎯 Hedefler

`[v0.6]`

- "Ay sonuna kadar 500 kelime öğren", "Her gün 30 dk çalış" gibi hedefler.
- Hedef türleri: öğrenilen kart sayısı, çalışma süresi, seri, deste tamamlama yüzdesi.
- İlerleme çubuğu, gidişata göre tahmin ("Bu hızla 3 gün geç kalırsın").

## 19. 🔔 Hatırlatıcılar

`[v1.0]`

- İşletim sistemi bildirimi: "Bugün 40 kartın var."
- Saat ve gün seçimi. Bugün çalışıldıysa bildirim gönderilmez.
- Uygulama tepside (system tray) çalışabilir.

## 20. 🔌 Eklentiler

`[v0.7]` · Ayrıntılar: [05-eklenti-sistemi.md](05-eklenti-sistemi.md)

- Güvenli, izole (WASM) eklentiler; izin sistemi.
- Eklentiler yeni kart türü, çalışma modu, sağlayıcı (TTS, STT, çeviri, AI, algoritma, içe aktarıcı, cevap kontrolcü), komut, arayüz parçası ve ayar ekleyebilir.
- Eklenti yöneticisi: kur (dosyadan / mağazadan), etkinleştir, devre dışı bırak, güncelle, kaldır, izinleri yönet.
- **Model indirme yöneticisi:** Yerel TTS ve Whisper modellerini ayarlardan indir, sil, seç.
- **Eklenti mağazası altyapısı:** Kaynak (registry) takılabilir. Hangi kaynağın kullanılacağı sonra karar verilecek.

## 21. 🤖 Yapay zeka

`[v0.8]` · Ayrıntılar: [06-ai-ve-entegrasyonlar.md](06-ai-ve-entegrasyonlar.md)

- Sağlayıcıdan bağımsız: Claude, OpenAI, OpenAI uyumlu uç noktalar, Ollama (yerel).
- Kullanıcı kendi API anahtarını girer. Anahtarlar işletim sistemi anahtar zincirinde.
- Kullanım ve harcama takibi.
- Yetenekler: dosyadan/metinden kart üretme, "bunu açıkla", akıllı cevap kontrolü, kart iyileştirme, zor kartı yeniden yazma, çeldirici üretme, AI ipucu, örnek cümle üretme.
- **MCP istemcisi** ve **MCP sunucusu.**

## 22. 🌉 Dış dünya ile bağlantı

`[v0.9]` · Ayrıntılar: [06-ai-ve-entegrasyonlar.md](06-ai-ve-entegrasyonlar.md)

- **Yerel HTTP API** (sadece localhost, token ile, izin onaylı).
- **Gelen kutusu:** Dışarıdan gelen kartlar önce buraya düşer (ayar ile direkt desteye).
- **Tarayıcı eklentisi:** Seç → sağ tık → "Desteye ekle". Bağlam (sayfa adresi, cümle) de kaydedilir.
- **Derin bağlantılar:** `flashcard://deck/<id>`, `flashcard://note/<id>`, `flashcard://study?query=...`, `flashcard://add?front=...&back=...`.
- **Global hızlı ekleme:** Uygulama arka plandayken global kısayolla küçük ekleme penceresi.
- **Pano izleme:** (varsayılan kapalı) Kopyalanan metin için "Kart olarak ekleyeyim mi?" önerisi. Filtrelenebilir (ör. sadece tek kelimeler, sadece İngilizce).

## 23. ⚡ Otomasyon kuralları

`[v0.9]` · 🧱 Olay yolu, sorgu dili, komutlar

- "**Ne zaman** (olay) + **eğer** (sorgu) + **yap** (komut)" kuralları.
- Örnekler:
  - Kart kırmızı bayraklanınca → `#düzelt` etiketi ekle.
  - Kart 8 kez unutulunca → askıya al ve `#leech` etiketle.
  - Gelen kutusuna İngilizce kelime düşünce → "Kelime kartı" not türüyle İngilizce destesine ekle, AI ile anlamını doldur.
- Kurallar sırayla ve sınırlı derinlikte çalışır (sonsuz döngü engellenir). Her kural çalışması kaydedilir.

## 24. 📖 Okuma modu

`[v0.10]` · 🧱 Sorgu dili, kelime takibi, AI/çeviri sağlayıcıları

- Metin yapıştırılır, dosya veya web sayfası açılır.
- **Kelime durumu renklendirme (LingQ tarzı):** Bilinen (renksiz), öğreniliyor (sarı), yeni (mavi), yok sayılan (gri). Durum, destedeki kartların FSRS verisinden ve kullanıcının işaretlemelerinden gelir.
- Kelimeye tıklayınca: anlam (sözlük/çeviri/AI), "Kart yap" (cümle bağlamıyla birlikte), "Biliyorum" işaretle.
- Kelime **kökü / ailesi** desteği (run/running/ran): dil bazında lemmatizasyon sağlayıcısı (takılabilir).
- Metindeki bilinen kelime yüzdesi ("Bu metnin %87'sini biliyorsun").

## 25. 🎬 Altyazıdan kart ve cümle madenciliği

`[v0.10]`

- .srt / .vtt yüklenir. Cümleler listelenir, bilinmeyen kelime içerenler vurgulanır (okuma modu ile aynı motor).
- Cümleden kart: cümle + çeviri + hedef kelime + zaman damgası.
- Video dosyası da verilirse: ilgili ses kesiti ve ekran görüntüsü karta eklenir (ffmpeg ile).

## 26. 📚 Artımlı okuma (SuperMemo tarzı)

`[v0.10]`

- Uzun bir makale "okuma kuyruğuna" alınır, parça parça okunur.
- Önemli kısım seçilir → "Özüt" (extract) olur → özütten cloze/kart üretilir.
- Okuma kuyruğu da aralıklı planlanır (makaleye tekrar dönme zamanı).

## 27. 🎓 Güven bazlı puanlama

`[v0.10]`

- Alternatif puanlama: 1-5 "Ne kadar eminim?" (Brainscape tarzı).
- Zamanlayıcıya 4'lü puana eşleme tablosuyla aktarılır. Ayarlardan seçilir.

## 28. 👥 Profiller

`[v1.0]`

- Aynı bilgisayarda birden fazla kullanıcı. Her profil ayrı veritabanı, ayrı ayarlar, ayrı eklenti izinleri.
- Profil değiştirme ekranı.

## 29. 🔒 Uygulama kilidi

`[v1.0]`

- Açılışta parola/PIN. Belirli süre boşta kalınca kilitlenme.
- Parola güvenli şekilde hash'lenir (Argon2). (Not: Bu bir erişim kilididir. İleride isteğe bağlı veritabanı şifreleme düşünülebilir, bkz. açık kararlar.)

## 30. 🌍 Çoklu dil (i18n)

`[v1.0]` (altyapı v0.1'den itibaren)

- Arayüz: **Türkçe** ve **İngilizce.**
- Çeviri anahtarları frontend'de. Backend hata kodları ve mesaj anahtarları döner, metin döndürmez (veya her ikisini döner).
- Eklentiler kendi çeviri dosyalarını getirebilir.
- Tarih, sayı formatları yerel ayara göre.

## 31. 🌙 Temalar

`[v1.0]`

- Açık, koyu, sistem teması. Vurgu rengi seçimi.
- Tema değişkenleri (CSS token) ile. Eklentiyle özel tema.
- Kart içeriği için ayrı stil (yazı tipi, boyut, hizalama), not türü bazında.

## 32. 👋 İlk açılış deneyimi

`[v1.0]`

- **İlk açılış rehberi:** "Ne öğrenmek istiyorsun?" (dil, sınav, genel) → uygun not türü ve preset ile başlatma. Kısa, atlanabilir.
- **Hazır başlangıç desteleri:** Örnek desteler (ör. temel İngilizce kelimeler, uygulama kullanım rehberi destesi). Silinebilir.

## 33. ⚙️ Ayarlar

`[v0.1'den itibaren]` · 🧱 Şemalı ayarlar

- Katmanlar: uygulama → profil → preset → deste → eklenti.
- Her ayar bir şemayla tanımlanır (tip, varsayılan, aralık, açıklama anahtarı, "gelişmiş" mi). Arayüz formları şemadan üretilebilir.
- **Ayar araması.**
- **Modül anahtarları:** Her özellik modülü açılıp kapatılabilir (okuma modu, pano izleme, AI, yerel API...).
- Ayarları dışa/içe aktarma.

## 34. 🔄 Uygulama bakımı

`[v1.0]`

- **Otomatik güncelleme** (Tauri updater, imzalı). Kullanıcıya sorarak.
- **Çökme kurtarma:** Oturum durumu ve taslaklar kalıcı.
- **Günlükler (log):** Yerel log dosyası, "Hata raporu için logları dışa aktar" butonu (otomatik gönderim yok).
- **Performans hedefi:** 100.000 notta arama < 100 ms, sıradaki kart < 20 ms, açılış < 2 sn.

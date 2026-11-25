# 16 — Anki uyumu: gerçek destelerle ne gidip geliyor (v1.4.2)

Kısa cevap: Anki'den gelen **notlar, alanlar, etiketler, kartların durumu, vadesi, aralığı, FSRS belleği,
tekrar geçmişi ve medya** bize gelir ve Anki'ye geri döner; sınanan paketlerde bunlar aynı kaldı. Aynı kalmayanlar
("Değişenler"), hiç taşınmayanlar ("Desteklenmeyenler") ve sınanmamış olanlar ("Sınanmayanlar") aşağıda, her biri
nedeniyle.

## Nasıl sınandı

Paketleri **gerçek Anki** üretir (resmî `anki` Python paketi, CI'daki sürüm `26.09.4`, kurulumu
`tools/anki/wide.py` başında). İki takım var:

| Takım | Ne içerir | Test |
|---|---|---|
| `real-*` (v1.0, `tools/anki/make_fixtures.py`) | Basic, ters kartlı, yazmalı, isteğe bağlı ters, cloze, ses, görüntü örtme, özel not tipi; askıya alınmış / gömülü / bayraklı kart; `.apkg` yeni (zstd) ve eski (schema 11), `.colpkg` | `fc-interop` `real_tests`, `fc-app` `real_anki_tests` |
| `wide-*` (v1.4.2, `tools/anki/wide.py`) | **FSRS açık** (`wide-fsrs.apkg`, yeni biçim, zamanlayıcı v3): not tipi başına iki şablon, üç boşluklu cloze, üç kat alt deste, hiyerarşik etiket (`lang::en::verb`), Türkçe adlı medya (`şarkı ğ.wav`, `çiçek.png`), askı / elle gömme / bayrak, iki kartlı **filtreli deste**, "vade tarihi ayarla" ve "unut" satırları. **FSRS kapalı** (`wide-sm2.apkg`, schema 11): SM-2 kolaylık katsayıları, unutma, yeniden öğrenme, kendi seçenek preset'i olan deste, "!"li ve "!"siz vade tarihi ayarlama (tekrar kartında ve yeni kartta) | `fc-app` `real_anki_wide_tests` |

Her paket için:

1. **Anki → biz:** uygulamanın kendi içe aktarma işiyle alınır, her kart Anki'nin kendi raporuyla (`wide-*.json`)
   karşılaştırılır: deste, durum, aralık, tekrar/unutma sayısı, bayrak, askı/gömme, FSRS kararlılık ve zorluk,
   kart başına geçmiş satırı (elle olanlar ayrıca), etiketler, medya adları ve baytları. FSRS belleği üç ondalığa
   kadar karşılaştırılır (Anki 32 bit kayan nokta tutar).
2. **Biz → Anki:** dışa aktardığımız `.apkg` gerçek Anki'de taze bir koleksiyona alınır, Anki'nin raporu
   ilk raporla karşılaştırılır (`wide.py compare`). Fark ya düzeltilir ya da aşağıdaki "değişenler" tablosunda
   nedeniyle yer alır (testteki `ACCEPTED` listesi; listede olmayan her fark testi düşürür).
   Bu adımda paketler test anında yeniden üretilir ve **büyük bir geçmiş** eklenir (400 kart × 8 cevap);
   repoya yalnız küçük paketler girer. Anki'nin "Veritabanını Denetle"si dışa aktardığımız pakette onaracak
   bir şey bulmamalıdır. `FC_ANKI_KEEP=<klasör>` paketleri ve raporları saklar (bir farka bakmak için).

```bash
python3 -m venv /tmp/ankivenv && /tmp/ankivenv/bin/pip install anki==26.09.4
FC_ANKI_PYTHON=/tmp/ankivenv/bin/python cargo test -p fc-app --lib real_anki -- --include-ignored --nocapture
```

## Aynen gidip gelenler

| Ne | Not |
|---|---|
| Not alanları (HTML dahil), not tipi, şablonlar | Anki'nin hazır not tipleri yerleşik tiplerimize, diğerleri aynı adla özel tipe |
| Etiketler, hiyerarşik etiketler (`a::b::c`) | |
| Desteler ve alt desteler | Tam yol (`Dil::İngilizce::Fiiller`) |
| Kart durumu: yeni / öğrenme / tekrar / yeniden öğrenme | |
| Vade ve aralık (gün) | Aralık = son **cevap** → vade (Anki'nin hesabı) |
| FSRS belleği (kararlılık, zorluk) ve son tekrar zamanı | |
| Tekrar ve unutma sayısı, bayrak, askıya alma | Anki tek bayrak tutar: birden çok bayrağımızdan en küçüğü gider |
| Tekrar geçmişi, **elle planlama satırları dahil** | Aşağıda |
| Medya (resim, ses), Türkçe/boşluklu dosya adları | Bayt bayt aynı, ad aynı |
| Görüntü örtme (Anki 23.10+) | Dikdörtgen olmayan şekiller dikdörtgene döner (v1.0'dan beri) |

### Elle planlama satırları (v1.4.2 değişikliği)

Anki "vade tarihi ayarla", "unut" ve FSRS "yeniden planla" işlemlerini tekrar geçmişine cevapsız bir satır
olarak yazar (tür 4/5, `ease` 0). Önceden bunları atlıyorduk; iki sonucu vardı: geçmiş eksik kalıyordu ve
bu satır kartın **son tekrarı** sayılıyordu, böylece vade tarihi değiştirilmiş bir kartın aralığı Anki'ye geri
dönünce kısalıyordu (son cevaptan değil, elle ayarlama gününden sayılıyordu).

Şimdi bu satırlar `mode = "manual"` olarak gelir:

- kartın geçmişinde "Elle planlandı" diye görünür, **son tekrar sayılmaz** (son tekrar = son cevap; hiç
  cevaplanmamış ama elle vadeye konmuş kartta aralığın başlangıcı o satırdır);
- **cevap değildir:** istatistik, hatırlama oranı, seri, FSRS optimizasyonu (`PRACTICE_MODE_SQL` /
  `is_practice_mode`), günlük sınırlar (`SCHEDULED_REVIEW_MODES`) ve "bugün çalışılanlar" onu saymaz. `rating`
  sütunu bu satırlarda yalnız yer tutar (şema "again…easy" ister; migration gerekmedi, `mode` serbest metin);
- "bilinen kelimeler" geçmişi (`words.history`) satırı **okur**, çünkü o geçmiş cevabı değil kartın aralığını
  izler: elle 30 güne konan kart o gün olgunlaşmış, "unut" denen kart o gün olgunluktan çıkmış sayılır
  (bugünkü sayımla tutarlı kalsın diye);
- türü Anki'nin kendi işaretinden okunur: satırdaki kolaylık katsayısı **0 ise "unut"** (kart yeniden yeni),
  değilse "vade tarihi ayarla" (kart tekrar durumunda). Anki'ye de öyle döner: tür 4, `ease` 0, katsayı yalnız
  "unut"ta 0. Bu ayrım önemli: Anki katsayısı 0 olan elle satırı sıfırlama sayar, FSRS hesabında ondan önceki
  geçmişi yok sayar ve "Veritabanını Denetle" kartın son tekrar zamanını siler.

### Aynı turda bulunan ve düzeltilen iki hata

| Hata | Etkisi | Düzeltme |
|---|---|---|
| Anki'ye giden aralık "en yakın güne" yuvarlanıyordu | Çalışma gününün **ikinci yarısında** cevaplanan her kartın aralığı Anki'ye 1 gün eksik gidiyordu (vade doğruydu) | Aralık gün sayısıdır: son cevabın günü → vade günü (yukarı yuvarlama). Birim testi: günün 1., 11., 13., 23. saatinde cevaplanan kartlar aynı aralığı verir |
| Yeniden öğrenmedeki kartın aralığı hep 1 gidiyordu | Anki o kartın "geri döneceği" aralığı saklar (FSRS'te unutma sonrası kararlılık; SM-2'de unutma sonrası aralık) | Unutma sonrası kararlılık yuvarlanıp gönderilir. 400 kartlık geçmişte Anki'nin sayısıyla birebir |

## Değişenler (bilerek)

Anki → biz → Anki turunda gerçek Anki'nin raporunda çıkan **bütün** farklar bunlardır (testteki `ACCEPTED`
listesi; listede olmayan fark testi düşürür).

| Ne | Ne olur | Neden |
|---|---|---|
| Not GUID'i | Dışa aktarmada not kimliğimizden yeni GUID üretilir | Anki GUID'ini saklamıyoruz. Sonuç: dışa aktardığımız paketi **aynı** Anki koleksiyonuna geri alırsan notlar güncellenmez, yanına kopya gelir. Taze koleksiyona sorun yok. Sonraki iş: GUID'i not kökeninde saklamak |
| Hazır not tipinin adı | `Basic` → `Klasik`, `Cloze` → `Boşluk doldurma` | Anki'nin hazır tipleri yerleşik tiplerimize oturur, geri dönerken yerleşik tipin adıyla gider. Alanlar ve şablonlar aynı |
| SM-2 kolaylık katsayısı (ease, ör. %235) | Anki'ye %250 döner (kartta ve geçmiş satırlarında) | Zamanlayıcımız FSRS; katsayıyı saklamıyoruz. FSRS açık Anki'de kullanılmaz; FSRS kapalı Anki'de sonraki aralıklar katsayıya göre değişir |
| Elle planlamadan sonra aralık | FSRS açıkken, hiç cevaplanmamış karta vade verilince Anki aralığı 0 bırakır: bizden vade günü kadar döner (ör. 0 → 3). FSRS kapalıyken "!"siz vade değişikliğinde Anki eski aralığı tutar: bizden son cevap → yeni vade döner (ör. 4 → 9) | Aralığı ayrı saklamıyoruz, vade − son cevap olarak türetiyoruz. Vade tarihi iki durumda da aynı |
| Filtreli deste | Kart kendi destesinde gelir, filtreli deste oluşmaz; dönüşte de yok | Filtreli deste bir sorgudur; bizde karşılığı kayıtlı sorgu / sorgudan çalışma |
| Gün aşan öğrenme adımı (Anki kuyruk 3) | Aynı anda vadeli gün içi öğrenme kartı (kuyruk 1) olarak döner | Öğrenme vadesini saniyeyle tutuyoruz; vade aynı |
| Kalan öğrenme adımı (`left`) | Dönüşte 0 | Adımlar bizde preset'ten gelir |
| Gömme türü (kardeş / elle) | Dönüşte elle gömme; yalnız ertesi güne kadar | Bizde tek gömme var |
| FSRS "yeniden planla" satırı (tür 5) | Dönüşte tür 4 (elle) | İkisini ayrı saklamıyoruz; ikisi de Anki'de cevap sayılmaz |
| Geçmiş satırındaki önceki aralık / öğrenme adımı süresi | Öğrenme adımlarında (negatif saniye) 0 döner | Geçmişte gün sayısı tutuyoruz |

## Desteklenmeyenler

| Ne | Durum |
|---|---|
| Deste seçenekleri (günlük sınırlar, adımlar, istenen hatırlama, FSRS ağırlıkları) | İçe alınmaz; desteler varsayılan preset'le gelir. Dışa aktarmada Anki varsayılanı |
| Anki eklentilerinin verisi, not tipi CSS'inin bize özel görünümü | Taşınmaz |
| Anki eşitleme (AnkiWeb) | Kapsam dışı |

## Sınanmayanlar

- **Günler arası gerçek geçmiş:** paketler tek oturumda üretilir (bütün cevaplar aynı gün, saniyeler içinde).
  Günlere yayılan aralık hesabı (günün hangi saatinde cevaplandığı, vadenin elle sonraki bir gün değiştirilmesi)
  birim testleriyle sınanır (`fc-interop` `anki::tests`), gerçek Anki'yle değil.
- **Görüntü örtme ve `.colpkg`:** `real-*` takımında; dışa aktardığımız paket gerçek Anki'de açılır ve sayılar
  karşılaştırılır (`check_import.py`), ama `wide.py compare`'in kart kart karşılaştırmasından geçmez.
- **Gün aşan öğrenme adımı (kuyruk 3), kardeş gömme (kuyruk −2), FSRS "yeniden planla" (tür 5):** kod yolu var,
  gerçek Anki paketi yok (paketi üreten betik bu durumları oluşturmuyor).
- **Kullanıcının kendi koleksiyonu, eklentiler, Anki'nin eski sürümleri:** yalnız `anki` 26.09.4 ile üretilen
  paketler sınandı (eski `.apkg` biçimi de bu sürümün "eski biçimle dışa aktar" seçeneğiyle üretildi).
- **Dışa aktardığımız paketi içinde aynı notlar bulunan bir Anki koleksiyonuna geri almak:** GUID değiştiği için
  kopya üretir (yukarıda); sınanan yol taze koleksiyon.


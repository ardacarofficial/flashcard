# 16 — Anki uyumu: gerçek destelerle ne gidip geliyor (v1.4.2)

Kısa cevap: Anki'den gelen **notlar, alanlar, etiketler, kartların durumu, vadesi, aralığı, FSRS belleği,
tekrar geçmişi ve medya** bize aynen gelir ve Anki'ye aynen döner. Değişenler ve desteklenmeyenler aşağıdaki
tablolarda, her biri nedeniyle.

## Nasıl sınandı

Paketleri **gerçek Anki** üretir (resmî `anki` Python paketi, CI'daki sürüm `26.09.4`, kurulumu
`tools/anki/wide.py` başında). İki takım var:

| Takım | Ne içerir | Test |
|---|---|---|
| `real-*` (v1.0, `tools/anki/make_fixtures.py`) | Basic, ters kartlı, yazmalı, isteğe bağlı ters, cloze, ses, görüntü örtme, özel not tipi; askıya alınmış / gömülü / bayraklı kart; `.apkg` yeni (zstd) ve eski (schema 11), `.colpkg` | `fc-interop` `real_tests`, `fc-app` `real_anki_tests` |
| `wide-*` (v1.4.2, `tools/anki/wide.py`) | **FSRS açık** (`wide-fsrs.apkg`, yeni biçim, zamanlayıcı v3): not tipi başına iki şablon, üç boşluklu cloze, üç kat alt deste, hiyerarşik etiket (`lang::en::verb`), Türkçe adlı medya (`şarkı ğ.wav`, `çiçek.png`), askı / elle gömme / bayrak, iki kartlı **filtreli deste**, "vade tarihi ayarla" ve "unut" satırları. **FSRS kapalı** (`wide-sm2.apkg`, schema 11): SM-2 kolaylık katsayıları, unutma, yeniden öğrenme, kendi seçenek preset'i olan deste | `fc-app` `real_anki_wide_tests` |

Her paket için:

1. **Anki → biz:** uygulamanın kendi içe aktarma işiyle alınır, her kart Anki'nin kendi raporuyla (`wide-*.json`)
   karşılaştırılır: deste, durum, aralık, tekrar/unutma sayısı, bayrak, askı/gömme, FSRS kararlılık ve zorluk,
   kart başına geçmiş satırı (elle olanlar ayrıca), etiketler, medya adları ve baytları.
2. **Biz → Anki:** dışa aktardığımız `.apkg` gerçek Anki'de taze bir koleksiyona alınır, Anki'nin raporu
   ilk raporla karşılaştırılır (`wide.py compare`). Fark ya düzeltilir ya da aşağıdaki "değişenler" tablosunda
   nedeniyle yer alır (testteki `ACCEPTED` listesi; listede olmayan her fark testi düşürür).
   Bu adımda paketler test anında yeniden üretilir ve **büyük bir geçmiş** eklenir (400 kart × 8 cevap);
   repoya yalnız küçük paketler girer.

```bash
python3 -m venv /tmp/ankivenv && /tmp/ankivenv/bin/pip install anki==26.09.4
FC_ANKI_PYTHON=/tmp/ankivenv/bin/python cargo test -p fc-app wide_packages_round_trip_through_real_anki -- --ignored --nocapture
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

Şimdi bu satırlar `mode = "manual"` olarak gelir: kartın geçmişinde "Elle planlandı" diye görünür, son tekrar
olarak sayılmaz, istatistik, FSRS optimizasyonu, günlük sınırlar ve "bugün çalışılanlar" onu cevap saymaz
(`fc_storage::repo::is_practice_mode`), Anki'ye yine tür 4, `ease` 0 satırı olarak döner. `rating` sütunu bu
satırlarda yalnız yer tutar (şema "again…easy" ister; migration gerekmedi).

## Değişenler (bilerek)

| Ne | Ne olur | Neden |
|---|---|---|
| Not GUID'i | Dışa aktarmada not kimliğimizden yeni GUID üretilir | Anki GUID'ini saklamıyoruz. Sonuç: dışa aktardığımız paketi **aynı** Anki koleksiyonuna geri alırsan notlar güncellenmez, yanına kopya gelir. Taze koleksiyona sorun yok. Sonraki iş: GUID'i not kökeninde saklamak |
| SM-2 kolaylık katsayısı (ease, ör. %235) | Anki'ye %250 döner | Zamanlayıcımız FSRS; katsayıyı saklamıyoruz. FSRS açık Anki'de kullanılmaz; FSRS kapalı Anki'de sonraki aralıklar katsayıya göre değişir |
| Filtreli deste | Kart kendi destesinde gelir, filtreli deste oluşmaz; dönüşte de yok | Filtreli deste bir sorgudur; bizde karşılığı kayıtlı sorgu / sorgudan çalışma |
| Gün aşan öğrenme adımı (Anki kuyruk 3) | Aynı anda vadeli gün içi öğrenme kartı (kuyruk 1) olarak döner | Öğrenme vadesini saniyeyle tutuyoruz; vade aynı |
| Kalan öğrenme adımı (`left`) | Dönüşte 0 | Adımlar bizde preset'ten gelir |
| Gömme türü (kardeş / elle) | Dönüşte elle gömme; yalnız ertesi güne kadar | Bizde tek gömme var |
| Geçmişte cevap süresi dışı alanlar (satırdaki ease katsayısı, filtreli deste türü 3) | Katsayı 2500, tür 3 → 1 | Aynı neden: SM-2 alanları |

## Desteklenmeyenler

| Ne | Durum |
|---|---|
| Deste seçenekleri (günlük sınırlar, adımlar, istenen hatırlama, FSRS ağırlıkları) | İçe alınmaz; desteler varsayılan preset'le gelir. Dışa aktarmada Anki varsayılanı |
| Anki eklentilerinin verisi, not tipi CSS'inin bize özel görünümü | Taşınmaz |
| Anki eşitleme (AnkiWeb) | Kapsam dışı |

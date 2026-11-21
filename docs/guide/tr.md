# Flashcard kullanıcı rehberi

Bu rehber Flashcard masaüstü uygulamasını günlük kullanım için anlatır. Uygulama bilgisayarında çalışır, verin bilgisayarında kalır ve telemetri göndermez.

Rehberde tırnak içinde geçen adlar (ör. **Desteler**, **Çalış**) uygulamadaki menü ve düğme adlarıdır. Uygulamayı İngilizce kullanıyorsan [İngilizce rehbere](en.md) bak.

## İçindekiler

1. [Kurulum](#kurulum)
2. [İlk açılış](#ilk-açılış)
3. [Desteler](#desteler)
4. [Sınıflandırmalar (seviye, sınav)](#sınıflandırmalar-seviye-sınav)
5. [Kart ekleme ve not türleri](#kart-ekleme-ve-not-türleri)
6. [Çalışma](#çalışma)
7. [Sorgu dili](#sorgu-dili)
8. [İçe ve dışa aktarma](#i̇çe-ve-dışa-aktarma)
9. [Eklentiler](#eklentiler)
10. [Yapay zeka](#yapay-zeka)
11. [MCP](#mcp)
12. [Tarayıcı eklentisi](#tarayıcı-eklentisi)
13. [Profiller ve uygulama kilidi](#profiller-ve-uygulama-kilidi)
14. [Hatırlatıcılar ve tepsi](#hatırlatıcılar-ve-tepsi)
15. [Çökme kurtarma ve günlükler](#çökme-kurtarma-ve-günlükler)
16. [Veri klasörü](#veri-klasörü)
17. [Yedek ve geri yükleme](#yedek-ve-geri-yükleme)
18. [Sorun giderme](#sorun-giderme)

---

## Kurulum

Şimdilik öncelikli platform Windows. macOS ve Linux sürümleri sonra gelecek. Projenin yayın (releases) sayfasında iki kurulum dosyası var:

| Dosya | Ne için |
|---|---|
| `Flashcard_<sürüm>_x64-setup.exe` | Önerilen. Yalnız senin kullanıcın için, yönetici izni istemeden `%LOCALAPPDATA%\Flashcard\` altına kurar. Kurulum Türkçe ya da İngilizce. |
| `Flashcard_<sürüm>_x64_en-US.msi` | İsteğe bağlı, kurumsal dağıtım için. Makine başına kurar, yönetici izni ister, `C:\Program Files\Flashcard\` altına kurar. |

1. `-setup.exe` dosyasını indir ve çalıştır.
2. Windows SmartScreen "Windows kişisel bilgisayarınızı korudu" der: **Ek bilgi** → **Yine de çalıştır**'a tıkla. Kurulum dosyaları kod imzalı olmadığı için bu uyarı beklenen bir durumdur.
3. WebView2 yoksa kurulum onu kendisi indirip kurar.
4. Kurulum bitince Flashcard'ı Başlat menüsünden aç.

Kurulumda yalnız masaüstü uygulaması var. `fc` komut satırı aracı gelmez (bkz. [MCP](#mcp)).

**Güncellemeler.** Uygulama güncelleme sunucusuna yalnız sen istediğinde bağlanır: **Günlükler ve güncellemeler** → **Güncellemeleri denetle**. Yeni sürüm çıktıysa **İndir**'e bas ("İndiriliyor ve doğrulanıyor…"), sonra **Kur ve yeniden başlat**. Uygulama kapanır, güncellemeyi soru sormadan kurar ve yeniden açılır; verin yerinde kalır. Her güncelleme projenin anahtarıyla imzalıdır; imzası tutmayan indirme reddedilir, hiçbir şey kurulmaz. Sen tıklamadan hiçbir şey kurulmaz. Her açılışta denetlemek için `updates.check_on_start` ayarını aç (varsayılan kapalı): `Ctrl+K` → **Ayar değiştir** → `{"scope": {"kind": "app"}, "key": "updates.check_on_start", "value": true}`. Bulunan güncelleme o zaman "… sürümü çıktı." şeridinde **Güncellemeye bak** düğmesiyle görünür.

**Kaldırma:** Windows *Ayarlar* → *Uygulamalar* → Flashcard → *Kaldır*. Veri klasörün (bkz. [Veri klasörü](#veri-klasörü)) kalır; kaldırıcıdaki uygulama verilerini silme kutusunu işaretlersen tüm profiller, ayarlar ve günlükler kalıcı olarak silinir.

## İlk açılış

Koleksiyon boşsa uygulama ilk açılış rehberiyle (**Flashcard'a hoş geldin**) açılır:

1. "Ne öğrenmek istiyorsun?": bir konu seç (**İngilizce**, **Japonca**, **Çince**, **Başka bir dil**, **YKS (TYT / AYT / YDT)**, **Başka bir sınav**, **Okul (1–12. sınıf)**, **Genel kültür**). Her konunun yanında kuracağı sınıflandırma yazar, ör. **Sınıflandırma: CEFR seviyeleri (A1–C2)**. **İleri**.
2. **Deste adı**'na bak, istediğin **Başlangıç desteleri**'ni işaretle. **Kur**.

Rehber o sınıflandırmayla bir deste, uygun bir not türü (dillerde **Kelime kartı**, diğerlerinde **Klasik**), diller ("Dil öğrenme", günde 15 yeni kart) ve sınavlar ("Sınav hazırlığı", günde 30 yeni kart) için bir deste ayarı şablonu ve seçtiğin başlangıç destelerini kurar. Hepsi tek bir geri alma adımıdır: `Ctrl+Z` tüm kurulumu kaldırır. **Geç** rehberi kapatır; bir daha çıkmaz. İçinde kart olan bir koleksiyonda rehber hiç çıkmaz.

**Başlangıç desteleri** (menü): bu uygulama için yazılmış küçük desteler, Türkçe ya da İngilizce (**Kartların dili**): temel İngilizce kelimeler (A1–A2), YKS TYT matematik temelleri, dünya başkentleri ve bir uygulama rehberi. **Ekle** desteyi koleksiyonuna değiştirip silebileceğin sıradan bir deste olarak ekler.

Solda menü, ortada seçtiğin ekran durur. Sık kullanacağın menüler:

| Menü | Ne işe yarar |
|---|---|
| **Desteler** | Deste ağacı: oluştur, adlandır, sil |
| **Ekle** | Yeni not (ve kartları) ekle |
| **Notlar** | Seçili destenin notları |
| **Çalış** | Aralıklı tekrar oturumu |
| **Öğren**, **Test** | Öğren modu ve deneme testi |
| **Göz at** | Sorgu diliyle arama ve toplu işlemler |
| **İçe / dışa aktar** | Dosya alıp verme, yedek, yazdırma |
| **Profiller ve kilit** | Ayrı koleksiyonlar, PIN/parola kilidi |
| **Hatırlatıcılar** | Günlük bildirim, tepsi |
| **Günlükler ve güncellemeler** | Sürüm, güncellemeler, günlükler, çökme raporları |
| **Ayarlar** | Dil ve tema |

Menüde ayrıca **Deste ayarları**, **Not türleri**, **İstatistikler**, **Planlama**, **Hedefler**, **Okuma**, **Gelen kutusu**, **Otomasyon**, **Yapay zeka**, **MCP**, **Eklentiler**, **Başlangıç desteleri**, **Bakım** ve **Kısayollar** var (modülü açılınca **Yerel API** de). Kapalı bir modülün menüsü görünmez.

**Ayarlar:** **Ayarlar** ekranında yalnız **Dil** (Türkçe / English), renk düzeni (**Sistem**, **Açık**, **Koyu**), **Tema** ön ayarı (**Varsayılan**, **Kâğıt**, **Adaçayı**, **Arduvaz**, **Yüksek kontrast**) ve isteğe bağlı özel **Vurgu rengi** var. Bazı modüllerin seçenekleri kendi ekranındadır: **Hatırlatıcılar**, **Profiller ve kilit**, **MCP**'deki yazma anahtarı.

**Komut paleti:** `Ctrl+K` ile açılır. Bir komut ya da ekran adı yaz, **Çalıştır**'a bas. Bazı komutlar **Parametreler (JSON)** kutusuna değer ister. Bu sürümde modülleri açıp kapatmanın ve diğer tüm ayarları değiştirmenin yolu burası:

- **Modülü aç/kapat**: `{"id": "local_api", "enabled": true}`
- **Ayar değiştir**: `{"scope": {"kind": "app"}, "key": "timed.seconds", "value": 20}`

**Genel kısayollar** (hepsi **Kısayollar** ekranından değiştirilebilir):

| Tuş | İşlem |
|---|---|
| `Ctrl+K` | Komut paletini aç |
| `Ctrl+F` | Kart ara |
| `Ctrl+N` | Yeni not |
| `Ctrl+Z` | Geri al |
| `Ctrl+Shift+Z` | Yinele |

Neredeyse her değişiklik geri alınabilir: not ekleme, silme, puanlama, toplu işlem, içe aktarma.

## Desteler

**Desteler** ekranında:

- **Yeni deste** üst düzey bir deste açar.
- **Yeni alt deste** seçili destenin altına deste ekler. Örnek ağaç: `Dil` → `İngilizce` → `A1`.
- **Yeniden adlandır** ve **Sil** ile desteyi değiştirirsin. Silinen deste alt desteleri, notları ve kartlarıyla çöp kutusuna gider.

Çöpe giden desteyi ve notları **Bakım** → **Çöp kutusu** → **Geri getir** ile kurtarırsın. Çöpteki öğeler varsayılan olarak 30 gün sonra kalıcı olarak silinir.

Sorgularda deste yolu `::` ile yazılır: `deck:"Dil::İngilizce"`. Bir deste seçildiğinde alt desteleri de dahil olur.

**Deste ayarları:** Günlük yeni kart, günlük en çok tekrar, öğrenme adımları, **Hedef hatırlama oranı**, **En uzun aralık (gün)** gibi değerler bir şablonda toplanır. **Yeni şablon** ile oluştur, **Destede kullan** ile desteye ata. Deste kendi şablonunu, yoksa üst destesininkini, o da yoksa varsayılanları kullanır.

## Sınıflandırmalar (seviye, sınav)

Etiketler serbesttir. Sınıflandırma ise yapılı bir alandır: adı ve seçenekleri vardır, bir desteye bağlanır ve tüm alt destelere geçer.

Örnekler:

- `İngilizce` destesinde **Seviye** = A1 · A2 · B1 · B2 · C1 · C2
- `YKS` destesinde **Sınav** = TYT · AYT · YDT
- `Japonca` destesinde **JLPT** = N5 … N1

Hazır şablonlar:

| Şablon kimliği | Ad | Seçenekler |
|---|---|---|
| `cefr` | Seviye | A1–C2 |
| `yks` | Sınav | TYT, AYT, YDT |
| `jlpt` | JLPT | N5–N1 |
| `hsk` | HSK | HSK 1–HSK 6 |
| `school_grade` | Sınıf | 1.–12. sınıf |
| `difficulty` | Zorluk | Kolay, Orta, Zor |

Şablondan kopyalanan sınıflandırma tamamen senindir: seçenek ekleyebilir, yeniden adlandırabilir, sıralayabilir, birleştirebilirsin. Ad değişse de kayıtlı sorgular çalışmaya devam eder.

**Tanımlama.** Bu sürümde sınıflandırmayı tanımlayan ayrı bir ekran yok. İlk açılış rehberi kendi destesine bir tane kurar. Bunun dışında:

- Komut paleti (`Ctrl+K`) → **Şablondan sınıflandırma** → `{"template_id": "cefr"}`. Bu, tüm koleksiyon için tanımlar.
- Komut satırı aracı `fc` ile bir desteye (kurulumda gelmez, bkz. [MCP](#mcp)): `fc attribute apply-template cefr --deck Dil --locale tr`.

**Kullanım.**

- Not eklerken destenin sınıflandırmaları alanların yanında çıkar. Zorunlu olanlar **Zorunlu** diye işaretlidir.
- Sorguda: `seviye:B1`, `seviye>=B1` (sıralı seçeneklerde), `sınav:TYT`. Anahtar büyük/küçük harf ve aksan duyarsızdır: `sinav:TYT` de çalışır.
- **Göz at** → seçimi değiştir → **Sınıflandırma** ile toplu atama.
- İçe aktarmada bir sütunu sınıflandırmaya eşleyebilirsin (aşağıda).
- **İstatistikler**'de **Sınıflandırmaya göre** kırılım.
- Özel oturum, öğren modu ve deneme testi bir sınıflandırmayı hedefleyebilir: `seviye:A2`, `sınav:TYT`.

## Kart ekleme ve not türleri

Flashcard **not** ile **kartı** ayırır. Not, içeriktir (alanlar). Kartlar, notun not türüne göre ondan üretilir. Örneğin **Klasik + ters** bir not iki kart üretir.

**Not ekleme:**

1. Soldan bir deste seç.
2. **Ekle**'ye git ve not türünü seç.
3. Alanları doldur, varsa sınıflandırmaları ve **Etiketler**'i gir.
4. **Ekle**'ye bas. "Not eklendi." mesajı çıkar.

Alanlarda Markdown, `$formül$` (KaTeX) ve `[[bağlantı]]` kullanabilirsin. Resim, ses ya da videoyu sürükleyip bırak veya yapıştır. Aynı dosyayı iki kez eklersen diskte tek kopya tutulur.

**Hazır not türleri:**

| Not türü | Kısaca |
|---|---|
| Klasik | Ön yüz → arka yüz, kendin puanla |
| Klasik + ters | İki yönde iki kart |
| İsteğe bağlı ters | Ters kart yalnız istersen |
| Yazarak cevaplama | Cevabı yazarsın, uygulama kontrol eder |
| Boşluk doldurma | `{{c1::Ankara}} başkenttir`; her numara ayrı kart, ipucu `{{c1::Ankara::şehir}}` |
| Çoktan seçmeli | Seçeneklerden birini seç |
| Resim üzerinde gizleme | Resmin üstüne maske çiz; her maske bir kart |
| Resimli quiz | Resim sorulur |
| Sesli kart | Ses dinletilir |
| Kelime kartı | Kelime, anlam, örnek cümle |
| Cümle kartı | Cümle, çeviri, hedef kelime |

**Not türleri** ekranında **Yeni not türü** açabilir, hazır bir türü **Kopyala** ile çoğaltabilirsin. Alan ekle (**Alan ekle**), kart şablonu ekle (**Şablon ekle**), **Görünüm (CSS)** düzenle. Şablon dilinde `{{alan}}`, `{{#alan}}…{{/alan}}`, `{{cloze:alan}}`, `{{type:alan}}`, `{{hint:alan}}`, `{{tts:en:alan}}` ve `{{FrontSide}}` vardır. Alan eklemek ya da çıkarmak mevcut kartları ve geçmişi bozmaz.

**Çalışırken hızlı kart:** Çalışma ekranında `N` tuşu **Metinden yeni kartlar** penceresini açar. Her satır bir kart olur: `porsuk :: badger`. Oturum bozulmaz, tüm kartlar tek adımda geri alınır.

## Çalışma

**Çalış** ekranında soldan bir deste seç (seçmezsen **Tüm desteler**) ve **Çalışmaya başla**'ya bas ya da `Enter`'a bas. Uygulama FSRS zamanlayıcısını kullanır.

**Puanlama.** Cevabı gördükten sonra dört düğmeden birini seç. Her düğmenin üstünde kartın bir sonraki görüleceği aralık yazar (`10dk`, `3g` gibi).

| Düğme | Tuş | Anlamı |
|---|---|---|
| **Tekrar** | `1` | Bilemedim |
| **Zor** | `2` | Zorlandım |
| **İyi** | `3` | Bildim |
| **Kolay** | `4` | Çok kolaydı |

**Çalışma kısayolları:**

| Tuş | İşlem |
|---|---|
| `Boşluk` veya `Enter` | Cevabı göster; cevapta **İyi** puanla |
| `1`–`4` | Puanla |
| `1`–`9` | Çoktan seçmeli kartta seçenek seç |
| `H` | İpucu |
| `S` | Askıya al |
| `B` | Göm (yarına kadar) |
| `Z` | Son cevabı geri al |
| `Ctrl+1`–`Ctrl+7` | Bayrak aç/kapa |
| `N` | Metinden yeni kartlar |
| `F` | Odak modu |
| `Esc` | Oturumu bitir |
| `R` (özet ekranında) | Yanlışları tekrar çalış |

`Boşluk`, `1`–`4`, `H`, `S`, `B`, `Z` ve `Esc` **Kısayollar** ekranından değiştirilebilir.

**Başlamadan önceki seçenekler:**

- **Deneme modu (takvimi değiştirmez)**: kartların planı değişmez.
- **Özel oturum**: **Sorgu** kutusuna `seviye:A2 is:due` gibi bir sorgu yaz ya da bir **Kayıtlı sorgu** seç.
- **Bugünün kartlarını ertele**: gün sayısını gir, **Ertele**'ye bas.

Oturum bitince **Oturum bitti** ekranı çalışılan kart sayısını, doğruluğu, süreyi ve yanlışları gösterir.

### Modlar

| Mod | Nerede | Ne yapar |
|---|---|---|
| Öğren | **Öğren** menüsü | Her kart önce çoktan seçmeli, sonra yazarak sorulur. İki adımda da doğru bilince kart öğrenilmiş olur. Yanlış yazılı cevap çoktan seçmeliye döner. |
| Test | **Test** menüsü | Seçilen kartlardan karışık sorulu deneme testi. Takvimini değiştirmez. |
| Güven | Çalış → **Puanlama** → **Güven 1–5** | Dört düğme yerine "Ne kadar emindin?" sorusuna 1–5 cevap verirsin. |
| Zamanlı | Çalış → **Süre sınırı** | Her karta süre sınırı. |
| Odak | Çalış → **Odak** düğmesi veya `F` | Tam ekran, dikkat dağıtıcılar gizli. |

**Öğren modu:** Deste seç, istersen **Sorgu**'ya `seviye:A2` yaz, **Öğrenmeye başla**'ya bas. Tuşlar: `1`–`4` seç, `Enter` kontrol et / devam, `Z` geri al, `Esc` bitir.

**Deneme testi:** **Soru sayısı** ve **Soru tipleri**'ni (**Çoktan seçmeli**, **Yazılı**, **Doğru / yanlış**) seç, istersen **Sorgu**'ya `sınav:TYT` yaz, **Test oluştur**'a bas. Kâğıdı **Yazdır** ya da **Cevap anahtarını yazdır** ile basabilirsin. **Teslim et** ile puanlanır; "az kaldı" yarım puan sayılır. Teslim geri alınabilir.

**Güven bazlı puanlama:** Varsayılan eşleme 1–2 = Tekrar, 3 = Zor, 4 = İyi, 5 = Kolay. Cevap tarafında `1`–`5` tuşları da çalışır. Eşlemeyi `confidence.map.1` … `confidence.map.5` ayarlarıyla değiştirirsin.

**Zamanlı mod:** **Saniye** kutusuna süreyi yaz. **Süre dolunca** ya **Tekrar say** ya da **Geç** seçilir. Sayaç kart ekrana gelince başlar, kart başınadır ve yeniden başlatılamaz. Süre dolduktan sonra verilen cevap kabul edilmez.

**Odak modu:** `focus.fullscreen`, `focus.hide_counts` ve `focus.auto` ayarları tam ekranı, sayaçları gizlemeyi ve her oturumun odakta başlamasını yönetir.

Pratik amaçlı modlar (deneme modu, test) istatistiklerde sayılmaz. Her mod ayrı bir modüldür ve kapatılabilir.

**Diğer çalışma ayarları** (ayar anahtarıyla): `study.scheduler` (FSRS veya SM-2), **Cevap kontrolü** (`study.answer_checker`: birebir, büyük/küçük harf ve noktalama yok sayılsın, yazım hatalarına izin ver, birden çok cevap), **ı/i, ş/s… eşit sayılsın** (`study.fold_turkish`), **İpucu türü** (`study.hint_strategy`).

## Sorgu dili

Sorgu dili **Göz at**, özel oturum, öğren/test, otomasyon ve MCP'de aynıdır. Boşlukla ayrılan terimler VE ile birleşir.

| Yazım | Anlamı |
|---|---|
| `kılıç` | Not metninde kelime (önek eşleşir) |
| `"kılıç kalkan"` | Tam ifade |
| `deck:İngilizce` · `deste:İngilizce` | Deste (alt desteler dahil); `deck:"Dil::İngilizce"` |
| `tag:fiil` · `etiket:fiil` | Etiket (alt etiketler dahil); `tag:none` = etiketsiz |
| `notetype:"Kelime kartı"` · `note:Klasik` | Not türü adı |
| `type:cloze` | Kart türü: `classic`, `typing`, `cloze`, `multiple_choice`, `image_occlusion` |
| `card:` | Kart şablonu |
| `is:new` `is:learn` `is:review` `is:relearn` | Kart durumu (`is:yeni` de olur) |
| `is:due` `is:suspended` `is:buried` `is:flagged` | Vadesi gelmiş, askıda, gömülü, bayraklı |
| `flag:red` · `flag:kirmizi` · `flag:1` | Bayrak: 1–7 ya da renk adı (TR/EN) ya da kendi bayrak adın; `flag:0` / `flag:none` bayraksız, `flag:*` herhangi |
| `stability>30` `difficulty>7` `lapses>=5` `reps<3` | Sayısal; `=` `!=` `<` `<=` `>` `>=` |
| `retrievability<0.8` | Şu anki hatırlama olasılığı (0–1) |
| `added:7` `edited:7` `reviewed:1` | Son N gün içinde eklenen / düzenlenen / tekrar edilen |
| `due:3` | N gün içinde vadesi gelecek |
| `seviye:B1` · `seviye>=B1` · `sınav:TYT` | Sınıflandırma (anahtar ya da ad); `seviye:*` = değeri olan |
| `kelime:app*` | Alan araması; `*` joker |
| `re:^ab` · `kelime:re:^ab` | Düzenli ifade (tüm alanlar / tek alan) |
| `nid:` `cid:` | Not / kart kimlikleri (virgülle) |
| `-tag:kolay` · `NOT tag:kolay` | Değil |
| `a OR b` · `(seviye:B1 OR seviye:B2)` | Veya, gruplama |
| `seviye!=B1` | `-seviye:B1` ile aynı (değeri olmayanlar da gelir) |

Hatalı bir sorguda uygulama hatanın yerini gösterir. **Göz at** ekranında sorguyu **Sorguyu kaydet** ile adlandırırsın; kayıtlı sorgu, çalışma ekranında **Kayıtlı sorgu** olarak da seçilebilir.

## İçe ve dışa aktarma

Hepsi **İçe / dışa aktar** ekranındadır. Dört sekme var: **İçe aktar**, **Dışa aktar**, **Yedek**, **Yazdır**. Önce soldan bir deste seç.

### Metin ve CSV

1. **İçe aktar** → **Kaynak**: **Dosya** ya da **Metin yapıştır**.
2. **Biçim**: **Otomatik** bırak ya da **Metin / CSV** seç.
3. Uygulama ayırıcıları kendisi bulur ve "Otomatik algılandı (%…)" yazar. Şunların hepsi tanınır:

```
dog, köpek
cat: kedi
Arda	porsuk
"Arda, Ali": porsuk
```

4. **Önizleme** tablosuna bak. Sütunlar yanlış bölündüyse düzelt:
   - **Terim ile tanım arası** kutusuna doğru ayırıcıyı yaz (`,` `;` `:` ` - ` `~` ` / ` gibi; sekme için `\t`; boşluklarla hizalanmış sütunlar için iki boşluk).
   - **Kartlar arası**: **Yeni satır**, **Boş satır** (tanım birden çok satırsa) ya da `;`.
   - Başlık satırı varsa **İlk satır başlık**'ı işaretle.
5. Her sütunun üstünden ne olacağını seç: bir alan, **Etiketler**, bir sınıflandırma ya da **Aktarma**.
6. **Not zaten varsa**: **Atla**, **Güncelle** ya da **Yine de ekle**.
7. Sınıflandırma sütununda bilinmeyen değerler varsa **Eksik sınıflandırma seçeneklerini oluştur**'u işaretle.
8. "… destesine aktar" düğmesine bas.

Dosya kodlaması kendiliğinden çözülür (UTF-8, UTF-16, Türkçe Windows-1254). Anki'nin `#separator:` başlık satırı da okunur. Otomatik algılama her dosyada doğru olmayabilir; önizleme her zaman elle düzeltmeye izin verir.

### Quizlet

Quizlet'in dışa aktarma metnini yapıştır, **Biçim** olarak **Quizlet** seç. Varsayılan: terim ile tanım arası sekme, kartlar arası yeni satır. Quizlet'te özel ayırıcı seçtiysen önizlemede aynısını yaz.

### Anki (.apkg, .colpkg)

- **İçe aktarma:** **Anki (.apkg, .colpkg)** dosyasını seç. Önizleme not, kart, medya ve deste sayısını gösterir. **Tekrar geçmişini ve zamanlamayı koru** işaretliyse kartlar Anki'deki planıyla gelir. Boşluk doldurma, resimli ve sesli kartlar desteklenir; eski ve yeni Anki paket düzenleri okunur.
- **Dışa aktarma:** **Dışa aktar** → **Anki destesi (.apkg)** ya da **Anki koleksiyonu (.colpkg)**. **Tekrar geçmişi ve zamanlamayı ekle** ile plan da gider. Paket, Anki 2.0'dan beri tüm sürümlerin okuduğu eski şemada yazılır. Dışa aktarılan paketler gerçek Anki ile (resmî `anki` kütüphanesi, 26.09) açılarak test edilir: notlar, kartlar, zamanlama, geçmiş, medya ve resim maskeleri (image occlusion) aynen gelir.

### JSON

İnsan tarafından düzenlenebilir not biçimi:

```json
{ "format": "fc-json", "version": 1,
  "notes": [ { "deck": "English::A1", "notetype": "Classic",
               "fields": { "front": "dog", "back": "köpek" },
               "tags": ["animals"], "attributes": { "level": "A1" } } ] }
```

`notetype` boşsa Klasik kullanılır. Alanlar ve sınıflandırmalar anahtarla ya da adla yazılır. Dışa aktarmada **JSON (.json)** seç.

### Deste paketi (.fcpack)

Bir desteyi başka bir Flashcard'a taşımanın en kayıpsız yolu: notlar, kartlar, medya, sınıflandırmalar ve (istersen) geçmiş birlikte gider. Paket not kimliklerini ve sınıflandırmaları olduğu gibi korur.

### Dışa aktarma ve yazdırma

- **Dışa aktar** seçili desteyi alt desteleriyle verir; deste seçili değilse tüm koleksiyonu. Biçimler: **Deste paketi (.fcpack)**, **Anki destesi (.apkg)**, **Anki koleksiyonu (.colpkg)**, **Metin (.txt)**, **JSON (.json)**.
- **Yazdır** sekmesinde **Düzen** (**Kart, sayfada 2 × 4**, **Kart, sayfada 3 × 5**, **Liste**) seç, **Hazırla**, sonra **Yazdır / PDF kaydet**. Arka yüzler çift taraflı baskı için aynalanır.

Uzun işler **İptal** edilebilir; iptal edilince hiçbir şey değişmez. Biten içe aktarma tek adımda geri alınır (**İçe aktarmayı geri al** ya da `Ctrl+Z`).

## Eklentiler

Eklentiler korumalı alanda (WASM) çalışır ve yalnız izin verdiğin şeyleri yapabilir. Yeni kart türü, çalışma modu, ses sağlayıcısı ya da not eklenirken alan dolduran kanca ekleyebilirler.

**Kurulum:**

1. **Eklentiler** → **Dosyadan kur…** → `.fcplugin` dosyasını seç.
2. Pencerede eklentinin adı, sürümü, ekledikleri ve istediği izinler görünür.
3. Vermek istemediğin izinlerin işaretini kaldır. "ayrıca sorulur" yazanlar hassas izinlerdir.
4. **Kur**'a bas.

Eklenti kartında **Aç** / **Kapat**, **İzinler**, **Ayarlar**, **Günlük** ve **Kaldır** var. Kaldırırken **Kaydettiği verileri de sil** seçilebilir; eklentinin oluşturduğu notlar kalır.

Bir eklenti donarsa ya da çökerse uygulama etkilenmez: çağrı süre, iş ve bellek sınırlarıyla kesilir. Art arda hata veren eklenti kendiliğinden kapanır ("Hatalar yüzünden kapatıldı"); **Aç** ile yeniden açarsın.

Eklenti mağazası henüz yok; eklentiyi dosyadan kurarsın.

## Yapay zeka

**Yapay zeka** ekranında üç sekme var: **Kart üret**, **Sağlayıcılar**, **Kullanım**.

**Sağlayıcı ekle:** **Sağlayıcılar** → **Sağlayıcı ekle**. Türler: **Anthropic Claude**, **OpenAI**, **OpenAI uyumlu**, **Ollama (yerel)**. API anahtarını gir ve **Anahtarı kaydet**'e bas. Anahtar yalnız işletim sisteminin anahtar zincirinde saklanır, veritabanına ya da yedeğe girmez. Anahtarlar profile aittir: her profilin kendi anahtarları olur. **Özellik başına model** ile örneğin kart üretimi için güçlü, kontrol için hızlı bir model seçebilirsin.

**Kart üret:**

1. Soldan bir deste seç.
2. **Kaynak**: **Metin**, **PDF** (sayfa aralığıyla) ya da **Web sayfası**.
3. **Not türü**, **Kaç tane** ve istersen **Ek talimat** (ör. "fiillere odaklan") gir.
4. **Önizle**. **Göndermeden önce** penceresi sağlayıcıyı, modeli, tahmini token ve maliyeti gösterir. **Gönderilecek her şeyi göster** ile tam metni görebilirsin.
5. **Gönder**. Gelen önerilerden istediklerini **Ekle** ile al.

Destede bir seviye sınıflandırması varsa üretim seviyeye uygun yapılır.

**Çalışırken:** Çalışma ekranında **AI ipucu**, **Açıkla** ve **AI ile kontrol et** düğmeleri çıkar.

**Maliyet:** **Kullanım** sekmesi bu ayın ve son 7 günün isteklerini gösterir. Aylık bütçe `ai.monthly_budget_usd` ayarıyla konur. Maliyetler fiyat tablosundan tahmindir; **Fiyatlar** tablosunu güncel tut. Ollama ile model bilgisayarında çalışır, dışarıya bir şey gitmez.

## MCP

MCP, Claude Desktop ve Claude Code gibi asistanların koleksiyonunla çalışmasını sağlar. **MCP** ekranındaki **Claude'dan kullan** bölümü kopyalayacağın kurulum parçacıklarını, araç listesini ve yazma anahtarını gösterir.

**Kurulum:** Sunucu `fc` komut satırı aracıyla çalışır ve koleksiyon dosyasını (`collection.db`) açar. `fc` kurulumla gelmez. Kaynak koddan `cargo build -p fc-cli --release` ile derle (`target\release\fc.exe` oluşur) ya da ayrıca edin, sonra tam yolunu yaz. Örneklerde `C:\Tools\fc.exe` bu yolun yerine geçer; onu ve koleksiyon yolunu kendininkilerle değiştir.

Claude Code:

```bash
claude mcp add flashcard -- "C:\Tools\fc.exe" --db "C:\Users\<sen>\AppData\Roaming\app.flashcard.desktop\collection.db" mcp serve
```

Claude Desktop (`%APPDATA%\Claude\claude_desktop_config.json`; JSON içinde `\` iki kez yazılır):

```json
{ "mcpServers": { "flashcard": {
    "command": "C:\\Tools\\fc.exe",
    "args": ["--db", "C:\\Users\\<sen>\\AppData\\Roaming\\app.flashcard.desktop\\collection.db", "mcp", "serve"] } } }
```

Bu yol ana profilin koleksiyonudur. Başka bir profil için `--db` o profilin dosyasını göstermeli: `...\app.flashcard.desktop\profiles\<kimlik>\collection.db` (bkz. [Veri klasörü](#veri-klasörü)); `fc` o zaman o profilin yapay zeka anahtarlarını kullanır. Uygulama kilidi `fc`'yi ve MCP sunucusunu kapsamaz.

**Araçlar:** **Desteleri listele**, **Not türlerini listele**, **Destenin sınıflandırmalarını listele**, **Kart ara**, **Not oku**, **Deste oluştur**, **Not ekle**, **Notu düzenle**, **Sınıflandırma ata (önce onay ister)**. Silen araç yok.

- Asistanın her değişikliği uygulamada sıradan bir adımdır; `Ctrl+Z` ile geri alınır.
- **Asistanlar değişiklik yapabilsin** kapalıysa asistanlar yalnız okur. Aynısını tek bağlantı için `mcp serve --read-only` yapar.
- **Dış MCP sunucuları** bölümünde uygulamanın yapay zeka özelliklerinin kullanacağı başka sunucuları **Sunucu ekle** ile eklersin. Uygulama başlatacağı programı gösterip onay ister.

## Tarayıcı eklentisi

Chrome, Edge, Brave gibi Chromium tabanlı tarayıcılar için. Bir sayfada seçtiğin kelimeyi ya da cümleyi uygulamanın **Gelen kutusu**'na gönderir.

**Kurulum:**

1. Eklentiyi yükle. Paketlenmemiş sürümü `chrome://extensions` → **Geliştirici modu** → **Paketlenmemiş öğe yükle** ile eklenti klasörünü seçerek kurarsın.
2. Uygulamada **Yerel API** modülünü aç (varsayılan kapalı): `Ctrl+K` → **Modülü aç/kapat** → `{"id": "local_api", "enabled": true}`. Menüde **Yerel API** belirir. Varsayılan port `28735`.
3. Eklenti düğmesi → **Bağlan**. Uygulamada **Bağlantı isteği** penceresi açılır; izin ver.

**Kullanım:**

1. Sayfada metni seç → sağ tık → **Desteye ekle** (ya da `Alt+Shift+A`, ya da araç çubuğu düğmesi).
2. **Ön yüz**, **Arka yüz**, **Deste** ve destenin **Sınıflandırmalar**'ını (ör. Seviye) doldur.
3. **Gelen kutusuna ekle**.
4. Uygulamada **Gelen kutusu** → öneriyi düzenle → **Kabul et** ya da **Reddet**. Sen kabul etmeden hiçbir şey karta dönüşmez.

Uygulama kapalıyken eklenen öğeler eklentide bekler ve uygulama açılınca gönderilir. Eklenti yalnız `127.0.0.1`'e bağlanır. Portu değiştirdiysen eklentinin ayarlarında **Yerel API portu**'na aynı değeri yaz. Bağlı programları ve izinlerini **Yerel API** ekranında görür, kaldırabilirsin.

**Program başına günlük öneri** (`local_api.inbox_daily_max`, varsayılan 200): bir programın 24 saatte önerebileceği en fazla kart. `Ctrl+K` → **Ayar değiştir** → `{"scope": {"kind": "app"}, "key": "local_api.inbox_daily_max", "value": 100}` ile değiştirirsin. Gelen kutusu önerilerini sen bakmadan kabul eden bir otomasyon kuralı **Otomasyon** ekranında uyarıyla işaretlenir: eşleştirilmiş bir programın ya da eklentinin önerdiği her şey o zaman hemen not olur.

Gelen kutusuna ayrıca `flashcard://` bağlantılarından ve `Ctrl+Alt+N` hızlı ekleme penceresinden öneri düşer.

### Subtitle (video altyazıları)

**Subtitle** tarayıcı eklentisi video altyazılarında **+** ile eklediğin kelimeyi doğrudan bir destede karta çevirir (gelen kutusuna düşmez): kelime ve anlamı, geçtiği cümle (kelime kalın), çevirisi, ekran görüntüsü, ses kaydı ve videonun o ana bağlantısı.

1. Uygulamada **Subtitle** modülünü aç (varsayılan kapalı): `Ctrl+K` → **Modülü aç/kapat** → `{"id": "subtitle", "enabled": true}`. **Ayarlar → Bağlantılar** altında **Subtitle** belirir.
2. **Subtitle** ekranında bir ad yazıp **Anahtar oluştur**'a bas ve anahtarı kopyala. Anahtar **yalnız bir kez** gösterilir.
3. Eklentide **Ayarlar → Öğrenme servisi → Subtitle Kartları**'nı seç; **Sunucu adresi**'ne ekrandaki adresi (`http://127.0.0.1:28735/subtitle`) ve **Erişim anahtarı**'na anahtarı yapıştır, **Bağlantıyı test et**'e bas.

Kelimeler eklentide seçtiğin desteye, seçmediysen ekrandaki **Deste**'ye, o da yoksa ilk kelimede oluşturulan **Subtitle** destesine gider. Aynı kelime aynı cümleden iki kez gelirse tek kart olur; başka bir cümleden gelirse yeni kart. Her kelime tek geri al adımıdır. Uygulama kapalıyken eklenti kelimeleri kuyrukta tutar, uygulama açılınca gönderir. Bir anahtarı **İptal et** ile hemen geçersiz kılarsın.

## Profiller ve uygulama kilidi

İkisi de **Profiller ve kilit** ekranındadır.

**Profiller.** Her profil kendi desteleri, medyası, ayarları ve yapay zeka anahtarlarıyla ayrı bir koleksiyondur; ör. ortak bir bilgisayarda her kişiye bir profil.

- **Profil ekle** yeni profil açar (adını **Yeni profil adı**'na yaz).
- **Aç** profile geçer: "Profil açılıyor — uygulama yeniden başlıyor…". Uygulama o profilin koleksiyonuyla yeniden başlar ve sen yine değiştirene kadar onu açar.
- **Yeniden adlandır** adını değiştirir.
- **Sil** profili kalıcı olarak kaldırır: klasörünü (koleksiyon, medya, yedekler, günlükler, çökme raporları) ve anahtar zincirindeki kayıtlarını (yapay zeka anahtarları, kilit özeti). Kilitli bir profil önce kendi PIN'ini/parolasını ister. **Ana profil** ve **Kullanımda** olan profil silinemez.

**Uygulama kilidi.** Her profilin kendi kilidi olabilir:

1. **Yeni PIN/parola** altında **PIN (rakam)** (4–12 rakam) ya da **Parola** (6–128 karakter) seç ve **Kilidi kur**'a bas.
2. **Hareketsizlikte kilitle**: **Hiçbir zaman**, 1, 5, 15 (varsayılan), 30 ya da 60 dk.
3. **Şimdi kilitle** (ya da tepsi menüsündeki **Kilitle**) hemen kilitler. **Değiştir** ve **Kilidi kaldır** **Şu anki PIN/parola**'yı ister.

Kilitli bir profille açılınca ya da açıkken kilitlenince uygulama **Flashcard kilitli** ekranını gösterir. PIN'ini ya da parolanı girip **Kilidi aç**'a bas ya da **Başka bir profil aç**'ı seç. 5 yanlış denemeden sonra bir sonrakini beklemen gerekir: 1 saniyeden başlar, her seferinde ikiye katlanır, en çok 5 dakika; sayaç yeniden başlatmada sıfırlanmaz.

PIN/parolanın kendisi hiç saklanmaz, yalnız tuzlanmış bir Argon2id özeti: sistem anahtar zincirinde ya da anahtar zincirinin kaydı yeniden başlatmada tutmadığı sistemlerde profil listesi dosyasında (`profiles.json`). Hangisi olduğu ekranda yazar.

Uygulama kilitliyken Yerel API, otomasyon kuralları, eklentiler, pano izleyici ve medya dosyaları kapalıdır. Gelen `flashcard://` bağlantıları işlenmez; kilit ekranı onları **Kilit açılınca çalışacak bağlantılar** altında listeler. Kilidi açınca çalışırlar; **Açılmasın**'a basarsan bırakılırlar.

**Kilit neye karşı korur.** Kilit yalnızca bu pencereye gelişigüzel erişimi engeller. Koleksiyon dosyası şifrelenmez: bilgisayar hesabını kullanabilen biri onu kopyalayabilir. `fc` komut satırı ve MCP sunucusu kilitlenmez. Kilidin özeti silinirse (ör. anahtar zincirindeki kayıt kaldırılırsa) uygulama kilitli kalır ve **Kilit bilgisi kayıp** der; klavyedeki herkes "Anladım: bu profilin kilidini kaldır"ı işaretleyip **Kilidi kaldır**'a basarak kilidi kaldırabilir. Yani kilit, işletim sistemi hesabına erişebilen birine karşı koruma sağlamaz.

## Hatırlatıcılar ve tepsi

**Hatırlatıcılar** ekranında:

- **Her gün hatırlat** günde bir işletim sistemi bildirimi açar ("Bugün … kart seni bekliyor"); varsayılan kapalı.
- **Saat** (varsayılan 19:00) ve **Günler** (varsayılan her gün).
- O gün zaten çalıştıysan ya da bekleyen kart yoksa hatırlatma gelmez. Uygulama o saatte kapalı ya da uykudaysa hatırlatma 3 saate kadar gecikmeli gelir.
- **Deneme bildirimi gönder** hemen bir tane gösterir.
- **Pencereyi kapatınca uygulama tepside kalsın**: pencereyi kapatmak onu yalnız gizler, hatırlatmalar gelmeye devam eder. Hatırlatma yalnız uygulama çalışırken (pencere ya da tepsi) gelir.

Tepsi simgesinin menüsü: **Flashcard'ı aç**, **Kilitle** (kilit kuruluysa kilitler), **Çık** (gerçekten kapatır). Simgeye tıklamak da pencereyi geri getirir.

## Çökme kurtarma ve günlükler

**Kurtarma.** Uygulama açılışta bir çalışma işareti (`run.json`) yazar, normal kapanışta siler. Sonraki açılışta hâlâ duruyorsa (çökme, görevi sonlandırma, elektrik kesilmesi) veritabanını denetler (`maintenance.check_after_crash` ayarı, varsayılan açık) ve **Flashcard düzgün kapanmadı** (çökmeden sonra **Flashcard beklenmedik şekilde kapandı**) penceresini gösterir. Pencere devam edebileceklerini listeler: veritabanı denetiminin sonucu, bitmemiş bir çalışma oturumu (**Çalışmaya devam et**), kaydedilmemiş formlar (yeniden açtığında geri gelir) ve çökme raporu. **Bakım'ı aç** **Günlükler ve güncellemeler**'e götürür, **Devam et** pencereyi kapatır. Koleksiyon her değişiklikten sonra kaydedilir; başka bir şey kaybolmaz.

**Günlükler ve güncellemeler** ekranı: sürüm, son çalıştırmanın düzgün kapanıp kapanmadığı, güncellemeler, **Günlük klasörü** ve **Veri klasörü**, **Veritabanını denetle**, çökme raporları ve **Çökme raporlarını sil**.

**Günlükler.** Uygulama yerel bir günlük tutar: `logs\flashcard.log`. 1024 KiB'ta yeni dosyaya geçer ve 4 eski dosya saklar (`maintenance.log_max_kb`, `maintenance.log_files` ayarları; ayrıntı düzeyi `maintenance.log_level`). Çökme raporları `crash` klasörüne yazılır. Her satırdaki gizli bilgiler yazılmadan önce çıkarılır.

**Hata raporu:** **Günlükler ve güncellemeler** → **Günlükleri dışa aktar…** → kaydedeceğin yeri seç. `.zip` günlükleri, çökme raporlarını ve kısa bir sistem özetini içerir. Her şey önce yeniden temizlenir: API anahtarları (uygulamanın tuttuğu anahtarların kendisi dahil), belirteçler, parolalar, e-posta adresleri, ev klasörü yollarındaki kullanıcı adı (`C:\Users\<user>`) ve kilit özeti. Mesaj kaç gizli bilginin çıkarıldığını söyler. Hiçbir şey bir yere gönderilmez; dosyayı raporuna kendin eklersin.

Çalışma işareti, günlükler ve çökme raporları profile aittir: etkin profilin klasöründe dururlar (bkz. [Veri klasörü](#veri-klasörü)).

## Veri klasörü

Windows'ta uygulama veri klasörü `%APPDATA%\app.flashcard.desktop\` (yani `C:\Users\<sen>\AppData\Roaming\app.flashcard.desktop\`). **Günlükler ve güncellemeler** etkin profilin **Veri klasörü**'nü ve **Günlük klasörü**'nü gösterir.

| Ne | Ana profil | Diğer profiller |
|---|---|---|
| Klasör | `%APPDATA%\app.flashcard.desktop\` | `%APPDATA%\app.flashcard.desktop\profiles\<kimlik>\` |
| Koleksiyon | `collection.db` | `collection.db` |
| Medya, güvenlik yedekleri | `media\`, `backups\` | `media\`, `backups\` |
| Günlükler, çökme raporları, çalışma işareti | `logs\`, `crash\`, `run.json` | `logs\`, `crash\`, `run.json` |

Profil listesi uygulama veri klasöründeki `profiles.json` dosyasıdır. `<kimlik>` uzun, üretilmiş bir kimliktir; bulmak için profili aç ve **Veri klasörü**'ne bak. Yapay zeka anahtarları ve kilit özeti bu klasörlerde değil, sistem anahtar zincirindedir.

## Yedek ve geri yükleme

**İçe / dışa aktar** → **Yedek**:

- **Yedek al…** tüm koleksiyonu tek bir `.fcbackup` dosyasına yazar: notlar, kartlar, geçmiş, sınıflandırmalar, ayarlar ve medya.
- **Yedekten geri yükle…** bir yedek seçtirir, tarihini ve not sayısını gösterip onay ister. Önce mevcut koleksiyonun güvenlik yedeği alınır, sonra yerine seçtiğin yedek yüklenir. Güvenlik yedeğinin yolu mesajda yazar (koleksiyon klasöründeki `backups` klasörü).

Bilmen gerekenler:

- Geri yüklemeden sonra geri alma geçmişi baştan başlar; önceki adımlar geri alınamaz.
- Daha yeni bir uygulama sürümünün yedeği eski sürüme yüklenmez.
- Yerel API'ye bağlı programların izinleri geri yüklemede kaldırılır; yeniden bağlanmaları gerekir.
- Yedek alma ve geri yükleme kullanımdaki profilde çalışır. Yedekte yapay zeka anahtarları ve uygulama kilidi yoktur.
- Windows'ta ana profilin koleksiyon dosyası `%APPDATA%\app.flashcard.desktop\collection.db`, başka bir profilinki `%APPDATA%\app.flashcard.desktop\profiles\<kimlik>\collection.db` konumundadır. Uygulama açıkken bu dosyayı elle kopyalama; **Yedek al…**'ı kullan.

## Sorun giderme

**İçe aktarınca sütunlar karıştı.**
Önizlemede **Terim ile tanım arası** kutusuna doğru ayırıcıyı yaz (sekme için `\t`), gerekirse **Kartlar arası**'nı değiştir. İçe aktarmayı zaten yaptıysan **İçe aktarmayı geri al** ya da `Ctrl+Z`.

**Yanlışlıkla bir şey sildim.**
`Ctrl+Z` ile geri al. Deste ya da not çöpe gittiyse **Bakım** → **Çöp kutusu** → **Geri getir**.

**"Geri alma burada durur" diyor.**
Koleksiyon bir yedekten geri yüklendi ya da çöp kalıcı olarak boşaltıldı. Daha önceki geçmiş geri alınamaz.

**"Şu an çalışılacak kart yok."**
Bugünün kartları bitti. **Deneme modu** ile ya da **Özel oturum**'a bir sorgu (`deck:İngilizce`) yazarak yine çalışabilirsin.

**Tarayıcı eklentisi "Flashcard uygulamasına ulaşılamıyor" diyor.**
Uygulama açık mı, **Yerel API** modülü açık mı, port eklentideki ile aynı mı kontrol et. Öğe kaybolmaz; eklentide bekler.

**Claude koleksiyonu değiştiremiyor.**
**MCP** ekranında **Asistanlar değişiklik yapabilsin**'i aç ve `--read-only` kullanmadığından emin ol. Sunucu eski şemalı bir koleksiyonu açmaz: önce uygulamayı bir kez aç.

**Yapay zeka "API anahtarı istiyor" diyor.**
**Yapay zeka** → **Sağlayıcılar**'dan anahtarı kaydet. "Sistem anahtar zinciri yok" uyarısı varsa anahtar saklanamaz.

**Bir eklenti "Hatalar yüzünden kapatıldı".**
**Günlük**'e ve **Son hata**'ya bak, sonra **Aç** ile yeniden dene. Sorun sürerse kaldır.

**Bir menü görünmüyor.**
Modülü kapalı olabilir. `Ctrl+K` → **Modülleri listele** ile durumuna bak, **Modülü aç/kapat** ile aç.

**Veritabanında sorun olduğunu düşünüyorum.**
**Bakım** → **Veritabanı denetimi** → **Denetle**. Sorun çıkarsa **Onar**; onarım da geri alınabilir.

**Windows "Windows kişisel bilgisayarınızı korudu" diyor.**
Bu SmartScreen; kurulum dosyası kod imzalı değil. **Ek bilgi** → **Yine de çalıştır**'a tıkla. Kurulum dosyasını yalnız projenin yayın sayfasından indir.

**Uygulama kilitli olduğunu söylüyor ve kilidi kaldırmamı istiyor.**
**Kilit bilgisi kayıp** ekranı, profilin kilitli olduğunu ama PIN/parola özetinin kaybolduğunu (ör. anahtar zincirindeki kayıt silinmiş) gösterir. Uygulama kontrolsüz açılmak yerine kilitli kalır. "Anladım: bu profilin kilidini kaldır"ı işaretle, **Kilidi kaldır**'a bas, sonra **Profiller ve kilit**'ten yeni bir kilit kur. Kartların etkilenmez.

**Profil değiştirince uygulama yeniden başladı.**
Bu beklenen davranış: başka bir profilde **Aç**, uygulamayı o profilin koleksiyonuyla yeniden başlatır. Geri dönmek için aynısını yap.

**Günlüklerim nerede? Hata raporunu nasıl gönderirim?**
**Günlükler ve güncellemeler** **Günlük klasörü**'nü gösterir (etkin profilin klasöründeki `logs`, bkz. [Veri klasörü](#veri-klasörü)). **Günlükleri dışa aktar…**'a bas, `.zip` dosyasını kaydet ve raporuna ekle. Gizli bilgiler çıkarılır, hiçbir şey kendiliğinden gönderilmez.

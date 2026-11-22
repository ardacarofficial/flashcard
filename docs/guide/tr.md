# Flashcard kullanıcı rehberi

Bu rehber Flashcard masaüstü uygulamasını günlük kullanım için anlatır. Uygulama bilgisayarında çalışır, verin bilgisayarında kalır ve telemetri göndermez.

Rehberde kalın yazılan adlar (ör. **Kütüphane**, **Çalış**) uygulamadaki menü ve düğme adlarıdır. Uygulamayı İngilizce kullanıyorsan [İngilizce rehbere](en.md) bak.

**v1.3'te ne değişti?** Kısa ve eğlenceli özet için [v1.3'te ne değişti](v1.3-tr.md) sayfasına bak.

## İçindekiler

1. [Kurulum](#kurulum)
2. [İlk açılış](#ilk-açılış)
3. [Desteler](#desteler)
4. [Sınıflandırmalar (seviye, sınav)](#sınıflandırmalar-seviye-sınav)
5. [Kart ekleme ve not türleri](#kart-ekleme-ve-not-türleri)
6. [Çalışma](#çalışma)
7. [İstatistikler ve planlama](#i̇statistikler-ve-planlama)
8. [Sorgu dili](#sorgu-dili)
9. [İçe ve dışa aktarma](#i̇çe-ve-dışa-aktarma)
10. [Eklentiler](#eklentiler)
11. [Yapay zeka](#yapay-zeka)
12. [MCP](#mcp)
13. [Tarayıcı eklentisi](#tarayıcı-eklentisi)
14. [Otomasyon](#otomasyon)
15. [Profiller ve uygulama kilidi](#profiller-ve-uygulama-kilidi)
16. [Hatırlatıcılar ve tepsi](#hatırlatıcılar-ve-tepsi)
17. [Çökme kurtarma ve günlükler](#çökme-kurtarma-ve-günlükler)
18. [Veri klasörü](#veri-klasörü)
19. [Yedek ve geri yükleme](#yedek-ve-geri-yükleme)
20. [Sorun giderme](#sorun-giderme)

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

**Güncellemeler.** Uygulama güncelleme sunucusuna yalnız sen istediğinde bağlanır: **Ayarlar** → **Gelişmiş** → **Günlükler ve güncellemeler** → **Güncellemeleri denetle**. Yeni sürüm çıktıysa **İndir**'e bas ("İndiriliyor ve doğrulanıyor…"), sonra **Kur ve yeniden başlat**. Uygulama kapanır, güncellemeyi soru sormadan kurar ve yeniden açılır; verin yerinde kalır. Her güncelleme projenin anahtarıyla imzalıdır; imzası tutmayan indirme reddedilir, hiçbir şey kurulmaz. Sen tıklamadan hiçbir şey kurulmaz. Her açılışta denetlemek için `updates.check_on_start` ayarını aç (varsayılan kapalı): `Ctrl+K` → **Ayar değiştir** → `{"scope": {"kind": "app"}, "key": "updates.check_on_start", "value": true}`. Bulunan güncelleme o zaman "… sürümü çıktı." şeridinde **Güncellemeye bak** düğmesiyle görünür.

**Kaldırma:** Windows *Ayarlar* → *Uygulamalar* → Flashcard → *Kaldır*. Veri klasörün (bkz. [Veri klasörü](#veri-klasörü)) kalır; kaldırıcıdaki uygulama verilerini silme kutusunu işaretlersen tüm profiller, ayarlar ve günlükler kalıcı olarak silinir.

## İlk açılış

Koleksiyon boşsa uygulama **Flashcard'a hoş geldin** penceresiyle açılır: "Dört kısa adımda başlayalım; her adım işi gerçekten yapar."

1. **Ne öğreniyorsun?**: bir konu seç (**İngilizce**, **Japonca**, **Çince**, **Başka bir dil**, **YKS (TYT / AYT / YDT)**, **Başka bir sınav**, **Okul (1–12. sınıf)**, **Genel kültür**). Her konunun yanında kuracağı sınıflandırma yazar, ör. **Sınıflandırma: CEFR seviyeleri (A1–C2)**. **Deste adı**'na bak, istersen **Başlangıç desteleri**'ni işaretle.
2. **Hazır desteyle başla** işaretli başlangıç destelerini ekler; **Kendi kelimelerimi ekleyeceğim** boş bir deste kurar.

Kurulum o sınıflandırmayla bir deste, uygun bir not türü (dillerde **Kelime kartı**, diğerlerinde **Klasik**), diller ("Dil öğrenme", günde 15 yeni kart) ve sınavlar ("Sınav hazırlığı", günde 30 yeni kart) için bir deste ayar paketi ve seçtiğin başlangıç destelerini kurar. Hepsi tek bir geri alma adımıdır: `Ctrl+Z` tüm kurulumu kaldırır. **Rehberi geç** pencereyi kapatır. İçinde kart olan bir koleksiyonda pencere hiç çıkmaz.

Sonra sağ altta küçük bir **Başlarken** listesi kalır. Her adımın düğmesi işi yapar: **İlk kelimelerini ekle**, **Çalış**, **Bugünün kartları ve ilerlemen nerede** (**Ana sayfa**, **İstatistikler**) ve isteğe bağlı **Tarayıcı eklentisini bağla**. **Gizle** listeyi küçültür, **Rehberi bitir** kapatır. Listeyi **Başlangıç desteleri** sayfasındaki **Başlangıç rehberini aç** ile yeniden açabilirsin.

**Başlangıç desteleri**: bu uygulama için yazılmış küçük desteler, Türkçe ya da İngilizce (**Kartların dili**): temel İngilizce kelimeler (A1–A2), YKS TYT matematik temelleri, dünya başkentleri ve bir uygulama rehberi. **Ekle** desteyi koleksiyonuna değiştirip silebileceğin sıradan bir deste olarak ekler.

**Ekranın düzeni.** Solda beş bölüm var; bir bölümün sayfaları üstte sekme olarak görünür:

| Bölüm | İçinde |
|---|---|
| **Ana sayfa** | Bugün kaç kart çalışılacağı, **Çalışmaya başla**, seri, hedef, bugün çalışılacak desteler |
| **Kütüphane** | **Desteler** (deste sayfası, çalışma modları) ve **Notlar** |
| **Oluştur** | **Ekle** (Kart ekle sayfası), **Yapay zeka**, **Başlangıç desteleri** |
| **Ara** | **Göz at**: sorgu diliyle arama ve toplu işlemler |
| **İstatistik** | **Genel bakış**, **Planlama**, **Hedefler** |

Sol altta **Gelen kutusu** (bekleyen öneri sayısıyla) ve **Ayarlar** durur. Kapalı bir modülün sayfası hiçbir yerde görünmez.

**Ayarlar.** Ayarlar yapmak istediğin işe göre beş gruptadır:

| Grup | Sayfalar |
|---|---|
| **Genel** | Dil, **Görünüm** (renk düzeni, **Tema**, **Vurgu rengi**), **Sınıflandırmalar**, **Hatırlatıcılar**, **Kısayollar**, **Profiller ve kilit** |
| **Çalışma ve kartlar** | **Deste ayarları**, **Not türleri**, **Modlar ve okuma** |
| **Bağlantılar** | **Yapay zeka**, **Subtitle** |
| **Veri** | **Dışa aktar ve yedek**, **Başlangıç desteleri** |
| **Gelişmiş** | "Teknik ayarlar: çoğu zaman gerekmez." **Otomasyon**, **Asistanlar (MCP)**, **Yerel API**, **Eklentiler**, **Bakım**, **Günlükler ve güncellemeler** |

Ayarlar kendini anlatır: her satırın altında ne işe yaradığı ve değiştirince ne olacağı yazar, çoğunda bir **Örnek:** vardır. Varsayılan değer **varsayılan** diye işaretlidir; değiştirdiğin bir satırı **Varsayılana dön** ile geri alırsın. Nadiren gereken satırlar her sayfanın altındaki **Gelişmiş ayarlar** bölümünde durur (açık/kapalı oluşu hatırlanır). Hangi sayfada olduğunu bilmediğin bir ayar için **Ayarlarda ara** kutusuna yaz: eşleşen satırlar vurgulanır, gerekirse **Gelişmiş ayarlar** kendiliğinden açılır. **Genel** sayfası **Ne değiştirmek istiyorsun?** sorularıyla başlar (ör. "Günde kaç yeni kelime gelsin?", "Her gün çalışmayı bana hatırlat"); birine tıklayınca doğru sayfa açılır.

**Komut paleti:** `Ctrl+K` ile açılır. Bir komut ya da ekran adı yaz, **Çalıştır**'a bas. Bazı komutlar **Parametreler (JSON)** kutusuna değer ister. Modülleri açıp kapatmanın ve ekranda satırı olmayan ayarları değiştirmenin yolu burası:

- **Modülü aç/kapat**: `{"id": "local_api", "enabled": true}`
- **Ayar değiştir**: `{"scope": {"kind": "app"}, "key": "timed.seconds", "value": 20}`

**Genel kısayollar** (hepsi **Ayarlar** → **Genel** → **Kısayollar**'dan değiştirilebilir):

| Tuş | İşlem |
|---|---|
| `?` | Klavye kısayolları listesi |
| `Ctrl+K` | Komut paletini aç |
| `Ctrl+F` | Kart ara |
| `N` veya `Ctrl+N` | Kart ekle (hızlı ekleme penceresi) |
| `Ctrl+Z` | Geri al |
| `Ctrl+Shift+Z` | Yinele |

**`?` kısayol listesi.** Yazı yazmıyorken `?`'ye bas (ya da üst çubuktaki **?** düğmesine, ya da paletten **Klavye kısayolları**): o an geçerli tüm kısayollar aranabilir bir listede açılır. **Kısayolları değiştir** seni **Kısayollar** sayfasına götürür. Yazı alanının içindeyken kısayollar çalışmaz: `Ctrl+Z` ve `Ctrl+A` orada metni geri alır ve seçer, kartlarına dokunmaz. Basılı tutulan bir tuş işlemi bir kez yapar.

Neredeyse her değişiklik geri alınabilir: not ekleme, silme, puanlama, toplu işlem, içe aktarma.

## Desteler

Desteler **Kütüphane** → **Desteler**'dedir (**Kartlar** ya da **Ağaç** görünümü):

- **Yeni deste** bir deste açar. Alt deste için adda `::` kullan, ör. `Dil::İngilizce::A1`.
- Bir desteye tıklayınca deste sayfası açılır: kart önizlemesi, **Çalışma modları** (**Kartlar**, **Öğren**, **Test**, **Eşleştir**, **Okuma**), ilerleme ve **Bu destedeki terimler** listesi.
- Deste sayfasındaki **Deste işlemleri** menüsünde **Yeniden adlandır**, **Yeni alt deste**, **Deste seçenekleri**, **Dışa aktar** ve **Desteyi sil** var. Silinen deste alt desteleri, notları ve kartlarıyla çöp kutusuna gider.

Terim listesinde **Terim** ve **Tanım** satır içinde düzenlenir. Uygulama hangi alanın terim, hangisinin anlam olduğunu not türündeki alan rollerinden bilir (bkz. [not türleri](#kart-ekleme-ve-not-türleri)); bilemediği bir not türünde kalem, tahmin etmek yerine notun tam düzenleyicisini açar.

Çöpe giden desteyi ve notları **Ayarlar** → **Gelişmiş** → **Bakım** → **Çöp kutusu** → **Geri getir** ile kurtarırsın. Çöpteki öğeler varsayılan olarak 30 gün sonra kalıcı olarak silinir.

Sorgularda deste yolu `::` ile yazılır: `deck:"Dil::İngilizce"`. Bir deste seçildiğinde alt desteleri de dahil olur.

**Deste ayarları** (**Ayarlar** → **Çalışma ve kartlar** → **Deste ayarları**): **Günlük yeni kart**, **Günlük en çok tekrar**, öğrenme adımları, **Hedef hatırlama oranı**, **En uzun aralık (gün)** gibi değerler bir **ayar paketi**nde toplanır. **Yeni paket** ile oluştur, **Bu destede kullan** ile desteye ata. Deste kendi paketini, yoksa üst destesininkini, o da yoksa varsayılanları kullanır.

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

**Tanımlama.** İlk açılış kendi destesine bir tane kurar. Bunun dışında:

- Komut paleti (`Ctrl+K`) → **Şablondan sınıflandırma** → `{"template_id": "cefr"}`. Bu, tüm koleksiyon için tanımlar.
- Komut satırı aracı `fc` ile bir desteye (kurulumda gelmez, bkz. [MCP](#mcp)): `fc attribute apply-template cefr --deck Dil --locale tr`.

**Kullanım.**

- Kart eklerken destenin sınıflandırmaları alanların yanında (hızlı eklemede **Diğer** altında) çıkar. Zorunlu olanlar **Zorunlu** diye işaretlidir.
- Sorguda: `seviye:B1`, `seviye>=B1` (sıralı seçeneklerde), `sınav:TYT`. Anahtar büyük/küçük harf ve aksan duyarsızdır: `sinav:TYT` de çalışır.
- **Göz at** → seçimi değiştir → **Sınıflandırma** ile toplu atama.
- İçe aktarmada bir sütunu sınıflandırmaya eşleyebilirsin (aşağıda).
- **İstatistikler**'de **Sınıflandırmaya göre** kırılım.
- Çalışma başlatıcısında **Seçenekler** → **Yalnızca şu kartlar** ya da **Sorgu** (`seviye:A2`, `sınav:TYT`) ile bir sınıflandırmayı hedefleyebilirsin.

## Kart ekleme ve not türleri

Flashcard **not** ile **kartı** ayırır. Not, içeriktir (alanlar). Kartlar, notun not türüne göre ondan üretilir. Örneğin **Klasik + ters** bir not iki kart üretir.

### Kart ekle: dört yol, tek yer

Kart eklemenin her yolu tek sayfada: **Oluştur** → **Ekle** (sayfa başlığı **Kart ekle**). Üstte **Nasıl eklemek istersin** sorusu ve dört yol var:

| Yol | Ne zaman |
|---|---|
| **Tek kart** | Bir iki kelime: hızlı ekleme penceresini açar |
| **Toplu** | Tablo gibi doldurmak: her satıra bir terim ve tanımı |
| **Yapıştır** | Excel'den, bir tablodan ya da belgeden kopyaladığın bir liste |
| **Dosyadan** | CSV/metin, Anki, Quizlet, JSON ya da deste paketi dosyası |

Dört yol da aynı **Kartlar nereye** seçimini kullanır (**Yeni deste** ya da **Mevcut deste**, ve **Not türü**) ve aynı sonuçla biter: "12 kart eklendi" · **Geri al** · **Desteyi aç** · **Şimdi çalış**. Atlanan satırlar varsa **Neden?** her birinin sebebini söyler. **Şimdi çalış** çalışma başlatıcısını o desteyle açar.

**Tek kart (hızlı ekleme).** Her yerden `N`, `Ctrl+N` ya da üst çubuktaki **Kart ekle** ile de açılır. Önce kelime, anlam ve isteğe bağlı örnek cümle gelir; diğer alanlar, sınıflandırma ve önizleme **Diğer** altındadır. `Enter` sonraki alana geçer, son alanda ekler; `Shift+Enter` yeni satır. Pencere açık kalır, sıradakini yazarsın. Aynı kelime zaten varsa pencere hemen söyler: "“go” zaten var…" ve **Yine de ekle** seçeneği sunar. Alttaki **Başka yol:** bağlantıları seçtiğin desteyle **Toplu**, **Yapıştır** ve **Dosyadan**'a geçer.

**Toplu.** **Terim** ve **Tanım** satırları. `Tab` sonraki alana geçer, son kartta `Enter` yeni kart ekler, `Ctrl+Enter` kaydeder (**Oluştur**). Satıra görsel ekleyebilirsin. Zaten var olan kartlar aşağıda bırakılır; düzeltip yeniden kaydedebilirsin.

**Yapıştır.** Listeni kutuya yapıştır: her satır bir kart. Uygulama ayırıcıları kendisi bulur (sekme, virgül, noktalı virgül, iki nokta, tire…) ve kartların nasıl görüneceğini canlı gösterir. Ayırıcı yanlışsa **Değiştir** ile **Terim ile tanım arasında** ve **Kartlar arasında** ayırıcıyı seç. Sonra **N kart ekle**. **Önce tabloda düzenle** listeyi **Toplu** tablosuna taşır.

**Dosyadan.** Dosyayı sürükleyip bırak ya da **Dosya seç…**. Adımlar: **Dosya seç** → **Sütunları eşle** → **İçe aktar**. Ayrıntılar [İçe ve dışa aktarma](#i̇çe-ve-dışa-aktarma) bölümünde.

**Sütunlar rolüne göre eşlenir.** Yapıştırırken ve dosyadan alırken her sütunun başlığından ne olacağını seçersin: bir alan (rol adıyla: **terim**, **anlam**, **örnek cümle**, **cümle çevirisi**, **kaynak**…), **Etiketler**, bir sınıflandırma ya da **Aktarma**. Uygulama eşlemeyi önceden doldurur: başlık satırındaki sözcüklerden (`word`, `kelime`, `meaning`, `anlam`, `sentence`, `örnek`…), yoksa bu not türünde son kullandığın eşlemeden, o da yoksa sırayla (terim, anlam, örnek…). Hangisinin kullanıldığı yazar. Böylece `kelime ⇥ anlam ⇥ cümle` satırlarındaki cümle notun örnek cümle alanına gider. Seçtiğin not türünde örnek cümle alanı yoksa uygulama uyarır: "Örnek cümleyi saklamak için “Kelime kartı” türüne geç". Hiçbir alana gitmeyen sütun da açıkça söylenir.

**Kart zaten varsa**: **Atla (önerilen)**, **Var olanı güncelle** ya da **Yine de ekle**. Her seçeneğin altında ne olacağı düz cümleyle yazar (ör. güncellemede çalışma geçmişin korunur).

Alanlarda Markdown, `$formül$` (KaTeX) ve `[[bağlantı]]` kullanabilirsin. Resim, ses ya da videoyu sürükleyip bırak veya yapıştır. Aynı dosyayı iki kez eklersen diskte tek kopya tutulur.

### Hazır not türleri

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
| Kelime kartı | Kelime, anlam, örnek cümle; **Tanıma** ve **Üretme** kartları |
| Cümle kartı | Cümle, çevirisi, hedef kelime, anlam, kaynak |
| Altyazı kartı | Subtitle eklentisinin kartları: kelime, geçtiği hâli, anlam, cümle ve çevirisi, kaynak, ekran görüntüsü |

**Kelime kartının arkası.** Kelime, Cümle ve Altyazı kartlarının arka yüzü üç ayrı parçadır: en büyük **anlam**; altında **Örnek** etiketiyle cümle ve onun çevirisi (ayrı satırda); en küçük **Kaynak**. Kendi düzenlediğin şablonlara dokunulmaz.

### Not türleri

**Ayarlar** → **Çalışma ve kartlar** → **Not türleri**. Sayfa önce öğrenci gözüyle açılır: türler kutucuk olarak (**Senin türlerin**, **Hazır türler**), seçtiğin türün alanları ve kartlarının canlı önizlemesi (**Arka yüzü göster** ile çevrilir). Her tür ne kadar notta kullanıldığını ve her notun kaç kart olacağını söyler.

- **Yeni not türü** hazır bir başlangıç noktasından başlar: **Kelime**, **Kelime + örnek cümle**, **Ters yönlü (iki yönlü)**, **Boşluk doldurma (cloze)**, **Subtitle** ya da **Sade: ön ve arka**. Hazır bir türü **Kopyasını oluştur** ile de çoğaltabilirsin.
- **Alan rolleri: "Ne içeriyor?"** Her alan için ne tuttuğunu seç: **Kelime (sözlükteki hâli)**, **Geçtiği hâli (ör. “went”)**, **Anlamı**, **Örnek cümle**, **Örnek cümlenin çevirisi**, **Nereden geldiği**, **Not, açıklama** ya da **Başka bir şey**. Roller sayesinde uygulama tahmin etmez: ipuçları, içe aktarma, gelen kutusu, terim listesi ve istatistikler doğru alanı kullanır. Bir rol bir türde yalnız bir alanda olabilir.
- **Kartlar neyi soruyor?** Her kart şablonu için cevabın hangi alan olduğunu seç (ya da **Arka yüzün tamamı**). Cevabı yazdığında, Test, Öğren ve Eşleştir'de cevabın bu alanla karşılaştırılır. Ör. altyazıdan gelen "went" kartında (go → gitmek) "gitmek" yazman yeter; örnek cümleyi ve kaynağı yazman beklenmez. Ters kart terimi sorar.
- Şablon kodu, görünüm (CSS) ve alan ayrıntıları **Gelişmiş: şablon kodu** altındadır. Şablon dilinde `{{alan}}`, `{{#alan}}…{{/alan}}`, `{{cloze:alan}}`, `{{type:alan}}`, `{{hint:alan}}`, `{{tts:en:alan}}` ve `{{FrontSide}}` vardır. Kaydetmeden ya da silmeden önce uygulama tam olarak neyin kaybolacağını söyler. Alan eklemek ya da çıkarmak mevcut kartları ve geçmişi bozmaz.

v1.2'den gelen ve hiç değiştirmediğin hazır türler (Kelime, Cümle, Altyazı kartı ve diğerleri) bu rolleri ve cevap alanlarını güncellemeyle kendiliğinden alır.

**Çalışırken hızlı kart:** Çalışma ekranında `N` tuşu **Metinden yeni kartlar** penceresini açar. Her satır bir kart olur: `porsuk :: badger`. Oturum bozulmaz, tüm kartlar tek adımda geri alınır.

## Çalışma

Uygulama FSRS zamanlayıcısını kullanır.

### Çalışmaya başlamak: başlatıcı

Her çalışma tek bir pencereden başlar: **Ana sayfa**'da **Çalışmaya başla** (ya da `Enter`), bir destenin **Çalış** düğmesi, deste sayfasındaki mod kutucukları, istatistiklerdeki **Bunları çalış** ya da kart ekledikten sonra **Şimdi çalış**. Deste seçmeden açarsan başlatıcı **Tüm desteler** için çalışır.

**Ana sayfa**'daki "Bugün N kart çalışılacak" sayısı, başlatıcının **Bugünün kartları** sayısı ve oturumun gerçekten getirdiği kartlar aynı hesaptan gelir. **Tüm desteler** sırası her destenin kendi günlük yeni kart ve tekrar sınırlarına uyar.
<!-- v1.3: verify after X1/X2 -->

Başlatıcıda yukarıdan aşağı:

1. **Yarım kalanlar**: bitmemiş bir oturum varsa en üstte ve odakta durur; **Devam et** kaldığın yerden sürdürür ("12 cevaplandı · 8 kaldı").
2. **Mod**: **Kartlar**, **Öğren**, **Test**, **Eşleştir**. Altında modun takvime ne yaptığı yazar.
3. **Ne çalışayım?**: beş kapsam, her biri tam olarak kaç kart getireceğini gösterir:

   | Kapsam | Ne getirir | Takvim |
   |---|---|---|
   | **Bugünün kartları** | Sırası gelen tekrarlar ve bugünkü yeni kartlar (günlük sınırlar içinde) | Değişir |
   | **Yeni kartlar** | Henüz hiç çalışmadığın kartlar, günlük sınır içinde | Değişir |
   | **Bugün çalıştıklarım** | Bugün cevapladığın her kart (hangi modda olursa olsun) | **pratik · takvimi değiştirmez** |
   | **Bugün zorlandıklarım** | Bugün "Tekrar" dediğin ya da yanlış cevapladığın kartlar | **pratik · takvimi değiştirmez** |
   | **Tüm kartlar** | Destedeki her kart | **pratik · takvimi değiştirmez** |

   Boş bir kapsam seçilemez ve nedenini söyler: "Bu destede henüz kart yok", "Bugün sırası gelen kart yok", "Bugünlük kart sınırın doldu (yeni kart ve tekrar sınırları)", "Sıradaki kart 5 dk sonra", "Bugün henüz çalışmadın", "Bugün zorlandığın kart yok".
4. **Kaç kart?** (Testte **Kaç soru?**): boş bırakırsan hepsi.
5. **Seçenekler** (kapalı gelir): **Pratik** ("Bugünün kartlarını takvime dokunmadan çalış."), **Süre sınırı** ve **Puanlama** (Kartlar), **Yalnızca şu kartlar** (sınıflandırma), **Sorgu** ve **Kayıtlı sorgu**, **Bugünün kartlarını ertele**, Testte **Soru tipleri**.

`Enter` başlatır (**Başla · 20 kart**), `Esc` kapatır. Başlatılamazsa pencere açık kalır ve nedenini yazar; açık oturumun bozulmaz. Uygulama her deste için son modu ve her mod için son kapsamı hatırlar.

### Kartlar oturumu

Kartı çevirip ne kadar bildiğini söylersin; sıradaki tekrar zamanı buna göre belirlenir.

**Cevap düğmeleri.** Varsayılan iki düğme: **Hâlâ öğreniyorum** (`1`) ve **Biliyorum** (`2`). Dört düğme isteyenler için: **Tekrar** (`1`), **Zor** (`2`), **İyi** (`3`), **Kolay** (`4`); her düğmenin üstünde kartın ne zaman geri geleceği yazar (`10dk`, `3g` gibi). Oturumda **Dört cevap düğmesi göster** / **İki cevap düğmesi göster** ile, kalıcı olarak **Ayarlar** → **Çalışma ve kartlar** → **Modlar ve okuma** → **Cevap düğmeleri** ile değiştirirsin.

**Tuşlar: bir tuş, bir iş.**

| Tuş | İşlem |
|---|---|
| `Boşluk` | Kartı çevir (ön ⇄ arka), istediğin kadar. **Hiç puan vermez.** |
| `1`–`2` (iki düğme) · `1`–`4` (dört düğme) | Puanla; yalnız kartın arkası en az bir kez görüldükten sonra |
| `1`–`9` | Çoktan seçmeli kartın sorusunda o şıkkı seç |
| `Enter` | Yalnız odaktaki düğmeye ya da karta basar, başka bir şey yapmaz |
| `H` | İpucu (soru tarafında) |
| `S` | Kartı durdur (sen açana kadar sorma) |
| `B` | Kartı yarına bırak (bugün bir daha sorma) |
| `Z` | Son cevabı geri al |
| `Ctrl+1`–`Ctrl+7` | Bayrak aç/kapa |
| `N` | Metinden yeni kartlar |
| `F` | Odak modu |
| `Esc` | Ara ver |
| `R` (oturum özetinde) | Yanlışları tekrar çalış |

Basılı tutulan bir tuş tek işlem sayılır: `2`'yi basılı tutmak yalnız bir kartı puanlar. Yazı alanındayken (ör. yazarak cevaplarken) yalnız `Esc` çalışır. `Boşluk`, puan tuşları, `H`, `S`, `B`, `Z` ve `Esc` **Kısayollar** sayfasından değiştirilebilir; `Enter`, şık rakamları, bayraklar, `R`, `F` ve `N` sabittir (**Çalışmada sabit tuşlar**). Ekrandaki tuş ipuçları senin atadığın tuşları gösterir.

**Ara vermek ve devam etmek.** `Esc` (ya da **Ara ver**) oturumu bitirmez: o ana kadar yaptıklarını gösterir ("Ara verdin — kaldığın yerden devam edebilirsin") ve oturumu açık tutar. **Devam et** ya da başlatıcının en üstündeki **Devam et** aynı oturuma döner.

**İpucu.** `H` ya da **İpucu**. Notun bir örnek cümlesi varsa ilk ipucu bağlamdır: kelimeyi o cümlenin içinde görürsün; cevap olarak istenen kelime cümlede boşluk olarak gizlenir ("Cümledeki boşluğu doldur", ör. `I ___ home.`). Sonraki ipuçları cevabın harflerini açar (**İpucu türü** ayarı: ilk harfler, her kelimenin başı ya da harflerin bir kısmı). Örnek cümlesi olmayan notlarda ipucu doğrudan harflerle başlar.

Hazır Kelime ve Altyazı kartlarında ön yüz yalnız kelimeyi gösterir; ilk ipucu örnek cümleyi kelime vurgulanmış olarak getirir. Ters (Üretme) kartta ilk ipucu, terimi boşluk olarak gizleyen cümledir.
<!-- v1.3: verify after X1/X2 -->

**Bekleme.** Kısa bir öğrenme adımındaki kart (ör. 10 dakika sonra) zamanı gelmeden de birkaç dakika erken sorulabilir; bu öne alma süresi bir ayardır (varsayılan 20 dakika). Kalan tüm kartlar gerçekten beklemek zorundaysa ekran "Sıradaki kart N dk sonra" der ve geri sayar: **Bekle**, **Başka bir şey çalış** ya da **Bitir**. Oturum açık kalır.
<!-- v1.3: verify after X1/X2 -->

**Oturum bitince** **Oturum bitti** ekranı çalışılan kart sayısını, doğruluğu, süreyi ve yanlışları gösterir. **Tekrar çalış** başlatıcıyı **Bugün zorlandıklarım** seçili açar; `R` yanlışları hemen tekrar çalıştırır.

### Modlar

| Mod | Takvim | Ne yapar |
|---|---|---|
| **Kartlar** | Değişir (pratik kapsamlarda değişmez) | Kartı çevir, ne kadar bildiğini söyle. |
| **Öğren** | Doğru bildiğin kartların takvimi ilerler (pratik kapsamlarda değişmez) | Her kart önce çoktan seçmeli, sonra yazarak sorulur. İki adımda da doğru bilince kart öğrenilmiş olur. Yanlış yazılı cevap çoktan seçmeliye döner. |
| **Test** | Hiç değişmez | Karışık sorulu bir sınav kâğıdı, sonunda puanın. |
| **Eşleştir** | Değişmez | Kelimeleri anlamlarıyla süreye karşı eşleştir. |

Hepsi başlatıcının **Mod** seçiminden başlar. Kartlar modunda ayrıca:

| Seçenek | Nerede | Ne yapar |
|---|---|---|
| Güven | Başlatıcı → **Seçenekler** → **Puanlama** → **Güven 1–5** | Düğmeler yerine "Ne kadar emindin?" sorusuna 1–5 cevap verirsin. |
| Zamanlı | Başlatıcı → **Seçenekler** → **Süre sınırı** | Her karta süre sınırı. |
| Odak | Oturumda **Odak** düğmesi veya `F` | Tam ekran, dikkat dağıtıcılar gizli. |

**Öğren:** Tuşlar: `1`–`4` seç, `Enter` kontrol et / devam, `Z` geri al, `Esc` bitir.

**Test:** **Kaç soru?** ve **Seçenekler** → **Soru tipleri** (**Çoktan seçmeli**, **Yazılı**, **Doğru / yanlış**; hiçbiri seçilmezse hepsi). Kâğıt ilk cevaba odaklanmış açılır. Yarım bıraktığın kâğıdın cevapları saklanır; başlatıcıdan **Devam et** ile dönersin. Kâğıdı **Yazdır** ya da **Cevap anahtarını yazdır** ile basabilirsin. **Teslim et** ile puanlanır; "az kaldı" yarım puan sayılır. Teslim geri alınabilir. Yazılı cevap, not türünde kartın sorduğu alanla karşılaştırılır.

**Güven bazlı puanlama:** Varsayılan eşleme 1–2 = Tekrar, 3 = Zor, 4 = İyi, 5 = Kolay. Cevap tarafında `1`–`5` tuşları da çalışır. Eşlemeyi `confidence.map.1` … `confidence.map.5` ayarlarıyla değiştirirsin.

**Zamanlı mod:** **Saniye** kutusuna süreyi yaz. **Süre dolunca** ya **Tekrar say** ya da **Geç** seçilir. Sayaç kart ekrana gelince başlar, kart başınadır ve yeniden başlatılamaz. Süre dolduktan sonra verilen cevap kabul edilmez.

**Odak modu:** `focus.fullscreen`, `focus.hide_counts` ve `focus.auto` ayarları tam ekranı, sayaçları gizlemeyi ve her oturumun odakta başlamasını yönetir (**Ayarlar** → **Çalışma ve kartlar** → **Modlar ve okuma**).

Pratik oturumları, Test ve Eşleştir kartların takvimini değiştirmez. Her mod ayrı bir modüldür ve kapatılabilir.

**Diğer çalışma ayarları** (ayar anahtarıyla): `study.scheduler` (FSRS veya SM-2), **Cevap kontrolü** (`study.answer_checker`: birebir, büyük/küçük harf ve noktalama yok sayılsın, yazım hatalarına izin ver, birden çok cevap), **ı/i, ş/s… eşit sayılsın** (`study.fold_turkish`), **İpucu türü** (`study.hint_strategy`), **Yeni gün şu saatte başlar** (`study.day_start_hour`, varsayılan 04:00; "bugün" sayıları buna göre).

## İstatistikler ve planlama

**İstatistik** → **Genel bakış** üç soruyla açılır:

- **Bugün nasılım?** Bugün cevapladıkların, doğru sayısı, süre, seri ve hedefin; dünle ya da son 7 günle karşılaştıran bir cümle ("Dünden hızlısın: dün 40 kart, bugün şimdiden 52.").
- **Önümde ne var?** Önümüzdeki 7 ya da 30 günün tekrarları; en yoğun gün işaretli. **Temponu planla** Planlama'ya götürür.
- **Nelerde zorlanıyorum?** Bugün, son 7 ya da 30 günde yanlış cevapladığın kelimeler, en çok yanlış yapılan üstte (pratik ve testler dahil). **Bunları çalış** başlatıcıyı bu kartlarla açar: pratik oturumudur, takvimin değişmez. Bir kelimeyi oradan durdurabilir ya da "zor" etiketi koyabilirsin.

Ayrıntılı grafikler altta, her biri tek cümlelik açıklamayla durur.

**Planlama** → **Temponu planla**: **Günde kaç yeni kart?** sorusuna bir sayı ver, istersen **Sınav tarihim (isteğe bağlı)** gir. Uygulama canlı bir tahmin gösterir: "Bu tempoyla 4 hafta sonra günde ~85 kart, ~12 dk.", en yoğun gün, yeni kartlarının ne zaman biteceği; sınav tarihi verdiysen hepsine yetişmek için günde kaç yeni kart gerektiği (**N yap**). Dakikalar son 30 gündeki kart başına ortalama sürenden hesaplanır. **Uygula** seçtiğin destenin ayar paketinde günlük yeni kart sayısını değiştirir (paketi yoksa bir tane oluşturur, sınav tarihini de ekler); tek adımda geri alınır. Yük dengeleme, FSRS optimizasyonu ve hedef hatırlamaya göre iş yükü **Gelişmiş** altındadır.

**Hedefler**: hedef boşlukları doldurulan bir cümledir (**Her gün kart**, **Desteyi bitir**, **Her gün dakika**…). Günlük hedefler her gün sıfırdan sayılır; son tarihli hedefler şu anki hızınla tahmini gösterir.

## Sorgu dili

Sorgu dili **Göz at**, çalışma başlatıcısının **Sorgu** seçeneği, otomasyon ve MCP'de aynıdır. Boşlukla ayrılan terimler VE ile birleşir.

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
| `is:due` `is:suspended` `is:buried` `is:flagged` | Vadesi gelmiş, durdurulmuş, yarına bırakılmış, bayraklı |
| `flag:red` · `flag:kirmizi` · `flag:1` | Bayrak: 1–7 ya da renk adı (TR/EN) ya da kendi bayrak adın; `flag:0` / `flag:none` bayraksız, `flag:*` herhangi |
| `stability>30` `difficulty>7` `lapses>=5` `reps<3` | Sayısal; `=` `!=` `<` `<=` `>` `>=` |
| `retrievability<0.8` | Şu anki hatırlama olasılığı (0–1) |
| `added:7` `edited:7` `reviewed:1` | Son N çalışma günü içinde eklenen / düzenlenen / tekrar edilen (gün **Yeni gün şu saatte başlar** ayarına göre) |
| `due:3` | N gün içinde vadesi gelecek |
| `seviye:B1` · `seviye>=B1` · `sınav:TYT` | Sınıflandırma (anahtar ya da ad); `seviye:*` = değeri olan |
| `kelime:app*` | Alan araması; `*` joker |
| `re:^ab` · `kelime:re:^ab` | Düzenli ifade (tüm alanlar / tek alan) |
| `nid:` `cid:` | Not / kart kimlikleri (virgülle) |
| `-tag:kolay` · `NOT tag:kolay` | Değil |
| `a OR b` · `(seviye:B1 OR seviye:B2)` | Veya, gruplama |
| `seviye!=B1` | `-seviye:B1` ile aynı (değeri olmayanlar da gelir) |

Hatalı bir sorguda uygulama hatanın yerini gösterir. **Göz at** ekranında sorguyu **Sorguyu kaydet** ile adlandırırsın; kayıtlı sorgu, çalışma başlatıcısında (**Kartlar** modu → **Seçenekler**) **Kayıtlı sorgu** olarak da seçilebilir.

## İçe ve dışa aktarma

**İçe aktarma** artık **Kart ekle** sayfasındadır: **Oluştur** → **Ekle** → **Dosyadan** (liste yapıştırmak için **Yapıştır**). **Dışa aktarma, yedek ve yazdırma** **Ayarlar** → **Veri** → **Dışa aktar ve yedek** sayfasındadır; orada "Kart mı içe aktarmak istiyorsun?" bağlantısı seni **Kart ekle › Dosyadan**'a götürür.

### Dosyadan içe aktarma

1. **Kartlar nereye**: deste ve not türünü seç.
2. Dosyayı bırak ya da **Dosya seç…**. Desteklenenler: CSV, TSV ya da metin; Anki (.apkg, .colpkg); Quizlet dışa aktarımı; JSON; deste paketi (.fcpack).
3. **Sütunları eşle**: her sütunun başlığından ne olacağını seç (bir alan rolü, **Etiketler**, bir sınıflandırma ya da **Aktarma**). Uygulama eşlemeyi başlık satırından, son eşlemenden ya da sırayla doldurur ve **Kartlar böyle görünecek** altında ön ve arka yüzü canlı gösterir. Başlık satırı varsa **İlk satır başlık**'ı işaretle.
4. Ayırıcılar yanlışsa **Değiştir**: **Terim ile tanım arasında** (sekme, virgül, noktalı virgül, tire, boşluklar ya da özel) ve **Kartlar arasında** (**Yeni satır**, **Boş satır** — tanım birden çok satırsa — ya da özel).
5. **Kart zaten varsa**: **Atla (önerilen)**, **Var olanı güncelle** ya da **Yine de ekle**.
6. Sınıflandırma sütununda bilinmeyen değerler varsa **Eksik sınıflandırma seçeneklerini oluştur**'u işaretle.
7. **N kartı içe aktar**.

Şu satırların hepsi tanınır:

```
dog, köpek
cat: kedi
Arda	porsuk
"Arda, Ali": porsuk
go	gitmek	I go to school by bus.
```

Dosya kodlaması kendiliğinden çözülür (UTF-8, UTF-16, Türkçe Windows-1254). Anki'nin `#separator:` başlık satırı da okunur. Otomatik algılama her dosyada doğru olmayabilir; önizleme her zaman elle düzeltmeye izin verir. Biçimi kendin seçmek için **Gelişmiş: biçimi elle seç**.

### Quizlet

Quizlet'in dışa aktarma metnini **Yapıştır**'a yapıştır ya da dosyasını seç. Varsayılan: terim ile tanım arası sekme, kartlar arası yeni satır. Quizlet'te özel ayırıcı seçtiysen ayırıcılarda aynısını seç.

### Anki (.apkg, .colpkg)

- **İçe aktarma:** **Dosyadan** ile `.apkg` ya da `.colpkg` dosyasını seç. Pakette kaç not, kart, deste ve medya dosyası olduğu yazar. **Tekrar geçmişini ve zamanlamayı koru** işaretliyse kartlar Anki'deki planıyla gelir (kapatırsan kartlar yeni başlar). Paket kendi destelerini ve not türlerini getirir; yukarıda bir deste seçtiysen hepsi onun altına gelir. Boşluk doldurma, resimli ve sesli kartlar desteklenir; eski ve yeni Anki paket düzenleri okunur.
- **Dışa aktarma:** **Dışa aktar ve yedek** → **Dışa aktar** → **Anki destesi (.apkg)** ya da **Anki koleksiyonu (.colpkg)**. **Tekrar geçmişi ve zamanlamayı ekle** ile plan da gider. Paket, Anki 2.0'dan beri tüm sürümlerin okuduğu eski şemada yazılır. Dışa aktarılan paketler gerçek Anki ile (resmî `anki` kütüphanesi, 26.09) açılarak test edilir: notlar, kartlar, zamanlama, geçmiş, medya ve resim maskeleri (image occlusion) aynen gelir.

### JSON

İnsan tarafından düzenlenebilir not biçimi:

```json
{ "format": "fc-json", "version": 1,
  "notes": [ { "deck": "English::A1", "notetype": "Classic",
               "fields": { "front": "dog", "back": "köpek" },
               "tags": ["animals"], "attributes": { "level": "A1" } } ] }
```

`notetype` boşsa Klasik kullanılır. Alanlar ve sınıflandırmalar anahtarla ya da adla yazılır; alan yerine rol adı da yazılabilir (`term`, `meaning`, `example`…). Dışa aktarmada **JSON (.json)** seç.

### Deste paketi (.fcpack)

Bir desteyi başka bir Flashcard'a taşımanın en kayıpsız yolu: notlar, kartlar, medya, sınıflandırmalar ve (istersen) geçmiş birlikte gider. Paket not kimliklerini ve sınıflandırmaları olduğu gibi korur.

### Dışa aktarma ve yazdırma

**Ayarlar** → **Veri** → **Dışa aktar ve yedek** (deste sayfasındaki **Deste işlemleri** → **Dışa aktar** de buraya gelir). Her sekmenin başında ne işe yaradığı yazar.

- **Dışa aktar** seçili desteyi alt desteleriyle verir; deste seçili değilse tüm koleksiyonu. Biçimler: **Deste paketi (.fcpack)**, **Anki destesi (.apkg)**, **Anki koleksiyonu (.colpkg)**, **Metin (.txt)**, **JSON (.json)**.
- **Yazdır** sekmesinde **Düzen** (**Kart, sayfada 2 × 4**, **Kart, sayfada 3 × 5**, **Liste**) seç, **Hazırla**, sonra **Yazdır / PDF kaydet**. Arka yüzler çift taraflı baskı için aynalanır.

Uzun işler **İptal** edilebilir; iptal edilince hiçbir şey değişmez. Biten içe aktarma tek adımda geri alınır (sonuç ekranında **Geri al** ya da `Ctrl+Z`).

## Eklentiler

Eklentiler korumalı alanda (WASM) çalışır ve yalnız izin verdiğin şeyleri yapabilir. Yeni kart türü, çalışma modu, ses sağlayıcısı ya da not eklenirken alan dolduran kanca ekleyebilirler.

**Kurulum:**

1. **Ayarlar** → **Gelişmiş** → **Eklentiler** → **Dosyadan kur…** → `.fcplugin` dosyasını seç.
2. Pencerede eklentinin adı, sürümü, ekledikleri ve istediği izinler görünür.
3. Vermek istemediğin izinlerin işaretini kaldır. "ayrıca sorulur" yazanlar hassas izinlerdir.
4. **Kur**'a bas.

Eklenti kartında **Aç** / **Kapat**, **İzinler**, **Ayarlar**, **Günlük** ve **Kaldır** var. Kaldırırken **Kaydettiği verileri de sil** seçilebilir; eklentinin oluşturduğu notlar kalır.

Bir eklenti donarsa ya da çökerse uygulama etkilenmez: çağrı süre, iş ve bellek sınırlarıyla kesilir (sınırlar sayfada yazar). Art arda hata veren eklenti kendiliğinden kapanır ("Hatalar yüzünden kapatıldı"); **Aç** ile yeniden açarsın.

Eklenti mağazası henüz yok; eklentiyi dosyadan kurarsın.

## Yapay zeka

Yapay zeka tamamen isteğe bağlıdır: bir sağlayıcı kurana kadar kapalıdır ve hiçbir şey gönderilmez. Kurunca bir metinden kart üretebilir, kart yazarken boş alanları doldurtabilir, çalışırken açıklama ya da ipucu isteyebilirsin.

**Nerede.** **Yapay zeka** sayfası şu an hem **Oluştur** bölümünde hem **Ayarlar** → **Bağlantılar**'dadır; üç sekmesi var: **Kart üret**, **Sağlayıcılar**, **Kullanım**. Sağlayıcı kurma ve kullanım Ayarlar'a, yapay zekayla kart üretme de **Kart ekle** sayfasına bir yol olarak taşınıyor.
<!-- v1.3: verify after X1/X2 -->

**Sağlayıcı ekle.** **Sağlayıcılar** → **Sağlayıcı ekle** bir galeri açar: **Claude**, **OpenAI**, **OpenRouter**, **Google Gemini**, **Groq**, **Mistral**, **DeepSeek**, **Ollama**, **LM Studio** ve **Özel (OpenAI uyumlu)**. Her birinin altında kısa bir açıklama ve "ücretsiz seçenek" ya da "yerel · ücretsiz" etiketi var. Seçince üç adım gelir:

1. **Anahtarı yapıştır**: "Anahtarı buradan alırsın:" bağlantısı sağlayıcının anahtar sayfasını gösterir (ör. OpenRouter için `openrouter.ai/keys`). Anahtar yalnız işletim sisteminin anahtar zincirinde saklanır, veritabanına ya da yedeğe girmez, uygulama onu bir daha göstermez. Ollama ve LM Studio'da bu adım **Sunucuyu bağla**dır, anahtar gerekmez. **Özel**'de adresi ve (gerekiyorsa) anahtarı girersin; kendi bilgisayarındaki ya da ev ağındaki sunucular anahtar istemez.
2. **Model seç**: liste sağlayıcıdan alınır; ara ya da model adını elle yaz.
3. **Bağlantıyı dene**: modele birkaç token'lık kısa bir istek gönderilir ve sonucu görürsün ("Bağlantı çalışıyor." · "… yanıt verdi · 420 ms").

Deneme başarısız olursa sebep düz cümleyle yazar ve sağlayıcının kendi mesajı da gösterilir (anahtar asla): "Anahtar kabul edilmedi…", "Hesapta kredi ya da kota kalmamış…", "Bu model bulunamadı ya da bu anahtarla kullanılamıyor…", "Şu an çok fazla istek var…", "Sunucuya ulaşılamadı…", "Güvenli bağlantı kurulamadı (sertifika)…".

Sağlayıcı listesinde her satır durumunu gösterir (**hazır**, **anahtar gerekli**, **model seçilmedi**, **deneme başarısız**) ve **Bağlantıyı dene** satırın içindedir. Adres **Gelişmiş** altındadır; adres değişirse kayıtlı anahtarın yeni adrese gönderilmesi önce sorulur. Anahtarlar profile aittir: her profilin kendi anahtarları olur. **Varsayılan sağlayıcı** her özellik için kullanılır; **Özellik başına model** ile örneğin kart üretimi için güçlü, kontrol için hızlı bir model seçebilirsin.

**Kart üret:**

1. Bir deste seç.
2. **Kaynak**: **Metin**, **PDF** (sayfa aralığıyla) ya da **Web sayfası**.
3. **Not türü**, **Kaç tane** ve istersen **Ek talimat** (ör. "fiillere odaklan") gir.
4. **Önizle**. **Göndermeden önce** penceresi sağlayıcıyı, modeli, tahmini token ve maliyeti gösterir. **Gönderilecek her şeyi göster** ile tam metni görebilirsin.
5. **Gönder**. Gelen önerilerden istediklerini **Ekle** ile al.

**Toplu** ekleme tablosundaki **AI ile üret** de buraya götürür. Destede bir seviye sınıflandırması varsa üretim seviyeye uygun yapılır.

**Çalışırken:** Çalışma ekranında **AI ipucu**, **Açıkla** ve **AI ile kontrol et** düğmeleri çıkar.

**Maliyet:** **Kullanım** sekmesi bu ayın ve son 7 günün isteklerini gösterir. Aylık bütçe `ai.monthly_budget_usd` ayarıyla konur. Maliyetler fiyat tablosundan tahmindir; **Fiyatlar** tablosunu güncel tut. Ollama ve LM Studio ile model bilgisayarında çalışır, dışarıya bir şey gitmez.

## MCP

MCP, Claude Desktop ve Claude Code gibi asistanların koleksiyonunla çalışmasını sağlar. **Ayarlar** → **Gelişmiş** → **Asistanlar (MCP)** sayfasının iki sekmesi var: **Asistanını bağla** ve **Dış MCP sunucuları**.

**Asistanını bağla:**

1. **Hangi asistanı kullanıyorsun?**: **Claude Desktop**, **Claude Code** ya da **Diğer MCP istemcisi** (Cursor, VS Code…).
2. **Nasıl kurulur?** altında üç adım ve kopyalanacak hazır metin var. Metin bu profilin gerçek `collection.db` yolunu ve `fc` programının yolunu içerir. **Kopyala**, sonra asistanına yapıştır (Claude Desktop: **Ayarlar → Geliştirici → Yapılandırmayı düzenle**; Claude Code: terminalde komutu çalıştır). Asistanı yeniden başlat.
3. **Nasıl denerim?**: asistanına "Destelerimi listele" de. Destelerinin adlarını sayarsa bağlantı çalışıyor.

Sunucu `fc` komut satırı aracıyla çalışır. `fc` kurulumla gelmez: kaynak koddan `cargo build -p fc-cli --release` ile derle (`target\release\fc.exe` oluşur) ya da ayrıca edin. Uygulama `fc`'yi kendi yanında ya da PATH'te bulamazsa "fc programı bulunamadı" der; tam yolunu oradaki kutuya yaz, metin kendiliğinden güncellenir.

Elle kurmak istersen (`C:\Tools\fc.exe` ve koleksiyon yolunu kendininkilerle değiştir):

```bash
claude mcp add flashcard -- "C:\Tools\fc.exe" --db "C:\Users\<sen>\AppData\Roaming\app.flashcard.desktop\collection.db" mcp serve
```

Claude Desktop (`%APPDATA%\Claude\claude_desktop_config.json`; JSON içinde `\` iki kez yazılır):

```json
{ "mcpServers": { "flashcard": {
    "command": "C:\\Tools\\fc.exe",
    "args": ["--db", "C:\\Users\\<sen>\\AppData\\Roaming\\app.flashcard.desktop\\collection.db", "mcp", "serve"] } } }
```

Bu yol ana profilin koleksiyonudur. Başka bir profil için `--db` o profilin dosyasını göstermeli: `...\app.flashcard.desktop\profiles\<kimlik>\collection.db` (bkz. [Veri klasörü](#veri-klasörü)); sayfadaki hazır metin bunu zaten açık profile göre yazar. `fc` o zaman o profilin yapay zeka anahtarlarını kullanır. Uygulama kilidi `fc`'yi ve MCP sunucusunu kapsamaz.

**Asistan neler yapabilir?** **Okuyabilir**: **Desteleri listele**, **Not türlerini listele**, **Destenin sınıflandırmalarını listele**, **Kart ara**, **Not oku**. **Değiştirebilir**: **Deste oluştur**, **Not ekle**, **Notu düzenle**, **Sınıflandırma ata (önce onay ister)**. Silen araç yok.

- Asistanın her değişikliği uygulamada sıradan bir adımdır; `Ctrl+Z` ile geri alınır.
- **Yalnızca okuyabilsin** açıksa asistan destelerini görür ama hiçbir şeyi değiştiremez: değiştiren araçlar ona hiç sunulmaz. Aynısını tek bağlantı için `mcp serve --read-only` yapar.
- **Dış MCP sunucuları** sekmesinde tersi olur: uygulamanın yapay zeka özelliklerinin kullanacağı başka sunucuları **Sunucu ekle** ile eklersin. Uygulama başlatacağı programı gösterip onay ister.

## Tarayıcı eklentisi

Chrome, Edge, Brave gibi Chromium tabanlı tarayıcılar için. Bir sayfada seçtiğin kelimeyi ya da cümleyi uygulamanın **Gelen kutusu**'na gönderir.

**Kurulum:**

1. Eklentiyi yükle. Paketlenmemiş sürümü `chrome://extensions` → **Geliştirici modu** → **Paketlenmemiş öğe yükle** ile eklenti klasörünü seçerek kurarsın.
2. Uygulamada **Yerel API** modülünü aç (varsayılan kapalı): `Ctrl+K` → **Modülü aç/kapat** → `{"id": "local_api", "enabled": true}`. **Ayarlar** → **Gelişmiş** altında **Yerel API** belirir. Varsayılan port `28735`.
3. Eklenti düğmesi → **Bağlan**. Uygulamada **Bağlantı isteği** penceresi açılır; izin ver.

**Kullanım:**

1. Sayfada metni seç → sağ tık → **Desteye ekle** (ya da `Alt+Shift+A`, ya da araç çubuğu düğmesi).
2. **Ön yüz**, **Arka yüz**, **Deste** ve destenin **Sınıflandırmalar**'ını (ör. Seviye) doldur.
3. **Gelen kutusuna ekle**.
4. Uygulamada **Gelen kutusu** → öneriyi düzenle → **Kabul et** ya da **Reddet**. Sen kabul etmeden hiçbir şey karta dönüşmez. Destesi olmayan bir öneri önce deste ister.

**Yakalanan cümle kaybolmaz.** Her satır hangi kart türüne dönüşeceğini söyler. Bir öğe cümleyle geldiyse cümle notun örnek cümle alanına, sayfa kaynak alanına yazılır; not türü seçilmemişse öğe cümleyi tutabilen **Altyazı kartı** olur ("Kart türü: Altyazı kartı (cümleyle geldiği için; cümle kartta kalır)"). Düzenleyip kabul ettiğinde de cümle ve diğer alanlar korunur.

Uygulama kapalıyken eklenen öğeler eklentide bekler ve uygulama açılınca gönderilir. Eklenti yalnız `127.0.0.1`'e bağlanır. Portu değiştirdiysen eklentinin ayarlarında **Yerel API portu**'na aynı değeri yaz. Bağlı programları ve izinlerini **Yerel API** ekranında görür, kaldırabilirsin.

**Program başına günlük öneri** (`local_api.inbox_daily_max`, varsayılan 200): bir programın 24 saatte önerebileceği en fazla kart. `Ctrl+K` → **Ayar değiştir** → `{"scope": {"kind": "app"}, "key": "local_api.inbox_daily_max", "value": 100}` ile değiştirirsin. Gelen kutusu önerilerini sen bakmadan kabul eden bir otomasyon kuralı **Otomasyon** ekranında uyarıyla işaretlenir: eşleştirilmiş bir programın ya da eklentinin önerdiği her şey o zaman hemen not olur.

Gelen kutusuna ayrıca `flashcard://` bağlantılarından ve `Ctrl+Alt+N` hızlı ekleme penceresinden öneri düşer.

### Subtitle (video altyazıları)

**Subtitle** tarayıcı eklentisi video altyazılarında **+** ile eklediğin kelimeyi doğrudan bir destede karta çevirir (gelen kutusuna düşmez): kelime ve anlamı, geçtiği cümle (kelime kalın), çevirisi, ekran görüntüsü, ses kaydı ve videonun o ana bağlantısı.

1. Uygulamada **Subtitle** modülünü aç (varsayılan kapalı): `Ctrl+K` → **Modülü aç/kapat** → `{"id": "subtitle", "enabled": true}`. **Ayarlar → Bağlantılar** altında **Subtitle** belirir.
2. **Subtitle** ekranında bir ad yazıp **Anahtar oluştur**'a bas ve anahtarı kopyala. Anahtar **yalnız bir kez** gösterilir.
3. Eklentide **Ayarlar → Öğrenme servisi → Subtitle Kartları**'nı seç; **Sunucu adresi**'ne ekrandaki adresi (`http://127.0.0.1:28735/subtitle`) ve **Erişim anahtarı**'na anahtarı yapıştır, **Bağlantıyı test et**'e bas.

Kelimeler eklentide seçtiğin desteye, seçmediysen ekrandaki **Deste**'ye, o da yoksa ilk kelimede oluşturulan **Subtitle** destesine gider. Aynı kelime aynı cümleden iki kez gelirse tek kart olur; başka bir cümleden gelirse yeni kart. Her kelime tek geri al adımıdır. Uygulama kapalıyken eklenti kelimeleri kuyrukta tutar, uygulama açılınca gönderir. Bir anahtarı **İptal et** ile hemen geçersiz kılarsın.

## Otomasyon

**Ayarlar** → **Gelişmiş** → **Otomasyon**: tekrar eden işleri uygulamaya bırakırsın. **Hazır bir kuralla başla** altında altı tarif var; her biri şu an kaç kartın uyduğunu canlı gösterir:

- **Sürekli unuttuğum kartları etiketle** (4. unutuşta "zor" etiketi)
- **Bir türlü akılda kalmayanları askıya al** (8 unutuşta)
- **Öğrendiğim kartları arşiv destesine taşı** (90 günden uzun hatırlayacakların)
- **Yeni Subtitle kartlarına etiket ekle**
- **Kırmızı bayraklı kartları askıya al**
- **Kolaylaşan kartlardan “zor” etiketini kaldır** (günde bir kez)

Tarif dolu olarak açılır; etiketi, desteyi ya da sayıyı değiştirebilirsin, kaydetmeden hiçbir şey olmaz. Kurallar tek, düzenlenebilir bir cümle olarak görünür; ham düzenleyici **Gelişmiş** altındadır. Çalışma geçmişi okunur bir listedir.

## Profiller ve uygulama kilidi

İkisi de **Ayarlar** → **Genel** → **Profiller ve kilit** ekranındadır (profil menüsünde de **Profil değiştir** ve **Şimdi kilitle** var).

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

**Ayarlar** → **Genel** → **Hatırlatıcılar** ekranında:

- **Her gün hatırlat** günde bir işletim sistemi bildirimi açar ("Bugün 35 kart seni bekliyor (20 tekrar, 15 yeni)."); varsayılan kapalı. Sayı, **Çalışmaya başla**'nın gerçekten getireceği kartlardır: günlük yeni kart ve tekrar sınırlarına uyar, henüz başlamadığın her kartı saymaz.
- **Saat** (varsayılan 19:00) ve **Günler** (varsayılan her gün).
- O gün zaten çalıştıysan ya da bekleyen kart yoksa hatırlatma gelmez ("bugün", **Yeni gün şu saatte başlar** ayarına göre sayılır). Uygulama o saatte kapalı ya da uykudaysa hatırlatma 3 saate kadar gecikmeli gelir.
- **Deneme bildirimi gönder** hemen bir tane gösterir.
- **Pencereyi kapatınca uygulama tepside kalsın**: pencereyi kapatmak onu yalnız gizler, hatırlatmalar gelmeye devam eder. Hatırlatma yalnız uygulama çalışırken (pencere ya da tepsi) gelir.

Tepsi simgesinin menüsü: **Flashcard'ı aç**, **Kilitle** (kilit kuruluysa kilitler), **Çık** (gerçekten kapatır). Simgeye tıklamak da pencereyi geri getirir.

## Çökme kurtarma ve günlükler

**Kurtarma.** Uygulama açılışta bir çalışma işareti (`run.json`) yazar, normal kapanışta siler. Sonraki açılışta hâlâ duruyorsa (çökme, görevi sonlandırma, elektrik kesilmesi) veritabanını denetler (`maintenance.check_after_crash` ayarı, varsayılan açık) ve **Flashcard düzgün kapanmadı** (çökmeden sonra **Flashcard beklenmedik şekilde kapandı**) penceresini gösterir. Pencere devam edebileceklerini listeler: veritabanı denetiminin sonucu, bitmemiş bir çalışma oturumu (**Çalışmaya devam et**), kaydedilmemiş formlar (yeniden açtığında geri gelir) ve çökme raporu. **Bakım'ı aç** **Günlükler ve güncellemeler**'e götürür, **Devam et** pencereyi kapatır. Koleksiyon her değişiklikten sonra kaydedilir; başka bir şey kaybolmaz.

**Günlükler ve güncellemeler** ekranı (**Ayarlar** → **Gelişmiş**): sürüm, son çalıştırmanın düzgün kapanıp kapanmadığı, güncellemeler, **Günlük klasörü** ve **Veri klasörü**, **Veritabanını denetle**, çökme raporları ve **Çökme raporlarını sil**.

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

**Ayarlar** → **Veri** → **Dışa aktar ve yedek** → **Yedek**:

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
Eşleme adımında sütun başlıklarından doğru alanı seç; ayırıcılar yanlışsa **Değiştir** ile **Terim ile tanım arasında**'yı düzelt (ör. **Sekme**). İçe aktarmayı zaten yaptıysan sonuç ekranında **Geri al** ya da `Ctrl+Z`.

**İçe aktardığım listenin örnek cümleleri kayboldu.**
Seçtiğin not türünde örnek cümle alanı yoksa cümle sütunu bir yere gitmez; uygulama bunu eşleme adımında söyler. **Kelime kartı** gibi örnek cümle alanı olan bir türe geç ya da kendi türünde bir alana **Örnek cümle** rolünü ver (**Not türleri** → **Ne içeriyor?**).

**İçe aktarma Ayarlar'da yok.**
v1.3'te taşındı: **Oluştur** → **Ekle** → **Dosyadan**. Dışa aktarma ve yedek **Ayarlar** → **Veri** → **Dışa aktar ve yedek**'te.

**Boşluk'a bastım ama kart puanlanmadı.**
Doğru: `Boşluk` yalnız kartı çevirir. Puan için arka yüzü gördükten sonra `1`–`2` (dört düğmede `1`–`4`) tuşuna ya da düğmeye bas.

**`Esc`'ye bastım, oturumum kayboldu mu?**
Hayır, ara verdin. Başlatıcının en üstündeki **Devam et** aynı oturuma döner.

**Yanlışlıkla bir şey sildim.**
`Ctrl+Z` ile geri al. Deste ya da not çöpe gittiyse **Ayarlar** → **Gelişmiş** → **Bakım** → **Çöp kutusu** → **Geri getir**.

**"Geri alma burada durur" diyor.**
Koleksiyon bir yedekten geri yüklendi ya da çöp kalıcı olarak boşaltıldı. Daha önceki geçmiş geri alınamaz.

**"Şu an çalışılacak kart yok." / Bugünün kartları boş.**
Başlatıcı nedenini kapsamın altında yazar: sırası gelen kart yok, günlük sınır doldu ya da sıradaki kart birkaç dakika sonra. Yine de çalışmak istersen **Bugün çalıştıklarım**, **Bugün zorlandıklarım** ya da **Tüm kartlar**'ı seç (pratik, takvim değişmez) ya da **Seçenekler** → **Sorgu**'ya bir sorgu yaz (`deck:İngilizce`). Günlük sınırı **Planlama** → **Temponu planla** ya da **Deste ayarları**'ndan değiştirirsin.

**Tarayıcı eklentisi "Flashcard uygulamasına ulaşılamıyor" diyor.**
Uygulama açık mı, **Yerel API** modülü açık mı, port eklentideki ile aynı mı kontrol et. Öğe kaybolmaz; eklentide bekler.

**Claude koleksiyonu değiştiremiyor.**
**Asistanlar (MCP)** ekranında **Yalnızca okuyabilsin**'i kapat ve `--read-only` kullanmadığından emin ol. Sunucu eski şemalı bir koleksiyonu açmaz: önce uygulamayı bir kez aç.

**Yapay zeka "Bu sağlayıcı API anahtarı istiyor" diyor.**
**Yapay zeka** → **Sağlayıcılar**'da sağlayıcının satırında anahtarı kaydet ve **Bağlantıyı dene**'ye bas; sonuç sorunun ne olduğunu düz cümleyle söyler. "Sistem anahtar zinciri yok" uyarısı varsa anahtar saklanamaz.

**Bir eklenti "Hatalar yüzünden kapatıldı".**
**Günlük**'e ve **Son hata**'ya bak, sonra **Aç** ile yeniden dene. Sorun sürerse kaldır.

**Bir menü görünmüyor.**
Modülü kapalı olabilir. `Ctrl+K` → **Modülleri listele** ile durumuna bak, **Modülü aç/kapat** ile aç. Bir ayarın nerede olduğunu bilmiyorsan **Ayarlarda ara** kutusuna yaz.

**Veritabanında sorun olduğunu düşünüyorum.**
**Ayarlar** → **Gelişmiş** → **Bakım** → **Veritabanı denetimi** → **Denetle**. Sorun çıkarsa **Onar**; onarım da geri alınabilir.

**Windows "Windows kişisel bilgisayarınızı korudu" diyor.**
Bu SmartScreen; kurulum dosyası kod imzalı değil. **Ek bilgi** → **Yine de çalıştır**'a tıkla. Kurulum dosyasını yalnız projenin yayın sayfasından indir.

**Uygulama kilitli olduğunu söylüyor ve kilidi kaldırmamı istiyor.**
**Kilit bilgisi kayıp** ekranı, profilin kilitli olduğunu ama PIN/parola özetinin kaybolduğunu (ör. anahtar zincirindeki kayıt silinmiş) gösterir. Uygulama kontrolsüz açılmak yerine kilitli kalır. "Anladım: bu profilin kilidini kaldır"ı işaretle, **Kilidi kaldır**'a bas, sonra **Profiller ve kilit**'ten yeni bir kilit kur. Kartların etkilenmez.

**Profil değiştirince uygulama yeniden başladı.**
Bu beklenen davranış: başka bir profilde **Aç**, uygulamayı o profilin koleksiyonuyla yeniden başlatır. Geri dönmek için aynısını yap.

**Günlüklerim nerede? Hata raporunu nasıl gönderirim?**
**Günlükler ve güncellemeler** **Günlük klasörü**'nü gösterir (etkin profilin klasöründeki `logs`, bkz. [Veri klasörü](#veri-klasörü)). **Günlükleri dışa aktar…**'a bas, `.zip` dosyasını kaydet ve raporuna ekle. Gizli bilgiler çıkarılır, hiçbir şey kendiliğinden gönderilmez.

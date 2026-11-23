# 11 · Tasarım (v1.3)

v1.3 temayı değiştirmez; **akışları kullanılır kılar.** v1.2'nin tasarım sistemi (koyu öncelikli, sık, tek kehribar vurgu, sayılar mono, gölge yerine kenarlık) aynen geçerli (§4). Değişen şey düzenleyici fikir: bu doküman artık ekranlardan değil **akışlardan** (§1) başlar. Hiçbir özellik, ayar, kısayol ya da modül anahtarı kaldırılmadı; taşınanlar hâlâ ulaşılabilir (palet dahil). Bu doküman bir yönergenin değil, **koddaki gerçek hâlin** kaydıdır.

## 0. Neden v1.3

Proje sahibi v1.2'yi Windows'ta denedi: *"Daha güzel görünüyor, altyapı sağlam duruyor ama kullanamıyorum. Gerçek kullanıcı deneyimi istiyorum."* Şikâyetleri: "Çalış'a basıyorum başlayamıyorum; bazen soruyor bazen sormuyor; modu seçemiyorum", `Space` iki iş yapıyordu (çevir **ve** puanla), ipuçları anlamsızdı, örnek cümle cevabın parçası gibi görünüyordu, jargon (göm / askıya al / yeni), içe aktarma ayarların içinde saklıydı, "bugün çalıştıklarımı yeniden çalışayım", ayarlar ve not türleri kafa karıştırıcı, yalnız Claude vardı (OpenRouter yok), MCP ve otomasyon kullanılamıyordu.

Dürüst teşhis: v1.2 **sahte (mock) bir backend'e karşı ekran görüntüleriyle** doğrulandı. Görüntüler güzeldi; ama Ana sayfanın "Bugün 1 kart" dediği koleksiyonda `study.start` sıfır kart veriyordu, `Ctrl+Z` bir yazı alanında kayıtlı kartı siliyordu, altyazı kartının yazılı cevap anahtarı anlam + cümle + kaynaktı. Hiçbiri bir görüntüde görünmez. v1.3'ün iki dersi:

1. **Testin sorusu "güzel mi?" değil, "oturup klavyeyle tahmin etmeden işini yapabiliyor mu, ve KAYDEDİLEN sonuç yaptığıyla aynı mı?"** (§5).
2. **Ekranın gösterdiği her sayı, düğmenin yapacağı işle aynı yerden gelir.** "Bugün 12 kart" yazıyorsa Başla 12 kart verir.

v1.1 → v1.2 geçmişi (ferah Quizlet esinli arayüzün reddi, koyu sık araç) [CHANGELOG 1.2.0](../CHANGELOG.md)'dadır.

## 1. İlkeler ve akışlar

### İlkeler

1. **Önce akış.** Her ekranın testi: *kullanıcı buraya ne yapmaya geliyor, bu kaç tuş sürüyor ve sonunda ne kaydedildi?*
2. **Bir tuş = bir iş.** Hiçbir tuşa gizli bir duruma bağlı ikinci bir anlam verilmez (§1.2).
3. **Tek doğru kaynak.** Sayılar, boş durumların nedenleri, cevap anahtarı, ipucu Rust'ta hesaplanır; ön yüz yalnız gösterir (kural 1). Aynı soruya iki yerde iki cevap olmaz.
4. **Ekran ne için olduğunu söyler.** Her sayfa bir cümleyle açılır; boş durum sıradaki adımı söyler; ayar "bunu değiştirirsem ne olur?" sorusunu cevaplar; jargon yok, **bir kavram tek sözle** (göm → "Yarına bırak", askıya al → "Kenara koy", vadesi gelen → "Sırası gelen"; EN Leave for tomorrow / Set aside / Due; "Ara ver" / Pause yalnız oturuma ara vermektir).
5. **Klavye öncelikli, fare de tam çalışır;** birincil eylem üzerine gelmeden görünür.
6. **Sade ≠ tasarımsız** (kural 9). Aşamalı gösterim: ikincil ayrıntılar "Diğer" / "Gelişmiş" altında.

### Akışlar

Düzenleyici fikir: her ekran bir akışın parçasıdır. Ana akışlar ve tuş sayıları:

| Akış | Ne yapılır | Tuş |
|---|---|---|
| Bugünü çalış | Ana sayfada `Enter` → başlatıcı açılır (yarım oturum varsa "Devam et" odakta) → `Enter` → `Space` çevir → `1`/`2` (ya da `1`–`4`) | 2 + kart başına 2 |
| Bugün zorlandıklarını yeniden çalış | Başlatıcıda `↓` ile "Bugün zorlandıklarım" → `Enter` (pratik, takvim değişmez); oturum sonunda "Tekrar çalış" bunu seçili açar | 3 |
| Kelime ekle | Herhangi bir yerde `N` → terim → `Tab` → anlam → (`Tab` → örnek cümle) → `Ctrl+Enter` (pencere açık kalır) | 1 + yazı + 2–3 |
| Çok kelime ekle | Oluştur → Toplu / Yapıştır / Dosyadan → sütunlar eşlenir, önizleme → ekle (tek geri al adımı) | yazı + 1 |
| Kartı bul ve düzelt | `Ctrl+K` ya da Ara'da `/` → sorgu → `↓` → `Enter` düzenle → `Ctrl+Enter` kaydet; ya da deste sayfasının terim listesinde satır içi | 4 + yazı |
| İlerlemeyi gör | İstatistik: "Bugün nasılım? / Önümde ne var? / Nelerde zorlanıyorum?" → "Bunları çalış" başlatıcıyı açar | 1–2 tıklama |
| Bir sağlayıcı bağla | Ayarlar → Bağlantılar → Yapay zeka → "Sağlayıcı ekle" → hazır seçenek (ör. OpenRouter) → anahtar → model → Test (kart üretmek ayrı: Oluştur › Yapay zeka) | birkaç tıklama + anahtar |

### 1.1 Çalışma başlatıcısı (`features/study/StudyLauncher.svelte`, `LauncherHost`)

"Çalışmak istiyorum" demenin **tek** yolu. Her giriş aynı iletişim kutusunu açar: Ana sayfa (düğme, satırlar, `Enter`), Kütüphane satırı, deste sayfasının "Çalış"ı (`Enter`) ve mod düğmeleri (o mod seçili), palet (ekranlar ve ham `study.start` komutu), kendi başına açılan mod sayfaları, oturum sonundaki "Tekrar çalış", İstatistik'in "Bunları çalış"ı ve kart eklemenin "Şimdi çalış"ı. Eski mod başına kurulum panelleri kalktı.

v1.4 (onaylanacak taslak `prototypes/v14/modlar.html`, önerilen A seçenekleri): iki sütun, hepsi `study.options`'tan (Rust, yan etkisiz sorgu; docs/02):

1. **Yarım kalanlar** — varsa en üstte ve odakta: "Devam et · 7 cevaplandı · 13 kaldı". `Enter` aynı oturuma döner.
2. **Nasıl çalışmak istersin?** — `study.options.modes` ne listeliyorsa (modülü kapalı mod yok): Kartlar · Test (dört şık, cevap hemen; arka uçta `quiz`) · Öğren · Sınav (karışık kâğıt; arka uçta `test`) · Eşleştir. Her satır simge + ad + **tek satır "ne yapar"** (`description_key`) + **"takvimi etkiler" / "pratik"** etiketi (`affects_schedule`); altında "Takvim = …" açıklaması. Radyo grubu: oklar gezer, `Tab` tek durak.
3. **Hangi kartlar?** — beş kapsam tam sayılarıyla; **aşamalı**: önce yalnız "Bugünün kartları", diğer dördü "Başka kartlar seç (4)" ile bir tık uzakta (seçili kapsam bugün değilse, ör. bugün boşken, kendiliğinden açık). Pratik kapsam etiketi yalnız takvimi etkileyen modda görünür. Boş kapsam devre dışıdır ve nedenini söyler. Aç/kapa sabiti: `launch.ts` `SCOPES_PROGRESSIVE` (B = hepsi açık).
4. **Sınavda neler olsun?** — yalnız Sınav seçiliyken, hep görünür: soru türleri onay kutusu (API'nin `question_kinds` / `default_question_kinds`'i; doğru/yanlış kapalı başlar), "Kaç soru?" (`default_questions`). Hiç tür yoksa Başla kapalı, nedeni altbilgide.
5. **Gelişmiş** — tek anahtar (altbilgide, cihazda hatırlanır): kart sayısı, pratik, süre/güven, sınıflandırma, sorgu + kayıtlı sorgu, erteleme. Kapalıyken başlatıcıda yalnız mod ve kartlar vardır.
6. **Altbilgi** — "Test · Tüm kartlar · 30 kart" + takvime ne olacağı tek cümle ("Pratik: takvimin değişmez" / "Takvimi etkiler: …") ve **Başla**. Başlarken Rust yeniden doğrular; o an boşsa (`CONFLICT_NOTHING_TO_STUDY`) iletişim kutusu nedeniyle açık kalır, **hiçbir oturum açılmaz ve yarım oturum kapanmaz.**

**Oturum sonu (`features/modes/SessionSummary.svelte`)** her modda aynı kalıp: puan halkası, tek cümle, birkaç sayı, **takvime ne olduğu tek satır** ("Pratikti: takvimin değişmedi" / "Takvimin güncellendi: N kartın sıradaki tekrarı ayarlandı"), "Yanlışları tekrar çalış (`R`)", "Tekrar çalış…", "Tamam" ve kaçırılanlar listesi (Test: kelime · doğrusu · senin seçimin; `modes.quiz.result`). Test, Öğren ve (kendi sonuç sayfasında takvim satırıyla) Sınav kullanır.

**Test ekranı (`features/modes/QuizRun.svelte`, `/learn` rotasında açılır).** Kelime, en çok dört anlam; `1`–`4` ya da tık; doğru/yanlış ve doğru şık hemen, altında kartın arka yüzü (cümle, kaynak). `Enter` sonraki; doğru cevap 1,1 sn sonra kendiliğinden geçer (sabit `modes.ts` `QUIZ_AUTO_NEXT_MS`, B = 0), yanlışta doğrusunu okuman için `Enter` bekler. `Esc` ara verir: oturum çekirdekte açık kalır (Devam et burada, Ana sayfada ve başlatıcıda), `Space` hiçbir şey yapmaz.

Klavye: `Enter` başlatır (odak "Devam et"te ya da kendi `Enter`'ı olan bir kontroldeyse o kontrol), `↑`/`↓`/`←`/`→` kapsamlar arasında, `Tab` sayı ve seçenekler, `Esc` kapatır. Son mod deste başına, son kapsam mod başına hatırlanır (cihazda). Ana sayfa sayısı sıfırken bile ölü bir "Başla" sunulmaz: başlatıcı pratik kapsamlarını gösterir.

**"Tüm desteler"** (deste seçilmemiş, `deck_id: null`) kuyruğu destelerin kendi kuyruklarının birleşimidir: her deste kendi ayar şablonuyla (günlük yeni/tekrar sınırı, sıra, kardeşleri yarına bırakma) planlanır, öğrenme kartları vade sırasıyla önde, tekrarlar ve yeniler desteler arasında sırayla (`plan.rs` `plan_collection`). Kardeşleri yarına bırakma desteler arasında da geçerlidir. Koleksiyon çapında üst sınır yalnız istenirse ya da `study.new_per_session` uygulama genelinde ayarlıysa. Ana sayfa, başlatıcı, `decks.summaries.today` ve hatırlatıcı aynı planlayıcıyı (`today_counts`) kullanır.

**Bekleme durumu.** Kalan kartların hepsi öğrenme adımında bekliyorsa oturum kapanmaz: "Sıradaki kart N dk sonra" + geri sayım, **Bekle** / **Başka bir şey çalış** (başlatıcı) / **Bitir**. Önden öğrenme penceresi bir ayardır (`study.learn_ahead_mins`, "Öğrenme kartını erken göster", varsayılan 20 dk, 0 = hep bekle); `next_due` kartın gerçekten sunulacağı andır, bekleme ekranı yalnız gerçekten beklemen gerektiğinde görünür. Oturum açıkken pencereyi küçültmek ekrandaki kartı almaz: oturum, kartı seçtiği pencereyi bir sonraki cevaba kadar korur; büyütmek hemen geçerlidir.

**Ara vermek.** Kartlar oturumunda `Esc` oturumu **duraklatır** (puanın hemen ardından basılan `Esc` de kaybolmaz: cevap kaydedilince ara verir): o ana kadarki özet ("Ara verdin — kaldığın yerden devam edebilirsin") ve odakta **Devam et** (`Enter` aynı oturuma döner; "Tamam" deste sayfasına ya da Ana sayfaya götürür, oturum açık kalır); oturum açık kalır, başlatıcının ilk satırı da onu sunar. Tür başına tek açık oturum vardır (Kartlar, Test, Öğren, Sınav, Eşleştir); Sınav kâğıdı cevaplarını taslak olarak saklar ve devam edince geri yükler.

### 1.2 Klavye modeli: bir tuş, bir iş

Kural: **bir tuşun anlamı gizli bir duruma bağlı olarak değişmez.** Uygulaması `features/study/keys.ts` (`keyAction`, saf fonksiyon, testli) ve `$lib/keyboard` (`isEditableTarget`).

| Tuş | Tek işi | Neyi asla yapmaz |
|---|---|---|
| `Space` | Kartı çevirir, ön ⇄ arka, her basışta | Puanlamaz (v1.2'de ikinci basış "Biliyorum" sayıyordu); Test'te ve Öğren'de hiçbir şey yapmaz (v1.4: Öğren'de "devam" değil) |
| `1`–`4` (2'li düzende `1`/`2`) | Puanlar: Tekrar / Zor / İyi / Kolay (2'li: Hâlâ öğreniyorum / Biliyorum) | Arka yüz görülmeden hiçbir şey yapmaz; 2'li düzende `3`/`4` boş. Şıklı kartın sorusunda şık seçer (şıklar ekranda) |
| `Enter` | Odaktaki kontrole basar | Sayfa kısayolu değildir; kartı odakta olmadan çevirmez/puanlamaz |
| `H` | İpucu (yalnız soru yüzünde) | — |
| `Z` | Son puanı geri alır (puan kaydedilirken basılırsa kaydı bekler, sonra geri alır) | — |
| `S` · `B` | Kartı kenara koy (sen geri alana kadar sorma) · yarına bırak (bugün bir daha sorma) | — |
| `Esc` | Oturumdan çıkar = **ara verir** (oturum açık kalır, Devam et döner) | Oturumu bitirmez, kart silmez |
| `Ctrl+Z` | Uygulamanın geri alması (`oplog.undo`) | **Yazı alanındayken** asla: orada metni geri alır (v1.2'de kayıtlı bir kartı siliyordu) |
| `Ctrl+A` | Ara'da tüm satırları seçer | Yazı alanındayken metni seçer |
| `N` | Hızlı Ekle | Yazı alanında, açık bir iletişim kutusunda, oturumda ve açık bir Select'te harf yazarken tetiklenmez |

Ortak kurallar: **tuş yalnız ekranda gördüğüne etki eder** — puanlanan kart, cevabı kaydedilirken olduğu gibi (arka yüzü) ekranda kalır ve sonraki kartın ön yüzü gelene kadar `Space`, rakamlar, `H`, `S`, `B` ve bayraklar ona hiçbir şey yapmaz ve sonraki karta da **saklanmaz** (kart kendi kendine öne dönmez); yalnız `Z` (puanı geri alır) ve `Esc` (kayıttan sonra ara verir) işler (`keys.ts` `leaving` evresi, K1); **basılı tutulan tuş tek iş yapar** (`event.repeat` yok sayılır: Kartlar, Öğren, Test, Eşleştir, Ara, terim listesi, tablolar, düzenleyiciler, Gelen kutusu, Hızlı Ekle, Oluştur); **yazı alanında yerel düzenleme kazanır**, açık bir iletişim kutusu varken kabuk kısayolları çekilir; ok tuşları gezinme ve metin içindir. Kısayol düzenleyicisinde değiştirilen tuşlar ipucu satırına, `aria-keyshortcuts`'a ve `?` yardımına yansır; `study.reveal` eylem kimliği kayıtlı tercihler bozulmasın diye korundu, işi artık "çevir".

### 1.3 Kart ekleme: tek yer (`features/notes/CreateScreen.svelte`, `add-way.svelte.ts`)

Oluştur sayfası "Nasıl eklemek istersin?" diye açılır: **Tek kart** (Hızlı Ekle penceresi, `N`) · **Toplu** (ızgara) · **Yapıştır** · **Dosyadan**. Dördü aynı hedef seçiciyi (yeni / var olan deste, not türü) ve aynı onayı paylaşır: "12 kart eklendi · Geri al · Desteyi aç · Şimdi çalış", atlanan satırlar nedenleriyle. Sayfa son kullanılan yolla açılır.

- **Hızlı Ekle:** önce terim / anlam / isteğe bağlı örnek (rollere göre, §1.5), diğer alanlar "Diğer" altında; yinelenen kart satır içinde açıklanır; altta aynı destede Toplu / Yapıştır / Dosyadan bağlantıları. `Ctrl+Enter` ekler, pencere açık kalır.
- **Yapıştır ve Dosyadan** tek rehberli akış (`io.import.*`): kaynak → sütunlar not türünün alanlarına **rol adlarıyla** eşlenir (canlı ön/arka önizleme), ayırıcı algılanır ve değiştirilebilir, yinelenen kural düz cümleyle → içe aktarma işi, sonuç, geri al. Eşleme başlıktan (TR/EN rol kelimeleri: "sentence", "örnek", "definition"…), yoksa not türünün son eşlemesinden, yoksa rol sırasından (terim, anlam, örnek…) gelir. Sürükle-bırak çalışır.
- İçe aktarma artık Ayarlar'da değil; **Ayarlar › Veri › Dışa aktar ve yedek** dışa aktarma, yedek ve yazdırmayı tutar ve içe aktarma için buraya bağlantı verir.
- **Oluştur, bulunduğun desteyle açılır:** bir desteden gelindiyse "Var olan deste" o deste seçili, değilse ilk deste; "Yeni deste" açık bir seçimdir. İmleç ilk terim hücresindedir.
- **Hızlı Ekle** örnek alanı olmayan bir türdeyken cümle yazmak isteyene "Kelime kartı" türünü tek tıkla önerir.
- **Oluştur › Yapay zeka** yalnız "Yapay zekayla kart üret"tir (deste formdaki bir alan; sağlayıcı yoksa "Sağlayıcı bağla" Ayarlar'a götürür). Sağlayıcılar ve kullanım **Ayarlar › Bağlantılar › Yapay zeka**'dadır.

**v1.4 (E3, prototip `prototypes/v14/olustur-ara-istatistik.html`):** başlık **Ekle**; sekmelerin yerine solda bir **yollar rayı** (`features/notes/AddWays.svelte`), iki grup: *Kendin yaz* (Tek kelime · Liste yapıştır · Satır satır) ve *Bir yerden getir* (Dosyadan · Yakalananlar (bekleyen sayısıyla) · Bir metinden seç · Yapay zekayla · Hazır desteler). Her yol tek satırla ne yaptığını söyler. Bu sayfada açılan yollar dikey bir sekme listesidir (`role="tablist"`, `aria-orientation="vertical"`, `↑`/`↓` Home/End); kendi sayfası olan yollar (Gelen kutusu, Okuma, Yapay zeka, Başlangıç desteleri) ↗ işaretli düğmelerdir ve o sayfayı açar. Modülü kapalı yol çizilmez. 1000 px altında ray, açıklamasız sarmalanan bir satıra döner. Hızlı Ekle penceresi (`N`) kaldı (prototip sorusu 5 için B; kapatmak kabuk işi).

### 1.4 Ayarlar kiti: ayar öğretir (`features/core/kit`)

Kurallar (her ayar sayfası uyar):

- **Sayfa bir cümleyle açılır:** bu sayfa ne için, ne zaman dokunursun (`SettingsPage`).
- **Her satır sonucu söyler:** `FormRow` 14 px etiket + **gövde boyunda (13 px) açıklama** "bunu değiştirirsem ne olur?"; isteğe bağlı somut örnek ("Örnek: Günde 20 yeni kart → …"), "varsayılan" işareti, değişmiş değerde **"Varsayılana dön"** (`settings.reset`, geri alınabilir). ≤ 900 px pencerede ya da dar sütunda kontrol etiketin altına iner.
- **Teknik olan "Gelişmiş" altında:** `SettingsAdvanced` sayfa başına kapalı başlar, durumu hatırlanır, arama sürerken kendiliğinden açılır. Not türlerinde şablon kodu/CSS/alan bayrakları, Planlama'da optimizasyon, Otomasyon'da ham kural düzenleyicisi, Yapay zeka sayfasında adres oradadır.
- **Gruplar göreve göre**, en sık kullanılan önce: Genel · Çalışma ve kartlar · Bağlantılar · Veri · ayrı çizgiyle **Gelişmiş** ("çoğu zaman gerekmez"). Genel sayfası "Ne değiştirmek istiyorsun?" sorularıyla başlar. Solda üstte **arama** (yalnız sunum: sayfa adları + i18n alt ağaçları, `settings-index.ts`), eşleşen satır vurgulanır.
- **Yıkıcı eylem** sayfanın sonunda `tone="danger"` bölümde ve onaydan önce mevcut kartlara tam olarak ne olacağını söyler.
- Kit üstüne kurulu sayfalar: **Not türleri** öğrenen görünümüyle açılır (tür kutucukları, alan listesi "Bu alan ne içeriyor?" rol seçicisiyle, her kart şablonu için "Bu kart neyi soruyor?", çevrilebilir canlı önizleme; hazır başlangıç noktalarından "Yeni not türü"; şablon kodu Gelişmiş'te; kaydetmeden/silmeden önce tam veri kaybı uyarısı). **MCP** iki sekme: "Asistanını bağla" (istemci seçici, bu profilin gerçek `collection.db` ve `fc` yollarıyla parça, kopyala, test, salt okunur anahtarı; `mcp.setup`) ve "Dış MCP sunucuları". **Otomasyon** altı hazır tarifle (canlı etkilenen kart sayısı), kural tek düzenlenebilir cümle, okunur çalışma geçmişi. **Yapay zeka** sağlayıcısı hazır seçeneklerden üç adımla (anahtar → model → test), anlaşılır hata sınıflarıyla (docs/06 §1.1). Hazır ayarlar, hatırlatıcılar, profiller/kilit, eklentiler (sandbox sınırları görünür), Subtitle ve yerel API sayfaları da ne yaptıklarını söyler.

### 1.5 Kelime kartları

Bir alan **ne içerdiğini** (rol: terim, geçtiği hâli, anlam, örnek, örnek çevirisi, kaynak, not), bir kart şablonu **neyi sorduğunu** (`answer_field`) söyler (docs/02, docs/03). Arayüze etkisi:

- **Kart arkası üç etiketli parça,** tipografi seviyesiyle: **anlam** (en büyük) › **örnek** (cümle + çevirisi, ayrı ve "Örnek" etiketli; cevabın parçası gibi görünmez) › **kaynak** (en küçük, "Kaynak" etiketli). Sınıflar Rust'tan gelir (`.fc-part-meaning`, `.fc-part-example`, `.fc-example-translation`, `.fc-part-source`, `.fc-blank`), renkleri `CardView`'de token'larla. Kullanıcının kendi şablonlarına dokunulmaz.
- **İpucu** iki türlüdür (`HintDto.kind`): `context` — "Cümledeki boşluğu doldur" etiketiyle örnek cümle (kelime boşluklu ya da vurgulu), `letters` — harf ipucu. Kural: **ipucu bağlamı gösterebilir, cevabı asla.** Yerleşik kelime/altyazı ileri kartının ön yüzü yalnız kelimeyi gösterir (Altyazı: geçtiği hâli, farklıysa yanında sözlük hâli); ilk ipucu kelimesi vurgulanmış cümledir (anlam cümlede geçiyorsa yalnız orası boşaltılır). Ters (üretim) kartta ilk ipucu kelimenin boşaltıldığı cümledir ("I ___ home."). Sonra harfler. Cümle zaten soruda görünüyorsa ya da cevabı ele verecekse harflerle başlanır. Mevcut koleksiyonlarda Altyazı ön yüzü migration 1301 ile (şablon düzenlenmemişse) güncellendi.
- **Yazılı cevap ve Test** yalnız sorulan alanı bekler: "went" kartında "gitmek" doğrudur.
- **Terim listesi** terim/tanım sütunlarını rollerden seçer, satır içi düzenleme doğru alana yazar; hangi alanın ne olduğu bilinmiyorsa kalem tam düzenleyiciyi açar.
- **v1.4 Bu cümledeki anlam** (`features/ai/MeaningInContext.svelte`, tek bileşen): not düzenleyicinin "Bu kelime" bölümünde satır içi, çalışma ekranında kartın "⋯" menüsünden iletişim kutusu (`MeaningDialog`). Yalnız `ai` modülü açık **ve** en az bir sağlayıcı kuruluysa görünür (`ai/ready.svelte.ts`). Saklı durumu gösterir (`ai.meaning.get`); "Öner" önce önizleme (`ai.preview` → `ai.run`; `ai.meaning.auto_suggest` açıksa önizlemesiz `ai.meaning.suggest` işi ve saklı öneri), öneri düzenlenebilir bir kutu + dayandığı sözcükler, deyim rozeti, güven, "bunlar da doğru" (kabul edilen cevap olur), "bu cümlede değil" ve kartın şimdiki anlamıyla gelir; "Uygula" `ai.meaning.apply` (tek geri alma adımı, bildirimde "Geri al"), kartın anlam alanı yalnız kutu işaretliyse değişir; "Yoksay" hiçbir şey yazmaz.

## 2. Bilgi mimarisi

```
┌────────────┬───────────────────────────────────────────────────────────┐
│ ▣ Flashcard│ Kütüphane › Desteler        [⌕ Ara ya da komut  Ctrl K] [+ Kart ekle N] │  ← üst çubuk 44 px
│            ├───────────────────────────────────────────────────────────┤
│ Ana sayfa  │                                                           │
│ Kütüphane  │   Sayfa başlığı (20/600)              [eylemler]          │
│ Oluştur    │   sekmeler                                                │
│ Ara        │   ───────────────────────────────────────────             │
│ İstatistik │   içerik (sola hizalı, en çok 1120 / geniş 1440)          │
│            │                                                           │
│ Gelen k. 3 │                                                           │
│ Ayarlar    │                                                           │
│ (profil)   │                                                           │
└────────────┴───────────────────────────────────────────────────────────┘
   216 px (1024 px altında 56 px, yalnız simge)
```

**Kenar çubuğu** (`shell/AppSidebar.svelte`, 216 px): uygulama işareti + ad; bölümler 30 px'lik sık satırlar: **Ana sayfa, Kütüphane, Oluştur, Ara, İstatistik**. "Oluştur" kart eklemenin tek yeridir (§1.3; v1.3'te içe aktarma da burada). Altta **Gelen kutusu** (bekleyen öğe sayısı mono), **Ayarlar**, profil menüsü. Etkin öğe `--surface-3` zemin + vurgu renkli simge. Menüler hâlâ `modules.list`'ten gelir (kural 7); modül → bölüm/grup eşlemesi tek yerde: `shell/navigation.ts` (`PLACEMENTS`). Kapalı modül her yerden (palet dahil) kaybolur.

**Üst çubuk** (`shell/TopBar.svelte`, 44 px, çalışma oturumu dışında **her zaman** görünür): solda iz (bölüm › sayfa, alt sayfada ekranın eklediği kırıntılar; `PageHeader` `back` ile otomatik), sağda:
- **"Ara ya da komut çalıştır"** alanı (`Ctrl K` ipucu): komut paletini açar; palet modülü kapalıysa Ara sayfasına gider.
- Birincil **"Kart ekle"** düğmesi (`N` ipucu): Hızlı Ekle'yi açar.

Bu girişler `shell/commands.svelte.ts`'te tek fonksiyondur: `openQuickAdd()` ve `openPalette()`. Uygulayan taraf kendini kaydeder (`setQuickAddHandler`, `setPaletteHandler`); çağıranlar hiçbir ekranı içe aktarmaz. Hızlı Ekle kayıtlı değilse (notlar modülü kapalı/yüklenmedi) kabuğun yedeği Oluştur sayfasını açar.

**Komut paleti** (`Ctrl+K`): önce Eylemler (Kart ekle, Ara, Geri al, Yinele), sonra ekranlar, sonra modüllere göre gruplu komutlar; kısayollar sağa hizalı. Taşınan her şey paletten bulunur.

**Ayarlar** tek sayfa (v1.3, kurallar §1.4): solda üstte **arama kutusu**, altında görev odaklı gruplar — en sık kullanılan önce: Genel (genel, hatırlatıcılar, kısayollar, profiller ve kilit) · Çalışma ve kartlar (deste ayarları, not türleri, modlar ve okuma) · Bağlantılar (yapay zeka, **Subtitle**) · Veri (dışa aktarma ve yedek, başlangıç desteleri; içe aktarma Oluştur'da) · ayrı çizgiyle **Gelişmiş** ("teknik ayarlar: çoğu zaman gerekmez": otomasyon, MCP, yerel API, eklentiler, bakım, günlükler ve güncellemeler). 1024 px altında alt menü tek bir Select olur, arama kutusu üstünde kalır; arama sürerken sonuçlar liste olarak görünür. Arama yalnız sunumdur: sayfa adları + her sayfanın gösterdiği i18n alt ağaçları (`core/kit/settings-index.ts`) içinde tüm kelimeleri aynı metinde arar, eşleşen sayfanın altında o metni gösterir; açık sayfada eşleşen `FormRow` vurgulanır, ilk eşleşmeye kaydırılır. Genel sayfası "Ne değiştirmek istiyorsun?" sorularıyla başlar (her biri ilgili sayfayı açar).

**v1.4: tek "Gelişmiş" anahtarı** (`shell/advanced.svelte.ts`, çekirdek ayarı `app.show_advanced`, varsayılan kapalı, profil başına hatırlanır). Kapalıyken kenar çubuğu yalnız günlük yerleri gösterir: **Ana sayfa, Kelimelerim** (rota `/words`, kayıtlı değilse yerine **Ara**), **İlerleme** (rota `/stats`); Kütüphane, Oluştur ve Ara anahtarla gelir, her zaman `Ctrl K`'dan ve bağlantılardan açılır (`visibleSections`; A/B "Gezinme" = `NAV_VARIANT` sabiti, B'de Kütüphane de temel). Ayarlar'da **Temel** grubu, her biri sorduğu soruyla adlanmış ≤ 6 sayfa: Günlük tempo, Kartların görünüşü (not türleri), Hatırlatma, Subtitle ve bağlantılar, Yedek, Dil ve görünüm; diğer gruplar (Çalışma · Uygulama · Bağlantılar (ayrıntı) · Veri ve bakım) yalnız anahtar açıkken görünür. Arama anahtar kapalıyken de bütün sayfaları bulur ve gelişmiş olanları "Gelişmiş'te" diye işaretler; böyle açılan sayfa bir kerelik gösterilir, üstte "Gelişmiş'i aç" bandı. Birden çok modülün ayarlarını toplayan kabuk sayfaları `ScreenDef.settingsPage` ile kaydolur (modüllerinden biri açıkken görünür); satırlar `core/kit/SchemaRows.svelte` ile `settings.schema`'dan çizilir (etiket + tek cümle açıklama Rust'ta). **Günlük tempo** (`features/presets/PaceSettings.svelte`, rota `/settings/pace`, D2 taslağının ilk açılış hâli) yalnız günlük soruları sorar: günde kaç yeni kelime (hazır tempolar 5/10/20/30, her biri `planning.forecast`'ın dakikasıyla), günde en çok kaç tekrar, ne kadar sağlam hatırlama (Rahat/Dengeli/Sıkı); üstte bu tempoyla günde ~kaç dakika, 3 hafta sonraki yük, bekleyen yeni kelimeler ve 60 günlük süre eğrisi (hepsi `planning.forecast`). Her değişiklik anında `planning.apply_pace` ile seçilen destenin ayar paketine yazılır (paketi yoksa çekirdek açar; tek geri alma adımı, bildirimde "Geri al"). Paketin geri kalanı (adımlar, sıra, FSRS, paket ↔ deste) tam düzenleyicide: **Gelişmiş › Çalışma › Deste ayarları** (`/presets`), sayfanın altındaki "Desteye göre ayarla" ve "İlgili ayarlar" bağlantılarından açılır. A/B "önerilen günlük yeni" = `RECOMMENDED_NEW` sabiti (A: 10, B: çekirdeğin varsayılanı).

**Sayfa kabı:** tek kap (`App.svelte` `.page`), kenar çubuğunun yanında sola hizalı, `--content-max` 1120 px, `--content-pad-x` 24 px (1024 altında 16); tablo/grafik sayfaları (Ara, İstatistik) `ScreenDef.width: 'wide'` → 1440 px. Sayfa başlığı her zaman ilk satırdır; deste seçici ve bölüm sekmeleri kabuk tarafından ekranın `PageHeader`'ına verilir (`page-chrome.ts`).

## 3. Ekranlar (uygulandığı hâliyle)

### 3.1 Ana sayfa ve çalışma oturumu

**Ana sayfa** (`features/home/HomeScreen.svelte`) "şimdi ne yapayım" sorusunu cevaplar: ilk satırda "Bugün · **N** kart çalışılacak" ve mono kırılım (Sırası gelen · Öğreniliyor · Yeni), sağda birincil **"Çalışmaya başla"** (`Enter`, başlatıcıyı açar); gelen kutusunda bekleyen varsa tek ince satır; v1.4: bildiğin kelimeler satırı (`words.stats`, İlerleme'ye götürür) ve yedek eskiyse tek "son yedek" satırı (`io.backup.status.stale`: hiç yok / eski / başarısız; Ayarlar › Veri'ye götürür); seri / bugün yanıtlanan / hedef şeridi; "Bugün çalışılacak desteler" sık tablosu, satır başına görünür "Çalış". Sırası gelen kart yoksa "Hepsi tamam" ve pratik önerisi; ölü düğme yok. **Bugünün sayıları başlatıcının kullandığı kuyruktan gelir** (`decks.summaries` / `decks.tree` içindeki `today_*` alanları, docs/03): Ana sayfa, Kütüphane ve deste sayfası aynı sayıyı söyler ve "Başla" o kadar kart verir. "Tüm desteler" sayısı destelerin kendi şablonlarıyla kurulan birleşik kuyruktur (§1.1).

**Çalışma oturumu:** oturum sürerken kabuk tamamen kaybolur (`ctx.setFocus(true)`). `features/modes/SessionFrame.svelte` tek ince üst satır çizer: menü düğmesi, mod ve deste, ilerleme çubuğu, mono "12 / 40", kalan kırılım, pratikse "pratik" çipi, çıkış (`Esc` = ara ver). Kart ortada (en çok 720 px, köşe 12), **cevap düğmeleri kart genişliğinde**: varsayılan 2'li (**Hâlâ öğreniyorum** `1`, **Biliyorum** `2`) ya da 4'lü (Tekrar/Zor/İyi/Kolay, mono sonraki aralıklarla; `study.answer_buttons`). Puan düğmeleri arka yüz görülene kadar pasif. Altında sessiz araç satırı (ipucu, geri al, bayrak, odak, daha fazla: "Kenara koy" ve "Yarına bırak" açıklamalarıyla) ve küçük mono kısayol ipuçları (kısayol düzenleyicisini izler). Öğren / Test / Eşleştir aynı çerçeveyi kullanır; Test kâğıdı açılınca ilk cevaba odaklanır. **Oturum özeti** sık; tamamlanmamışsa "Ara verdin" + Devam et, bitmişse "Tekrar çalış" (başlatıcı, bugün zorlandıkların seçili) ve "Yanlışları tekrar çalış" (`R`). Test kâğıdı klavyeyle: `A`–`D` (ya da `1`–`4`) şık seçip sonrakine geçer, yazılı cevapta `Enter` sonrakine geçer, son cevap "Teslim et"e iner, `Ctrl+Enter` teslim eder. Eşleştir kutucukları rolü olan türlerde terim (`term`, yoksa `surface`) ↔ şablonun cevap alanıdır (yoksa `meaning`), cümle asla.

### 3.2 Kütüphane, deste sayfası, Ara

**Kütüphane** (`features/decks/LibraryScreen.svelte`): sekmeler **Desteler | Notlar**; varsayılan sık **ağaç-tablo** (`ListRow`, 34 px; ad, kart, bugün, yeni, ustalık, sınıflandırma çipleri), ızgara ikincil görünüm (`fc.library.view`). v1.4: dili belli olmayan not varsa (`words.stats.unassigned_notes`) listenin üstünde tek satır, tıklayınca dili olmayan ilk desteyi açar. Satırda üzerine gelmeden görünen "Çalış" düğmesi başlatıcıyı o desteyle açar (Ana sayfanın deste satırlarında da).

**Deste sayfası** (`features/decks/DeckPage.svelte`): başlık + sayılar + çipler + **Dil** (v1.4: `words.language` deste kapsamı, seçici; boşken "kelimeleri sayılmıyor" uyarısı) + ⋯; birincil **"Çalış"** (`Enter`) ve mod düğmeleri (Kartlar, Öğren, Test, Eşleştir başlatıcıyı o mod seçili açar; Okuma kendi ekranı); önizleme kartı (`←`/`→`, `Space`), ilerleme, bugünün sayıları, alt desteler. **Terim listesi** (`TermList.svelte`) gerçek bir tablo; satır içi düzenleme `notes.terms`'ün rollerden seçtiği alanlara yazar (`term_field`, `definition_field`), belirsiz not türlerinde tam düzenleyici açılır.

**Ara** (`features/browse/BrowseScreen.svelte`, geniş): sorgu alanı (`/`), ince araç çubuğu, sık sanal tablo, kart bilgisi yan paneli; `↑`/`↓`, `Space`, `Shift+↑/↓`, `Enter` düzenle, `Esc`; seçim varken toplu işlem çubuğu. v1.3: `added:N` / `edited:N` / `reviewed:N` yerel çalışma günlerini sayar (`study.day_start_hour`), kayan 24 saati değil.

**v1.4 (E3) Ara süzgeç çipleri** (`features/browse/FilterChips.svelte`, `filter-chips.ts`): arama kutusunun altında Durum, Dil, Seviye (CEFR), Sıklık, Nereden, Karşılaşma, Ses, Görsel, Kaynak, Kelime. Bir değer seçmek sorgu kutusuna o yüklemi yazar (aynı anahtarlı eski jetonun yerine; `-` ile başlayan jetonlara dokunmaz) ve sorguyu çalıştırır; etkin çipin × düğmesi jetonu siler. Sorgu dili görünür kalır, öğrenilir; yardım (`?`) yeni yüklemleri birer örnekle listeler. Modülü kapalı çip çizilmez (`words`, `word_data`, `subtitle`).

**v1.4 (E3) Kelimelerim** (`features/words/WordsScreen.svelte`, route `/words`, `words` modülünün `main_menu` girdisi; kenar çubuğundaki yeri kabuğun işi): büyük arama kutusu (`/`), durum çipleri sayılarıyla (`words.stats`), sıralama (A–Z / en sık / seviye; `word_data` açıkken), satırda kelime · durum · sıklık sırası · CEFR · kart sayısı, "Daha fazla" ile imleçli sayfalama; `↑`/`↓`, `Enter` yan paneli açar, `Esc` kapatır. Panel: durum tek cümle, kartları, sıklık/seviye, "Zaten biliyorum / Yeniden öğren / Yok say" (`reading.words.mark`), "Nerede gördün" (`subtitle.encounters.list`). Boş hâl Ekle'ye götürür; dili bilinmeyen kart sayısı tek satır ipucu.

### 3.3 İlerleme (eski İstatistik) ve Planlama

`features/stats/` (geniş, rota `stats`; v1.4, D3 taslağı). İlk görünüm **üç soruya üç cevap**, her biri bir cümle + bir küçük grafik: **Bugün ne yaptım?** (`stats.today`: "Bugün N kelime çalıştın, M tanesini bildin", dakika · yeni · seri, Rust'ın hız yargısı, varsa hedef çubuğu, son 7 günün çubukları; altta bugün zorlandıkların ve **"Bunları çalış · N"** — başlatıcı pratik + o kartlar), **İlerliyor muyum?** (`words.stats`: "N kelimeyi biliyorsun" — "hangi kelimeleri biliyorum"un tek doğrusu; yeni/öğreniyorum/biliyorum yığın çubuğu, eşik cümlesi, dili belli olmayan not sayısı, **Kelimelerim**'e bağlantı; `words` modülü kapalıyken kartların karnesi) ve **Yarın ne var?** (tahminin yarını + 7 gün, en yoğun gün yazıyla; "Günde kaç yeni kelime?" Planlama'ya). Altında tek **"Daha fazla"** düğmesi (kişisel olarak hatırlanır): özet kutuları, çalışma takvimi, hatırlama oranı, aşamalar, gelişim, 30 gün, saatler/süreler, son günlerde zorlandıkların (pencere, kenara koy/etiket), **sürekli unuttukların** ve deste karneleri, her birinin altında tek satırlık anlamı. **Gelişmiş** (uygulamanın tek anahtarı `app.show_advanced`, `shell/advanced.svelte.ts`; Ayarlar, kenar çubuğu, başlatıcı ve İlerleme aynı değeri okur/yazar): süzgeç satırı (deste/dönem/sorgu), başlıkta Planlama/Hedefler, kararlılık/zorluk sekmesi, sınıflandırmaya göre tablo; kapalıyken sayfa tüm koleksiyonu son 30 günle gösterir.

**Kart onarımı (yeni desen, `LeechRepair.svelte`):** sürekli unutulan kartın satırındaki **"Düzelt"** bir pencere açar: "neden hep unutuyorsun?" — son yanlış cevaplar ve **yazdığın** cevap (`typed_answers`), kartın beklediği ve kabul ettiği cevaplar, `stats.leech_repairs.plan`'ın önerileri (sebep cümlesi + önce → sonra önizleme), **Uygula** = `stats.leech_repairs.apply` (tek geri al adımı, `Ctrl+Z`), altında önceki düzeltmelerin önce/sonra oranı ve "henüz az cevap" uyarısı. Yapay zekayla yeniden yazma önerisi gösterilir, Yapay zeka sayfasından yürür.

**Planlama** "Günde kaç yeni kart?" + "Sınav tarihim" ile açılır, canlı tahmin (`planning.forecast`, Rust'ta saf FSRS beklenen-değer modeli): büyük tek cümle, günlük iş yükü grafiği (0. gün bugünün gerçek kuyruğudur, Ana sayfa ve başlatıcının sayısı; sonraki günler şablonların günlük yeni/tekrar sınırlarına uyar, sınırı aşan tekrarlar ertesi güne kayar; gün "kart" sayısı kart, dakika cevap sayar), haftalık tablo, ne değişeceğini söyleyen "Uygula" (`planning.apply_pace`, tek geri al adımı); sınavlar, yük dengeleme ve optimizasyon "Gelişmiş" altında. **Hedefler** boşluklu cümleler (yeni: "günde N kart"). **`?`** kısayol yardımını açar: önce **"Bu sayfada"** (sayfanın kendi tuşları: Ana sayfanın `Enter`'ı, Kütüphane okları, Oluştur'un `Ctrl+Enter`'ı, Test kâğıdının `A`–`D`'si; `shell/page-keys.svelte.ts`), sonra canlı `shortcuts.list` (bağlama göre gruplu, aranabilir).

### 3.4 Diğer ekranlar

Gelen kutusu (sık inceleme listesi, `J`/`K`, `A`, `E`, `R`, `X`/`Space`, `Ctrl+Enter`; her satır neye dönüşeceğini söyler — cümle taşıyan öğe "Altyazı kartı" olur; destesiz satır göndermeden önce deste ister), ilk açılış rehberi (4 adımlı, kapatılabilir kontrol listesi: kelime ekle ya da başlangıç destesi, çalış, bugünün kartları ve ilerleme nerede, isteğe bağlı tarayıcı eklentisi; Başlangıç desteleri sayfasından yeniden açılır), başlangıç desteleri, kilit ekranı, eklentiler, okuma, yerel API ve Subtitle ayar sayfaları aynı sistemde. Subtitle bağlantısı: [06 §5.1](06-ai-ve-entegrasyonlar.md).

### 3.5 Kısayollar (başvuru)

Değiştirilebilir olanlar Ayarlar → Genel → Kısayollar'da (`palette::UI_ACTIONS`), sabit olanlar da orada listelenir; `?` hepsini gösterir. Burada varsayılanlar. Kural §1.2.

| Bağlam | Tuş | Eylem |
|---|---|---|
| Her yer | `N` · `Ctrl+N` | Hızlı Ekle (`N`: yazı alanında, iletişim kutusunda, oturumda değil) |
| Her yer | `Ctrl+K` · `Ctrl+F` · `?` | Komut paleti · Ara sayfası · kısayol yardımı |
| Her yer | `Ctrl+Z` / `Ctrl+Shift+Z` | Geri al / Yinele — yazı alanında değilken |
| Ana sayfa | `Enter` | Başlatıcıyı aç |
| Deste sayfası | `Enter` · `←`/`→` · `Space` | Başlatıcıyı aç · önizlemede gez · önizlemeyi çevir |
| Başlatıcı | `Enter` · ok tuşları · `Tab` · `Esc` | Başla (odaktaki kontrol kendi `Enter`'ını taşıyorsa o) · kapsam · sayı/seçenekler · kapat |
| Kartlar | `Space` · `1`–`4` | Çevir (asla puanlamaz) · puanla, yalnız arka görüldükten sonra (2'li: `1` Hâlâ öğreniyorum, `2` Biliyorum) |
| Kartlar | `H` · `S` · `B` · `Z` · `Esc` | İpucu · kenara koy (sen geri alana kadar) · yarına bırak · geri al · ara ver |
| Kartlar (puan kaydedilirken) | `Z` · `Esc` | Kayıttan sonra puanı geri al · kayıttan sonra ara ver; başka her tuş hiçbir şey yapmaz, sonraki karta saklanmaz |
| Kartlar | `Ctrl+1`–`7` · `F` | Bayrak · odak modu |
| Kartlar (özet) | `R` | Yanlışları tekrar çalış |
| Öğren / Test | `1`–`4` (Test: `A`–`D` da) · `Enter` · `Ctrl+Enter` | Seçenek · (Test) yazılı cevaptan sonrakine · (Test) teslim et |
| Hızlı Ekle | `Tab` · `Enter` · `Shift+Enter` · `Ctrl+Enter` · `Esc` | Sonraki alan · sonraki alan / son alanda ekle · yeni satır · ekle (açık kalır) · kapat (metin varsa iki kez) |
| Oluştur (Toplu) | `Tab` · `Enter` (son alan) · `Ctrl+Enter` | Sonraki alan · yeni satır · kaydet |
| Terim listesi | çift tık / kalem · `Ctrl+Enter` · `Esc` | Satır içi düzenle · kaydet · vazgeç |
| Ara | `/` · `↑`/`↓` · `Space` · `Shift+↑/↓` · `Enter` · `Esc` | Sorgu · gez · seç · genişlet · düzenle · kapat/temizle |
| Ara | `Ctrl+A` · `Ctrl+J` · `Ctrl+I` · `Delete` | Tümünü seç · kenara koy · kart bilgisi · sil (yazı alanında değilken) |
| Not düzenleyici | `Ctrl+B` · `Ctrl+I` · `Ctrl+Enter` | Kalın · italik · kaydet |
| Gelen kutusu | `J`/`K` · `A` · `E` · `R` · `X` · `Ctrl+Enter` | Gez · kabul · düzenle · reddet · seç · seçimi kabul |

## 4. Tasarım sistemi

### 4.1 Token'lar (`src/lib/theme/tokens.css`)

İki katman: `--fc-*` tema renkleri (backend `themes.current` hazır temalar ve özel vurgu için ezer; varsayılan hazır tema = v1.2 tasarımı, `fc_core::theme`), ve üstüne kurulu tasarım token'ları (`--surface-*`, `--text-*`, `--accent`…). Yeni kod yalnız ikinci katmanı kullanır. Renkler `light-dark()` ile bir kez tanımlanır; `data-theme` yoksa işletim sistemi izlenir. Açık/koyu/sistem, hazır temalar ve özel vurgu rengi korunur.

| Token | Koyu | Açık | Kullanım |
|---|---|---|---|
| `--surface-0` | `#0E0E10` | `#FAFAF8` | Uygulama zemini |
| `--surface-1` | `#151518` | `#FFFFFF` | Paneller, kenar çubuğu |
| `--surface-2` | `#1C1C20` | `#F3F3F0` | Yükseltilmiş: giriş, üzerine gelme |
| `--surface-3` | `#25252B` | `#E9E9E4` | Etkin / seçili |
| `--border` / `--border-strong` | `#2A2A31` / `#3A3A43` | `#E2E2DC` / `#CFCFC7` | 1 px ayırıcılar |
| `--text-1` / `--text-2` / `--text-3` | `#EDEDEF` / `#A3A3AE` / `#74747F` | `#1A1A1C` / `#5C5C63` / `#85858C` | Birincil / ikincil (açıklamalar, ikincil sayılar) / **yalnız** yer tutucu ve süs simgesi — okunması gereken metin asla `--text-3` değil (v1.3 karşıtlık düzeltmesi) |
| `--accent` + `--accent-fg` | `#F5B544` + `#17130A` | aynı | Dolgu (birincil düğme, çubuk, etkin sekme çizgisi) ve üstündeki metin |
| `--accent-text` | `#F5B544` | `#9A5B00` | Yüzey üstünde vurgu metni/simgesi (AA) |
| `--accent-soft` | vurgu %14 | vurgu %14 | Hafif vurgu zemini |
| `--focus-ring` | `#F5B544` | `#9A5B00` | Odak halkası |
| `--success` | `#3DD68C` | `#147A4B` | Doğru |
| `--danger` | `#FF6369` | `#C8323B` | Yanlış, yıkıcı |
| `--warning` | `#FF9D4D` | `#B4520A` | Uyarı (turuncu: vurgu ile karışmaz) |

Anlamlı renklerin `-solid` (dolgu) + `-solid-fg` ve `-soft` (çip/şerit zemini) çeşitleri var. Sıcak nötrler (mavi-gri değil). Kontrast çiftleri `tests/contrast.test.ts` ile denetlenir. Ham renk yasak; `--fc-*` yeni kodda kullanılmaz.

### 4.2 Tipografi

- **Inter** (OFL, paketli, ağdan yüklenmez) arayüz için; Türkçe karakterler latin-ext dilimde.
- **JetBrains Mono** (OFL, paketli, 400–600) **sayılar, sayımlar, süreler, kısayol ipuçları, tablo sayıları** için: `--font-mono`, `<span class="num">` = mono + tabular rakamlar. Ana sayfanın "141"i, oturumun "12 / 40"ı, tablo sütunları, `Kbd`.
- Ölçek 11/12/13/14/16/20/24 (+32); **gövde 13 px**, vurgu 14, sayfa başlığı 20/600; çalışma kartı metni 28–40 (uzun metinde küçülür). Satır yüksekliği 1.45 (sıkı 1.3). Ayar açıklamaları gövde boyunda (13 px; v1.2'de 12 idi). Kart arkasında üç seviye: anlam (kart metni) › örnek (gövde) › kaynak (küçük, ikincil).

### 4.3 Yoğunluk, şekil, derinlik, hareket, odak

- **Yoğunluk:** kontrol yüksekliği 28 (sm) / 32 (md) / 36 (lg); tablo ve liste satırı 34 (`--row-h`); kenar çubuğu satırı 30; ayar satırı 44; üst çubuk 44. Boşluk 4 px ızgara (4, 8, 12, 16, 20, 24, 32, 48, 64), sayfa iç boşluğu 24.
- **Köşe:** 6 kontrol, 8 panel/iletişim kutusu, **12 yalnız çalışma kartı**, 999 çip.
- **Kenarlık, gölge değil:** ayrım 1 px kenarlık ve yüzey basamaklarıyla. Tek bir yumuşak gölge (`--shadow-float`) yalnız yüzen katmanlarda: iletişim kutusu, menü, açılır pencere, bildirim. Gradyan, parıltı, cam efekti yok.
- **Hareket:** 100–150 ms ease-out (`--motion-fast`, `--motion`); `prefers-reduced-motion`'da 0 (kart çevirme dahil).
- **Odak:** 2 px vurgu halkası, 2 px boşlukla; klavyede her zaman görünür.

### 4.4 Bileşenler (`src/lib/ui/`, ayrıntı: `src/lib/ui/README.md`)

Tüm ekranlar yalnız bunları kullanır; bileşenler kendini `data-fc-ui` ile işaretler.

| Grup | Bileşenler |
|---|---|
| Eylemler | Button (primary/secondary/ghost/danger, sm/md/lg, yükleniyor, `kbd` ipucu), IconButton, Menu, Tile, OptionTile |
| Form | Field, Input, Textarea, Select (özel liste kutusu, yazarak arama), Checkbox, Switch, SegmentedControl, RadioGroup, TimeField |
| Gösterim | Card, Chip, Badge, Avatar, Kbd (mono), StatTile (büyük mono sayı), Icon (Lucide alt kümesi, `icons.ts`), ProgressBar (parçalı), ProgressRing, EmptyState, Skeleton, Spinner, ListRow (34 px liste/ağaç-tablo satırı), Table (sanal, sabit 34 px satır, `numeric` sütun mono), NumberedRow, Flashcard |
| Katmanlar | Dialog/Sheet (odak tuzağı, odak dönüşü), Tooltip, Toast/ToastHost (`toast.show`) |
| Gezinme ve düzen | PageHeader (`back`, `meta`, `actions`, `tabs`, `chrome`), Tabs, Sidebar, NavItem (30 px, mono rozet) |
| Kabuk (`src/lib/shell/`) | AppSidebar, TopBar, DeckPicker, ProfileMenu, SettingsLayout, `commands.svelte.ts`, `navigation.ts` |
| Ekran kitleri | `features/core/kit` (SettingsPage, SettingsSection, FormRow, SettingsAdvanced, LoadState), `features/modes/SessionFrame` (+ OptRow), `features/study/StudyLauncher` (tüm modların tek başlangıcı; v1.3'te SetupPanel kalktı), `features/stats` (Panel, Heatmap, BarChart) |

İlk yüklenen kod (`App.svelte`, `lib/shell/`, `*Host` bileşenleri) `$lib/ui`'yi dosya dosya alır, tembel ekranlar varilden (`tests/eager-imports.test.ts`). Sayılar/tarihler yalnız `$lib/format` (Intl, etkin dil; lint `tests/number-format.test.ts`).

### 4.5 Erişilebilirlik

- Klavye ile her şey; görünür odak halkası; ARIA rolleri (liste kutusu, radyo grubu, ağaç satırları, sekmeler, iletişim kutusu).
- Renk tek başına ipucu değildir: doğru/yanlış renk + simge + metin; ilerleme çubuğu parçaları erişilebilir adda.
- Durum duyuruları `role="status"` ile (Hızlı Ekle "Kart eklendi…", ısı haritası odak günü); grafiklerin ekran okuyucu tablosu var.
- Tüm metin çiftleri iki temada WCAG AA (testli); `prefers-reduced-motion` saygılı; 200% yakınlaştırmada taşma yok (900 px görüntülerinde denetlenir).
- **Dil:** her metin i18n anahtarı (TR/EN, `tests/no-hardcoded-text.test.ts`); terimler tutarlı, kavram başına tek söz ("Sırası gelen", "Öğreniliyor", "Kenara koy", "Yarına bırak", "Ara ver" her yerde). Varsayılan dil sistem dilinden; mock modunda da ayarlanan dil uygulanır.

## 5. Doğrulama

v1.3'ün kuralı: **mock ekran görüntüleri yalnız görsel bir kontroldür; kabul, gerçek çekirdeğe karşı koşan yolculuklardır.** Bir akış "bitti" sayılmadan önce onu anlatan bir yolculuk yeşil olmalı ve yolculuk **kaydedilen sonucu** (not alanları, oturum satırları, cevap kayıtları, takvim) API'den geri okuyarak doğrulamalı; piksel ya da ekrandaki yazı tek başına kanıt değildir.

### 5.1 Yolculuklar: gerçek çekirdek, dev köprüsü (kabul)

`fc --db <dosya> dev serve [--port N] [--token T]` (yalnız debug derlemelerinde ya da `dev-bridge` özelliğiyle; sürüm derlemesinde yok) loopback'te `POST /rpc { method, params }` ve `GET /events` (SSE) sunar; masaüstü IPC'siyle **aynı dağıtımı** kullanır (`fc_app::transport::dispatch`: zarf, hata biçimi, kilit). Ön yüz üçüncü taşıyıcıyla (`vite --mode bridge`, `VITE_FC_BRIDGE_URL` / `VITE_FC_BRIDGE_TOKEN`) tarayıcıda gerçek Rust çekirdeğine bağlanır. Ayrıntı: docs/07 §3.1, `apps/desktop/e2e/README.md`.

```bash
pnpm -C apps/desktop e2e                         # fc'yi derler, geçici profil, tohum, köprü + vite, tüm yolculuklar
pnpm -C apps/desktop e2e -- --grep S2            # tek yolculuk (argümanlar playwright test'e gider)
FC_BIN=target/debug/fc pnpm -C apps/desktop e2e  # cargo build'i atla
FC_E2E_KEEP=1 pnpm -C apps/desktop e2e           # geçici profili sakla (yolu yazılır)
```

`e2e/run.mjs`: `fc`'yi derler (debug), geçici profil açar, köprüyü rastgele port + belirteçle başlatır, `seed.mjs` ile **yalnız gerçek komutlarla** tohumlar (İngilizce destesi: 30 Klasik not; Subtitle destesi: tarayıcı eklentisinin HTTP sözleşmesiyle `fc api serve` üzerinden 10 yakalama; ilk açılış rehberi atlanmış), vite'ı bridge modunda rastgele portta açar, Playwright'ı (başsız Chromium) koşar, her şeyi kapatıp profili siler. Hatalı yolculukların izleri `e2e/.results/`. CI: `ci.yml` içindeki `journeys` işi (ubuntu-24.04).

**Yolculuk ekleme:** `journeys/<ad>.spec.ts`; yolculuklar tek profilde sırayla koşar, bu yüzden kendi destesini `uniq()` adıyla kurar ve başkasının verisine dayanmaz. Yardımcılar `../fixtures`: `rpc(method, params)` (kullanıcı olarak herhangi bir genel metot; durumu kurmak ve **sonucu doğrulamak** için), `seed()`, `makeDeck`, `notesIn`, `deckIdByPath`, `openApp(page, { lang })`, `press`, `focused`, `goToSection`, `choose` (`$lib/ui` Select), `t(key)` (kullanıcının okuduğu metni anahtardan: kelime sabitlenmez). Önce klavye; rol + `t()` + `data-testid` seçiciler. Önce tek başına (`--grep`), sonra tüm takım. Bilinen bir hatayı gösteren yolculuk **silinmez, atlanmaz**: hata iddiası, yolculuğun kendi ön koşulları geçtikten sonra `test.fail(true, '<hata>')` ile işaretlenir; hata düzelince Playwright "beklenen başarısızlık geçti" der ve işaret aynı commit'te kaldırılır.

Hepsi **geçmek zorunda**; şu an hiçbir yolculukta `test.fail` yok (S7c, X1'in ipucu düzeltmesiyle yeşile döndü). Dosyalar `apps/desktop/e2e/journeys/`; liste `apps/desktop/e2e/README.md` ile aynıdır.

| Yolculuk | Ne yapılır → ne doğrulanır |
|---|---|
| J1 | Hızlı Ekle: `N`, yaz, `Ctrl+Enter` ×2 → seçilen destede tam yazılan alanlarla iki not |
| J2 | Oluştur › Toplu: yeni deste, 3 satır, `Ctrl+Enter` → deste + 3 not; tek `Ctrl+Z` (`oplog.undo`) üçünü ve desteyi kaldırır |
| J3 | Ara, `↓ Enter`, arka yüzü düzenle, `Ctrl+Enter` → alan veritabanında değişmiş, başka hiçbir şey değil |
| J4 | Oluştur › Dosyadan: var olan deste, CSV **dosya seçiciyle** seçilir, eşlenmiş önizleme "3" der → içe aktar → notlar CSV'nin alanlarıyla var |
| J5 | 21 yeni / sınır 20 / 20 çalışılmış: Ana sayfa `study.start`'ın veremeyeceğini vaat etmez |
| J6 | Altyazı kartı yazılı test: yalnız anlam ("gitmek") doğru cevaptır |
| J7a | Not düzenleyicide (Ara → `Enter`) `Ctrl+A` alanın metnini seçer, satırları değil |
| J7b | Hızlı Ekle'de yazarken `Ctrl+Z` metni geri alır, önce kaydedilen kartı değil |
| S1 | Ana sayfa → `Enter` → başlatıcı → `Enter` ("Tüm desteler"): `Space`×3 hiçbir şey puanlamaz, `2` bir kart puanlar, basılı `2` yine bir |
| S2 | Ana sayfa → `Enter` → `Enter`, 3 cevap, `Esc` (ara), Ana sayfadan başlatıcı: "Devam et" ilk ve odakta, **aynı oturumu** kaldığı yerden sürdürür |
| S3 | Deste sayfası → Test → 10 soru → hepsini cevapla → `Ctrl+Enter`: puan görünür, hiçbir kartın takvimi değişmez |
| S4 | İki kart "Tekrar" → "Bugün zorlandıklarım" 2 sayar; oturum pratik (`trial`) satırları yazar, takvim değişmez |
| S5 | 21 yeni / sınır 20, 20 çalışılmış: başlatıcı ve Ana sayfa nedeniyle (günlük sınır) 0 gösterir; boş başlatma reddedilir, açık oturum korunur |
| S6 | 1 dk / 10 dk adımlar: pencere 0'da "Sıradaki kart … sonra", oturum açık, `next_due` = vade, geri sayım bitince kart kendiliğinden gelir; varsayılan pencerede (`study.learn_ahead_mins` = 20) "Tekrar" denen kart hemen geri gelir |
| S7a | Altyazı kartı go/went, başlatıcı → Test (yalnız yazılı): kâğıt cevap alanında açılır, "gitmek" doğru (kâğıt anahtarıyla geri okunur) |
| S7b | Altyazı kartı arkası (Kartlar, `Space`): anlam, örnek (+ çeviri), kaynak ayrı ve `cards.part.*` etiketli parçalar |
| S7c | İpucu kuralı: Altyazı ön yüzü yalnız kelime (went + "go"); `H` = kelimesi vurgulu satır, anlam asla, sonra harfler; ters kart (Kelime "production") kelimeyi boşaltır: "I ___ home after the party." |
| S8 | Terim listesi: Altyazı notunun "Tanım"ını düzenlemek `meaning`'i değiştirir, `surface`'i asla |
| S9 | Oluştur › Yapıştır `kelime⇥anlam⇥cümle` ×3 → Kelime not türünde örnek alanı cümleyi tutar; tek `Ctrl+Z` üçünü kaldırır |
| S10 | Ayarlar › Not türleri: alan rolü ve kartın "neyi sorduğu" kaydet + yeniden yüklemeden sağ çıkar (`notetypes.get`) |
| S11 | Ayarlar › Günlük tempo → "Desteye göre ayarla" → Deste ayarları: 15 yeni kartlı destede günde 5 → başlatıcı ve `study.options` 5 sunar; varsayılana dön → 15 |
| P1 | v1.4 Ayarlar › Günlük tempo: 5 yeni, 150 tekrar, Sıkı → `presets.for_deck` 5/150/0,95 okur, `study.options` 5 yeni sunar; "Geri al" yalnız son değişikliği geri alır |
| S12 | Hızlı Ekle: kayıttan sonra alanda `Ctrl+A` / `Ctrl+Z` metni düzenler; kayıtlı not, destenin not sayısı ve oplog dokunulmamış |
| O1 | Kütüphane → deste → Oluştur: "Var olan deste" o deste seçili, imleç ilk terimde; `Ctrl+Enter` notu **o** desteye kaydeder, deste oluşturmaz |
| O1b | Kenar çubuğundan Oluştur: imleç ilk terim hücresinde |
| O2 | Kütüphane satırının görünür "Çalış"ı → `Enter` → `Space`, `2` → `Esc`: odak "Devam et"te, `Enter` **aynı** oturumu sürdürür; `Esc` → `Tab` → "Tamam" deste sayfasına döner, duraklatılmış oturum açık kalır |
| O3 | Destenin yarım Test'i varken deste sayfası → Öğren: başlatıcı Test'i "Devam et"te listeler ama odak orada değil; `Enter` Öğren'i başlatır, Test açık kalır |
| O4 | Test kâğıdı yalnız klavyeyle, 6 soru: `A`–`D` seçip ilerler, yazılı cevap + `Enter` ilerler, son cevap "Teslim et"e iner, `Ctrl+Enter` teslim eder → notlanmış kâğıtta boş cevap yok |
| C1 | Altyazı kartlarında Eşleştir: kutucuklar kelime (`term`) ↔ anlam (cevap alanı), satır/çeviri/kaynak asla; tur oynanır, kart başına `review_log.mode = match` cevabı geri okunur |
| C2 | Planlama'nın "bugün"ü başlatıcınınkidir: 25 yeni (günde 20), günde 5 tekrar → "Bugün ~20 kart" = `study.options`; sonraki hiçbir günde 20'den çok yeni ya da 5'ten çok tekrar planlanmaz |
| C3 | Adımı 10 dk sonra dolan öğrenme kartı ekranda iken pencere 0'a indirilir: `Space` + `2` kaydedilir (tekrar kaydı geri okunur, "artık oturumda değil" yok); sonraki kart yeni değerle seçilir ve bekler |
| C4 | Bir notun iki kartı iki destede: her deste tek başına kendi kartını sunar, "Tüm desteler" kuyruğu (Ana sayfa → başlatıcı) yalnız birini sayar ve sunar; kardeşlerini bırakmayan şablonlu deste kartını korur |
| E4a | Deste sayfasında dil seçilir (İngilizce): `settings.get` deste kapsamında `en` döner, destenin 3 notu `words.stats.unassigned_notes`'tan düşer |
| E4b | İlerleme'nin "İlerliyor muyum?" cümlesi `words.stats`'ın sayısıdır; "Daha fazla" ayrıntıları açar |
| K1 | Puandan hemen sonra tuş: dört düğme, dört kart art arda `1`/`2`/`3`/`4` ve **beklemeden** `Space` (kayıt bilerek 400 ms yavaşlatılır; kart yarısında `Space`, yarısında tıklamayla — odak kartta — çevrilir): kayıt sürerken puanlanan kart arka yüzünde kalır, sonraki kart ön yüzüyle gelir; her kartın oturumda basılan puanla tam bir tekrarı vardır |

### 5.2 Windows: kurulu uygulamaya karşı WebDriver (pilot, kapıda değil)

Linux'taki yolculuklar Windows'un üç sorusunu cevaplayamaz: kurulu NSIS paketi açılıyor mu, WebView2'de klavye aynı mı, gerçek IPC aynı sonucu kaydediyor mu. `.github/workflows/windows-build.yml` önce imzasız NSIS kurulumunu artefakt olarak üretir (`installer` işi; değişmedi), sonra **`continue-on-error: true`** ile ayrı bir `webdriver-pilot` işi (windows-2025) kurulumu sessizce yükler, `tauri-driver` + WebView2 sürümüne uyan `msedgedriver` ile **kurulu** uygulamaya bağlanır ve üç yolculuk koşar: uygulama açılır ve Ana sayfa çizilir; Hızlı Ekle (`N`, yaz, `Ctrl+Enter`) kütüphanede görünen bir not oluşturur; Ana sayfa → `Enter` → başlatıcı → `Enter` → `Space`, `Space`, `2` → oturum tam bir kart ilerler. Koşucu imajı, WebView2 ve sürücü sürümleri günlüğe yazılır. Kod `apps/desktop/e2e-windows/` (bağımlılıksız, W3C WebDriver HTTP). **Durum:** ilk CI koşusu (45038113867) kurulumu ve sürücü eşleşmesini geçti, oturum açılamadı (`DevToolsActivePort file doesn't exist`): koşucu yönetici yetkisiyle çalışıyor ve WebView2 150'den beri yükseltilmiş bir süreçte `msedgedriver`'ın kullandığı `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`'ı yok sayıyor. Pilot artık uygulamanın exe'si için HKLM WebView2 politikasıyla DevTools portunu açıyor, önce uygulamayı tek başına 15 sn çalıştırıyor (`PREFLIGHT`), oturum stratejilerini sırayla deniyor (`SESSION`) ve her başarısız denemenin tanısını artefakta koyuyor (`apps/desktop/e2e-windows/README.md`). Bu düzeltme okunarak bulundu, **Windows'ta henüz koşmadı;** yolculuklar hiç koşmadı. Art arda temiz koşulardan sonra yayın kapısına yükseltilir. `windows-smoke` (`Flashcard.exe --smoke`) ucuz bir tanı olarak kalır.

### 5.3 Görsel kontrol: mock ekran görüntüleri

`apps/desktop/scripts/screens.mjs` mock modunda her ekranın **açık/koyu × TR/EN × 1280/900** görüntüsünü `apps/desktop/screens/` altına alır (git'e girmez; CI'da `screens` artefaktı). Bu **yalnız görseldir**: hiyerarşi, hizalama, yoğunluk, taşma, kontrast, boş durumlar. Görüntüler açılıp gözle bakılır; otomatik bulgu sayısı tek başına yetmez ve hiçbir akışın çalıştığını kanıtlamaz.

```bash
pnpm -C apps/desktop screens            # tüm matris: --langs tr,en --themes light,dark --widths 1280,900
pnpm -C apps/desktop screens --quick    # yalnız EN, açık, 1280
pnpm -C apps/desktop screens --strict   # bulgu varsa çıkış kodu 1
# diğer: --langs tr --themes dark --widths 900 --port 23456 --url <dev:mock adresi> --out <klasör> --no-static
```

Çıktı `screens/<dil>-<tema>-<genişlik>/<ad>.png` + `index.html` + `report.json`. Vite mock modunda rastgele portta başlar (paralel worktree'ler çakışmaz). Ekranlar kenar çubuğundan, alt gezinmelerden (`subnav-item`, `settings-group*`, sekmeler; 1024 px altında `settings-select`) ve `open-dialog-*` kancalarından keşfedilir. Durumlar: deste sayfası ve terim listesi; her mod düğmesi **başlatıcıdan** (`launcher-start`) başlatılır → `state-<mod>`, `-active`, `-flipped`; Kartlar'da `state-cards-paused` (`Esc` = ara) ve kuyruğun tamamı cevaplandıktan sonra `state-cards-summary`; `state-test-result`, `state-match-finished`, Kütüphane ızgara/ağaç, Ara kart bilgisi / toplu çubuk / düzenleyici, Hızlı Ekle, palet; ayrı sayfada `?mock=fresh` (ilk açılış), `locked`, `update` ve `waiting` (`state-home-waiting` → `state-launcher` "Devam et" ile → `state-cards-waiting`). **Beklenen bir durum yakalanamazsa `state-missing` bulgusu** yazılır (`--strict` başarısız olur): kanca taşındıysa sessizce atlanmaz.

Otomatik kontroller: `overflow-x`, `outside-viewport`, `text-clipped`, `raw-control` (+ kaynak taraması `raw-control-static`), `contrast` (WCAG AA), `i18n-raw-key`, `page-error`, `state-missing`. Mock verisi (`src/lib/api/mock/`, `features/*/mock.ts`) Rust'ın davranışını taklit eder (başlatıcı kapsamları, boş başlatma, bekleme, rolleri taşıyan yerleşik not türleri) ama **gerçek değildir**: bir mock'un doğru davranması çekirdeğin doğru davrandığını göstermez.

## 6. Kabul kriterleri (v1.3)

- [x] Tek başlatıcı: her çalışma girişi aynı iletişim kutusunu açar; "Devam et" ilk; beş kapsam tam sayılarıyla; boş kapsam nedenini söyler; boş başlatma oturum açmaz ve yarım oturumu kapatmaz (S1, S2, S5; Rust: `study.options` sayısı = `study.start`'ın verdiği).
- [x] Bir tuş = bir iş: `Space` asla puanlamaz, puan yalnız arka görüldükten sonra, basılı tuş tek iş, `Esc` ara verir, yazı alanında `Ctrl+Z`/`Ctrl+A` metne aittir, puan kaydedilirken basılan tuş eski karta da sonrakine de düşmez (S1, S2, J7, S12, K1; `keys.test.ts`).
- [x] Pratik kapsamları takvime dokunmaz (S3, S4).
- [x] Kelime kartları: cevap anahtarı yalnız sorulan alan, kart arkası üç etiketli parça, "Tanım" doğru alanı düzenler, yakalanan cümle kaybolmaz (J6, S7a, S7b, S8, S9, S10).
- [x] İpucu kuralı: bağlam evet, cevap asla; ileri ve ters kartta (S7c).
- [x] "Tüm desteler" kuyruğu her destenin şablonuna uyar, kardeşleri desteler arasında da yarına bırakır; S1/S2 Ana sayfadan sırasız koşar (S1, S2, C4). Planlama'nın "bugün"ü aynı kuyruktur (C2); önden öğrenme penceresi ayardır ve değişikliği ekrandaki kartı almaz (S6, C3).
- [x] Sahibin gözüyle: Oluştur bulunduğun desteyle açılır, ara ver → Devam et odakta, başlatıcı istenen modu başlatır, Test kâğıdı klavyeyle (O1–O4); Eşleştir rollerden (C1).
- [x] Kart ekleme tek yerde (Tek kart / Toplu / Yapıştır / Dosyadan), içe aktarma Ayarlar'dan çıktı (J1, J2, J4, S9).
- [x] Ayarlar öğretir: açıklamalar gövde boyunda, örnek, varsayılan + "Varsayılana dön", Gelişmiş, arama (S11; `kit.test.ts`).
- [x] Tasarım sistemi korunur: token'lar, mono sayılar, kenarlık; ham kontrol yok, sabit metin yok, sayılar `$lib/format`'tan (lint testleri).
- [ ] Windows: kurulu uygulamada üç WebDriver yolculuğu yeşil (pilot, kapıda değil) ve gerçek bir Windows'ta elle koşu.
- [ ] Subtitle tarayıcı eklentisi gerçek bir tarayıcıda (sözleşme testleri ve tohumdaki yakalamalar geçiyor).

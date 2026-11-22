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
- Deste başına özet: yeni, öğreniliyor, bugün sırası gelen kart sayıları.
- `[v1.3]` "Bugün" sayıları envanter değil, **bugünün kuyruğudur**: o desteye şimdi başlatılan normal oturumun sunacağı kartlar (günlük ve oturum başı sınırlar, alt deste ayarı, kardeşleri yarına bırakma, boş kartlar dahil). `decks.tree` / `decks.summaries` bunları `today_due`, `today_learning`, `today_new`, `today_total` alanlarında verir (eski `due`/`learning`/`new` alanları aynı değeri taşır); Ana sayfa, Kütüphane, deste sayfası, başlatıcı ve hatırlatıcı aynı kuyruk planlayıcısını (`today_counts`) kullanır (bkz. [03 §9.1](03-mimari.md)).
- `[v1.3]` **"Tüm desteler"** (`deck_id: null`) kuyruğu, destelerin kendi kuyruklarının birleşimidir: her üst düzey deste ve alt destelerini dahil etmeyen bir destenin alt desteleri, **her biri kendi ayar şablonuyla** (günlük yeni/tekrar sınırı, sıra, kardeşleri yarına bırakma) planlanır; öğrenme kartları vade sırasıyla önde, sonra tekrarlar ve yeniler desteler arasında sırayla dağıtılır. Bir destede sınır dolunca diğerleri sıfırlanmaz. Koleksiyon çapında üst sınır yalnızca istenirse (`new_limit`) ya da `study.new_per_session` uygulama genelinde ayarlıysa uygulanır. Kardeşleri yarına bırakma **desteler arasında da** geçerlidir: notunun bir kartı kuyruktaysa, kardeşlerini bırakan destedeki tekrar/yeni kart dışarıda kalır (öğrenme kartları hep kalır).

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
- **Alan rolleri** `[v1.3]`: Her alana isteğe bağlı bir rol verilir: alanın *ne içerdiği*. `term` (sözlük hâli: *go*), `surface` (görüldüğü hâli: *went*), `meaning` (anlam), `example` (terimi içeren cümle), `example_translation`, `source`, `note`. Bir rol bir not türünde en fazla bir alanda olur; alan adından tahmin edilmez. Not türü düzenleyicisinde "Bu alan ne içeriyor?" seçicisi.
- **Cevap alanı** `[v1.3]`: Her kart şablonu isteğe bağlı olarak *neyi sorduğunu* söyler (`answer_field`): ileri kelime kartı anlamı, ters kart terimi sorar. Yazılı cevap, Test, Öğren ve Eşleştir'de beklenen cevap bu alanın metnidir: altyazıdan gelen *went* (go → gitmek) kartına "gitmek" yazan doğru sayılır; örnek cümle, çevirisi ve kaynak cevaba karışmaz. Düzenleyicide "Kartlar ne soruyor?".
- **Etiketli kart arkası** `[v1.3]`: Kelime, Cümle ve Altyazı kartlarının arkası üç ayrı parçadır: anlam (en büyük), örnek (cümle + çevirisi, "Örnek" etiketli), kaynak (en küçük, "Kaynak" etiketli).
- `[v1.3]` Hazır not türleri roller ve cevap alanlarıyla gelir; var olan koleksiyonlarda **migration 1300** bunları yalnızca kullanıcının düzenlemediği yerleşik türlere ekler (düzenlenmiş şablon, alan ve CSS'e dokunulmaz). Ayrıntı: [03 §4.1](03-mimari.md).
- **Not türleri ekranı (öğrenen görünümü)** `[v1.3]`: Ayarlar › Not türleri önce tür kutucuklarını, alan listesini ve çevrilebilen canlı kart önizlemesini gösterir. "Yeni not türü" hazır başlangıç noktalarından açılır; şablon kodu, CSS ve alan bayrakları "Gelişmiş: şablon kodu" altındadır. Kaydetmeden ya da silmeden önce hangi verinin kaybolacağı tam olarak söylenir.
- Kart şablonları alanları yerleştiren bir şablon dili kullanır: `{{kelime}}`, `{{#ornek}}...{{/ornek}}` (koşullu), `{{tts:kelime}}` (sesli okuma), `{{type:anlam}}` (yazarak cevaplama), `{{hint:ornek}}` (ipucu), `{{unless_same:diger:alan}}` (alan, metni diğer alanınkiyle aynı değilse; v1.3).
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
- **Tempo planlayıcı** `[v1.3]`: "Günde kaç yeni kart?" ve isteğe bağlı "Sınav tarihim" seçildikçe önümüzdeki haftaların günlük iş yükü (kart/gün, dk/gün) canlı tahmin edilir: "Bu tempoyla 3 hafta sonra günde ~85 kart, ~14 dk". Model saf Rust'tadır (`fc_study::planning::forecast`): kartların bugünkü FSRS durumundan, şablonun ağırlıkları ve hedef hatırlamasından **beklenen değer** simülasyonu (rastgelelik yok; kart sayısı korunur; yeni kart temposunda monoton). Dakikalar son 30 günün ortalama cevap süresinden. Sorgu `planning.forecast`; "Uygula" (`planning.apply_pace`) destenin şablonundaki günlük yeni kart sınırını yazar (şablonu yoksa deste adıyla bir şablon oluşturup atar), istenirse sınav tarihini ekler — tek geri al adımı; önce hangi şablonun ve destelerin değişeceğini söyler.
- **Günlük sınırlar şablondadır** `[v0.4]`: yeni kart/gün (`new_per_day`, varsayılan 20) ve tekrar/gün (`reviews_per_day`, varsayılan 200); oturum başına yeni kart `study.new_per_session`. Sınırlar yalnız **bugünün kartlarına** uygulanır: sınır dolduktan sonra da pratik kapsamlarıyla (§6.1) istendiği kadar çalışılır. (Yeni kartların "tanıtım sırası" ayarlanabilir: rastgele, ekleme sırası.)

## 6. ✅ Çalışma modu

`[v0.2]` · 🧱 Oturum motoru, zamanlayıcı, kart türü motoru, olay kaydı

- Akış: kart gösterilir → (cevap verilir) → çevrilir → puanlanır → sonraki kart.
- **Oturum motoru** backend'dedir: sıradaki kartı backend seçer, frontend sadece gösterir.
- Kart sırası: sırası gelenler, öğrenilmekte olanlar, yeniler. Karıştırma seçenekleri: rastgele, yeniden eskiye, en zordan kolaya.
- **Kardeş kartlar** aynı gün gösterilmez (yarına bırakılır), ayarla kapatılabilir.
- **Cevap süresi** ölçülür ve olay olarak kaydedilir (cevabı gösterene kadar ve puanlayana kadar ayrı ayrı).
- **Geri al:** son puanlama geri alınır, kart tekrar gösterilir.
- **Oturum sonu ekranı:** çalışılan kart sayısı, doğruluk, süre, "**Yanlışları tekrar çalış**" butonu. `[v1.3]` "Yeniden çalış" başlatıcıyı açar (yanlış varsa "Bugün zorlandıklarım" önceden seçili).
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

### 6.1.1 Çalışma başlatıcı

`[v1.3]` · 🧱 Kuyruk planlayıcısı, oturum motoru, modül kaydı

- **Tek giriş:** Her "çalış" girişi (Ana sayfanın Başlat düğmesi, deste satırları ve `Enter`; Kütüphane satırı; deste sayfasının Çalış düğmesi, `Enter` ve mod düğmeleri; komut paleti; eklenen kartların özetindeki "Şimdi çalış"; istatistiklerdeki "Bunları çalış"; oturum sonundaki "Yeniden çalış") **aynı pencereyi** açar. Hiçbiri doğrudan oturum başlatmaz. Pencere verisini `study.options` sorgusundan alır (yan etkisiz).
- **Pencerenin sırası:** yarım kalan oturumlar en üstte ve odakta ("Devam et · 7 cevap"); mod (Kartlar / Öğren / Test / Eşleştir: `study.options.modes`, kapalı modülün modu gelmez; her birinin takvime ne yaptığı yazılır); beş kapsam tam sayılarıyla; kapalı "Seçenekler" (pratik, süre/güven, sınıflandırma, sorgu, kayıtlı sorgu, erteleme, test soru türleri). `Enter` başlatır, `Esc` kapatır. Deste başına son mod ve mod başına son kapsam hatırlanır (yalnızca görünüm tercihi, tarayıcı deposunda).
- **Kapsamlar** (`study.start { scope }`; `study.options` hep beşini bu sırayla verir):

  | Kapsam | Ne | Takvimi etkiler mi? |
  |---|---|---|
  | `due` · Bugünün kartları | Normal oturumun kuyruğu: sırası gelenler, öğrenme adımları ve yeniler, sınırlar içinde | ✅ |
  | `new` · Yeni kartlar | Yalnızca hiç görülmemiş kartlar, yeni kart sınırı içinde | ✅ |
  | `studied_today` · Bugün çalıştıklarım | Bugünkü çalışma gününde herhangi bir modda cevaplanan farklı kartlar | ❌ pratik |
  | `struggled_today` · Bugün zorlandıklarım | Bugün "Tekrar" denen ya da yanlış cevaplanan farklı kartlar | ❌ pratik |
  | `all` · Tümü | Destedeki her kart (askıdakiler hariç) | ❌ pratik |

- Son üçü **pratiktir**: istek ne derse desin oturum `trial` olur, kart kümesi başlarken **sabitlenir**, yarına bırakılmış kardeşler de çalışılır, öğrenme adımı beklenmez.
- **Sayı, sunulacak olandır:** bir kapsamın sayısı, o an başlatılan oturumun sunacağı kart sayısının tam kendisidir (Rust testiyle güvencede: `scopes[due].count` = `study.start { kind: normal, scope: due }`'nun sunduğu kartlar). "Bugün" yerel çalışma günüdür (`study.day_start_hour`, varsayılan 04:00).
- **Boş kapsam** devre dışı görünür ve nedenini söyler (`empty_reason`): `no_cards` (destede kart yok), `nothing_due`, `daily_limit_reached` ("bugünün kart sınırı"), `waiting` (yalnızca adımını bekleyen öğrenme kartları), `none_studied_today`, `none_struggled_today`. Durum değişecekse `next_available_at` ne zaman olacağını verir: sınırda bir sonraki çalışma gününün başı, beklemede ilk kartın gösterilebileceği an.
- **Boş başlatma hiçbir şey yapmaz:** seçilen küme boşsa `study.start` `CONFLICT_NOTHING_TO_STUDY` hatası döner (`details.reason`, `details.next_available_at`); oturum oluşturmaz, açık oturumu **kapatmaz**. Başlatıcı açık kalır.

### 6.1.2 Açık oturumlar, devam etme, bekleme

`[v1.3]`

- **Her türden bir oturum açık kalabilir** (Kartlar, Öğren, Test, Eşleştir, özel, yanlışlar): yeni bir Kartlar oturumu yarım test kâğıdını kapatmaz; aynı türden yeni başlatma yalnızca o türün açık oturumunu kapatır. Test kâğıdı cevaplarını taslak olarak saklar (`modes.test.save_draft`), devam edilince geri gelir.
- **Devam et:** başlatıcı bu destenin yarım oturumlarını (her tür, en yenisi önce) cevaplanan ve kalan sayılarıyla sunar. Önceki çalışma gününden kalan Kartlar oturumu sunulmaz (yeni başlatma bugünün kuyruğunu kurar); silinmiş destenin oturumu da.
- **`Esc` = duraklat:** kart kalmışken Kartlar oturumundan çıkmak o ana kadar yapılanı gösterir (`study.summary`) ve oturumu **açık bırakır**; başlatıcıdaki ve özetteki "Devam et" aynı oturuma döner. Hiç cevap verilmemişse oturum kapanır; kuyruk bittiğinde özet gelir.
- **Bekleme durumu:** kalan kartların hepsi öğrenme adımını bekliyorsa oturum açık kalır. `SessionDto.next_due` kartın gösterilebileceği anı verir; ekran "Sıradaki kart N dk sonra" geri sayımını ve **Bekle / Başka bir şey çalış / Bitir** seçeneklerini gösterir. Başka kart kalmadığında öğrenme kartı, adımı dolmadan en çok **önden öğrenme penceresi** kadar erken gösterilir: `study.learn_ahead_mins` ayarı ("Öğrenme kartını erken göster", varsayılan 20 dk, 0 = adım dolana kadar hep bekle). `next_due` ve `next_available_at` kartın gerçekten sunulacağı andır (vade − pencere); bekleme ekranı yalnız gerçekten beklemen gerektiğinde görünür.
- **Ayar değişikliği ekrandaki kartı almaz:** oturum açıkken pencere küçültülürse ekrandaki kart geçerli kalır; oturum, gösterilen kartı seçtiği pencereyi bir sonraki cevaba kadar korur (`SessionState.learn_ahead_ms`, `Session::shown_window_ms`). Büyütülen pencere hemen geçerlidir.
- **Tek kavram, tek söz:** sırası gelen (EN "Due"; tarih sütunu "Tekrar tarihi"), **Kenara koy** / "Kenarda" (askıya alma; EN "Set aside"), **Yarına bırak** / "Yarına bırakıldı" (gömme; EN "Leave for tomorrow"), oturumdan çıkış **Ara ver** (EN'de tek "Pause"), öğrenme sayısı "Öğreniliyor"; sayıların ne demek olduğu yazar.

### 6.2 Çalışma sırasında araçlar

- 💬 **İpucu motoru:** Ayarlanabilir ipucu stratejileri: ilk harf, her kelimenin ilk harfi, harflerin bir kısmı, belirli bir alan (ör. örnek cümle), AI ipucu. Kaç ipucu kullanıldığı kaydedilir.
  - `[v1.3]` **İpucu türleri** (`study.hint` → `HintDto.kind` + `html`): **bağlam** (`context`): notun örnek cümlesi (`example` rolündeki alan) terimi içeriyorsa ilk ipucu, o geçişin gizlendiği cümledir (`I ___ home.`). **Harf** (`letters`): sonraki seviyeler ve kullanılabilir cümlesi olmayan notlar, seçili stratejiyle. Cümle kartın ön yüzünde zaten görünüyorsa ya da cümle cevabı ele verecekse harf ipucuyla başlanır.
  - **Kural:** ipucu bağlamı gösterebilir ama cevabı asla. Yerleşik kelime/altyazı **ileri** kartının ön yüzü yalnızca kelimeyi gösterir (Altyazı'da geçtiği hâli, farklıysa yanında sözlük hâli: "went go"); ilk ipucu, kelimesi vurgulanmış örnek cümledir (bağlam; anlam cümlede geçiyorsa yalnız orası boşaltılır). **Ters** (üretim) kartın ilk ipucu kelimenin boşaltıldığı cümledir ("I ___ home."). Sonraki seviyeler harftir. Mevcut koleksiyonlarda yerleşik Altyazı ön yüzü migration 1301 ile (yalnızca şablon düzenlenmemişse) yalnız kelimeye indi; yerleşik Kelime kartının ön yüzünde zaten cümle yoktu.
- 📝 **Kart üstüne not:** Kullanıcının kişisel notu (ör. "Bunu şöyle hatırlıyorum"). Not türü alanlarından ayrıdır.
- 🚩 **Bayraklama:** Renkli bayraklar (kırmızı, turuncu, yeşil, mavi, mor...). Bayrak adları özelleştirilebilir (ör. kırmızı = "düzelt").
- ⏸️ **Kenara koy** (askıya alma): Kart sen kenardan çıkarana kadar gösterilmez.
- ⛱️ **Yarına bırak** (gömme): Kart sadece bugün gösterilmez.
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

`[v1.3]` **Beklenen cevap:** kendi kendine puanlanan bir kartın şablonunda `answer_field` varsa beklenen cevap o alanın metnidir (§3); yoksa arka yüzün cevap kısmı (eski davranış). Yazma, boşluk doldurma ve çoktan seçmeli motorlar kendi cevaplarını belirlemeye devam eder. Bu kural Test, Öğren ve Eşleştir'in cevap anahtarlarını da düzeltir.

Sonuç sadece doğru/yanlış değil, **derecelidir**: `doğru`, `neredeyse`, `yanlış` + fark (diff). Kullanıcı sonucu geçersiz kılıp kendi puanını verebilir. Kontrolcü bir puan **önerir**, varsayılan olarak puanlamayı kullanıcı yapar (ayarla otomatik puanlama açılabilir).

## 8. 🏷️ Etiketler

`[v0.1]` · 🧱 Veri modeli

- Notlara etiket verilir. Hiyerarşik etiketler: `dil::fiil::düzensiz`.
- Etiket yeniden adlandırma ve birleştirme (tüm notlarda).
- Etiket rengi.

### 8.1 🎚️ Sınıflandırmalar (seviye, sınav, ünite…)

`[v0.1 veri modeli + not ekleme formu · v0.4 sorgu/filtre/toplu atama · v0.6 istatistik · v0.10 seviye ilerlemesi]` · 🧱 Veri modeli (`attribute_defs`/`attribute_values`), komutlar, sorgu dili, modül kaydı

Etiketler serbesttir. Sınıflandırma ise **kullanıcının tanımladığı yapılı bir boyuttur**: adı, türü ve seçenekleri vardır, bir deste grubuna bağlanır ve alt destelere miras kalır.

- **Örnekler:** `İngilizce` grubunda *Seviye* = A1 · A2 · B1 · B2 · C1 · C2. `YKS` grubunda *Sınav* = TYT · AYT · YDT, *Ders* = Matematik · Fizik…, *Konu* = serbest liste. `Japonca` grubunda *JLPT* = N5…N1.
- **Türler:** sıralı seçim (seviye gibi, `>=` karşılaştırılabilir), sırasız seçim, çoklu seçim, sayı, metin, tarih, evet/hayır.
- **Seçenekler tamamen özelleştirilebilir:** ekle, yeniden adlandır, sırala, renk ver, birleştir, sil (kullanan notlar başka seçeneğe taşınır). Ad değişse de kimlik değişmez.
- **Kapsam ve miras:** Sınıflandırma koleksiyon geneline veya bir desteye tanımlanır. O destenin tüm alt destelerinde geçerlidir. Alt deste ek sınıflandırma ekleyebilir.
- **Hazır şablonlar** (kopyalanıp değiştirilebilir): CEFR (A1–C2), YKS (TYT/AYT/YDT), LGS, KPSS, JLPT, HSK, okul sınıfı (1–12), zorluk (kolay/orta/zor). Şablon kopyalandıktan sonra tamamen kullanıcının olur.
- **Not ekleme/düzenleme:** Destenin geçerli sınıflandırmaları formda alan olarak çıkar. Varsayılan değer verilebilir ve "son kullanılanı hatırla" seçilebilir. Zorunlu yapılabilir.
- **Kullanıldığı yerler:** sorgu dili (`seviye:B1`, `seviye>=B1`, `sınav:TYT ders:fizik`) `[v0.4]`, özel çalışma oturumu ve filtreli deste `[v0.2/v0.4]`, toplu atama `[v0.4]`, istatistik kırılımı `[v0.6]`, içe aktarmada sütun eşleme `[v0.5]`, AI üretiminde seviyeye uygun içerik `[v0.8]`, seviye ilerlemesi (bir seviyenin %X'i olgunlaşınca sonrakini aç) `[v0.10]`, eklentiler için okuma/yazma API'si `[v0.7]`.
- **Aynı yapı taşıyla gelen başka özelleştirmeler:** kaynak kitap/sayfa, ünite/hafta, öncelik, kelime türü (isim/fiil), okul/kurs dönemi… Kullanıcı ne isterse, kod değişikliği olmadan.
- Not düzeyinde atanır. Kart düzeyi gerekirse (ör. yalnızca bir yön zor) kart düzeyinde de atanabilir.

## 9. 🔎 Arama ve sorgu dili

`[v0.4]` · 🧱 Sorgu dili (DSL), SQLite FTS5

- Tek bir **sorgu motoru** her yerde kullanılır: arama, özel oturum, toplu işlem, istatistik filtreleri, otomasyon kuralları, eklentiler.
- Örnek sorgular:
  - `deck:İngilizce tag:fiil` → destede ve etikette
  - `deck:"Dil::İngilizce"` → tırnakla boşluk
  - `is:due` · `is:new` · `is:suspended` · `is:buried` · `is:flagged` · `flag:red`
  - `stability>30` · `difficulty>7` · `lapses>=5` · `reps<3` · `retrievability<0.8`
  - `added:7` (son 7 gün) · `reviewed:1` · `due:0` · `due:3`. `[v1.3]` `added:N`, `edited:N` ve `reviewed:N` yerel **çalışma günlerini** sayar (`study.day_start_hour`): `reviewed:1` = bugünkü çalışma günü, `reviewed:2` = bugün ve dün (önceden kayan 24 saatlik dilimlerdi).
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
- **Toplu işlemler:** Seçili kartlara: taşı, etiket ekle/çıkar, kenara koy, yarına bırak, bayrakla, sil, vadeyi sıfırla, not türünü değiştir (alan eşleme ile), bul-değiştir. Hepsi **tek transaction** ve **tek geri al**.
- **Çöp kutusu:** Silinen notlar 30 gün (ayarlanabilir) tutulur, geri yüklenebilir, sonra kalıcı silinir.
- **Tekrar eden kart tespiti:** Kart eklerken ve içe aktarırken, "benzersiz" işaretli alana göre (normalize edilmiş) uyarı. Ayrıca "Tekrarları bul" aracı.
- **Veritabanı kontrolü:** Sahipsiz kart, eksik medya, bozuk alan, tutarsız zamanlama tespiti ve onarımı. Önce rapor, sonra onayla onarım.
- **Deste ayar şablonları (preset):** Algoritma parametreleri, hedef hatırlama oranı, kardeş gömme, yeni kart sırası vb. ayarlar bir presette toplanır, birden çok desteye uygulanır.
- **Terim listesi** `[v1.3]` (deste sayfası, `notes.terms`): sütunlar rollere göre seçilir (Terim = `term`, Tanım = `meaning` alanı); her satır `term_field` / `definition_field` anahtarlarını taşır ve satır içi düzenleme bunlara yazar (Altyazı kartında "Tanım"ı düzenlemek artık "geçtiği hâli" değil anlamı değiştirir). Terim ve anlam rolü birlikte olmayan türlerde ilk iki alan gösterilir ve satır `ambiguous: true` gelir: kalem tahmin etmek yerine tam editörü açar.

## 11. ℹ️ Kart bilgisi ve geçmiş

`[v0.2, v0.4]` · 🧱 Olay kaydı

- Kartın tüm geçmişi: her tekrarın tarihi, puanı, süresi, o andaki stabilite/zorluk, kullanılan ipucu, oturum türü.
- Eklenme tarihi, son değişiklik, bir sonraki vade, mevcut hatırlama olasılığı (v1.4: istatistik ve `retrievability<…` sorgusuyla aynı sayı — hazır ayarın kişisel sönümü, tam çalışma günü; `fc_core::recall`).
- Hafıza grafiği (zamana göre stabilite).
- Notun düzenleme geçmişi (eski alan değerleri).

## 12. ↩️ Geri al / yinele

`[v0.2]` · 🧱 Olay kaydı, komutlar

- Her komut geri alınabilir: puanlama, düzenleme, silme, toplu işlem, içe aktarma.
- Sınırsız geri al yığını (oturum boyunca), Ctrl+Z / Ctrl+Shift+Z.
- Arayüz son işlemi gösterir: "Geri alındı: 50 kart taşındı".

## 13. ⌨️ Klavye kısayolları ve komut paleti

`[v0.2, v0.4]` · 🧱 Komutlar

- Çalışmada (`[v1.3]` **bir tuş, bir iş**):
  - `Space` kartı çevirir (ön ⇄ arka, istendiği kadar); **asla puanlamaz**.
  - `1-4` puanlar (iki düğmeli görünümde yalnız `1` "Hâlâ öğreniyorum" ve `2` "Biliyorum"), ama **yalnızca arka yüz görüldükten sonra**; önce hiçbir şey yapmaz. Çoktan seçmeli kartın sorusunda rakamlar seçenek seçer.
  - `H` ipucu (soru yüzünde), `S` kenara koy, `B` yarına bırak, `Z` son puanı geri al, `Esc` ara ver (oturum açık kalır, §6.1.2), `Ctrl/⌘+1–7` bayrak; özet ekranında `R` yanlışları tekrarlar.
  - `Enter` sayfa kısayolu değildir: yalnızca odaktaki denetimi çalıştırır (odaktaki kart çevrilir, odaktaki düğmeye basılır).
  - Basılı tutulan tuş bir kez işler (tekrar etmez). Tuş açıklamaları ve `aria-keyshortcuts` kısayol düzenleyicisindeki atamayı izler.
- Genel: `Ctrl+K` komut paleti, `Ctrl+N` yeni not (Hızlı ekleme), `Ctrl+F` ara, `Ctrl+Z` / `Ctrl+Shift+Z` geri al / yinele.
- `[v1.3]` **Yazarken tuşlar alana aittir:** odak bir metin alanındayken genel kısayollar çalışmaz (`Ctrl+A` metni seçer, `Ctrl+Z` yazıyı geri alır, harfler yazılır); yalnızca komut paleti açılabilir. Açık bir pencere (Hızlı ekleme, onay, kilit ekranı) kendi tuşlarına sahiptir; açık bir seçim listesinde yazılan harf seçeneği seçer. Basılı tutulan kısayol bir kez çalışır (ör. `Ctrl+Z` on kez geri almaz).
- `[v1.3]` **Kısayol sayfası:** `?` (ya da üst çubuk ve palet) bağlama göre gruplanmış (genel, çalışma, liste, editör) aranabilir bir kısayol sayfası açar. Tablo `shortcuts.list`'ten, yani kısayol düzenleyicisinin düzenlediği kaynaktan gelir; arayüz hangi kısayolun var olduğunu bilmez.
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

### 15.4 Tek yerden kart ekleme

`[v1.3]` · 🧱 Komutlar, içe aktarıcı, iş kuyruğu, alan rolleri

- **Kart ekle** sayfasında dört yol: **Tek kart** (Hızlı ekleme), **Toplu** (tablo), **Yapıştır**, **Dosyadan**. Hepsi aynı hedef seçiciyi (yeni ya da var olan deste, not türü) ve aynı onayı paylaşır: "12 kart eklendi · Geri al · Desteyi aç · Şimdi çalış", atlanan satırlar nedenleriyle. "Şimdi çalış" başlatıcıyı açar (§6.1.1).
- **Yapıştır** ve **Dosyadan** tek rehberli akıştır (`io.import.*`): kaynak → sütunlar not türünün alanlarına eşlenir (rol adlarıyla, canlı ön/arka önizlemeyle) → ayırıcılar algılanır, değiştirilebilir → tekrar kuralı düz sözle → içe aktarma işi, sonuç, geri al. Sürükle-bırak çalışır.
- **Rol hedefleri:** bir sütun bir alan anahtarına ya da bir **role** eşlenebilir (`{kind: "role", role}`; türde o rol yoksa bilinmeyen alan gibi reddedilir). "kelime / anlam / cümle" sütunları rolü olan türlerde doğru alanlara gider, cümle sütunu `example` alanında kalır. Eşleme önerisi: önce başlıktan (TR/EN rol sözcükleri, ör. "sentence", "örnek", "definition"; alan adları), yoksa o not türü için son eşlemeden, yoksa rol sırasından (term, meaning, example…). JSON içe aktarmada alan adı yerine rol adı kabul edilir.
- **Hızlı ekleme:** önce terim / anlam / isteğe bağlı örnek (diğer alanlar "Daha fazla" altında), tekrarlar satır içinde açıklanır, aynı onay; altta seçili desteye Toplu / Yapıştır / Dosyadan bağlantıları.

## 16. 📦 Dışa aktarma, deste paketi, yedek, yazdırma

`[v0.5]`

- **Deste paketi:** Tek dosya (`.fcpack`, zip tabanlı): notlar, kartlar, not türleri, medya, isteğe bağlı öğrenme geçmişi. Sürüm bilgisi taşır. Paylaşılabilir.
- **Tam koleksiyon yedeği:** Tüm koleksiyon tek dosya (`.fcbackup`). Elle alınır ("Yedek al" butonu). Geri yükleme önce mevcut durumun yedeğini alır.
- `[v1.4]` **Her zaman geri dönen yedek.** Yedek dosyadan dosyaya **akışla** yazılır ve okunur (bellek en büyük tek medya dosyası kadar; uygulamanın yazabildiği her yedeği geri okuyabilmesi için toplam boyut tavanı yok). Dosya önce `.part` olarak yazılır, **geçici bir profile geri yüklenip** sayılar (deste, not türü, not, kart, tekrar geçmişi, medya) ve her medyanın SHA-256'sı karşılaştırılır; ancak geçerse yerine konur. Eksik (diskten silinmiş ya da bozulmuş) bir medya yedeği durdurmaz: yedek yazılır, **eksik** olarak işaretlenir ve `missing_media` listesinde söylenir. Geri yükleme, manifestteki her girdinin var olduğunu ve eşleştiğini canlı veriye dokunmadan önce denetler.
- `[v1.4]` **Otomatik yedek:** `io.auto_backup` (kapalı / her gün / her hafta, varsayılan her gün) ve `io.auto_backup_keep` (saklanan doğrulanmış kopya, varsayılan 7). Kopyalar koleksiyonun yanındaki `backups/auto/` klasörüne gider; yeni kopya doğrulanmadan eskiler silinmez, en yeni sağlam kopya hiç silinmez. `io.backup.verify` herhangi bir yedeği geçici profilde dener; `io.backup.status` son doğrulanmış yedeği, sıradaki zamanı ve son hatayı söyler; `io.backup.auto` hemen bir tane başlatır. İptal edilebilir iş.
- **Yazdırma / PDF:** Desteyi kâğıt kart düzeninde (ön yüzler bir sayfa, arka yüzler ayna sıralı) veya liste halinde yazdırma.
- `[v1.3]` Dışa aktarma, yedek ve yazdırma Ayarlar › Veri › **Dışa aktar ve yedekle** sayfasındadır, her biri bir satırlık amaç cümlesiyle; içe aktarma oradan Kart ekle › Dosyadan'a bağlanır (§15.4).

## 17. 📊 İstatistikler

`[v0.6]` · 🧱 Olay kaydı, sorgu dili

Her istatistik global, deste, etiket veya sorgu bazında görüntülenebilir; tarih aralığı seçilebilir.

| İstatistik | Açıklama |
|---|---|
| 🔥 Seri | Kaç gündür aralıksız çalışıldığı, en uzun seri |
| ✅ Başarı oranı | Doğru cevap yüzdesi (yeni / genç / olgun kartlar ayrı) |
| 📅 Aktivite ısı haritası | Gün gün çalışma yoğunluğu |
| 🐌 Zor kartlar (leech) | Unutma sayısı eşiği aşan kartlar. Eşik ayarlanabilir. Eylem: etiketle, kenara koy, AI ile yeniden yaz |
| ⏱️ Cevap süreleri | Ortalama, dağılım, en yavaş kartlar |
| 🔮 Gelecek tahmini | Önümüzdeki günlerde sırası gelecek kart sayısı |
| 🧠 Hafıza durumu | Yeni / öğreniliyor / genç / olgun / askıda dağılımı |
| 📈 Gelişim | Haftalık/aylık çalışma ve başarı trendi |
| 🕐 En verimli saat | Saatlere göre başarı oranı |
| 📉 Stabilite / zorluk dağılımı | FSRS parametrelerinin histogramları |
| 🎯 Gerçekleşen vs hedef hatırlama | Hedef %90 ise gerçekte kaç |
| 📈 Deste karnesi | Destenin ne kadarının "oturduğu" yüzde olarak |

- **Genel bakış üç soruyla açılır** `[v1.3]`: **Bugün nasılım?** (`stats.today`: bugün / dün cevaplar, doğruluk, süre, seri, hedef; Rust'ın "dün ile aynı tempodasın / gerisindesin / öndesin" yargısı), **Önümde ne var?** (gelecek tahmininin 7 günü — bugün ve en yoğun gün yazıyla işaretli — bir tıkla 30 gün), **Nelerde zorlanıyorum?** (`stats.struggles`: son 1 / 7 / 30 günde "Tekrar" ya da yanlış cevaplanan kartlar, her modda, pratik ve test dahil; kelime, anlam, yanlış/cevap, son görülme. Kelime ve anlam alan rollerinden seçilir (`term`, yoksa `surface`; `meaning`), rolsüz türlerde bilinen alan adlarından). Birincil eylem **"Bunları çalış"**: çalışma başlatıcısını (§6.1.1) Rust'ın listeyle verdiği `cid:` sorgusu ve **pratik** (takvim değişmez) önceden seçili açar. Kenara koy / etiketle satır eylemi olarak kalır. Diğer tüm istatistikler altta "Ayrıntılar" başlığında, her biri bir satırlık "bu ne anlama geliyor" açıklamasıyla.
- İstatistikler olaylardan hesaplanır. Yeni istatistik eklemek şema değişikliği gerektirmez.
- Büyük koleksiyonlar için ön hesaplanmış özet tabloları (olaylardan türetilir, her an yeniden üretilebilir).

## 18. 🎯 Hedefler

`[v0.6]`

- "Ay sonuna kadar 500 kelime öğren", "Her gün 30 dk çalış" gibi hedefler.
- Hedef türleri: öğrenilen kart sayısı, çalışma süresi, seri, deste tamamlama yüzdesi, **günlük kart** `[v1.3]` (`daily_cards`: son tarih yoksa bugünkü cevap sayısı, varsa hedef konduğundan beri toplam).
- `[v1.3]` Hedef koymak boşluklu bir cümledir: "Her gün __ kart çalış", "__ tarihine kadar __ destesini bitir" (destenin başlanmamış kartlarını öğrenmek: son tarihli öğrenilen kart hedefi), "Her gün __ dakika", "__ gün üst üste", "__ destesinde oturmuş kart oranını %__ yap"; listelenen hedefler de aynı cümleyle okunur. Ana sayfanın hedef kutucuğu aynı `goals.list` verisini gösterir.
- İlerleme çubuğu, gidişata göre tahmin ("Bu hızla 3 gün geç kalırsın").

## 19. 🔔 Hatırlatıcılar

`[v1.0, v1.3]` · 🧱 Kuyruk planlayıcısı, şemalı ayarlar

- İşletim sistemi bildirimi: "Bugün 40 kartın var." (sırası gelen + öğrenme ve yeni ayrı sayılır).
- `[v1.3]` **Sayı, Başlat'ın sunacağı kuyruktur:** tüm koleksiyon için ortak kuyruk planlayıcısının bugünkü kuyruğu (günlük yeni/tekrar sınırları ve oturum başı sınır dahil), yani Ana sayfanın gösterdiği sayı. v1.2 destelerin envanterini topluyordu (görülmemiş her kart "yeni" sayılıyordu, günlük sınır yok sayılıyordu), bu yüzden bildirim Başlat'ın sunamayacağı kartları da vaat ediyordu. Koleksiyon kuyruğu destelerin kendi sınırlarıyla kurulur (§1); bildirim de aynı `today_counts` sayısını kullanır.
- Saat ve haftanın günleri seçilir. Günde en çok bir bildirim; seçilen saat kaçırıldıysa (uygulama kapalıydı) 3 saate kadar yine gönderilir, sonra o gün atlanır.
- **Bugün çalışıldıysa** (bugünkü çalışma gününde en az bir normal cevap) ya da kuyruk boşsa bildirim gönderilmez. `[v1.3]` "Bugün", sabit 04:00 yerine `study.day_start_hour` ayarına göre başlar.
- "Bildirimi dene" (`reminders.test`) aynı sayıyla hemen bir bildirim gönderir; `reminders.status` sıradaki bildirim zamanını ve sayıları verir.
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
- **Hazır sağlayıcılar** `[v1.3]`: Claude, OpenAI, OpenRouter, Google Gemini, Groq, Mistral, DeepSeek, Ollama (yerel), LM Studio (yerel), "Özel (OpenAI uyumlu)". Bunlar dört protokolün (Anthropic, OpenAI, OpenAI uyumlu, Ollama) üstünde **ön ayardır**, yeni motor değil. Anahtar gerekip gerekmediği tür ve adresten anlaşılır: barındırılan uyumlu servisler (OpenRouter gibi) anahtar ister, bu bilgisayardaki ya da yerel ağdaki sunucular istemez (v1.2'de OpenRouter'ın anahtar alanı hiç görünmüyordu).
- **Kurulum üç adım** `[v1.3]`: anahtar → model (sağlayıcının listesinden aranabilir seçici, `ai.models` işi; listede olmayan ad da yazılabilir) → **Bağlantıyı dene** (`ai.providers.test`: tek ve en küçük istek; sonuç düz sözle: anahtar yok/yanlış, kredi yok, model yok, adres yanlış, hız sınırı, servis çalışmıyor, ulaşılamıyor, sertifika). Sağlayıcı satırları durumu gösterir (hazır, anahtar gerekli, model gerekli, test başarısız). Ayrıntı: [06 §1.1](06-ai-ve-entegrasyonlar.md).
- `[v1.3]` İki yer, iki iş: **Oluştur › Yapay zeka** yalnızca "Yapay zekayla kart üret"tir (deste formdaki bir alan; sağlayıcı yoksa kuruluma bağlantı verir); **Ayarlar › Bağlantılar › Yapay zeka** sağlayıcılar ve kullanım/harcama sekmeleridir.
- `[v1.3]` AI ipucu, kontrol ve açıklama "cevap" olarak kartın **cevap alanını** (şablonun `answer_field`'ı) alır, bütün arka yüzü değil.
- **Asistanını bağla** `[v1.3]` (MCP ekranının ilk sekmesi): istemci seçilir (Claude Desktop, Claude Code, diğer), bu profilin gerçek `collection.db` ve `fc` yollarıyla doldurulmuş, kopyalanabilir parçacık gösterilir (`mcp.setup`), nasıl deneneceği yazar; asistanın neler yapabileceği (araç listesi, okur/değiştirir) ve "Yalnızca okuyabilsin" anahtarı. Dış MCP sunucuları ayrı sekmededir. Ayrıntı: [06 §2.1](06-ai-ve-entegrasyonlar.md).

## 22. 🌉 Dış dünya ile bağlantı

`[v0.9]` · Ayrıntılar: [06-ai-ve-entegrasyonlar.md](06-ai-ve-entegrasyonlar.md)

- **Yerel HTTP API** (sadece localhost, token ile, izin onaylı).
- **Gelen kutusu:** Dışarıdan gelen kartlar önce buraya düşer (ayar ile direkt desteye).
  - `[v1.3]` **Bağlam kaybolmaz:** öğe kabul edilirken yakalanan cümle notun `example` rolündeki alanına (kelime bulunursa kalın), sayfa `source` alanına yazılır (yalnızca o alanlar varsa ve boşsa). Not türü seçilmemiş ve cümle taşıyan öğe, cümleyi tutabilen yerleşik **Altyazı kartı** olur; cümlesiz öğe eskisi gibi Klasik. Satır hangi kart türüne dönüşeceğini söyler; destesi olmayan satır göndermeden önce deste sorar; düzenlenerek kabul edilen öğe diğer alanlarını korur.
- **Tarayıcı eklentisi:** Seç → sağ tık → "Desteye ekle". Bağlam (sayfa adresi, cümle) de kaydedilir.
- **Derin bağlantılar:** `flashcard://deck/<id>`, `flashcard://note/<id>`, `flashcard://study?query=...`, `flashcard://add?front=...&back=...`.
- **Global hızlı ekleme:** Uygulama arka plandayken global kısayolla küçük ekleme penceresi.
- **Pano izleme:** (varsayılan kapalı) Kopyalanan metin için "Kart olarak ekleyeyim mi?" önerisi. Filtrelenebilir (ör. sadece tek kelimeler, sadece İngilizce).

## 23. ⚡ Otomasyon kuralları

`[v0.9]` · 🧱 Olay yolu, sorgu dili, komutlar

- "**Ne zaman** (olay) + **eğer** (sorgu) + **yap** (komut)" kuralları.
- Örnekler:
  - Kart kırmızı bayraklanınca → `#düzelt` etiketi ekle.
  - Kart 8 kez unutulunca → kenara koy ve `#leech` etiketle.
  - Gelen kutusuna İngilizce kelime düşünce → "Kelime kartı" not türüyle İngilizce destesine ekle, AI ile anlamını doldur.
- Kurallar sırayla ve sınırlı derinlikte çalışır (sonsuz döngü engellenir). Her kural çalışması kaydedilir.
- **Hazır tarifler** `[v1.3]`: altı tarif: "Sürekli unuttuğum kartları etiketle" (Tekrar denince, `lapses>=4` → "zor" etiketi), "Bir türlü akılda kalmayanları kenara koy" (`lapses>=8`), "Öğrendiğim kartları arşiv destesine taşı" (`stability>=90`), "Yeni Subtitle kartlarına etiket ekle" (not eklenince), "Kırmızı bayraklı kartları kenara koy", "Kolaylaşan kartlardan 'zor' etiketini kaldır" (günde bir, `tag:"zor" stability>=30`). Tarif yalnızca var olan tetikleyici + sorgu + eylemin önceden doldurulmuş taslağıdır (yeni yapı taşı yok). Her tarif etkilenecek kart sayısını canlı gösterir; kural düzenlenebilir tek bir cümle olarak okunur; ham düzenleyici "Gelişmiş" altında; çalışma geçmişi okunur cümlelerle.

## 24. 📖 Okuma modu

`[v0.10]` · 🧱 Sorgu dili, kelime takibi, AI/çeviri sağlayıcıları

- Metin yapıştırılır, dosya veya web sayfası açılır.
- **Kelime durumu renklendirme (LingQ tarzı):** Bilinen (renksiz), öğreniliyor (sarı), yeni (mavi), yok sayılan (gri). Durum, destedeki kartların FSRS verisinden ve kullanıcının işaretlemelerinden gelir.
- Kelimeye tıklayınca: anlam (sözlük/çeviri/AI), "Kart yap" (cümle bağlamıyla birlikte), "Biliyorum" işaretle.
- Kelime **kökü / ailesi** desteği (run/running/ran): dil bazında lemmatizasyon sağlayıcısı (takılabilir).
- Metindeki bilinen kelime yüzdesi ("Bu metnin %87'sini biliyorsun").
- `[v1.4]` **Ortak kelime dizini:** renkler, yüzde ve Subtitle eklentisinin bilinen kelimeleri aynı kalıcı dizinden gelir; elle, Anki'den, Subtitle'dan ya da okumadan gelen her `term` notu sayılır. Elle işaret kartı yener; "bilinen" eşiği ayardır (`words.known_interval_days`, 21 gün). Destenin dili `words.language` ile verilir; dili bilinmeyen notlar raporlanır. API: `words.lookup` (toplu), `words.list`, `words.stats`, `words.index.rebuild` (docs/03 §4.2).

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
- Yanlış denemede artan bekleme (5 ücretsiz deneme, sonra 1 sn, 2 sn, 4 sn … en çok 5 dk). Sayaç ve bir sonraki deneme zamanı profil başına `profiles.json`'da tutulur: uygulamayı yeniden başlatmak yeni deneme hakkı vermez. Denemenin kontrolü ve kaydı tek kilit altındadır (paralel denemeler ücretsiz hakkı paylaşamaz).
- **Kilitliyken** pencere yalnız kilit ekranına ulaşır; `flashcard://` bağlantıları işlenmez, sınırlı ve tekilleştirilmiş bir kuyrukta bekler, kilit açılınca işlenir; medya (`fcmedia://`) ve hızlı ekleme ön doldurması kapalıdır; yerel HTTP API, otomasyon zamanlayıcısı ve pano izleyici durur.
- **Kapalı kalır (fail closed):** profil kilitli işaretli ama özeti yoksa (ör. anahtar zinciri kaydı silinmiş) uygulama kendiliğinden açılmaz; kilit ekranı açık bir kurtarma akışı gösterir (onay → `lock.recover` kilidi kaldırır, kullanıcı yenisini koyar).
- AI sağlayıcı anahtarları profil başına ayrı anahtar zinciri hesaplarındadır (varsayılan profil eski adları korur: `ai.provider.<id>`; diğerleri `profile.<profil>.ai.provider.<id>`). Kilit özeti `lock.profile.<profil>`; yerel API belirteçleri koleksiyonun içinde (özet olarak) tutulur, yani zaten profile özeldir.

**Tehdit modeli (kullanıcı için).** Uygulama kilidi yalnızca **gelişigüzel erişime** karşı korur: bilgisayarın başına oturan başka biri pencereyi açamasın diye. Şunlara karşı **korumaz**:

- **Veritabanı şifrelenmez** (docs/08 K4): `collection.db` ve medya dosyaları diskte açıktır; dosyaları okuyabilen herkes içeriği görür.
- Aynı işletim sistemi kullanıcısının haklarıyla çalışan herhangi bir süreç anahtar zinciri kaydını silebilir ya da `profiles.json`'u düzenleyebilir (kilidi kaldırmak, deneme sayacını sıfırlamak). Silinmiş özet sessizce açılmaz (yukarıda), ama kilit kaydı tümden silinirse profil kilitsiz açılır.
- **MCP sunucusu (`fc mcp serve`) ve CLI** ayrı süreçlerdir ve kilidi yok sayar; koleksiyona doğrudan erişirler.
- **Linux'ta** anahtar zinciri yeniden başlatmalar arasında saklamadığı için özet `profiles.json`'dadır (tuzlu Argon2id özeti; PIN'in kendisi değil, ama kısa bir PIN çevrimdışı tahminle bulunabilir).

Gerçek koruma için işletim sistemi hesabını parolayla kilitleyin ve disk şifrelemesi (BitLocker, FileVault, LUKS) kullanın.

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
- `[v1.3]` Rehber, ekranı kapatmayan 4 adımlı bir kontrol listesidir: kelime ekle ya da başlangıç destesi seç → çalış → bugünün kartları ve ilerlemenin nerede durduğu → (isteğe bağlı) tarayıcı eklentisi. Her an atlanabilir; Başlangıç desteleri sayfasından yeniden açılır.

## 33. ⚙️ Ayarlar

`[v0.1'den itibaren]` · 🧱 Şemalı ayarlar

- Katmanlar: uygulama → profil → preset → deste → eklenti.
- Her ayar bir şemayla tanımlanır (tip, varsayılan, aralık, açıklama anahtarı, "gelişmiş" mi). Arayüz formları şemadan üretilebilir.
- **Ayar araması.**
- **Modül anahtarları:** Her özellik modülü açılıp kapatılabilir (okuma modu, pano izleme, AI, yerel API...).
- Ayarları dışa/içe aktarma.
- **Kendini anlatan ayarlar** `[v1.3]` (ayar kiti, `features/core/kit`): her satır ne yaptığını gövde boyutunda bir açıklama ve somut bir örnekle söyler; değer varsayılandan farklıysa "varsayılan" işareti ve **Varsayılana dön** (`settings.reset`, geri alınabilir). Her sayfanın "Gelişmiş" bölümü kapalı başlar, açık/kapalı durumu sayfa başına hatırlanır. Dar pencerede satırlar alt alta dizilir.
- `[v1.3]` Ayar araması yalnızca sunumdur (i18n metinlerinden sayfa dizini; eşleşen satırlar vurgulanır, arama sürerken "Gelişmiş" açılır).
- `[v1.3]` Ayarlar beş görev grubundadır: **Genel**, **Çalışma ve kartlar**, **Bağlantılar**, **Veri**, **Gelişmiş** (ayrı ve açıklamalı). Genel sayfası "Neyi değiştirmek istiyorsun?" bağlantılarıyla açılır. Şablonlar, hatırlatıcılar, profiller/kilit, eklentiler (sandbox sınırları görünür), Subtitle ve Yerel API sayfaları ne yaptıklarını ve bir satırı değiştirmenin neyi değiştirdiğini anlatır.

## 34. 🔄 Uygulama bakımı

`[v1.0]`

- **Otomatik güncelleme** (Tauri updater, imzalı). Kullanıcıya sorarak.
- **Çökme kurtarma:** Oturum durumu ve taslaklar kalıcı.
- **Günlükler (log):** Yerel log dosyası, "Hata raporu için logları dışa aktar" butonu (otomatik gönderim yok).
- **Performans hedefi:** 100.000 notta arama < 100 ms, sıradaki kart < 20 ms, açılış < 2 sn.

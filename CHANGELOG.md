# Changelog

Biçim: [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/), sürümleme: [SemVer](https://semver.org/lang/tr/).

## [Unreleased]

## Unreleased (v1.4)

- A3 — ortak kelime dizini: okuma modu, Subtitle `known-words` ve yeni `words.lookup/list/stats/index.rebuild` aynı kalıcı dizini okur (migration `1410_word_index`); çekim birleşir, türetme birleşmez; elle işaret kazanır, "bilinen" eşiği `words.known_interval_days`; deste dili `words.language`, dili bilinmeyen notlar raporlanır (docs/03 §4.2).

## [1.3.0] — v1.3 "kullanılır": akışlar, kelime kartları, gerçek çekirdekte doğrulama

Workspace sürümü `1.3.0`. Proje sahibi v1.2'yi Windows'ta denedi: "Daha güzel ama kullanamıyorum." v1.2 sahte bir backend'e karşı ekran görüntüleriyle doğrulanmıştı; kullanılamaz akışlar böyle geçti. v1.3 yeni özellik için özellik eklemez: var olanı **kullanılır, öngörülebilir ve kendini anlatan** hâle getirir. Tema aynı (docs/11 §4); düzenleyici fikir artık akışlar (docs/11 §1). Hiçbir özellik, ayar ya da kısayol kaldırılmadı; taşınanlar ulaşılabilir. Migration'lar `1300_vocabulary_roles` ve `1301_subtitle_word_front` (v1.3 aralığı 1300–1399).

### Çalışma
- **Tek başlatıcı.** "Çalış" demenin her yolu (Ana sayfa düğmesi/satırları/`Enter`, Kütüphane satırı, deste sayfasının Çalış'ı ve mod düğmeleri, palet, oturum sonundaki "Tekrar çalış", İstatistik'in "Bunları çalış"ı, kart eklemenin "Şimdi çalış"ı) aynı iletişim kutusunu açar: önce yarım kalanlar ("Devam et · 7 cevaplandı · 13 kaldı"), sonra mod (Kartlar / Öğren / Test / Eşleştir, her biri takvime ne yaptığını söyler), sonra **ne çalışayım**, kaç kart, kapalı seçenekler. `Enter` başlatır, `Esc` kapatır. Mod başına kurulum panelleri kalktı. Son mod deste başına, son kapsam mod başına hatırlanır.
- **Beş kapsam, tam sayılarıyla:** Bugünün kartları · Yeni kartlar · Bugün çalıştıklarım · Bugün zorlandıklarım · Tüm kartlar. Son üçü **pratik**: takvimi değiştirmez, kart kümesi başlarken dondurulur. Boş kapsam nedenini söyler ("Bugünlük kart sınırın doldu", "Sıradaki kart 6 dk sonra", "Bugün henüz çalışmadın").
- **Gösterilen sayı = verilen kart.** Ana sayfa, Kütüphane, deste sayfası, başlatıcı ve hatırlatıcılar aynı kuyruk planlayıcısından sayar; "Bugün 12" diyen yer 12 kart verir.
- **Klavye: bir tuş, bir iş.** `Space` kartı her basışta çevirir (ön ⇄ arka) ve **asla puanlamaz**; puan tuşları yalnız arka yüz görüldükten sonra çalışır; `Enter` yalnız odaktaki düğmeye basar; basılı tutulan tuş tek iş yapar; puanlanan kart cevabı kaydedilirken arka yüzünde kalır ve o arada basılan tuş (`Space`, rakam, `H`…) ne ona ne sonraki karta düşer, `Z` kayıttan sonra puanı geri alır (K1); ipuçları ve `aria-keyshortcuts` kısayol düzenleyicisini izler.
- **Ara ver, kaldığın yerden devam et.** `Esc` oturumu bitirmez, duraklatır: o ana kadarki özet + "Devam et"; başlatıcının ilk satırı da aynı oturumu sunar. Tür başına tek açık oturum; Test kâğıdı cevaplarını taslak olarak saklar.
- **Bekleme.** Kalan kartların hepsi öğrenme adımında bekliyorsa oturum kapanmaz: "Sıradaki kart N dk sonra" + geri sayım, Bekle / Başka bir şey çalış / Bitir.
- **"Tüm desteler" kuyruğu her destenin kendi şablonuyla** (X1): destelerin kendi kuyruklarının birleşimi (günlük yeni/tekrar sınırı, sıra, kardeşleri yarına bırakma deste başına; öğrenme kartları vade sırasıyla önde, tekrar ve yeniler desteler arasında sırayla). Bir destede sınır dolunca diğerleri etkilenmez; koleksiyon üst sınırı yalnız istenirse ya da `study.new_per_session` ayarlıysa. Kardeşler **desteler arasında da** yarına bırakılır: bir notun iki destedeki kartlarından bugün yalnız biri gelir (öğrenme kartları hep kalır).
- **Dürüst önden öğrenme** (X1): pencere bir ayar oldu (`study.learn_ahead_mins`, "Öğrenme kartını erken göster", varsayılan 20 dk, 0 = hep bekle); `next_due` kartın gerçekten sunulacağı an, bekleme ekranı yalnız gerçekten beklemen gerektiğinde. Oturum açıkken ayarı küçültmek **ekrandaki kartı almaz** (oturum, kartı seçtiği pencereyi bir sonraki cevaba kadar korur); büyütmek hemen geçerli.
- **Kavram başına tek söz** (X2): **Sırası gelen** (tarih sütunu "Tekrar tarihi") / Due, **Kenara koy** · Kenarda / Set aside (eski "Askıya al", "Durdur"), **Yarına bırak** · Yarına bırakıldı / Leave for tomorrow (eski "Göm"), **Ara ver** / Pause (EN'de tek "Pause"), "Öğreniliyor". CLI'nın oplog özetleri ve deste sayıları da aynı sözlerle. Sorgu dilinde `is:suspended` / `is:buried` çalışmaya devam eder; yeni eşanlamlılar `is:kenarda` / `is:aside` ve `is:yarına` / `is:tomorrow`.
- **Sahibin gözüyle** (X2): Ana sayfa ve Kütüphane deste satırlarında üzerine gelmeden görünen **Çalış**; ara verince odak **Devam et**'te (`Enter` aynı oturum, "Tamam" deste sayfasına ya da Ana sayfaya döner, oturum açık kalır); puanın hemen ardından basılan `Esc` artık kaybolmaz; başlatıcı yalnız açıldığı modun yarım oturumunu odaklar (deste sayfası → Öğren, yarım Test varken Öğren'i başlatır).
- **Test kâğıdı klavyeyle:** `A`–`D` (ya da `1`–`4`) seçip sonrakine geçer, yazılı cevapta `Enter` sonrakine, son cevap "Teslim et"e iner, `Ctrl+Enter` teslim eder; doğru/yanlış sorusu "Önerilen cevap" der.
- **`?` önce "Bu sayfada"**: sayfanın kendi tuşları (Ana sayfanın `Enter`'ı, Kütüphane okları, Oluştur'un `Ctrl+Enter`'ı, Test kâğıdının `A`–`D`'si), sonra genel tablo.
- **Eşleştir kutucukları rollerden:** Altyazı ve Kelime kartlarında kelime (`term`, yoksa `surface`) ↔ şablonun cevap alanı (yoksa `meaning`); cümle asla (eskiden "bring He brought his sister…").

### Kelime kartları
- **Alan rolleri ve "kart neyi soruyor".** Not türü alanı ne içerdiğini söyler (terim, geçtiği hâli, anlam, örnek, örnek çevirisi, kaynak, not); kart şablonu hangi alanı sorduğunu (`answer_field`). Yerleşik türler (Klasik, ters, Kelime, Cümle, Altyazı…) bunları bir data migration ile alır; **senin değiştirdiğin şablonlara dokunulmaz.** Not türü düzenleyicisinde "Bu alan ne içeriyor?" ve "Bu kart neyi soruyor?".
- **Doğru cevap yalnız sorulan alan.** "went" (go → gitmek) kartında yazılı cevap, Test, Öğren ve Eşleştir yalnız "gitmek"i bekler; ters kart terimi.
- **İpucu kuralı: bağlam evet, cevap asla** (X1). Yerleşik Kelime/Altyazı ileri kartının ön yüzü yalnız kelimeyi gösterir (Altyazı: "went", yanında küçük "go"); ilk ipucu kelimesi vurgulanmış örnek cümledir (anlam cümlede geçiyorsa yalnız orası boşaltılır). Ters (üretme) kartta ilk ipucu kelimenin boşaltıldığı cümledir ("I ___ home."). Sonra harfler; cümle zaten soruda görünüyorsa ya da cevabı ele verecekse harflerle başlanır. Migration `1301` mevcut koleksiyonlarda Altyazı ön yüzünü günceller (yalnız şablon düzenlenmemişse).
- **Kart arkası üç etiketli parça:** anlam (en büyük) › örnek (cümle + çevirisi, ayrı ve "Örnek" etiketli) › kaynak (en küçük). Örnek cümle artık cevabın parçası gibi görünmez.
- Deste sayfasının terim listesi terim/tanım sütunlarını rollerden seçer; "Tanım"ı düzenlemek anlamı değiştirir. Hangi alanın ne olduğu bilinmeyen türlerde kalem tam düzenleyiciyi açar.
- İstatistik'in "Nelerde zorlanıyorum?" tablosu kelimeyi ve anlamı rollerden alır.

### Kart ekleme
- **Tek yer:** Oluştur "Nasıl eklemek istersin?" — Tek kart (Hızlı Ekle, `N`) · Toplu · Yapıştır · Dosyadan. Aynı hedef seçici (yeni/var olan deste, not türü), aynı onay: "12 kart eklendi · Geri al · Desteyi aç · Şimdi çalış", atlanan satırlar nedenleriyle.
- **Yapıştır ve Dosyadan tek rehberli akış:** sütunlar not türünün alanlarına rol adlarıyla eşlenir (canlı önizleme), ayırıcı algılanır ve değiştirilebilir, yinelenen kural düz cümleyle; eşleme başlıktan ("sentence", "örnek", "definition"…), yoksa son eşlemeden, yoksa rol sırasından gelir. **Cümle sütunu örnek alanına gider.** Sürükle-bırak.
- İçe aktarma Ayarlar'dan çıktı; Ayarlar › Veri › Dışa aktar ve yedek dışa aktarma, yedek ve yazdırmayı tutar.
- Hızlı Ekle önce terim / anlam / isteğe bağlı örnek; yinelenen kart satır içinde açıklanır; altta aynı destede Toplu / Yapıştır / Dosyadan.
- **Yakalanan cümle kaybolmuyor:** gelen kutusundan kabulde cümle örnek alanına, sayfa kaynak alanına yazılır; cümle taşıyan öğe not türü seçilmemişse "Altyazı kartı" olur. Satır neye dönüşeceğini söyler; destesiz satır göndermeden önce deste ister.
- İlk açılış: 4 adımlı, kapatılabilir rehber (kelime ekle ya da başlangıç destesi, çalış, bugünün kartları ve ilerleme nerede, isteğe bağlı tarayıcı eklentisi).
- **Oluştur bulunduğun desteyle açılır** (X2): bir desteden gelince "Var olan deste" o deste seçili, imleç ilk terimde; "Yeni deste" açık bir seçim (eskiden `Ctrl+Enter` "Yeni desteye bir başlık ver" diyordu).
- Hızlı Ekle, örnek alanı olmayan bir türde cümle yazmak isteyene tek tıkla **Kelime kartı** türünü önerir.

### Ayarlar
- **Ayar öğretir:** her sayfa bir cümleyle açılır; her satırın açıklaması gövde boyunda ve "bunu değiştirirsem ne olur?" sorusunu cevaplar, somut örnek, "varsayılan" işareti ve **Varsayılana dön** (`settings.reset`, geri alınabilir); teknik kısımlar sayfa başına hatırlanan **Gelişmiş** altında; ayarlarda **arama**. Dar pencerede satırlar alt alta.
- Gruplar göreve göre: Genel · Çalışma ve kartlar · Bağlantılar · Veri · ayrı çizgiyle Gelişmiş ("çoğu zaman gerekmez"). Genel sayfası "Ne değiştirmek istiyorsun?" ile başlar.
- **Not türleri** öğrenen görünümüyle: tür kutucukları, alan listesi + çevrilebilir canlı önizleme, hazır başlangıç noktalarından yeni tür, şablon kodu/CSS "Gelişmiş: şablon kodu" altında, kaydetmeden/silmeden önce tam veri kaybı uyarısı.
- **MCP:** "Asistanını bağla" (istemci seç, bu profilin gerçek yollarıyla hazır parça, kopyala, test, salt okunur anahtarı; `mcp.setup`) ve ayrı "Dış MCP sunucuları".
- **Otomasyon:** altı hazır tarif, etkilenen kart sayısı canlı; kural tek düzenlenebilir cümle; ham düzenleyici Gelişmiş'te; okunur çalışma geçmişi.
- Hazır ayarlar, hatırlatıcılar, profiller ve kilit, eklentiler (sandbox sınırları görünür), Subtitle ve yerel API sayfaları ne yaptıklarını anlatır.

### Yapay zekâ
- **Hazır sağlayıcılar:** Claude, OpenAI, **OpenRouter**, Gemini, Groq, Mistral, DeepSeek, Ollama, LM Studio ve özel OpenAI uyumlu adres — dört protokolün üstünde. OpenRouter gibi barındırılan uyumlu adresler artık anahtar ister (v1.2'de anahtar alanı gizleniyordu); bu bilgisayardaki/yerel ağdaki sunucular istemez.
- **Üç adım:** anahtar → model (listeden aranabilir, liste gelmezse elle) → **Test**. Test tek küçük istek atar ve sonucu düz söyler: tamam / anahtar yok / anahtar yanlış / kredi yok / model yok / adres yanlış / hız sınırı / servis çalışmıyor / ulaşılamıyor / TLS; sağlayıcının mesajıyla (anahtar maskeli).
- Sağlayıcı satırları durumlarını ve son testi gösterir; varsayılan sağlayıcı, yönlendirme ve fiyatlar korunur.
- **İki yer, iki iş** (X2): **Oluştur › Yapay zeka** yalnız "Yapay zekayla kart üret" (deste formun bir alanı; sağlayıcı yoksa "Sağlayıcı bağla"); **Ayarlar › Bağlantılar › Yapay zeka** sağlayıcılar + kullanım.
- AI ipucu, kontrol ve açıklama "cevap" olarak kartın **cevap alanını** alır (örnek ve kaynak değil).

### İstatistik ve Planlama
- **İstatistik üç soruyla açılır:** Bugün nasılım? (bugün ve dün, gidişat cümlesi) · Önümde ne var? (7 gün, en yoğun gün yazıyla) · Nelerde zorlanıyorum? ("Bunları çalış · N" başlatıcıyı pratik + bu kartlarla açar). Ayrıntılar altta, her panel tek satırlık anlamıyla.
- **Planlama "Günde kaç yeni kart?" ile başlar:** sınav tarihi ve canlı tahmin (Rust'ta saf FSRS iş yükü modeli), ne değişeceğini söyleyen "Uygula" (tek geri al adımı; destenin hazır ayarı yoksa oluşturup atar). Optimizasyon ve yük dengeleme Gelişmiş'te.
- **Tahminin "bugün"ü = Ana sayfanın bugünü:** 0. gün bugünün gerçek kuyruğu (Ana sayfa ve başlatıcının sayısı); sonraki günler şablonların günlük yeni/tekrar sınırlarına uyar, sınırı aşan tekrarlar ertesi güne kayar; günün "kart"ı kart, dakikası cevap sayar (eskiden "Bugün ~52 kart" derken Ana sayfa 20 diyordu).
- Hedefler boşluklu cümleler; yeni hedef türü "günde N kart".
- **`?`** kısayol yardımı: önce sayfanın kendi tuşları, sonra canlı kısayol tablosu, bağlama göre gruplu, aranabilir.
- Hatırlatıcılar her görmemiş kartı değil, Başla'nın gerçekten vereceği kuyruğu sayar; "bugün çalışıldı" çalışma günü başlangıcına uyar.
- Sorgu dili: `added:N` / `edited:N` / `reviewed:N` kayan 24 saat yerine yerel çalışma günlerini sayar.

### Düzeltmeler
- **Vaat edilen ≠ verilen kart:** 21 yeni kart, sınır 20, 20'si çalışılmışken Ana sayfa "1 kart" diyordu, `study.start` sıfır veriyordu. Artık her sayı aynı kuyruktan; boş başlatma `CONFLICT_NOTHING_TO_STUDY` ile reddedilir, **oturum açmaz ve yarım oturumu kapatmaz** (eskiden yarım oturum boş bir yenisi uğruna kapanıyordu).
- **Yazı alanında `Ctrl+Z` kayıtlı bir kartı siliyordu** (Ara'da genel geri al çalışıyordu), `Ctrl+A` tablo satırlarını seçiyordu. Yazı alanında yerel düzenleme kazanır; açık iletişim kutusunda kabuk kısayolları çekilir.
- **`Esc` oturumu bitiriyordu**, "Devam et" hiç çalışamıyordu. Artık ara verir.
- **`Space` puanlıyordu:** iki basış "Biliyorum" sayıp kartı geçiyordu. Artık yalnız çevirir.
- **Altyazı kartlarında yanlış cevap anahtarı:** beklenen cevap anlam + cümle + çeviri + kaynaktı. Artık yalnız anlam.
- **"Tanım"ı düzenlemek yanlış alanı değiştiriyordu:** Altyazı notunda anlam yerine "geçtiği hâli" (went).
- **OpenRouter için anahtar alanı gizliydi** (yeni uyumlu sağlayıcı "anahtar gerekmez" sayılıyordu).
- Gelen kutusundan ve CSV'den gelen cümle kayboluyordu (Klasik'in ön/arka alanlarına sığmıyordu).
- Modülü kapalı bir çalışma girişi ölü tıklamaydı (yönlendirici eksik rotayı sessizce yutuyordu); Ana sayfa artık ölü bir Başla sunmaz, eksik rota açıklanır.
- Paletteki ham `study.start` komutu oturumu çalıştırıp hiçbir yerde göstermiyordu; artık başlatıcıyı açar.
- Test klavyeyle başlatılınca odak sayfada kalıyordu; kâğıt ilk cevaba odaklanır.
- Başlatıcı yeni kart sınırı dolduğunda "tekrar sınırı doldu" diyordu; artık "Bugünlük kart sınırın doldu".
- Açık bir Select'te harf yazarak seçerken `n` Hızlı Ekle'yi açıyordu.
- Basılı tutulan tuş tek seferlik eylemleri tekrarlıyordu (Ara, terim listesi, tablolar, Öğren, düzenleyiciler, Gelen kutusu, Hızlı Ekle, Oluştur).
- İstatistik'in "unutulanlar" tablosu sabit alan adlarına bakıp kelime türlerinde cümleyi ya da çeviriyi gösteriyordu.
- Yeni ekranlarda ikincil metin okunmuyordu (`--text-3` yalnız yer tutucu/süs).
- `html_to_text`, `&` işaretinden sonra 12 bayt içinde çok baytlı harf gelince çöküyordu (`Tom & ğğğğğğ`).
- Eklenti erişimi: yeni `ai.test` işi AI alanında sınıflanır (not metni taşımaz); `study.options` eklentiler için `decks:read` ister.

### Geliştirici
- **Dev köprüsü:** `fc --db <dosya> dev serve [--port N] [--token T]` (yalnız debug ya da `dev-bridge` özelliği; sürümde yok): loopback HTTP, `POST /rpc` + SSE `/events`, rastgele belirteç, Host/Origin korumaları. Masaüstü IPC ile **aynı dağıtım** (`fc_app::transport::dispatch`). Ön yüzde üçüncü taşıyıcı `vite --mode bridge`.
- **Yolculuklar** (`apps/desktop/e2e/`, `pnpm -C apps/desktop e2e`): Playwright, gerçek çekirdek, gerçek komutlarla tohumlanmış geçici profil (eklentinin HTTP sözleşmesiyle Altyazı yakalamaları dahil); doğrulama **kaydedilen sonuç** üzerinde. J1–J7, S1–S12, O1–O4 (sahibin gözüyle: Oluştur'un destesi, ara ver → Devam et, başlatıcının mod odağı, klavyeyle Test), C1–C4 (rollerle Eşleştir, Planlama'nın bugünü, önden öğrenme değişikliği ekrandaki kartı almaz, desteler arası kardeşler); hepsi geçmek zorunda, `test.fail` yok (S7c yeşile döndü). CI'da Linux `journeys` işi. Mock ekran görüntüleri artık yalnız görsel kontrol (docs/11 §5).
- **Windows kurulumu `fc` CLI'sını da taşır** (`bundle.externalBin`, `pnpm stage-cli` sürüm derlemesi), böylece Ayarlar › MCP onu gösterebilir; kurulumda `fc.exe` yoksa CI kırılır. **Windows CI derlemesi koşana kadar doğrulanmadı.**
- **Windows WebDriver pilotu** (`windows-build.yml` → `webdriver-pilot`, `continue-on-error`): kurulu NSIS uygulamasına `tauri-driver` + uyumlu `msedgedriver` ile üç yolculuk; kod `apps/desktop/e2e-windows/`. İlk CI koşusu oturum açamadı (`DevToolsActivePort file doesn't exist`: yükseltilmiş koşucuda WebView2 150+ `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`'ı yok sayıyor); pilot artık HKLM WebView2 politikasıyla DevTools portunu açıyor, uygulamayı önce tek başına deniyor, oturum stratejilerini sırayla deniyor ve her başarısız denemenin tanısını artefakta yazıyor. Düzeltme Windows'ta henüz koşmadı.
- API (sürümlü, TS tipleri üretildi): `study.options`; `study.start` + `scope`; `SessionDto.next_due`; `modes.test.save_draft`; `HintDto.kind` + `html`; `NoteTerm.term_field` / `definition_field` / `ambiguous`; `ColumnTarget::Role`; `decks.*` `today_*` alanları; `ai.providers.list` → `presets`, `ai.providers.test` (`ai.test` işi), `ai.models` işinin sonucu tipli (`AiModelList`); `mcp.setup`; `stats.today`, `stats.struggles`; `planning.forecast`, `planning.apply_pace`; hedef türü `daily_cards`; hata kodu `CONFLICT_NOTHING_TO_STUDY`.
- Çekirdek: `FieldDef.role`, `CardTemplate.answer_field` (isteğe bağlı JSON, eski kayıtlar değişmeden yüklenir); `fc-study::hint` (`context_hint`, `leveled_hint`), `fc-study::planning::forecast`; ortak kuyruk planlayıcısı `study/plan.rs`. **Migration 1300** yalnız veri: değiştirilmemiş yerleşik not türlerine rol/cevap alanı/yeni kart arkası, idempotent, v1.2'nin her yerleşiğinden fikstürle testli (iki dil). **Migration 1301** yalnız veri: gönderildiği gibi kalmış yerleşik Altyazı ön yüzü yalnız kelime (şablon süzgeci `unless_same`). Kuyruk: `Planner::collection_roots` / `plan_collection`; oturum `SessionState.learn_ahead_ms` + `Session::shown_window_ms`; `support::content::term_and_definition`.
- `screens.mjs`: modlar başlatıcıdan başlar, `?mock=waiting`, `state-cards-paused`, yakalanamayan durum `state-missing` bulgusu.
- `$lib/keyboard` (`isEditableTarget`): neyin yazı alanı olduğuna tek yer karar verir.

## [1.2.0] — v1.2 arayüz yeniden tasarımı + Subtitle bağlantısı

Workspace sürümü `1.2.0`. v1.1 arayüzü (ferah, büyük yuvarlak kartlar, açık gri + mor, ağır kart ekleme formu) proje sahibince rahatsız edici bulunup reddedildi; arayüz akışlardan başlayarak yeniden tasarlandı (docs/11). **Hiçbir özellik kaldırılmadı**, Rust davranışı ve API değişmedi.

### Tasarım sistemi ve kabuk
- Token'lar: koyu öncelikli, sıcak nötrler, **tek vurgu kehribar** `#F5B544` (açık temada metin olarak `#9A5B00`); doğru/yanlış/uyarı yalnız anlam taşıdığında. Gövde 13 px, kontroller 28/32/36, satırlar 34, köşe 6/8 (12 yalnız çalışma kartı), gölge yerine 1 px kenarlık, 100–150 ms hareket, 2 px odak halkası. Varsayılan hazır tema (`fc_core::theme`) ve uygulama simgesi yeni tasarıma geçti.
- **JetBrains Mono** (OFL) paketlendi: sayılar, sayımlar, süreler, kısayol ipuçları, tablo sayıları.
- `$lib/ui` sık sisteme uyarlandı; yeni **ListRow** (34 px liste/ağaç-tablo satırı), **StatTile** (büyük mono sayı), Button `kbd` ipucu.
- Kabuk: 216 px kenar çubuğu (Oluştur artık vurgulu blok değil), oturum dışında her zaman görünen **44 px üst çubuk**: iz, "Ara ya da komut çalıştır" (`Ctrl+K`), birincil **"Kart ekle"** (`N`). Giriş noktaları `shell/commands.svelte.ts` (`openQuickAdd`, `openPalette`). Palet önce Eylemler (Kart ekle, Ara, Geri al, Yinele).

### Akışlar ve ekranlar
- **Hızlı Ekle** penceresi: `N` / `Ctrl+N` / üst çubuk; deste ve not türü hatırlanır, `Ctrl+Enter` ekler ve pencere açık kalır, alanlar temizlenir, "Eklendi · n" + Geri al.
- **Oluştur** tablo benzeri ızgara: tek kurulum satırı ("Diğer" altında açıklama/sınıflandırma), numaralı terim | tanım satırları; kaydet yine tek `notes.create_many` geri al adımı.
- **Ana sayfa** "Bugün · N kart" + mono kırılım + `Enter` ile başla, işi olan desteler sık tablo, kompakt seri/hedef şeridi, tek satır gelen kutusu.
- **Çalışma oturumu pencerenin tamamını alır:** kenar çubuğu ve üst çubuk gizlenir, `SessionFrame` ince üst satırı (ilerleme, mono "12 / 40", kalan, `Esc`); kart en çok 720 px, cevap düğmeleri kart genişliğinde (2'li ve 4'lü). Öğren, Test, Eşleştir, Okuma aynı dilde.
- **Kütüphane** ağaç-tablo varsayılan (ızgara ikincil), **deste sayfası** "Çalış" ve mod düğmeleri ilk ekranda, terim listesi satır içi düzenlemeli tablo. **Ara** sorgu öncelikli, sık sanal tablo, yan panelde kart bilgisi, klavyeyle seçim.
- **İstatistik** kompakt mono kutucuklar, 12 px ısı haritası (klavye + ipucu), mono eksenli grafikler; Planlama, Hedefler, Gelen kutusu (J/K/A/E/R/X), not düzenleyici, not türleri, eklentiler, AI ile üret, başlangıç desteleri, ilk açılış ve kilit ekranı da yeni sistemde. Ayarlar kutusuz kit (`features/core/kit`: 44 px satırlar, yıkıcı bölüm en sonda).

### Subtitle tarayıcı eklentisi bağlantısı
- Yeni `subtitle` modülü (varsayılan kapalı): yerel HTTP sunucusunda `/subtitle/v1` altında eklentinin "Subtitle Kartları API v1" sözleşmesi (me, decks, media, captures, captures:batch, known-words). Kişisel erişim anahtarları (bir kez gösterilir, yalnız SHA-256), migration `1200_subtitle`, kendi izni olmayan `Actor::Subtitle`; yakalama doğrudan destede not olur, tek geri al adımı. Ayarlar → Bağlantılar → Subtitle. Ayrıntı: docs/06 §5.1, kullanıcı rehberi.

### Son cila
- **Subtitle ayarları sayfası** diğer bağlantı sayfalarıyla aynı düzende: mono sunucu adresi + Kopyala, kısa numaralı kurulum adımları, anahtarlar sık satırlar (ad, mono önek, oluşturulma / son kullanım, kırmızı tonlu İptal et), yeni anahtar bir kez mono gösterilir ve "bir daha göremezsin" notu taşır; deste seçici, gelen kelime sayısı ve son zamanı; yükleniyor / hata / modül kapalı (buradan açılabilir) durumları.
- Pasif düğmeler okunur: etiket ortada, kısayol ipucu soluk ama okunaklı (kilit ekranı "Kilidi aç", Oluştur).
- Anahtar (Switch) kapalıyken her iki temada sakin nötr; açıkken vurgu rengi.
- Ara tablosunda seçili satırlar, fare üstündeyken ya da klavye imleci üzerindeyken de seçili görünür.
- Deste seçici, seçimsiz durumun anlamını söyler: çalışma ekranlarında ve Dışa aktar'da "Tüm desteler", diğerlerinde "Deste seç".
- Deste sayfası: ⋯ menüsü başlık satırıyla hizalı; alt destede üst çubuktaki iz üst desteleri de gösterir (tıklanabilir). Başlığın altında açıklama ya da çip olan her sayfada eylemler başlık satırına hizalanır.
- Not türleri sayfası diğer ayar sayfalarıyla aynı genişlikte.
- Türkçe metinlerde kalan resmî hitaplar ("tıklayın", "basın", "ekleyin"…) "sen" diline çevrildi.
- Gelen kutusunda destesiz bir öneri kabul edilince belirsiz "geçerli değil" yerine o satırın deste seçicisi "Önce bir deste seç." der ve odaklanır.

### Araçlar
- `pnpm screens` her koşuda rastgele bir port seçer (`--port N` ile sabitlenir): farklı worktree'lerdeki paralel koşular aynı geliştirme sunucusunu paylaşmaz. Çalışan bir oturumdan başka ekrana geçmeden önce çıkar.

## [1.1.0] — v1.1 arayüz tasarımı (`feat/v1.1-design`)

Workspace sürümü `1.1.0`. Arayüz baştan tasarlandı (docs/11); davranış ve iş mantığı Rust'ta kaldı.

### Tasarım sistemi ve kabuk
- `$lib/ui`: Button, IconButton, Input, Textarea, Select (özel liste kutusu), Checkbox, Switch, SegmentedControl, RadioGroup, Chip/Badge, Card, Tile, Dialog/Sheet, Menu, Tabs, Tooltip, Toast, ProgressBar/Ring, EmptyState, Skeleton, sanal Table, Kbd, Avatar, Sidebar/NavItem, PageHeader, çevrilebilir Flashcard; token'lar (`theme/tokens.css`), paketlenmiş Inter ve Lucide simgeleri.
- Kenar çubuğu 5 bölüm (Ana sayfa, Kütüphane, Oluştur, Ara, İstatistik) + Gelen kutusu + Ayarlar + profil; modül → bölüm/grup eşlemesi tek yerde (`navigation.ts`). Ayarlar gruplu alt menü; yeni **Okuma ve modlar** grubu (`study.mode_settings` → şema güdümlü sayfa: cevap düğmeleri, güven ölçeği, süreli/odak mod, öğren/test uzunluğu, okuma varsayılanları).
- Kütüphane sekmeleri yalnız **Desteler | Notlar**; çalışma modları deste sayfasının mod döşemelerinden (Eşleştir dahil) ve Ana sayfadaki "Çalışmaya başla"dan açılır.
- **Odak düzeni:** Kartlar / Öğren / Test / Eşleştir'de oturum sürerken deste seçici ve sekmeler gizlenir, kart ve cevap düğmeleri ortalanır, kısayol ipuçları 1280 ve 900 genişlikte görünür (`ScreenCtx.setFocus`).
- Ham `<select>`/checkbox/radio kalmadı: lint testi istisnasız (izin listesi silindi).

### Ekranlar
- **Ana sayfa** (bugün, çalışmaya başla, seri, hedef, gelen kutusu, son desteler), **Kütüphane** ve **deste sayfası** (önizleme kartı, mod döşemeleri, ilerleme, terim listesi) — Rust: `decks.summaries`, `notes.terms`.
- **Çalışma modları** yeniden: Kartlar (varsayılan 2 düğme, ayarla 4; `study.answer_buttons`), Öğren, Test, süreli/güven/odak; yeni **Eşleştir** modu (Rust `modes.match.*`, pratik modu, en iyi süre).
- **Oluştur** (satır satır terim–tanım, yapıştırarak içe aktarma, tek geri al adımı — `notes.create_many`), **Ara** (sorgu çubuğu, sanal tablo, toplu işlemler), not düzenleyici, **Gelen kutusu**, **Okuma**.
- **İstatistik**, Planlama, Hedefler; Ayarlar sayfaları (Genel, Çalışma, Bağlantılar, Eklentiler, Veri, Profiller ve kilit), ilk açılış, kilit ekranı, kurtarma/güncelleme pencereleri.
- AI ile üret, Hızlı ekle penceresi, dış istekler (eşleştirme onayı, pano önerisi), Notlar listesi de tasarım sistemine geçti.
- Yapıştırarak içe aktarmada ayırıcı denetimi Rust'ın algıladığı ayırıcıyı gösterir (yeni "Boşluklar" seçeneği = hizalı sütunlar); geliştirme mock'u da artık algılar.

### Performans, erişilebilirlik, görsel denetim
- İlk JS **160,6 kB** (bütçe 320 kB): yalnız etkin dilin sözlüğü yüklenir; ilk yüklenen kod `$lib/ui`'yi dosya dosya alır (lint: `tests/eager-imports.test.ts`).
- Başarı rengi açık temada metin rengine doğru koyulaştırıldı (çip/rozet üzerinde 4,38 → 5,3:1); durum renkleri kendi yumuşak zeminlerinde de AA (testli).
- `pnpm -C apps/desktop screens`: TR/EN × açık/koyu × 1280/900 ekran görüntüleri, deste sayfasından her mod (boş, çalışırken, çevrilmiş); `text-clipped` `.sr-only` öğeleri yok sayar.

## [1.0.0] — v1.0 Cila ve Windows sürümü (`feat/v1.0-integration`)

Workspace sürümü `1.0.0`. Elle yapılacak son adım: temiz bir Windows'ta kurulum → ilk açılış → çalışma → güncelleme betiği (docs/07 §8), henüz gerçek bir Windows'ta koşulmadı.

### v1.0 entegrasyonu — tur 2 (+ iz P performans/i18n/temalar)
- **Profiller:** oturum kuyruğu (`session_queue`), istatistik önbellekleri ve tema ayarları profilin kendi koleksiyonunda; profil geçişi bellekteki çalışma kuyruğu önbelleğini boşaltır.
- **Kilit:** kilit ekranı ve ilk açılış profilin temasıyla çizilir (`themes.current` kilitliyken okunabilen tek ayar: renkler, şema, hazır tema kimliği); kilitliyken önbellekteki tema silinmez, açılınca yeniden okunur. Kilitliyken kabuk hiçbir ekranı (tembel parçalar dahil) bağlamaz.
- **Çökme kurtarma × artımlı kuyruk:** gerçek bir süreç oturum ortasında öldürülür (`abort`), sonraki açılış çalıştırma işaretinden kirli kapanışı görür, oturumu sunar ve öldürülen sürecin son kaydettiği kuyrukla birebir devam eder (`maintenance::kill_tests`).
- Tur 1 ekranları (Profiller, Başlangıç desteleri, Hatırlatıcılar, Günlükler ve güncellemeler) tembel yüklenir; i18n lint tur 1'in tüm ekranlarında (ilk açılış, profiller, kilit, hatırlatıcılar, günlükler, kurtarma penceresi) geçer.
- Eklentiler: `themes.current`/`themes.presets` = ayar okuması, `themes.set` açıkça reddedilir (`domain_themes`); eklentilerin tema katkı noktası yok. Migration `1070`–`1072` iz P'nin `1070`–`1089` aralığında (`V1_LANE_RANGES`). docs/10 §37.

### v1.0 entegrasyonu — tur 1 (iz Q + iz O + iz N)
- Üç v1.0 izi birleşti. **Loglar, çökme raporları ve çalıştırma işareti profil başına** (etkin profilin klasöründe); profil geçişi eski çalıştırmayı temiz kapatır.
- **Profil silme anahtar zincirini temizler:** profilin AI anahtarları ve kilit özeti silinir. `fc --db <profil>/collection.db` o profilin AI anahtarlarını kullanır.
- **Log dışa aktarma** profile ait anahtarları ve kilit özetini (`$argon2id$…`) de maskeler.
- **Kilit ekranı** kilitliyken gelen `flashcard://` bağlantılarını çalışmadan önce listeler; "Açılmasın" ile bırakılabilir.
- **Gelen kutusu:** program başına günlük öneri sınırı (`local_api.inbox_daily_max`, 200); önerileri incelemeden kabul eden kurallar Otomasyon ekranında uyarıyla gösterilir.
- Eklentiler `profiles.*`, `lock.*`, `maintenance.*`, `updates.*`'e ulaşamaz ve kanca takamaz (alan başına test). Migration alt aralıkları testle zorunlu. Kullanıcı rehberi yeni özelliklerle güncellendi. docs/10 §35.

### v1.0 iz Q — içe aktarma kalitesi, gerçek Anki, kullanıcı rehberi (`feat/v1.0-quality`)
- **Ayırıcı algılama:** `~`, ` / `, `==` ve hizalı sütunlar (boşluk/çift sekme) tanınır; cümle listeleri, tireli kelimeler, cloze satırları ve düzyazı paragraflar tek sütun kalır; `;` virgülü, `:` `=`'i geçer; tanımda tekrar eden ayırıcı tanımda kalır. Ayardan önce kör yazılmış yeni tutulmuş sette **%96,2 (100/104)**; önceki tutulmuş sette ayar öncesi %90,8 (99/109). docs/10 §34.
- **Gerçek Anki doğrulaması:** resmî `anki` 26.09.4 ile üretilmiş küçük paketler (yeni/eski düzen `.apkg`, `.colpkg`, zamanlamasız) test fikstürü; bizim `.apkg`/`.colpkg`'miz gerçek Anki'de açılıp karşılaştırılır (CI). Düzeltmeler: Anki 2.1.50+ WAL paketleri ve `unicase` harmanlaması okunur; resim maskesi Anki'nin kendi not türüyle dışa aktarılır; "Default" destesi çoğalmaz; aralık ve `lrt` doğru yazılır; resim/ses alanı medyası pakete girer.
- **Kullanıcı rehberi** (TR/EN): `docs/guide/`.

### v1.0 Windows sürümü — iz O (`feat/v1.0-release`: kurulum, imzalı güncelleme, çökme kurtarma, loglar)
- **Kurulum:** NSIS (kullanıcı başına, yönetici izni yok, TR/EN) + isteğe bağlı MSI; WebView2 önyükleyici; sürüm workspace'ten. Kod imzası yok (K8, SmartScreen notu docs/07 §8).
- **İmzalı otomatik güncelleme:** `tauri-plugin-updater`, Ed25519 açık anahtarı `tauri.conf.json`'da (özel anahtar repoda değil, GitHub sırrı). Yeni `updates` modülü: `updates.status/check/install/apply`; denetim yalnız istenince (`updates.check_on_start` varsayılan kapalı), indirme iptal edilebilir iş, imzası tutmayan indirme reddedilir, kurulumdan önce çalıştırma temiz kapanır.
- **Çökme kurtarma:** çalıştırma işareti (`run.json`), panik kancası yerel çökme raporu yazar (gönderim yok, maskeli); düzgün kapanmayan çalıştırmadan sonra açılışta SQLite hızlı bütünlük denetimi ve kurtarma penceresi (açık oturuma devam, form taslakları, rapor). Yeni `maintenance.*` metotları ve form taslakları (migration `1050_maintenance`, not ekleme formu kendiliğinden kaydeder).
- **Loglar:** `logs/flashcard.log`, boyutla döner (ayarlanabilir), her satır yazılmadan maskelenir; "Günlükleri dışa aktar" zip'i uygulamanın tuttuğu tam anahtarlarla bir kez daha maskeler. Yeni ekran *Günlükler ve güncellemeler*.
- **CI/yayın:** `windows-smoke` (sessiz NSIS kurulumu + `Flashcard.exe --smoke`), `release.yml` (etiket `v*` → taslak Release, imzalı `latest.json`). Elle test betiği docs/07 §8.5.
- Eklentiler `maintenance`/`updates` alanlarına erişemez; MCP, otomasyon ve yerel API de erişemez (testli).
### v1.0 iz N — ilk açılış, başlangıç desteleri, profiller, uygulama kilidi, hatırlatıcılar (`feat/v1.0-ux`)
- **İlk açılış rehberi:** boş koleksiyonda "ne öğrenmek istiyorsun?" → konuya uygun sınıflandırma şablonu (CEFR, JLPT, HSK, YKS, sınıf, zorluk), not türü, ön ayar ve başlangıç desteleri; hepsi tek geri al adımı, atlanabilir. `onboarding.status/options/apply/skip`.
- **Başlangıç desteleri:** bu uygulama için yazılmış küçük TR/EN desteler (günlük İngilizce, TYT matematik, dünya başkentleri, uygulama rehberi), notlar sınıflandırma değeriyle. `starter.list/install`.
- **Profiller:** her profil ayrı koleksiyon (`profiles/<id>/collection.db` + medya); varsayılan profil eski `collection.db`'de kalır. Profil listesi `profiles.json`'da, koleksiyonların dışında. Geçişte uygulama yeni koleksiyonla yeniden başlar. `profiles.*`.
- **Uygulama kilidi:** PIN/parola yalnız Argon2id özeti olarak (OS anahtar zinciri), hareketsizlikte otomatik kilit, yanlış denemede artan bekleme; kilitliyken pencere yalnız kilit ekranına erişir, **yerel HTTP API 423 döner ve durur, otomasyon zamanlayıcısı ve eklentiler çalışmaz**. Pencere kilidi, veritabanı şifrelemesi değil. `lock.*`, `lock_changed`.
- **Kilit güvenlik incelemesi:** kilitliyken `flashcard://` bağlantıları kuyrukta bekler (sınırlı, tekil) ve kilit açılınca işlenir, `take_deep_links`/medya protokolü/hızlı ekleme ön doldurması kapalı; yanlış deneme sayacı `profiles.json`'da (yeniden başlatma sıfırlamaz), kontrol + kayıt tek kilitte; özeti kayıp kilit kapalı kalır, yeni `lock.recover` + `LockState.recovery_required` ile açık kurtarma; AI anahtarları profil başına anahtar zinciri hesaplarında (varsayılan profil eski adları korur). Tehdit modeli docs/02 §29.
- **Hatırlatıcılar ve tepsi:** seçilen saat/günlerde günlük OS bildirimi (o gün çalışıldıysa yok), sistem tepsisi menüsü, "pencere kapanınca tepside kal". `reminders.status/test`, `reminder_due`.
- Migration `1000_ux_state` (+ migration testi). Paylaşılan değişiklikler: `fc-http` kilit kontrolü, `outside.rs` sunucu/pano kilidi, `App::invoke` arka plan aktörü kontrolü, otomasyon `run_due`.

### v1.0 iz P — performans, i18n tamlığı + lint, temalar (`feat/v1.0-perf`)
- **Performans takımı** (`fc-app/src/perf_tests.rs`): 100k not / 200k kart / 2M tekrar kaydı (sınıflandırmalar, etiketler, her kart durumu) SQLite dosyasında kurulur, ayrı bir süreç masaüstü gibi açıp ölçer. docs/02 §34 hedefleri release'te karşılandı (8 çekirdek, %10 öğrenme kartı — kuyrukta ~19 300 kart): açılış 626 ms (< 2 sn), sıradaki kart p95 7,2 ms (< 20 ms; medyan 4,1 ms), oturum başlatma 156 ms (< 500 ms), 100k'da en yavaş arama 70 ms (< 100 ms), tarayıcıda herhangi bir sayfa 81 ms, 10 bin satır içe aktarma 1,2 sn, 2M tekrarda önbellekli özet 643 ms / sınıflandırma kırılımı 1,06 sn (< 1,5 sn) / deste kapsamı 2,6 sn, tepe bellek 301 MB; okuma modunda 10 bin kelime renklendirme 16,6 ms (< 50 ms). 1/10 ölçekli eşi her `cargo test`'te koşar. Sayılar ve ölçüm koşulları docs/10 §36.
- **Düzeltilen darboğazlar:** kardeş gömme ve çalışma adayı okumaları dizini kullanmıyordu (`?n IS NULL OR …` → yalnız verilen koşullar); yük dengeleyici her cevapta binlerce vade zamanını okuyordu (gün başına sayım SQL'de, kapsayan dizin); kuyruktaki her kart için ayrı okuma (tek sorgu); her aramada tüm etiketlerin kullanım sayımı (sayımsız liste); sınıflandırma kırılımı ve deste kapsamlı istatistik 2M satırı belleğe alıyordu (akış halinde, yalın satır); enum sütunları satır başına JSON değeri üretiyordu. Tarayıcı: vade sıralaması dizinli, orta noktadan sonraki sayfalar dizini tersten yürür, sıralama yürüyüşü not satırlarını okumaz (migration **1070**).
- **Kuyruktan bağımsız cevap** (migration **1071**): oturum kuyruğu `session_queue` satırları; cevap yalnız değişen satırları ve oturum kaydını bir kez yazar, son kuyruk bellekte (revizyonla doğrulanır), geri alma anlık görüntüsü tam kuyruk yerine fark (geri al yine birebir); ölü/boş kartlar yalnız sıradaki adaylarda denetlenir. `study.start` vade dizini ve `cards_new_order` ile yalnız kuyruğun alabileceğini okur (2,1 sn → 156 ms). **Sınıflandırma kırılımı önbelleği** (migration **1072**): değer başına gün sayımları, kart kümelerinin parmak iziyle doğrulanır (2,97 → 1,06 sn). i18n lint artık yalnız `#[cfg(test)]` öğesini atlar (sonrasını tarar); `abort_flag_interrupts_training` deterministik.
- **i18n lint:** masaüstünde Svelte AST taraması (işaretleme, görünen öznitelik/prop, betik cümleleri; her anahtar TR+EN), tarayıcı eklentisinde sayfa/manifest/betik taraması, backend'de modüllerin ilan ettiği ve kodun gönderdiği her `*_key`'in TR+EN metni. CLI hataları `message_key` çevirisiyle yazar. Bulunan eksik ayar etiketleri eklendi.
- **Temalar** (docs/02 §31): açık/koyu/sistem, sakin hazır temalar (varsayılan, kâğıt, adaçayı, arduvaz, yüksek karşıtlık) ve özel vurgu rengi; renkler `fc_core::theme`'de hesaplanır (WCAG karşıtlığı korunur), `themes` modülü kapatılabilir, ayar tek geri al adımı.
- **Ön yüz:** çalışma ekranı dışındaki her özellik ekranı ilk açılışta yüklenir (ayrı parça); `pnpm bundle:check` ilk yük bütçesini CI'da denetler.

## [0.10.0] — Faz 4 (v0.9 + v0.10 birleşik, `feat/phase4-integration`)

### Faz 4 entegrasyonu — tur 2, son (+ v0.9 otomasyon, yerel API/gelen kutusu, tarayıcı eklentisi)
- Üç v0.9 izi v0.10 modlar/okuma, v0.7 eklentiler ve v0.8 AI/MCP ile birleşti. Workspace sürümü **0.10.0** (masaüstü, Tauri, tarayıcı eklentisi aynı).
- **İzinler tek tabloda:** `Registry::permits`'te otomasyon (kural eylemleri) ve yerel API (onaylı kapsamlar) kolları yan yana; ikisi de kullanıcıya özel izinleri (`settings:exec`, `mcp:write`) asla tutmaz. Yeni test: her aktör × her hassas alan (eklentiler, MCP sunucuları, AI anahtarları/yazmaları, içe/dışa aktarma, `settings:exec`, geçmiş, kural yönetimi, yerel API istemcileri, okuma kesitleri, mod iç metotları, gelen kutusu). Kuralları yalnız kullanıcı/CLI/sistem yönetir; yerel API kural oluşturamaz, çalıştıramaz.
- **Gelen kutusu → otomasyon:** yeni tetikleyici "gelen kutusuna öneri düşünce" ve eylem "not olarak kabul et" (`inbox.accept`, isteğe bağlı deste). Koşul, önerinin olacağı kart üzerinde aynı sorgu diliyle değerlendirilir — sınıflandırmalar destesine göre çözülür (`deck:İngilizce seviye:B1`). Öneri, kabulü ve kabulün tetiklediği kurallar **tek geri al adımı**. Gelen kutusu kuralları yalnız gelen kutusu eylemlerini, diğerleri yalnız kart eylemlerini kullanabilir.
- **Mod cevapları kuralları tetikler:** öğren, güven ve zamanlı ("Tekrar") cevapları da "kart cevaplanınca" kurallarını çalıştırır.
- **Eklentiler:** `automation.*` ve `local_api.*` açıkça reddedilir ve kancalanamaz; gelen kutusu/otomasyon/yerel API olayları süzme katmanında sınıflandı (alan başına test). Eklenti v1 yüzeyi yalnız büyüdü (`inbox.add/list/count/reject`, `clipboard.check`).
- **Tarayıcı eklentisi gerçek sözleşmeyle:** elle yazılmış DTO dosyası silindi (tipler üretilmiş `@fc/api`'den), eşleştirme yoklaması `POST`, her istekte `X-Flashcard-Client` ve POST'ta JSON; `real-app.spec.ts` gerçek `fc api serve`'e karşı geçiyor ve CI'da `FC_BIN` ile koşuyor.
- **Geçmiş:** gelen kutusu ve kural yazımları sıradan, replay edilebilir geri al adımları; çöp temizliği bariyerine uyar. docs/10 §33.

### Faz 4 entegrasyonu — tur 1 (v0.10 modlar + v0.10 okuma)
- İki v0.10 izi v0.7 eklentileriyle birleşti: yeni metotlar eklenti süzme katmanında sınıflandı (öğren cevabı, test teslimi, satır içi kart ve okumadan kart **kâhin**: okuma izni olmadan çalışmaz), okuma olayları okuma izni olmadan gelmez, eklenti API'si (`api_v1.json`) yalnız büyüdü.
- **Okuma güvenliği:** eklenti dosya/web içe aktaramaz, ffmpeg kesiti başlatamaz, bunlara kanca takamaz; altyazının video yolu eklentiye hiç görünmez ve eklenti onu ayarlayamaz. Genel kural: eklentinin çağıramadığı metoda kanca takılamaz. ffmpeg yolu (`settings:exec`) yalnız kullanıcı/CLI.
- Okuma ekranında satır içi kart oluşturucu ("Yeni kartlar"). docs/10 §29.

### v0.10 Öğrenme modları — iz M (`feat/v0.10-modes`: öğren, test, güven, zamanlı, odak, satır içi kart)
- **Hepsi v0.2 oturum motorunun parçası:** öğren ve test yeni oturum **türleri** (`study.start { kind: learn | test }`), zamanlı ve güven bir oturumun **seçenekleri** (`study.start { modes: { timed, rating_scale } }`). Cevaplar motorun ortak yolundan (`study::ext::answer_core` + `AnswerHooks`) geçer: zamanlayıcı, planlama, `review_log`, tek geri al adımı (`study.restore`). `SessionDto.modes` ekranın ihtiyacını taşır (öğren aşaması/seçenekleri, sayaç, güven düğmeleri, test durumu). Eklemeli v1 değişiklikleri.
- **Öğren modu (Quizlet tarzı):** her kart önce çoktan seçmeli, sonra yazarak sorulur; backend cevabı yargılar (doğru → İyi, az kaldı → Zor, yanlış → Tekrar), yanlış yazılı cevap çoktan seçmeliye döner; kart iki aşamada da doğru bilinince **öğrenildi**. Tanımak hatırlamak değildir: tekrar durumundaki kartta çoktan seçmeli cevap yalnız pratiktir (`trial_learn`, takvim değişmez), böyle kartı yazılı aşama puanlar. Sınıflandırma hedeflenebilir (`seviye:A2`). `modes.learn.answer`.
- **Test oluşturucu:** karışık soru tipli (çoktan seçmeli, yazılı, doğru/yanlış) kâğıt oturum başında sabitlenir, yazdırılır (cevap anahtarıyla ya da anahtarsız), teslimde puanlanır (az kaldı = ½ puan); takvimi asla değiştirmez, soru başına `review_log.mode = test`; teslim geri alınabilir. `sınav:TYT` gibi sınıflandırmayla deneme testi. `modes.test.paper/submit`.
- **Güven bazlı puanlama:** 1–5 "ne kadar emindim?"; eşleme tablosu ayarlardan (`confidence.map.1…5`, varsayılan 1–2 Tekrar, 3 Zor, 4 İyi, 5 Kolay), ölçek deste/şablon/uygulama ayarıyla seçilir (`confidence.rating_scale`). Her düğme kendi puanının aralığını gösterir. `modes.confidence.answer`.
- **Zamanlı mod:** kart başına süre (3–600 sn) **backend saatiyle**; geç gelen düğme cevabı `CONFLICT_TIME_UP`, erken bitirme `CONFLICT_TIMER_RUNNING`; süre dolunca "Tekrar" (`timed`) ya da kart geçilir (tekrar kaydı yok). Sayaç **kart bazında** ve kart ekrana gelince başlar (`modes.timed.start`); hiç yeniden başlatılamaz, cevapla biter, geri almada sıfırlanır — askıya alma/gömme sonrası ya da vadesi gelen öğrenme kartı görülmeden "Tekrar" almaz. `modes.timed.start/expire`.
- **Odak modu:** tam ekran, menü/deste seçici/sayaçlar gizli (F); ayarlar `focus.fullscreen`, `focus.hide_counts`, `focus.auto`.
- **Satır içi kart oluşturma (RemNote tarzı):** `soru :: cevap` satırları kart olur (cloze `{{…}}`, `[[bağlantı]]`, kod içi `::` ayırıcı sayılmaz; `\::` kaçış). Çalışma ekranında N ile, oturumu bozmadan; önizleme yinelenenleri gösterir, oluşturma tek geri al adımı. `modes.inline.preview/create`; bileşen okuma ekranına da takılabilir.
- **`review_log.mode` modları ayırır:** takvimi değiştirenler `normal`, `learn`, `timed`, `confidence`; pratik `trial`, `trial_learn`, `trial_timed`, `trial_confidence`, `test`. İstatistik ve FSRS optimizasyonu pratik satırları saymaz (`fc_storage::repo::is_practice_mode`); günlük limitler takvimi değiştiren **her** modu sayar, elle seçilen oturumlar (özel, yanlışlar, öğren, test) limitsizdir. Altı modül (`learn`, `practice_test`, `confidence`, `timed`, `focus`, `inline_cards`) ayarlardan kapatılabilir.
- **Testler:** `fc-app/src/modules/modes/tests.rs` (9 senaryo × 3 depo), `io/tests.rs::imported_history_keeps_practice_modes`, `fc-study` modes (öğren/test/güven/zaman), `fc-core` inline_cards, masaüstü `features/modes/modes.test.ts`. docs/10 §27.

### v0.10 Okuma modu, altyazı ve artımlı okuma — okuma izi (`feat/v0.10-reading`)
- **Okuma modu (docs/02 §24):** metin yapıştır, dosya (`.txt`/`.html`/`.pdf`, her kodlama) ya da web sayfası aç; kelimeler LingQ tarzı renklenir — **yeni** (mavi), **öğreniliyor** (sarı), **biliniyor**, **yok sayıldı** (gri). Durum destedeki kartların FSRS verisinden gelir: olgun kart (aralık ≥ 21 gün, istatistikle aynı tanım) = biliniyor; kartı olmayan kelimede "Biliyorum" / "Yok say" işaretleri (geri alınabilir, toplu "tüm yeni kelimeleri bildim"). "Bu metnin %87'sini biliyorsun".
- **Lemmatizasyon:** `run/running/ran` ve `kalem/kalemlerimizden` aynı kelime; yerleşik İngilizce ve Türkçe kural tabanlı lemmatizer'lar, diğer diller küçük harf (takılabilir `Lemmatizer` arayüzü). Kartın kelimesi `word` alanından (yoksa sıralama alanından) okunur; sayılan kartlar ayarla daraltılabilir.
- **Kart yap:** tıklanan kelimeden **Kelime kartı** (cümle örnek olarak, kelime kalın) ya da yeni yerleşik **Cümle kartı** (cümle + çeviri + hedef kelime + kaynak/zaman damgası).
- **Altyazıdan kart ve cümle madenciliği (§25):** `.srt`/`.vtt` yüklenir (etiketler, BOM, Windows-1254, VTT başlık/NOTE/STYLE), satırlar zamanlarıyla listelenir ve aynı motorla renklenir; video verilirse **ffmpeg** ile satırın ses kesiti (mp3) ve ekran görüntüsü (jpeg) karta eklenir (iptal edilebilir iş, ayarlanabilir ffmpeg yolu).
- **Artımlı okuma (§26):** okuma kuyruğu (öncelikli, aralıklı: "bugünlük bu kadar" → 1, 2, 4… gün), erteleme, seçili parçadan **özüt** (kuyruğa yüksek öncelikle girer) ve özütten **cloze** kartı.
- **Şema:** migration `0900_reading` (`reading_items`, `word_marks`); çöp temizliği okuma öğelerini de siler. API `reading.*` (UTF-16 ofsetler), olaylar `reading_items_changed`, `word_marks_changed`; hata kodları `error_codes/reading.in`. docs/10 §28.
- **Masaüstü:** "Okuma" ekranı (kitaplık, okuma kuyruğu, okuyucu, kelime paneli), TR/EN.
- **İnceleme düzeltmeleri:** ffmpeg yolu artık ayrı `settings:exec` izni ister (yalnız kullanıcı/CLI; eklenti, otomasyon, yerel API ve MCP değiştiremez), mevcut çalıştırılabilir dosyanın mutlak yolu olmalı, yedekten geri yüklemede silinir; ffmpeg stderr'i çalışırken okunur, Windows'ta konsol penceresi açılmaz, geçici klasör `tempfile`. Türkçe ek soyucu ünlü uyumlu şablonlar + en az 3 harfli kök kuralıyla aşırı kesmez (`ayna`, `hasta`, `anne`, `kedi` bozulmaz; `Türkiye`/`Türkiye'de` aynı anahtar). Kuyrukta vadesi gelenler önceliğe göre. Altyazıda boş satırsız cue'lar ve tek `<` düzgün. İngilizce kısaltmalar (`don't` → `do`) ve tireli kelimeler; `bit`/`US` yanlış eşleşmez. 10 bin kelime renklendirme ölçümü.
- **Testler:** `fc-core::reading` (durum kuralı, özet, cümleler, aralıklar, cloze), `fc-providers::lemma`, `fc-interop::subtitle`, depolama `0900` migration + gidiş-dönüş, `fc-app` `modules/reading/tests.rs` (bellek + SQLite: FSRS tutarlılığı, Türkçe + işaret geri al, öğe/kuyruk/özüt/cloze, kartlar, `.srt` dosyasından cümle kartı, sahte ffmpeg ile kesit + hata yolları, modül anahtarı), `reading.test.ts`.

## [0.9.0] — v0.9 Dış dünya ile bağlantı (Faz 4'te 0.10.0 ile birlikte birleştirildi)

### v0.9 Otomasyon kuralları (iz J, `feat/v0.9-auto`)
- **"Ne zaman + eğer + yap" kuralları** (`automation` modülü): tetikleyiciler — not eklendi/düzenlendi, kart cevaplandı (puana göre), bayraklandı (renge göre), sınıflandırma değişti, içe aktarma bitti, zamanlanmış (15 dk–30 gün); koşul sorgu dilinde (sınıflandırmalar dahil, ör. `seviye:A1 lapses>=5`; seçenek adı değişse de çalışır); eylemler izinli, geri alınabilir komutlar: etiket ekle/kaldır, askıya al, göm, bayrak, desteye taşı, **sınıflandırma ata**, ilerlemeyi sıfırla.
- **Değişiklik ve kuralları tek geri al adımı:** kuralın çalışması tetikleyen batch'in devamı olarak yazılır (aktör `automation:<kural>`) — çalışma ekranında geri al cevabı ve kuralın yaptığını birlikte geri alır; geri al/yinele kural tetiklemez. Zamanlanmış/elle çalışma kendi adımı. Otomasyon aktörü yalnız bu eylemlere ulaşır (gerçek `permits()`); kuralları yalnız kullanıcının kendi istemcileri (kullanıcı/CLI/sistem) yönetir — eklentiler, MCP, yerel API ve kurallar izin alsa bile yönetemez.
- **Hızlı:** olay kuralları yalnız olayın kartlarında aranır, kurallar ve koşullar dağıtım başına bir kez yüklenir, aynı koşul tek aramada; 100 kural × 100k notta cevap başına ek yük ~0,5 ms (bench).
- **Zamanlayıcı:** her vadeli kural kendi bütçesiyle çalışır (hiçbiri kaybolmaz; `last_run_at` çalışmayla aynı işlemde yazılır); saat geri kayarsa durmaz.
- **Döngü güvenliği:** döngü tespiti (kural kendini, doğrudan ya da başka kurallar üzerinden tetikleyemez), derinlik sınırı (`automation.max_depth`, 3) ve kaynak değişiklik başına bütçe (`automation.max_runs_per_event`, 20). Her çalışma kaydedilir (uygulandı / eşleşme yok / atlandı / başarısız).
- **Önizleme (kuru çalışma):** kaydetmeden hangi kartların değişeceği; eylemler geri sarılan bir işlemde gerçekten çalıştırılır, hiçbir şey yazılmaz.
- **Masaüstü:** Otomasyon ekranı (kural listesi, sıralama, aç/kapa, şimdi çalıştır, düzenleyici, önizleme, son çalışmalar), TR/EN; zamanlanmış kurallar için dakikalık zamanlayıcı. Migration `0850_automation`. docs/10 §30.

### v0.9 dış dünya — yerel HTTP API, gelen kutusu, derin bağlantılar, hızlı ekleme, pano (iz I, `feat/v0.9-api`)
- **Yerel HTTP API** (yeni crate `fc-http`, axum): yalnız `127.0.0.1`/`[::1]` (loopback dışı adres yapılandırmada yazılamaz), `Host` denetimi, web sayfası kökenleri her zaman `403`, CORS yalnız tarayıcı eklentisi kökenine; **eşleştirme + kullanıcı onayı** (`POST /v1/pair` → uygulamada "X bağlanmak istiyor" → izinleri daraltarak onay), token yalnız bir kez verilir ve **saklanmaz** (SHA-256 özeti), eşleştirildiği kökene bağlı; istemci başına kapsamlı izinler (`inbox:write`, `decks:read`…), açık metot listesi (iç, geri alınamaz, `io`/eklenti/MCP/AI anahtarı metotları asla); hız sınırı (`429` + `Retry-After`; eşleştirme ayrıca ad başına), 256 KiB gövde sınırı (`413`); **CSRF ek katmanı**: `ping` dışı her istek `X-Flashcard-Client` başlığı, her `POST` JSON `Content-Type` ister (eşleştirme yoklaması `POST /v1/pair/{id}`); bekleyen eşleştirmeler sunucu açılışında bırakılır, dolu kuyrukta en eskisi düşer; istemci başına gelen kutusu kotası (`local_api.inbox_max_items`/`inbox_max_kib`, `CONFLICT_INBOX_QUOTA`); onay penceresinde kapsam onay kutuları. Modül `local_api` **varsayılan kapalı**; `fc api serve` aynı sunucuyu masaüstü olmadan çalıştırır. Sözleşme docs/06 §3.
- **Gelen kutusu:** eklentiden, bağlantılardan, panodan, yerel API'den gelen kart önerileri (bağlam: sayfa, cümle; sınıflandırma: `Seviye: B1`) inceleme ekranında düzenlenip desteye **tek geri al adımıyla** kabul edilir ya da reddedilir; tekrar gönderim `client_ref` ile çoğalmaz.
- **`flashcard://` bağlantıları:** deste/not/çalışma bağlantıları ekranı açar, `add` bağlantısı yalnız gelen kutusuna önerir; **tek örnek** (ikinci başlatma bağlantıyı çalışan pencereye verir), uygulama kapalıyken açılış bağlantısı kaybolmaz.
- **Global hızlı ekleme** penceresi (`Ctrl+Alt+N`, ayarlanabilir; pano metniyle doldurabilir) ve **pano önerileri** (varsayılan kapalı; kapalıyken pano okunmaz; parola/anahtar/kart numarası gibi görünen metin ve işletim sisteminin gizli işaretlediği içerik yok sayılır).
- Yedekten geri yükleme tüm yerel API istemcilerini iptal eder. Migration `0800_inbox_local_api`. docs/10 §31.
- **Testler:** `fc-http` gerçek TCP (token'sız ret, yalnız loopback, izin reddi, eklenti kelimesi gelen kutusuna, web kökeni/büyük gövde/patlama reddi); `fc-cli/tests/api_http.rs` gerçek ikililerle; `fc-app` `inbox`/`local_api` testleri × 3 depo; `fc-core` bağlantı ayrıştırıcı + pano süzgeci; depolama 0800; kabuk `outside.rs` (açılış/ikinci örnek bağlantıları, şema + tek örnek yapılandırması, pano izleyici, sunucu yaşam döngüsü); arayüz `features/inbox/inbox.test.ts`.

### v0.9 tarayıcı eklentisi (iz K, `feat/v0.9-ext`)
- **`apps/browser-extension`: Chromium MV3 eklentisi** (Chrome/Edge/Brave; TypeScript + Vite). Seçili kelime/cümle → sağ tık **Desteye ekle**, `Alt+Shift+A` ya da araç çubuğu → ön/arka yüz, deste ve destenin **sınıflandırmaları** (ör. Seviye; tüm türler, deste başına son kullanılan hatırlanır) → uygulamanın **gelen kutusuna** (`POST /v1/inbox`). Seçimin geçtiği cümle, sayfa başlığı ve adresi bağlam olarak gider.
- **Eşleştirme:** eklenti uygulamadan onay ister, token `chrome.storage.local`'da; iptal edilince unutulur, yeniden bağlanmak önerilir. Yalnız `127.0.0.1`, telemetri yok.
- **Uygulama kapalıyken kuyruk:** gönderilemeyenler sırayla saklanır, uygulama açılınca (en geç 1 dk) gönderilir; `client_ref` ile yeniden gönderme tekrar öğe yaratmaz; kalıcı ret ayarlarda listelenir. Uygulama kapalıyken son bilinen desteler gösterilir.
- TR/EN (`chrome.i18n`). Testler: vitest (birim + docs/06 §3'e göre sahte API), Playwright (paketlenmemiş eklenti başsız Chromium'da; gerçek `fc api serve`'e karşı `FC_BIN` ile). CI'da `extension` işi. Ayrıntı: docs/06 §5, `apps/browser-extension/README.md`.

## Faz 3 entegrasyonu (v0.6–v0.8 birleşik, `feat/phase3-integration`)

### Faz 3 entegrasyonu — tur 4 (+ v0.7 eklentiler)
- v0.7 eklentiler v0.6 istatistik, v0.8 AI/MCP ve Faz 2 kurallarıyla birleşti. Workspace sürümü **0.8.0**.
- **İzinler tek yerde, her aktör için doğru** (`Registry::permits`): kullanıcı/CLI/sistem her şey; MCP seçili izinler; eklenti yalnız verilenler; **otomasyon ve yerel API hiçbir şey** (v0.9'a kadar varsayılan ret; önceden neredeyse her şey). Eklentiler MCP sunucularına, AI anahtarlarına ve AI yazmalarına, `io`'ya, geçmişe (`oplog`), istatistik önbelleğine (`stats:write` verilemez) ve optimizer'a ulaşamaz — kayıtlı izin olsa da, hangi yoldan gelirse gelsin.
- **AI anahtarı adresle gider:** `ai_keys:write` olmadan `keep_key: true` anahtarı korumaz (Faz 3 tur 3'ten ertelenen test).
- **Süzme katmanı yeni alanlarda:** istatistik, planlama, hedefler, AI, MCP; bu alanların olayları okuma izni olmayan eklentinin kancasına hiç gitmez.
- **Eklenti TTS'i çalışmada:** kartın `{{tts:…}}` düğmesi kayıtlı sağlayıcıyla (eklenti) konuşur, yoksa tarayıcı sesine düşer.
- Derleme: `dev`/`test` profillerinde `debug = "line-tables-only"` (daha küçük `target/`). docs/10 §25.

### Faz 3 entegrasyonu — tur 3 (+ v0.8 AI, `feat/phase3-integration`)
- v0.8 AI, MCP, istatistik ve Faz 2 geçmiş kurallarıyla birleşti: AI'nin eklediği/düzenlediği notlar replay listesindeki sıradan geri al adımlarıdır (`ai.usage.record` defter satırı, replay edilemez); AI iş şeridi içe/dışa aktarma ve optimizer işleriyle yan yana çalışır, iptal edilebilir; MCP hiçbir `ai.*` metoduna (okumalar ve `batch.execute` içi dahil) erişemez, anahtar görmez; seviyeye duyarlı üretimin sınıflandırmaları istatistik kırılımlarında görünür, AI kullanımı yalnız AI ekranında. Menüde AI, Hedefler'den sonra. Testler `modules/ai/tests_phase3.rs`. docs/10 §22–§23.

### Faz 3 entegrasyonu — tur 2 (+ v0.6 istatistik, `feat/phase3-integration`)
- v0.6 Faz 2 düzeltmeleri ve MCP ile birleşti: istatistik/planlama/hedef ekranları `data_changed` (Claude'un ya da CLI'nin başka süreçteki yazmaları) ve geri yüklemede yenilenir; geri yükleme istatistik önbelleğini açıkça boşaltıp geçersiz kılar (sonraki yenilemede yeniden kurulur); çöp temizliği bariyeri istatistikleri ve önbelleği bozmaz; MCP istatistik/optimizer metotlarına erişemez. Uçtan uca test `phase3_stats_tests`. docs/10 §21.

### Faz 3 entegrasyonu — tur 1 (+ v0.8 MCP, `feat/phase3-integration`)
- MCP Faz 2 son inceleme düzeltmeleriyle birleşti: MCP yazmaları replay izin listesinden geçen sıradan geri al adımları, geçmiş bariyerleri MCP adımlarına da uygulanır, yedekten geri yükleme onayları siler ve geçmişi bariyerle başlatır (onay geri alınarak geri getirilemez).
- `fc mcp serve` açılışta yalnız bağlanır (`App::attach_state`): masaüstünün sürmekte olan işlerini kesmez, içe aktarmasını geri sarmaz, çöpü boşaltmaz. docs/10 §19.

## [0.8.0] — Yayımlanmadı · Yapay zeka (iz G, `feat/v0.8-ai`) + MCP (iz H, `feat/v0.8-mcp`)

### Yapay zeka
- **`fc-ai` crate'i (adaptörler, iş mantığı yok):** `fc_providers::ai::AiProvider` uygulamaları — **Anthropic Claude** (Messages API; şemalı çıktı zorunlu araç çağrısıyla, `claude-opus-5-5` / `claude-sonnet-5-5` / `claude-haiku-5-5` önerilir, varsayılan `claude-sonnet-5-5`), **OpenAI**, **OpenAI uyumlu** uç noktalar (OpenRouter, LM Studio…), **Ollama** (yerel, `/api/chat`). Engelleyen HTTP (`ureq`, rustls) iş parçacığında `fc_ai::block_on` ile. HTTP durumları `ProviderErrorKind`'a (401/403 → yetkisiz, 429 → sınır, 5xx/529 → ulaşılamıyor, yeniden denenebilir).
- **API anahtarları yalnız işletim sistemi anahtar zincirinde** (`keyring`: Windows Credential Manager, macOS Keychain, Linux keyutils; masaüstü ve CLI aynı `app.flashcard` servisini kullanır). `Secret` yazdırılamaz/serileştirilemez; `ai.providers.set_key` geri al adımı değildir (parametresi oplog'a hiç yazılmaz); sağlayıcı hata gövdeleri anahtardan (tam ve 8+ karakterlik parçaları) arındırılır; sağlayıcı silinince anahtarı da silinir.
- **`ai` modülü:** sağlayıcılar (ekle/düzenle/kaldır, varsayılan, model listesini yenile — iş), **özellik başına sağlayıcı/model** (`ai.routes.*`), **düzenlenebilir fiyat tablosu** (`ai.prices.*`, migration `0700_ai`), **kullanım takibi** (`ai_usage`: her çağrı sağlayıcı/model/özellik/token/tahmini maliyet; bu ay / son 7 gün, özelliğe ve modele göre) ve **aylık bütçe uyarısı** (`ai.monthly_budget_usd`). Zaman aşımı ve yeniden deneme ayarları; özellik istemleri gelişmiş ayarlardan görülür/düzenlenir (`ai.prompt.<özellik>`).
- **Her istek önce önizlenir:** `ai.preview` gönderilecek isteğin tamamını (sağlayıcı, model, talimat, mesajlar, öğe sayısı, token/maliyet üst sınırı, bütçe) ve özetini döner; `ai.run` aynı özeti ister, istek değiştiyse `CONFLICT_AI_PREVIEW_STALE`; iş, önizlenen isteğin kendisini gönderir. Cevap şemayla doğrulanır, geçersizse bir onarım denemesi.
- **AI özellikleri:** kaynaktan kart üretme (metin + görsel), alan doldurma, açıklama, ipucu, anlam bazlı cevap kontrolü, kart iyileştirme (yalnız değişen alanlar), zor kartı yeniden yazma + anımsatıcı, çeldirici üretme (çoktan seçmeli türün `distractors` alanına). **Seviyeye duyarlı:** destenin sınıflandırmaları (verilen ya da varsayılan değerler, ör. `Seviye: B1`) isteme girer ve üretilen notlara yazılır. Uygulama `ai.notes.add` / `ai.notes.update` ile — tek geri al adımı, aynı notlar atlanır.
- **Masaüstü:** "Yapay zeka" ekranı (kart üret → önizle → gönder → taslakları düzenle → ekle; sağlayıcılar, anahtar, özellik başına model, fiyatlar; kullanım ve bütçe), TR/EN.
- **İnceleme sonrası (v0.8 tur 2):** anahtar başka adrese **sessizce gitmez** (adres/tür değişince silinir; onay + `ai_keys:write` ile korunur; geri al da siler), önizleme özeti tür+adres+modeli kapsar ve iş başlarken yeniden doğrulanır; kuyruktaki AI işi modül kapatıldıysa **gönderilmez**; veri içindeki `</source>` gibi ayraç etiketleri etkisizleştirilir; `Retry-After` (en çok 60 sn) ve sapmalı (jitter) bekleme; oturumluk anahtar zinciri arayüzde uyarılır; AI işleri **ayrı iş şeridinde** (içe/dışa aktarma beklemez).
- **Kaynaklar:** **PDF'ten** (sayfa aralığı seçerek, `lopdf`) ve **web sayfasından** (betik çalıştırmadan okunur metin; süre/boyut/tür sınırlı) kart üretme — `ai.sources.extract` işi metni düzenlenebilir kaynak kutusuna getirir, gönderim yine önizlemeden geçer.
- **SSRF koruması (web kaynağı):** ad kendimiz çözülür; loopback, özel (RFC1918), link-local (bulut metadata dahil), CGNAT, benzersiz-yerel IPv6, `0.0.0.0`, multicast ve IPv4-eşlemeli IPv6 adresleri `blocked_host` ile reddedilir; bağlantı doğrulanan IP'ye sabitlenir, her yönlendirme (en çok 5) baştan doğrulanır. Yerel sayfalar yalnız `ai.web.allow_private_hosts` (varsayılan kapalı) açılırsa çekilir.
- **Akış:** açıklama ve ipucu gelirken gösterilir (Claude/OpenAI SSE, Ollama NDJSON → `ai_partial` olayı). **Çalışma ekranında** "AI ipucu", "Açıkla", "AI ile kontrol et" (modül kapalı ya da sağlayıcı yoksa gizli; her istek önizlemeli).
- **Testler:** `fc-ai` 30 (yukarıdakilere ek: üç sağlayıcının akışı — parçaların zamanla geldiği dahil, akış hata olayı, `stream`'i yok sayan sunucu, `Retry-After` ayrıştırma, PDF sayfa aralığı/hatalar, web okuma: makale seçimi, betik/menü/altbilgi atma, yönlendirme, tür/boyut/durum hataları); `fc-app` `modules/ai/tests_v2.rs` (adres değişince anahtar, özet kapsamı + kuyruktaki işin yeniden doğrulanması, modül kapalıyken gönderim yok, `Retry-After` beklemesi + 60 sn tavanı, sapma, oturumluk anahtar zinciri, akış olayları + yeniden denemede sıfırlama, PDF/web kaynağından önizlemeli üretim, AI şeridi uzun bir istek sürerken dışa aktarmanın bitmesi); `prompts` etiket etkisizleştirme; depolama şerit talebi; `study-ai.test.ts`.
- **Testler (ilk tur):** `fc-ai` 16 (her sağlayıcı yerel sahte HTTP sunucusuna karşı: tel biçimi, şemalı çıktı, görsel, model listesi, hata eşleme + anahtar sızıntısı; gerçek API testleri `FC_AI_LIVE_*` ile isteğe bağlı); `fc-app` `modules/ai/tests.rs` (uçtan uca seviyeye duyarlı üretim + kullanım + geri al; önizlenmeyen hiçbir şey gönderilmez; anahtar veritabanı dosyası/WAL, döküm, oplog, olaylar, işler ve hatalarda yok; özellikler; yeniden deneme + onarım; yönlendirme; fiyat/bütçe; model listesi işi), `prompts`/`usage` birim testleri; depolama `0700` migration testi; `ai.test.ts`.

### MCP sunucusu ve istemcisi
- **MCP sunucusu:** `fc --db <collection.db> mcp serve [--read-only]` — Claude Desktop / Claude Code koleksiyonla stdio üzerinden çalışır (resmi Rust SDK `rmcp`, yeni crate `fc-mcp`). Araçlar: `list_decks`, `list_note_types`, `list_classifications`, `search_cards` (sorgu dili, `seviye>=B1`), `get_note`, `create_deck`, `add_notes` (sınıflandırma değerleriyle, hepsi-ya-da-hiçbiri, tek geri al), `update_note`, `set_classification` (`confirm: true` olmadan yalnız önizleme). Silen araç yok. Kurulum parçacıkları docs/06 §2.1'de ve uygulamanın **MCP** ekranında.
- **Güvenlik:** yazmalar sıradan komutlar (oplog aktörü `mcp:<istemci adı>`, uygulamadan geri alınır); MCP aktörü yalnız seçili izinleri taşır (ayar/modül/geri al/iç metot/dış sunucu kaydı reddedilir); `mcp.server.allow_writes` ayarı ve `--read-only` yazma araçlarını gizler ve reddeder; `mcp` modülü kapatılabilir.
- **MCP istemci iskeleti:** dış MCP sunucuları `mcp.servers.add/update/remove` ile kaydedilir (migration `0750_mcp_servers`; ortam değişkeni/sır saklanmaz), `fc_mcp::McpClient` stdio sunucuya bağlanır, araçları listeler/çağırır; dış ipuçlarına güvenmeyen onay politikası. `fc mcp tools <ad>|-- <komut…>`.
- **Testler:** gerçek `fc mcp serve` ikilisiyle stdio JSON-RPC kabul testi (`fc-cli/tests/mcp_stdio.rs`), bellek içi protokol + istemci testleri (`fc-mcp/tests`), araç anlamı ve izinler × 3 depo (`fc-app` `modules/mcp/tests.rs`), `0750` migration testi, masaüstü ekran testi. İki ortak test, sonraki sürümlerin migration'larına dayanıklı hale getirildi.

- **İnceleme düzeltmeleri:** `mcp:write` yalnız kullanıcı ve CLI'de (Plugin/Automation/LocalApi reddedilir); dış sunucu kaydı kullanıcı onayı taşır (migration `0751_mcp_server_approval`, `McpServerDto.approved`), `McpClient::connect_registered` kapalı/onaysız kaydı başlatmaz, yedekten geri yükleme onayları siler, arayüz komutu gösterip onay ister; MCP aktörü yalnız araç kataloğunun metotlarına erişir (silme/çöp boşaltma `notes:write` ile kapsansa da kapalı, testli); `set_classification` önizlemesi `confirm_token` döner, uygulama onu şart koşar, 2000 kart üst sınırı; `fc mcp serve` şemayı migrate etmez, eskiyse reddeder; masaüstü MCP/CLI yazmalarını oplog konumundan fark edip `data_changed` olayıyla ekranlarını yeniler (Ctrl+Z'nin Claude'un adımını geri alabileceği belgelendi); docs/10 §10'a v0.8 aralığı (`0750–0799`).
## [0.7.0] — Yayımlanmadı · Eklenti altyapısı (iz F, `feat/v0.7-plugins`)

### Eklendi
- **Workspace:** `fc-core`, `fc-api`, `fc-storage`, `fc-providers`, `fc-app`, `fc-cli` ve Tauri 2 kabuğu `fc-desktop`.
- **fc-core:** saf alan modeli (UUIDv7 kimlikler, `Clock`/`Rng`, deste/not/not türü/kart/etiket), sınıflandırma tanımları + miras çözümü + hazır şablonlar (CEFR, YKS…). G/Ç yok (CI saflık betiği).
- **fc-storage:** SQLite (WAL, tek yazıcı + okuma havuzu), checksum'lı migration'lar (`0001_init`, migrate öncesi yedek), tüm repository'ler, FTS5, deste alt ağacı çöpü, tam geri alınabilir seçenek silme/birleştirme.
- **fc-api:** v1 metot kataloğu (public/internal), hata modeli, olaylar, `gen-ts` ile TypeScript tipleri (`--check` CI'da).
- **fc-app:** komut/sorgu dağıtıcı, registry, oplog + geri al/yinele (iç ters metotlarla birebir), olay yolu, modüller, şemalı ayarlar, iş kuyruğu arayüzü; deste/not/etiket/sınıflandırma/ayar/modül handler'ları.
- **fc-cli (`fc`):** `deck create/list/rename/move/delete/restore`, `note add/list/edit/delete/restore` (`--attr anahtar-veya-ad=değer`), `attribute define/apply-template/set/list/option add|rename|reorder|delete`, `undo`/`redo` (TR/EN özet), genel `fc call <metot> [<json>|-]` (yalnız public metotlar).
- **Masaüstü:** Svelte 5 arayüz (deste ağacı, klasik not ekleme + sınıflandırma alanları, not listesi, ayarlar), TR/EN i18n, tema token'ları; tek `invoke` IPC + `fc://event` olay akışı; menüler `modules.list`'ten.
- **Faz 2 hazırlığı:** metot kataloğu, hata kodları, repo trait'leri, `MemStore`, modül listesi ve i18n sözlükleri alan/özellik başına dosyalara bölündü; migration'lar klasörden otomatik toplanır ve numara aralıkları testle zorunlu (docs/10 §10).
- **CI:** fmt, clippy `-D warnings`, testler, TS tip tutarlılığı, frontend check/test/build, Windows derlemesi, `fc-core` saflık kontrolü, dependabot.

### Güvenlik (bağımsız inceleme bulguları)
- `before:`/`after:` kancaları artık `plugins.*`, `oplog.*`, `modules.*`, `settings.*`, `shortcuts.*`, `io.*`, `batch.*`, `jobs.*`, `mcp.*` ve AI sağlayıcı/anahtar metotlarına takılamaz; her kanca hedef metodun kendi iznini de ister (önceden bir eklenti kullanıcının izin değişikliğini ya da dışa aktarma yolunu yeniden yazabiliyordu).
- `io:*` (serbest dosya yolu), `settings:write`, `modules:write`, `oplog:write`, `mcp:*`, `ai_keys:*`, `ai_requests:*`, `ai:write` eklentilere verilemez; `Registry::permits` eklenti için bunları kayıtlı izin olsa bile reddeder (eklenti kendi yakıt/bellek sınırını yükseltemez).
- Eklenti deposu ve ayar yazımları işlemin geri al grubuna girer: eklenti hata verirse geri sarılır.
- **Kanca zinciri (yeniden inceleme):** hata veren eklenti çağrısının geri sarılması artık eklentinin aktörüyle çalışır (önceden `System`: geri sarmanın olayları `event:` kancalarını yeniden tetikliyor, her işlemde katlanarak büyüyordu). Ek olarak iş parçacığı başına **yeniden giriş koruması** (kanca çalışırken olan hiçbir şey, aktörü ne olursa olsun, `event:` kuyruğuna girmez; kanca kanca içinde başlamaz) ve **işlem başına kanca bütçesi** (`plugins.max_hooks_per_action`, varsayılan 200): aşılınca işlemin kalan kancaları atlanır, eklenti kapatılır, kullanıcıya i18n bildirim (`plugins_changed.reason = plugins.notice.hook_budget`). Test: `failed_hook_rollback_does_not_cascade`, `hook_budget_per_action` (WAT fixture'ları).
- **Bellek sınırı (yeniden inceleme):** multi-memory, threads/paylaşımlı bellek, memory64, özel sayfa boyutu, GC, istisnalar, stack switching, component model, wide arithmetic, relaxed SIMD kapalı; `Limiter` tüm bellekleri ve tabloları **toplam** sayar, örnek başına 1 bellek, en çok 4 tablo. 99 × 64 MB'lık modül derlenmez (`many_memories.wat`).
- **Eklenti deposu (yeniden inceleme):** `kv.set` artık manifestte istenip verilmiş **`storage:kv`** izni ister (silme her zaman serbest); eklenti başına kota `plugins.kv_max_keys` (10 000) ve `plugins.kv_max_mb` (16) — uygulama ayarı, eklenti değiştiremez; büyütmeyen yazım ve geri yükleme (`SetPluginKv.restore`) kotaya takılmaz. `PluginRead::plugin_kv_usage`.

- **Kaynak sınırları (üçüncü inceleme):** (1) *Son tarih atlatma:* son tarih artık host çağrılarında geçen süreyi de sayar (her host çağrısının girişinde ve çıkışında + epoch; yalnız dosya penceresinde kullanıcıyı bekleme hariç, `HostCalls::paused`); çağrı başına en çok `plugins.max_host_calls` (10 000) host çağrısı ve `plugins.host_mb_per_call` (64) MiB sınırdan geçen veri (`HOST_LIMIT`, arıza); her host çağrısı yakıt tüketir (50 000 + işlevin kendi bedeli + bayt başına 4). Bir işlemin kancaları toplam `plugins.hook_ms_per_action` (10 000 ms) ile sınırlı, kancası arıza veren eklenti o işlemde atlanır: 1 000 000 `kv.get` döngüsü komut olarak da kanca olarak da kısa sürede durur. (2) *Host belleği atlatma:* her host fonksiyonu için istek/yanıt boyutu, çağrı başına ve dakikalık sayı sınırı (`fc_plugins::limits::HOST_FNS`, docs/05 §12 tablosu); istek boyutu ayrıştırılmadan denetlenir, aşan çağrı `VALIDATION_PLUGIN_HOST_LIMIT` alır. `ui.notify` artık **`ui:notify`** izni ister (3/çağrı, 10/dakika, ≤ 4 KiB): 400 × 1 MiB bildirim reddedilir, tepe bellek ~2 MiB (entegrasyon testi `plugin_host_memory`). `kv.list` sayfalı (`after`, `limit`; sayfa ≤ 1000 kayıt / ≈ 4 MiB; `PluginRead::list_plugin_kv_page`). İşlem yanıtı ≤ 4 MiB (TTS 16 MiB) ayrıştırılmadan denetlenir; girdi > 16 MiB ise eklenti çağrılmaz (`INPUT_TOO_LARGE`). Arayüz oturumu `state` ≤ 1 MiB, eklenti başına 8 oturum; `event:` kuyruğu ≤ kanca bütçesi + 1; manifest katkı listeleri ≤ 64, izin ≤ 128, ayar şeması ≤ 100. Günlük satırı kesilirken çok baytlı karakterde panik düzeltildi.
- **Veri akışı denetimi (dördüncü inceleme):** (1) *Bir eklenti diğerini kapatabiliyordu:* kanca bütçesi, ve her bütçe, zinciri **başlatan** eklentiye yazılır (`host::initiator`): A'nın `decks.create` döngüsü B'nin `before:` kancasını tüketince A kapanır, bütçeyi aşan çağrısı reddedilir (B'nin kancası olmadan devam etmez), B açık kalır; B'nin A'nın çağrısı içindeki arızaları B'ye sayılmaz (günlükte), kancaların dakikalık host çağrısı pencereleri A'nındır. (2) *`oplog:read` her okuma iznini atlatıyordu:* `oplog:read`, `batch:write` (sonuç süzmeyi ve kancaları atlatıyordu) ve `jobs:write` verilemez. **Tek süzme katmanı** `plugins::redact`: eklentiye giden her değer (metot sonucu, hata, `before:`/`after:`/`event:` kanca girdisi, iş sonucu, başka eklentinin ayar kapsamı) çağıran eklentinin o veri alanındaki **okuma** izniyle süzülür; izin yoksa sorgu reddedilir, komut yanıtı ve olay yalnız kimliklere indirilir; çapraz alanlı sonuçlar (arama satırları, kart bilgisi, çalışma oturumu) her alanın iznini ister; `bulk.find_replace` (kâhin) okuma izni olmadan çalışmaz; `before:`/`after:` kancası gördüğü verinin okuma iznini de ister (kurulumda ve çalışırken). Her alana bir birim testi, katalogdaki her metodun sınıflandırıldığını denetleyen test. (3) *Uzun tek host çağrısı:* `fc_storage::interrupt` — host çağrısının veritabanı ifadeleri eklenti çağrısının son tarihinde kesilir (SQLite ilerleme işleyicisi); tüm koleksiyonu tarayan `browse.duplicates/check_database/repair_database/purge_trash` eklentilere kapalı. (4) `fs.open` sınırı okurken de uygular (`/dev/zero` testi). `models.list` artık yol değil `ready` döner. docs/05 §13 tehdit modeli.
- **Eklenti başına kanca bütçesi (beşinci inceleme):** paylaşılan işlem bütçesi, kullanıcı işleminde sıradaki kancanın sahibini kapatıyordu: `com.aaa.victim` + `com.zzz.attacker` (ikisi de `event:note_updated`), 150 nota `bulk.add_tags` → kurban `AutoDisabled`; tek zararsız eklenti bile 201+ notta kapanıyordu. Artık her eklentinin işlem başına kendi sayı/süre bütçesi var; aşanın yalnız kendi kalan kancaları atlanır, kullanıcıya işlem başına tek bildirim (`DomainEvent::PluginHooksSkipped`, `plugins.notice.hooks_skipped`), bütçe aşımı kimseyi kapatmaz (eklenti işleminde bütçeyi aşan çağrı yine reddedilir). Kapatma yalnız eklentinin kendi arızalarıyla (döngüdeki kanca zaman aşımıyla `plugins.max_failures` sonra kapanır). `event:` kuyruğu abone eklenti başına sınırlı. `plugins.notice.hook_budget` kaldırıldı.
### Değişti
- `decks.create` isteğe bağlı `parent_id` alır; deste yolunu artık Rust kurar (arayüz birleştirmiyor).
- Masaüstü başlangıç hatası yerel hata penceresinde gösterilir.
- `rust-toolchain.toml` 1.100.0'a sabitlendi.

## [0.6.0] — Yayımlanmadı · İstatistik ve akıllı planlama (iz E, `feat/v0.6-stats`)

- **İstatistikler (`stats` modülü):** seri (güncel/en uzun), başarı oranı (yeni-öğrenilen / genç / olgun / yeniden öğrenilen), gerçek hatırlama vs hedef, aktivite ısı haritası, gelecek tahmini (+ gecikmiş), hafıza durumu, haftalık/aylık gelişim, saatlere göre başarı, cevap süreleri (ortalama, dağılım, en yavaş kartlar), stabilite/zorluk histogramları, zor kartlar (eşik ayarı `stats.leech_threshold`; `stats.apply_to_leeches` kapsamın tüm zor kartlarını Rust'ta seçip askıya alır / etiketler, tek geri al adımı), deste karnesi ("oturmuş" yüzde, şu anki hatırlama). Hepsi `fc_core::stats`'taki saf fonksiyonlar; kapsam: deste, etiket, sorgu (dolayısıyla sınıflandırma değeri), tarih aralığı. `stats.breakdown` her istatistiği bir sınıflandırmanın değerlerine böler (Seviye başına hatırlama, Sınav başına iş yükü). İçe aktarılan tekrarlar sayılır (`imported`), deneme tekrarları sayılmaz.
- **İstatistik önbelleği:** çalışma günü başına toplamlar (`DayAggregate`, birleşmeli monoid) `stats_day_cache`'te; önbellek + sonradan eklenen satırlar = yeniden hesaplama. Silinen/değişen tekrar satırı (geri al, içe aktarmanın atılması, geri yükleme) tetikleyicilerle `review_log_generation`'ı artırır, önbellek kullanılmaz. `stats.refresh_cache` (türetilmiş veri, geri al adımı değil).
- **Akıllı planlama (`planning` modülü):** yük dengeleme (bulanıklık aralığında en az yüklü gün; deterministik), kolay günler (hafta günü başına normal/azaltılmış/en az), sınav tarihi modu (`planning.exams.*`: deste ve/veya sorgu — ör. `sınav:TYT` — kapsamındaki kart sınavdan sonraya kalacaksa sınavdan önceki son 3 güne çekilir). Çalışma motoru planı yalnız planlı cevaplarda uygular; modül kapatılınca saf zamanlama.
- **FSRS kişisel optimizasyonu (`optimizer` modülü):** `optimizer.run` preset'i kullanan destelerin geçmişinden (içe aktarılanlar dahil) FSRS-6 ağırlıklarını `fsrs-rs` ile öğrenen **iptal edilebilir arka plan işi** başlatır; sonuç `presets.update` ile kaydedilir (tek geri al adımı). Az veride `CONFLICT_NOT_ENOUGH_REVIEWS`. `optimizer.workload`: hedef hatırlama oranlarının günlük tekrar tahmini.
- **Hedefler (`goals` modülü):** öğrenilen kart, çalışma dakikası (son tarihsiz = günlük), seri, deste tamamlama yüzdesi; ilerleme ve gidişat tahmini ("bu hızla 3 gün geç kalırsın"). Tüm komutlar geri alınabilir.
- **Masaüstü:** İstatistikler, Planlama, Hedefler ekranları (TR/EN); grafikler yalnız Rust'ın hesapladığını çizer.
- **Tekrar geçmişi kalıcı:** `0501_review_log_keeps_history` `review_log`'u FK'siz (CASCADE'siz) yeniden kurar; kart kalıcı silinse de ısı haritası, seri, toplamlar ve optimizer verisi korunur.
- **Düğme önizlemeleri** planlamayı (yük dengeleme, kolay günler, sınav modu) içerir: gösterilen aralık = uygulanan. Kapsamsız **hedef hatırlama** kullanıcının preset'lerinden gelir.
- `fsrs-rs`'in okuduğu `FSRS_NO_OUTLIER` süreç başında silinir (`fc_app::system::pin_process_env`): optimizasyon her makinede aynı.
- **Şema:** `0500_stats_planning` (önbellek tabloları, `review_log` tetikleyicileri, `exam_dates`); hedefler 0001'in `goals` tablosunu kullanır. Önbellek tabloları yedeklere girmez (türetilmiş veri).
- **Testler:** `fc_core::stats` (monoid özelliği proptest), `fc_core::goal`, `fc_study::planning` (365 günlük simülasyon: günlük yük varyansı 29.5 → 8.4; kolay günler; sınav günü hatırlama), `fc_study::optimize` (deterministik eğitim, iptal), depolama 0500 + tetikleyiciler, `fc-app` `modules/stats/tests.rs` × 3 depo (önbellek = yeniden hesaplama; kırılım; sınav modu; yük dengeleme; hedefler; optimizer işi + kuyrukta/eğitim sırasında iptal), `stats.test.ts`.

## Faz 2 entegrasyonu (v0.2–v0.5 birleşik; `main`'de `v0.5.0`)

### Faz 2 entegrasyonu — tur 3 (+ v0.4, `feat/phase2-integration`)
- **Tek preset okuma yolu:** çalışma deste ayarlarını yalnız `presets::effective` ile okur (deste → üst deste → varsayılanlar); tur 1'in `engine::preset_of`'u (yalnız destenin kendi şablonu) kalktı. Tek şema (`PresetConfig`): limitler, (yeniden) öğrenme adımları, hedef hatırlama, **`fsrs_weights`** (önceden `extra.fsrs_weights`), yeni **`maximum_interval_days`** (önceden `extra`; aynı anahtar, eski satırlar kayıpsız okunur), kardeş gömme, yeni/tekrar sırası (`retrievability` = en düşük hatırlama önce, `fc_study::session::Order::Preset`). `study.bury_siblings` ve `study.order` ayarları kalktı (preset'te); kardeş gömme varsayılanı **açık** (docs/02). Masaüstü preset düzenleyicisine "En uzun aralık".
- **Özel bayrak adları sorguda:** `browse::flag_names` ← v0.3 `card_flag_defs` (`flag:önemli`, boşluklu ad `_` ile ya da tırnakla); renk adları TR/EN her zaman; bayrak *n* = bit *n − 1*. `bulk.set_flag` geçersiz bayrakta `VALIDATION_FLAG` (v0.3 ile aynı). Kart tarayıcısının bayrak listesi `flags.list` adlarını gösterir.
- **Özel çalışma oturumları sorgu dilinden:** `StudyFilter.query` (`seviye:A2 is:due`) ve `StudyFilter.saved_query_id` (kayıtlı sorgu = filtreli deste; kanonik saklandığı için seçenek adı değişse de çalışır); oturum `sessions.query`'ye yazılır. Çalışma ekranında sorgu kutusu ve kayıtlı sorgu seçimi.
- **`bulk.change_notetype`:** seçili notları (tek kaynak türünden) başka bir not türüne taşır; alan eşleme (hedef ← kaynak; varsayılan: aynı anahtar, sonra sıra), şablon eşleme (eşlenen şablonun kartı kimliği, zamanlaması ve geçmişiyle kalır; cloze/maske anahtarı `c<n>`/`m<n>` korunur), eşlenmeyen kartlar çöpe, eksikler üretilir. Tek komut = tek geri al (`bulk.restore_notes` artık not türünü, `bulk.restore_cards` şablon anahtarı ve motoru da geri yazar; üretilen kartlar geri alınca çöpe gider). Kart tarayıcısında "Not türünü değiştir".
- **Arama dizini düz metin:** not yazımı ve içe aktarma dizine cloze cevaplarını (`{{c1::Ankara::şehir}}` → `Ankara`), bağlantı etiketlerini, HTML'siz metni yazar; resim/ses/video alanları ve çıplak `media:<hash>` dizine girmez (`fc_core::note::index_text`). Migration **`0302_search_plain_text`** dizini notlardan yeniden kurar (SQL fonksiyonları `fc_index_text`/`fc_index_sort`); yedekten geri yükleme dizini her zaman notlardan yeniden kurar.
- **Komut paleti ve kısayollar:** palet dört izin tüm public komutlarını TR/EN etiketle listeler (`locales/*/commands.json`); kısayol düzenleyici çalışma tuşlarını kapsar (`study.reveal/again/hard/good/easy/hint/suspend/bury/undo/end`, bağlam `study`). Ctrl/⌘+Z çalışmada artık kabuğun genel `ui.undo`'su (önceden ikisi birden geri alıyordu); çalışma ekranı dışarıdan gelen geri al/yinele'de kendini yeniler.
- `browse::PurgeTrashJob` v0.5 iş havuzuna kaydedildi (`JobContext::invoke`); açılışta çöp temizliği yarım kalmış geri al gruplarının geri sarılmasından sonra koşar.
- **Testler:** `phase2_manage_tests.rs` × 3 depo (preset → kuyruk/aralık + miras + geri al; özel bayrak adları; sorgudan/kayıtlı sorgudan özel oturum; not türü değiştirme + birebir geri al/yinele; düz metin dizini, içe aktarılan notlar dahil; palet + çalışma kısayolları); `migration_0302_rebuilds_the_index_as_folded_plain_text`; eski şemalı yedek bayat dizinle geri yüklenir → birebir; `preset_orders`; `index_text_strips_markup`; `keys.test.ts` yeniden bağlanan tuşlar. v0.4'ün `negation_is_complement` proptest'i `or/and/not` kelimesinde yanlış alarm veriyordu (operatör), düzeltildi.

### Faz 2 entegrasyonu — tur 2 (+ v0.5, `feat/phase2-integration`)
- **Medya tek depoda:** içe/dışa aktarma ve yedek v0.3'ün içerik adresli dosya deposunu kullanır (`io::media::FileMedia`); geçici `BlobMedia` ve `media_blobs` kalktı — `0402_drop_media_blobs`, açılışta kalan baytları önce dosyalara taşır. Aynı içerik diskte tek dosya. Yedekler medya dosyalarını içerir, geri yükleme onları yeniden yazar.
- **Anki → v0.3:** Anki'nin hazır not türleri (Basic, ters, isteğe bağlı ters, yazarak, Cloze, Image Occlusion) yapılarından tanınıp yerleşik türlere eşlenir; Image Occlusion şekilleri v0.3 maskelerine çevrilir. `[sound:x]` ↔ `<audio src="media:…">`, resim/ses alanlarında çıplak `media:` başvurusu. İçe aktarılan kartlar kart türü motorlarıyla çizilir.
- **Anki bayrakları** numara ↔ bit doğru çevrilir (7 = mor = `card_flag_defs` 7; önceden numara bit maskesi gibi yazılıyordu).
- **İçe aktarılan geçmiş** `review_log.mode = "imported"`: kart geçmişinde görünür, günlük çalışma limitlerine sayılmaz.
- **Satır içe aktarma** (metin/CSV/JSON) kartları v0.3 motorlarından üretir: boşluk doldurma türüne de satır aktarılır.
- Yarıda atılan içe aktarma, eklediği ve kimsenin kullanmadığı medya satırlarını da siler (`ImportedIds.media`).
- **Dokümantasyon:** v0.5 ayırıcı algılama kriteri **kısmen** karşılandı olarak işaretlendi (tutulmuş sette %92,9, 65/70); v1.0'a iyileştirme maddesi eklendi.
- **Testler:** `phase2_io_tests.rs` uçtan uca (CSV → sınıflandırma, Anki cloze/resim/ses/geçmiş → diskte tekil medya → çizim → çalışma, limit → yedek → koleksiyon klasörünü sil → geri yükle → birebir); Anki yerleşik eşleme ve resim gizleme dönüşümü; 0402 taşıma migration testi.

### Faz 2 entegrasyonu — tur 1 (v0.2 + v0.3, `feat/phase2-integration`)
- **Çalışma kartları kart türü motoruyla çizilir:** `StudyCardDto` artık `sides` (temizlenmiş HTML, CSS, etkileşim, TTS) ve `flags` taşır; düz metin `question/answer` ve `fc_study::render` kalktı (`fc_study::text::html_to_text` kaldı). Çalışma ekranı `CardView` ile medya, KaTeX, kod ve not türü CSS'ini gösterir.
- **Boş kartlar** (silinen cloze/maske) kuyruğa girmez, oturumda atlanır, puanlanamaz.
- **Cevap kontrolü motor üzerinden:** yazarak türler v0.2 kontrolcüleriyle, çoktan seçmeli motorla (`CheckAnswer.choice`); soruda 1–9 seçenek seçer.
- **Preset günlük limitleri:** normal oturum `new_per_day`/`reviews_per_day`'den bugün çalışılanı düşer (`StudyRead::review_logs_since`, `BuildOptions.review_limit`).
- **Bayraklar çalışmada:** Anki numaralı adlar/renkler (`flags.list`), Ctrl/⌘+1–7 aç/kapa.
- **Testler:** `phase2_tests.rs` uçtan uca (cloze + klasik + Seviye → çalış → puanla → geri al birebir; deneme modu `cards`'ı değiştirmez; günlük limitler) × 3 depo; `review_logs_since` depolama testi; `review_limit` kuyruk testi; tuş haritası ve bayrak bileşen testi.

## [0.5.0] — Yayımlanmadı · İçe / dışa aktarma (iz D, `feat/v0.5-io`)

### Eklendi
- **Workspace:** `fc-core`, `fc-api`, `fc-storage`, `fc-providers`, `fc-app`, `fc-cli` ve Tauri 2 kabuğu `fc-desktop`.
- **fc-core:** saf alan modeli (UUIDv7 kimlikler, `Clock`/`Rng`, deste/not/not türü/kart/etiket), sınıflandırma tanımları + miras çözümü + hazır şablonlar (CEFR, YKS…). G/Ç yok (CI saflık betiği).
- **fc-storage:** SQLite (WAL, tek yazıcı + okuma havuzu), checksum'lı migration'lar (`0001_init`, migrate öncesi yedek), tüm repository'ler, FTS5, deste alt ağacı çöpü, tam geri alınabilir seçenek silme/birleştirme.
- **fc-api:** v1 metot kataloğu (public/internal), hata modeli, olaylar, `gen-ts` ile TypeScript tipleri (`--check` CI'da).
- **fc-app:** komut/sorgu dağıtıcı, registry, oplog + geri al/yinele (iç ters metotlarla birebir), olay yolu, modüller, şemalı ayarlar, iş kuyruğu arayüzü; deste/not/etiket/sınıflandırma/ayar/modül handler'ları.
- **fc-cli (`fc`):** `deck create/list/rename/move/delete/restore`, `note add/list/edit/delete/restore` (`--attr anahtar-veya-ad=değer`), `attribute define/apply-template/set/list/option add|rename|reorder|delete`, `undo`/`redo` (TR/EN özet), genel `fc call <metot> [<json>|-]` (yalnız public metotlar).
- **Masaüstü:** Svelte 5 arayüz (deste ağacı, klasik not ekleme + sınıflandırma alanları, not listesi, ayarlar), TR/EN i18n, tema token'ları; tek `invoke` IPC + `fc://event` olay akışı; menüler `modules.list`'ten.
- **Faz 2 hazırlığı:** metot kataloğu, hata kodları, repo trait'leri, `MemStore`, modül listesi ve i18n sözlükleri alan/özellik başına dosyalara bölündü; migration'lar klasörden otomatik toplanır ve numara aralıkları testle zorunlu (docs/10 §10).
- **CI:** fmt, clippy `-D warnings`, testler, TS tip tutarlılığı, frontend check/test/build, Windows derlemesi, `fc-core` saflık kontrolü, dependabot.

### Değişti
- `decks.create` isteğe bağlı `parent_id` alır; deste yolunu artık Rust kurar (arayüz birleştirmiyor).
- Masaüstü başlangıç hatası yerel hata penceresinde gösterilir.
- `rust-toolchain.toml` 1.100.0'a sabitlendi.

## [0.4.0] — Yayımlanmadı · Kart yönetimi (iz C, `feat/v0.4-manage`)

### Eklendi
- **Workspace:** `fc-core`, `fc-api`, `fc-storage`, `fc-providers`, `fc-app`, `fc-cli` ve Tauri 2 kabuğu `fc-desktop`.
- **fc-core:** saf alan modeli (UUIDv7 kimlikler, `Clock`/`Rng`, deste/not/not türü/kart/etiket), sınıflandırma tanımları + miras çözümü + hazır şablonlar (CEFR, YKS…). G/Ç yok (CI saflık betiği).
- **fc-storage:** SQLite (WAL, tek yazıcı + okuma havuzu), checksum'lı migration'lar (`0001_init`, migrate öncesi yedek), tüm repository'ler, FTS5, deste alt ağacı çöpü, tam geri alınabilir seçenek silme/birleştirme.
- **fc-api:** v1 metot kataloğu (public/internal), hata modeli, olaylar, `gen-ts` ile TypeScript tipleri (`--check` CI'da).
- **fc-app:** komut/sorgu dağıtıcı, registry, oplog + geri al/yinele (iç ters metotlarla birebir), olay yolu, modüller, şemalı ayarlar, iş kuyruğu arayüzü; deste/not/etiket/sınıflandırma/ayar/modül handler'ları.
- **fc-cli (`fc`):** `deck create/list/rename/move/delete/restore`, `note add/list/edit/delete/restore` (`--attr anahtar-veya-ad=değer`), `attribute define/apply-template/set/list/option add|rename|reorder|delete`, `undo`/`redo` (TR/EN özet), genel `fc call <metot> [<json>|-]` (yalnız public metotlar).
- **Masaüstü:** Svelte 5 arayüz (deste ağacı, klasik not ekleme + sınıflandırma alanları, not listesi, ayarlar), TR/EN i18n, tema token'ları; tek `invoke` IPC + `fc://event` olay akışı; menüler `modules.list`'ten.
- **Faz 2 hazırlığı:** metot kataloğu, hata kodları, repo trait'leri, `MemStore`, modül listesi ve i18n sözlükleri alan/özellik başına dosyalara bölündü; migration'lar klasörden otomatik toplanır ve numara aralıkları testle zorunlu (docs/10 §10).
- **CI:** fmt, clippy `-D warnings`, testler, TS tip tutarlılığı, frontend check/test/build, Windows derlemesi, `fc-core` saflık kontrolü, dependabot.

### Değişti
- `decks.create` isteğe bağlı `parent_id` alır; deste yolunu artık Rust kurar (arayüz birleştirmiyor).
- Masaüstü başlangıç hatası yerel hata penceresinde gösterilir.
- `rust-toolchain.toml` 1.100.0'a sabitlendi.

### Testler
- `store_suite!` (28 servis testi) MemStore, SQLite bellek ve SQLite dosya üzerinde.
- Uçtan uca kabul testi (`fc-app/src/acceptance.rs`) ve gerçek `fc` ikilisiyle CLI testi.
## [0.3.0] — Yayımlanmadı · Kart türleri ve içerik (iz B, `feat/v0.3-cards`)

### Eklendi
- **Workspace:** `fc-core`, `fc-api`, `fc-storage`, `fc-providers`, `fc-app`, `fc-cli` ve Tauri 2 kabuğu `fc-desktop`.
- **fc-core:** saf alan modeli (UUIDv7 kimlikler, `Clock`/`Rng`, deste/not/not türü/kart/etiket), sınıflandırma tanımları + miras çözümü + hazır şablonlar (CEFR, YKS…). G/Ç yok (CI saflık betiği).
- **fc-storage:** SQLite (WAL, tek yazıcı + okuma havuzu), checksum'lı migration'lar (`0001_init`, migrate öncesi yedek), tüm repository'ler, FTS5, deste alt ağacı çöpü, tam geri alınabilir seçenek silme/birleştirme.
- **fc-api:** v1 metot kataloğu (public/internal), hata modeli, olaylar, `gen-ts` ile TypeScript tipleri (`--check` CI'da).
- **fc-app:** komut/sorgu dağıtıcı, registry, oplog + geri al/yinele (iç ters metotlarla birebir), olay yolu, modüller, şemalı ayarlar, iş kuyruğu arayüzü; deste/not/etiket/sınıflandırma/ayar/modül handler'ları.
- **fc-cli (`fc`):** `deck create/list/rename/move/delete/restore`, `note add/list/edit/delete/restore` (`--attr anahtar-veya-ad=değer`), `attribute define/apply-template/set/list/option add|rename|reorder|delete`, `undo`/`redo` (TR/EN özet), genel `fc call <metot> [<json>|-]` (yalnız public metotlar).
- **Masaüstü:** Svelte 5 arayüz (deste ağacı, klasik not ekleme + sınıflandırma alanları, not listesi, ayarlar), TR/EN i18n, tema token'ları; tek `invoke` IPC + `fc://event` olay akışı; menüler `modules.list`'ten.
- **Faz 2 hazırlığı:** metot kataloğu, hata kodları, repo trait'leri, `MemStore`, modül listesi ve i18n sözlükleri alan/özellik başına dosyalara bölündü; migration'lar klasörden otomatik toplanır ve numara aralıkları testle zorunlu (docs/10 §10).
- **CI:** fmt, clippy `-D warnings`, testler, TS tip tutarlılığı, frontend check/test/build, Windows derlemesi, `fc-core` saflık kontrolü, dependabot.

### Değişti
- `decks.create` isteğe bağlı `parent_id` alır; deste yolunu artık Rust kurar (arayüz birleştirmiyor).
- Masaüstü başlangıç hatası yerel hata penceresinde gösterilir.
- `rust-toolchain.toml` 1.100.0'a sabitlendi.

### Düzeltildi (inceleme)
- Not türü CSS'i kapsamdan kaçamaz: `sanitize_css` kaçışları normalleştirir, parantezleri dengeler; frontend dengesiz CSS'i basmaz.
- Silinen cloze numarasının kartı `empty: true` (`cards.render`, `cards.meta`); çalışma atlar.
- Çeldirici havuzu tek sınırlı, tohumlu sorgu (deste boyunca not okuma yok).
- `media.ref_count` okunurken sayılır (kalıcı silmede bayatlamaz); geri alınan `media.add`'in dosyası açılışta temizlenir.
- Bayrak numaralaması Anki'ninki (1 kırmızı … 5 pembe, 7 mor), `card_flag_defs` tek kaynak.
- Kullanılmayan `NoteAddForm` kaldırıldı.

## [0.2.0] — Yayımlanmadı · Çalışma çekirdeği (iz A, `feat/v0.2-study`)

### Eklendi
- **Workspace:** `fc-core`, `fc-api`, `fc-storage`, `fc-providers`, `fc-app`, `fc-cli` ve Tauri 2 kabuğu `fc-desktop`.
- **fc-core:** saf alan modeli (UUIDv7 kimlikler, `Clock`/`Rng`, deste/not/not türü/kart/etiket), sınıflandırma tanımları + miras çözümü + hazır şablonlar (CEFR, YKS…). G/Ç yok (CI saflık betiği).
- **fc-storage:** SQLite (WAL, tek yazıcı + okuma havuzu), checksum'lı migration'lar (`0001_init`, migrate öncesi yedek), tüm repository'ler, FTS5, deste alt ağacı çöpü, tam geri alınabilir seçenek silme/birleştirme.
- **fc-api:** v1 metot kataloğu (public/internal), hata modeli, olaylar, `gen-ts` ile TypeScript tipleri (`--check` CI'da).
- **fc-app:** komut/sorgu dağıtıcı, registry, oplog + geri al/yinele (iç ters metotlarla birebir), olay yolu, modüller, şemalı ayarlar, iş kuyruğu arayüzü; deste/not/etiket/sınıflandırma/ayar/modül handler'ları.
- **fc-cli (`fc`):** `deck create/list/rename/move/delete/restore`, `note add/list/edit/delete/restore` (`--attr anahtar-veya-ad=değer`), `attribute define/apply-template/set/list/option add|rename|reorder|delete`, `undo`/`redo` (TR/EN özet), genel `fc call <metot> [<json>|-]` (yalnız public metotlar).
- **Masaüstü:** Svelte 5 arayüz (deste ağacı, klasik not ekleme + sınıflandırma alanları, not listesi, ayarlar), TR/EN i18n, tema token'ları; tek `invoke` IPC + `fc://event` olay akışı; menüler `modules.list`'ten.
- **Faz 2 hazırlığı:** metot kataloğu, hata kodları, repo trait'leri, `MemStore`, modül listesi ve i18n sözlükleri alan/özellik başına dosyalara bölündü; migration'lar klasörden otomatik toplanır ve numara aralıkları testle zorunlu (docs/10 §10).
- **CI:** fmt, clippy `-D warnings`, testler, TS tip tutarlılığı, frontend check/test/build, Windows derlemesi, `fc-core` saflık kontrolü, dependabot.

### Testler
- `store_suite!` (28 servis testi) MemStore, SQLite bellek ve SQLite dosya üzerinde.
- Uçtan uca kabul testi (`fc-app/src/acceptance.rs`) ve gerçek `fc` ikilisiyle CLI testi.
## [0.1.0] — Yayımlanmadı (inceleme bekliyor)

v0.1 · Temel: her şeyin üstüne kurulacağı iskelet ve API sözleşmesinin ilk hali (docs/04, docs/10).

### Değişti
- `decks.create` isteğe bağlı `parent_id` alır; deste yolunu artık Rust kurar (arayüz birleştirmiyor).
- Masaüstü başlangıç hatası yerel hata penceresinde gösterilir.
- `rust-toolchain.toml` 1.100.0'a sabitlendi.

### Eklendi
- **Workspace:** `fc-core`, `fc-api`, `fc-storage`, `fc-providers`, `fc-app`, `fc-cli` ve Tauri 2 kabuğu `fc-desktop`.
- **fc-core:** saf alan modeli (UUIDv7 kimlikler, `Clock`/`Rng`, deste/not/not türü/kart/etiket), sınıflandırma tanımları + miras çözümü + hazır şablonlar (CEFR, YKS…). G/Ç yok (CI saflık betiği).
- **fc-storage:** SQLite (WAL, tek yazıcı + okuma havuzu), checksum'lı migration'lar (`0001_init`, migrate öncesi yedek), tüm repository'ler, FTS5, deste alt ağacı çöpü, tam geri alınabilir seçenek silme/birleştirme.
- **fc-api:** v1 metot kataloğu (public/internal), hata modeli, olaylar, `gen-ts` ile TypeScript tipleri (`--check` CI'da).
- **fc-app:** komut/sorgu dağıtıcı, registry, oplog + geri al/yinele (iç ters metotlarla birebir), olay yolu, modüller, şemalı ayarlar, iş kuyruğu arayüzü; deste/not/etiket/sınıflandırma/ayar/modül handler'ları.
- **fc-cli (`fc`):** `deck create/list/rename/move/delete/restore`, `note add/list/edit/delete/restore` (`--attr anahtar-veya-ad=değer`), `attribute define/apply-template/set/list/option add|rename|reorder|delete`, `undo`/`redo` (TR/EN özet), genel `fc call <metot> [<json>|-]` (yalnız public metotlar).
- **Masaüstü:** Svelte 5 arayüz (deste ağacı, klasik not ekleme + sınıflandırma alanları, not listesi, ayarlar), TR/EN i18n, tema token'ları; tek `invoke` IPC + `fc://event` olay akışı; menüler `modules.list`'ten.
- **Faz 2 hazırlığı:** metot kataloğu, hata kodları, repo trait'leri, `MemStore`, modül listesi ve i18n sözlükleri alan/özellik başına dosyalara bölündü; migration'lar klasörden otomatik toplanır ve numara aralıkları testle zorunlu (docs/10 §10).
- **CI:** fmt, clippy `-D warnings`, testler, TS tip tutarlılığı, frontend check/test/build, Windows derlemesi, `fc-core` saflık kontrolü, dependabot.

### Değişti
- `decks.create` isteğe bağlı `parent_id` alır; deste yolunu artık Rust kurar (arayüz birleştirmiyor).
- Masaüstü başlangıç hatası yerel hata penceresinde gösterilir.
- `rust-toolchain.toml` 1.100.0'a sabitlendi.

### Testler
- `store_suite!` (28 servis testi) MemStore, SQLite bellek ve SQLite dosya üzerinde.
- Uçtan uca kabul testi (`fc-app/src/acceptance.rs`) ve gerçek `fc` ikilisiyle CLI testi.

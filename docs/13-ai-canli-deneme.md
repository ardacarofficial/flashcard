# 13 — Yapay zekâ gerçek bir sağlayıcıyla (v1.4, G1)

v1.4'e kadar uygulamanın AI'ı hiç gerçek bir sağlayıcıya karşı çalıştırılmamıştı: her şey sağlayıcıların
belgelerini taklit eden yerel bir sahte sunucuya karşı test edilmişti (docs/06 §1.1). Bu belge ilk gerçek
denemeyi kaydeder: ne koşuldu, ne çalıştı, ne bozuktu ve düzeltildi, neyin hâlâ denenmediği.

## Ne koşuldu

- **Tarih / ortam:** 2026-11-23, Linux geliştirme konteyneri. Giden HTTPS, TLS'i yeniden sonlandıran bir vekil
  sunucudan geçer (kendi kök sertifikası işletim sisteminin deposunda); vekil `openrouter.ai` isteklerine gerçek
  bir kimlik bilgisi ekler. Uygulamaya OpenRouter biçiminde **sahte** bir anahtar verildi
  (`sk-or-v1-` + 64 sıfır); gerçek anahtar hiçbir yerde görülmedi, yazılmadı.
- **Sağlayıcı / model:** OpenRouter (ön ayardan), model `google/gemini-2.5-flash-lite` (girdi $0,10 / çıktı $0,40
  milyon token başına; Türkçe iyi, yapılandırılmış çıktıyı destekler). Hız sınırı denemesi için ücretsiz
  `google/gemma-4-31b-it:free`.
- **Yol:** gerçek çekirdek, mock yok. Keşif `fc dev serve` (dev köprüsü, taze SQLite profili, anahtar süreç
  belleğinde — dev köprüsünün kendi deposu; üründeki "anahtar yalnız işletim sistemi anahtar zincirinde" kuralı
  değişmedi) üzerinden, kullanıcının çağırdığı aynı genel yöntemlerle; her sonuç **kaydedildikten sonra** API'den
  geri okundu (`notes.get`, `ai.meaning.get`, `ai.usage.summary`, `jobs.get`).
- **Harcama:** bütün deneme boyunca ~35 ücretli istek (üçü 30 kartlık uzun istek), toplam **< $0,01**.

## Yeniden koşmak

```bash
# 1–2. adımlar, gerçek SQLite koleksiyonunda (~5 istek, < 1 sent)
FC_AI_LIVE=1 [FC_AI_LIVE_OPENROUTER_KEY=sk-or-v1-…] [FC_AI_LIVE_MODEL=…] \
  cargo test -p fc-app --lib live_openrouter -- --ignored --nocapture
# "Bu cümledeki anlam" arayüzden (yolculuk G1; varsayılan takımda atlanır)
FC_AI_LIVE=1 FC_BIN=target/debug/fc pnpm -C apps/desktop e2e -- --grep G1
```

Anahtar verilmezse OpenRouter biçiminde sahte bir anahtar gider (yalnız kimlik ekleyen bir vekil arkasında işe
yarar). İkisi de `FC_AI_LIVE=1` olmadan hiçbir şey göndermez; CI'da çalışmaz.

## Sonuçlar

| Adım | Ne yapıldı | Sonuç |
|---|---|---|
| Sağlayıcı ekle | `ai.providers.list` → `openrouter` ön ayarı → `create` → `set_key` | ✅ `needs_key` → `ready`, ön ayar adresten tanındı |
| Model listesi | `ai.providers.refresh_models` | ✅ 442 model, ~1 sn (ilk denemede **TLS hatası**, bkz. 1) |
| Bağlantıyı dene | `ai.providers.test` | ✅ `ok`, 230–280 ms, cevap veren model adıyla |
| Metinden kart | `ai.preview` → `ai.run` → `ai.notes.add` → `notes.get` | ✅ 3 Klasik not, ön + arka (ilk denemede **arkasız**, bkz. 4) |
| Bu cümledeki anlam | Altyazı notu ("pick up", "Can you **pick up** the kids at five?", kaynağın anlamı "kaldırmak") → önizle → gönder → `ai.meaning.apply` (alana da yaz) | ✅ "birini bir yerden almak", ifade = evet, `phrase` "pick up the kids", kabul edilen cevaplar kaydedildi, alan değişti, `original` = kaldırmak |
| `ai.meaning.suggest` | "runs a small bakery", `ai.meaning.auto_suggest` açık | ✅ öneri "işletmek" olarak saklandı, not alanı aynı kaldı (bir koşuda "ișletmek", bkz. 7) |
| Çalışırken yardım | açıkla (akışlı), ipucu, cevabı kontrol et | ✅ Türkçe; açıklama ara sıra `<explanation>` etiketiyle (bkz. 6) |
| Kullanım paneli | `ai.usage.summary` | ✅ istek/token/özellik/model dökümü; maliyet ilk denemede **$0** (bkz. 2) |
| Arayüz | Yolculuk G1: Ara → Altyazı notu → Enter → "Bu cümledeki anlam" → Öner → önizleme → Gönder → "Kartın anlamına da yaz" → Uygula | ✅ ekrandaki anlam = `ai.meaning.get` = not alanı |
| Yanlış model | `google/no-such-model-g1` | ✅ test: `model_not_found`; istek: önce genel "Sağlayıcı isteği reddetti" (bkz. 5) |
| İptal | 30 kartlık istek, 1,5 sn sonra `jobs.cancel` | ✅ iş `cancelled`; ama AI şeridi **11 sn** kilitli kaldı (bkz. 3) |
| Zaman aşımı | `ai.request_timeout_secs` = 5, uzun istek | ✅ 5,0 sn'de başarısız; önce "Hizmet kullanılamıyor" + `timeout: global` (bkz. 5) |
| Bozuk JSON | "JSON verme, yalnız BANANA yaz" talimatı | Model şemaya uydu (yapılandırılmış çıktı zorlanıyor): bozuk cevap gerçek API'de üretilemedi; onarım yolu kayıtlı cevaplarla test edildi (bkz. 4) |
| Hız sınırı | ücretsiz modele aynı anda 25 istek (doğrudan) | 25 × HTTP 429, `Retry-After` yok; nedeni yalnız `error.metadata.raw`'da (bkz. 8) |

## Bulunan ve düzeltilen hatalar

Her biri ağsız bir testle (kayıtlı ya da kayıttan kurulmuş cevaplarla) yeniden üretilir: `crates/fc-ai/src/tests_openrouter.rs`,
`crates/fc-app/src/modules/ai/tests_openrouter.rs`, `prompts.rs` birim testleri.

1. **TLS: yalnız uygulamaya gömülü kök sertifikalar.** `ureq`'in varsayılanı (webpki listesi) işletim sisteminin
   deposundaki kökleri görmez. HTTPS'i denetleyen bir antivirüs ya da şirket vekili (Windows'ta yaygın) kendi kökünü
   oraya koyar: tarayıcı çalışır, uygulama **her sağlayıcıda** "güvenli bağlantı kurulamadı" der. Düzeltme:
   `platform-verifier` — sertifika işletim sisteminin deposuyla doğrulanır (Windows, macOS, Linux), sayfa okuyucu
   (`sources.rs`) dahil.
2. **OpenRouter'da maliyet hep $0.** Maliyet yalnız fiyat tablosundan hesaplanıyordu; OpenRouter'ın yüzlerce modeli
   için satır yok → kullanım paneli $0, aylık bütçe hiçbir şeyi durdurmuyordu. OpenRouter her cevapta gerçek
   ücreti bildirir (`usage.cost`, akışlı cevapların son parçasında da). Düzeltme: `Usage.cost_usd`
   (`fc-providers`, isteğe bağlı alan), OpenAI uyumlu bağdaştırıcı okur, `ai.usage.record` (`cost_usd`, iç
   yöntem) ve `AiJobResult.cost_usd` sağlayıcının rakamını kullanır, yoksa fiyat tablosu.
3. **İptal AI şeridini serbest bırakmıyordu.** İş `cancelled` görünüyordu ama HTTP çağrısı (en çok zaman aşımı kadar)
   bitene kadar şerit doluydu: iptalden sonraki ipucu 11 sn bekledi. Düzeltme: istek kendi iş parçacığında, iş
   50 ms'de bir iptali yoklar ve hemen biter (sonra: 1,0 sn). Bırakılan çağrının cevabı atılır; token'ları
   kaydedilmez (sağlayıcı yine de ücretlendirebilir — bilinen sınır).
4. **Arkasız kartlar kabul ediliyordu.** Klasik'te yalnız `front` zorunlu (sıralama alanı); şema `back`'i isteğe
   bağlı bırakıyordu ve model cevabı ön yüze gömdü (`"front": "What is a 'spoiler'? (Arka: …)"`), uygulama üç
   arkasız not önerdi. Düzeltme: kart üretmede kalıbın sorduğu alanlar (`answer_field`) zorunlu; eksikse cevap
   bir kez onarıma gider. Onarımdan sonra da bozuksa mesaj artık ne yapılacağını söyler.
5. **Yanlış model ve zaman aşımı anlaşılmıyordu.** "Sağlayıcı isteği reddetti (builtin.ai.openai_compatible: HTTP
   400: …)" ve "Hizmet kullanılamıyor (… timeout: global)". Düzeltme: yeni hata kodları
   `PROVIDER_AI_MODEL_NOT_FOUND` ("Sağlayıcı bu modeli tanımıyor … başka bir model seç") ve `PROVIDER_AI_TIMEOUT`
   ("Sağlayıcı zamanında cevap vermedi … bekleme süresini uzat"), TR/EN; `details.reason` artık bağdaştırıcının iç
   kimliği olmadan sağlayıcının kendi sözü.
6. **Etiketle sarılmış düz metin.** Açıklamaların yaklaşık yarısı `<explanation>…</explanation>` içinde geldi
   (istemdeki `<card>` etiketlerini taklit); ekranda etiket görünürdü. Düzeltme: tek bir dış etiket soyulur.
7. **Türkçe harfler.** Model bir kez "ișletmek" (Romence ș, U+0219) yazdı, bir kez de "cocuk", "ogrenmek"
   (aksansız). Kabul edilen cevap böyle saklansaydı "işletmek" yazan yanlış sayılırdı. Düzeltme: hedef Türkçeyse
   ș/Ș → ş/Ş; istem doğru harfleri ister (aksansız yazımı ikinci koşuda görülmedi). İstem ayrıca "sen" diye hitap
   ister (ipucu "düşünün" demişti).
8. **OpenRouter'ın sardığı hata.** Üst sağlayıcı hatalarında `error.message` yalnız "Provider returned error"; neden
   `error.metadata.raw`'da. Düzeltme: ikisi birlikte gösterilir (bağlantı testi dahil).

## Bilinen sınırlar ve denenmeyenler

- **Diğer sağlayıcılar gerçek API'ye karşı denenmedi:** Claude (Anthropic Messages API), OpenAI, Gemini, Groq,
  Mistral, DeepSeek, Ollama, LM Studio. Bu konteynerde yalnız OpenRouter'a erişim vardı. OpenRouter OpenAI uyumlu
  yolu kullandığından Gemini/Groq/Mistral/DeepSeek/LM Studio'nun **istek biçimi** sınandı sayılabilir, cevap/hata
  biçimleri değil. Claude yerel yolu (araçla yapılandırılmış çıktı) yalnız sahte sunucuya karşı test edildi.
- **Gerçek bozuk JSON** üretilemedi (OpenRouter `response_format`'ı zorluyor); onarım yolu kayıtlı cevaplarla.
- **Hız sınırı** uygulama üzerinden değil, doğrudan 25 eşzamanlı istekle görüldü; uygulamanın 429 eşlemesi ve
  yeniden denemesi kayıtlı cevapla test edildi. OpenRouter `Retry-After` göndermedi: uygulama artan beklemeyle
  dener.
- **Görsel girdi**, PDF/web kaynağından kart, `fill_fields`, `improve`, `rewrite_leech`, `distractors` gerçek
  sağlayıcıyla denenmedi (aynı istek/ayrıştırma yolu).
- **İptal edilen ya da zaman aşımına uğrayan isteklerin ücreti** bilinmiyor: OpenRouter bağlantı kopunca üretimi
  durdurabilir ya da ücretlendirebilir; uygulama bu çağrıları kaydetmez.
- **Önizlemede maliyet tahmini** OpenRouter modelleri için hâlâ "bilinmiyor" (fiyat tablosu boşsa). OpenRouter'ın
  model listesi fiyatları da verir; bunları fiyat tablosuna almak sonraki iş.
- **Windows'ta** platform doğrulayıcı derlenmedi/çalıştırılmadı (Windows CI pilotu ve elle koşu bekliyor).

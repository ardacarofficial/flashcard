# 14 — Subtitle eklentisi gerçek bir tarayıcıda (v1.4 G2)

Şimdiye kadar yalnız eklentinin istemci kodu (`src/flashcards/api.ts`, `outbox.ts`) Node'da gerçek `fc api serve`'e karşı koşuyordu (`tools/subtitle-contract`, docs/06 §5.1). Bu doküman, eklentinin **derlenmiş hâlinin** gerçek Chromium'a yüklenip sahibin yaptığı gibi kullanıldığı denemeyi anlatır: kurulum, geçenler, bulunanlar, eklenti deposunda gereken değişiklik ve hâlâ sahibin kendi tarayıcısını isteyenler.

Betik: `tools/subtitle-real-browser/run.mjs`. Varsayılan CI'da değildir; eklenti deposu ya da Chromium yoksa `SKIP:` yazıp 0 ile çıkar.

```bash
cargo build -p fc-cli
node tools/subtitle-real-browser/run.mjs            # eklentiyi kopyadan derler (git clone + pnpm install + pnpm build)
# isteğe bağlı: FC_BIN=… SUBTITLE_REPO=/home/user/subtitle SUBTITLE_DIST=<hazır dist> CHROMIUM=/opt/pw-browsers/chromium
#               SHOTS=<klasör> (ekran görüntüleri)  REPORT=<dosya.json>  KEEP=1 (geçici profili silme)
```

Her adım sonucu Flashcard'ın API'sinden (`fc call`) **kaydedilmiş hâliyle** geri okur; sonunda bir JSON rapor yazar (toast metinleri, tanı satırları, dışarıya giden ve kesilen istekler, konsol hataları).

## 1. Kurulum: ne gerçek, ne değil

| Parça | Bu denemede | Gerçek kullanımda |
|---|---|---|
| Eklenti | `/home/user/subtitle` deposunun **kopyası** (`git clone`, depoya dokunulmaz), `pnpm build` çıktısı `dist/` — değiştirilmeden `--load-extension` ile yüklenir (sürüm 3.1.3, commit `7fe2843`) | aynı paket |
| Arka plan (servis çalışanı), içerik betikleri, YouTube'un MAIN-world sayfa betiği (`assets/js/youtube.js`), ayarlar sayfası | gönderilen kod, olduğu gibi | aynı |
| Tarayıcı | Playwright `launchPersistentContext`, `/opt/pw-browsers/chromium` (yeni başsız), `tr-TR` | Chrome |
| Flashcard | gerçek `fc --db <geçici> api serve --port <boş port>`; `subtitle` modülü açık, anahtar `subtitle.keys.create` ile (uygulamadaki Subtitle ekranının çağırdığı komut) | masaüstü uygulaması aynı sunucuyu `local_api.port`'ta (28735) açar |
| Video sayfası | `https://www.youtube.com/watch?v=fcRealTest1` — **test tarafından sunulan küçük bir taklit** (Playwright o adresi yanıtlar): `#movie_player.html5-video-player`, `<video>` (eklenti deposunun `playground/public/media/sample.webm`'i, bayt aralıklarıyla), CC düğmesi açık, oynatıcı altyazıyı YouTube gibi `/api/timedtext?…&fmt=json3` ile ister | gerçek YouTube |
| Çeviri | Google Çeviri'nin yanıtları **hazır** (ağ yok): `batchexecute` (sözlük), `translate_a/single` (satır ve ikinci satır) | gerçek Google / cihazdaki sözlük |
| `chrome.permissions.request` ("Kaydet"in sunucu izni sorusu) | tarayıcının yerel izin penceresine otomasyon tıklayamaz; ayarlar sayfasında bu tek çağrı **"izin verilmedi"** yanıtı veren ve soruyu kaydeden bir yerine-koyucuyla değiştirildi (hiçbir izin verilmiş gibi yapılmaz) | sahibi "İzin ver"e basar |

Neden YouTube taklidi: eklenti içerik betiğini yalnız `manifest.js`'teki `MATCHES` adreslerine (hepsi `https://` gerçek siteler) enjekte eder; `sites.ts`'te yerel bir adresi tanıyan tek yol Jellyfin'in `/web/` yoludur ama o da `MATCHES`'te değildir (isteğe bağlı izinle sonradan enjekte edilir) ve "Deneysel"dir. Playground ise eklentiyi uzantı çalışma zamanı olmadan (`chromeShim`) çalıştırır, yani gönderilen yol değildir. Tarayıcının kendisi `www.youtube.com` adresini açtığı için eklentinin **gerçek YouTube yolu** (MAIN-world betik `fetch`'i yakalar, `esYoutubeCaptionsData` → `Youtube` servisi → `getSubs`) baştan sona çalışır; taklit olan yalnız YouTube'un sayfası ve sunucusudur.

Dışarıya hiçbir şey gitmez: yerel geri döngü dışındaki her istek ya yukarıdaki taklitlerle yanıtlanır ya da kesilip rapora yazılır. Kesilenler: `fonts.googleapis.com` (ayarlar sayfasının yazı tipi) ve `github.com/Nitrino/easysubs/releases/…/en-tr.json.gz` (eklenti en→tr Wiktionary sözlüğünü kendiliğinden indirmeye çalışıyor; inemeyince kelime Google'a düşüyor, bu yüzden yakalamanın `definition.source`'u `dict`).

## 2. Sahibin yaptığı gibi: adımlar ve sonuç

13 adımın hepsi geçti (betik birkaç kez koştu, biri sıfırdan `git clone` + `pnpm install` + `pnpm build` ile; son koşu 13/13).

| # | Adım (eklentide) | Eklentinin kendi arayüzü | Flashcard'dan geri okunan (kalıcı) |
|---|---|---|---|
| 1 | Ayarlar → Öğrenme servisi → **Subtitle Kartları**; Sunucu adresi `http://127.0.0.1:<port>/subtitle`, Erişim anahtarı yapıştırılır, **Kaydet**, **Bağlantıyı test et** | "Bağlandı: Chromium · 0 deste"; Kaydet'in notu: "Kaydedildi, ama sunucuya erişim izni verilmedi. Sunucu CORS ile izin vermiyorsa kartlar gönderilemez." (izin penceresi yerine-koyucusu yüzünden; bkz. §3) | `/subtitle/v1/me` ve `/decks` eklentinin kökeninden (`chrome-extension://…`) CORS ile geçer |
| 2 | Video oynar, 1. satır "I just need to pick up my **keys**." → kelimenin üzerine gelinir → **+** | toast **"Karta eklendi"** | tanı: `created`; **not + 1 kart** (`Subtitle` destesi kendiliğinden); `word`=`key` (eklentinin sözlük hâli), `surface`=`keys`, `meaning`=`anahtarlar`, `sentence` kelime **kalın**, `translation`=satırın çevirisi, `image`=`media:<sha256>` (videonun o karesi, JPEG, köken `original`), `source`="The Night Train · 0:01" + `…watch?v=fcRealTest1&t=1s` bağlantısı; karşılaşmada `video_id`, `site=youtube`, `t_start_ms/t_end_ms` = satırın zamanları, `term_offset` |
| 3 | 2. satır "Where did you put the **keys**?" → **+** | "Karta eklendi" | tanı: **`attached`**; aynı lemma için **2 karşılaşma, tek not, tek kart** (kimliği aynı), tam biri birincil; kart ilk sahneyi gösterir, ikinci sahne `examples` alanında (cümle, çeviri, kendi resmi, 0:07 bağlantısı) — `subtitle.same_word` varsayılanı |
| 4 | Deyim: "pick" üzerine gelinir → açılır pencere **"pick up"** → **+** | "Karta eklendi" | ayrı not `word`=`pick up`, `kind`=`phrase`, cümlede "**pick up**" kalın |
| 5 | Flashcard **kapalı** (sunucu durdurulur) → 3. satırda "lights" → **+** | "Kuyruğa alındı, birazdan yeniden denenecek" | hiçbir şey yazılmaz |
| 6 | Flashcard **yeniden başlar** (aynı port); hiçbir şeye tıklanmaz | — | kelime 17–27 sn içinde kendiliğinden gelir (eklentinin kuyruk alarmı; Chrome alarmı en erken 30 sn sonraya kurar); `light` notu |
| 7 | Yeniden başlatmadan sonra 4. satırda "train" → **+** | "Karta eklendi" | hemen yazılır |
| 8 | Ayarlarda **yanlış anahtar** → Kaydet → Bağlantıyı test et; sonra "station" → **+** | test: "Erişim anahtarı geçersiz ya da süresi dolmuş (401): the access key is not valid (invalid)"; toast: "Subtitle Kartları: …(401)…"; Kuyruk kartında **Gönderilemeyenler: station** + Tekrar dene / Sil | hiçbir şey yazılmaz |
| 9 | Doğru anahtar geri → Kaydet → test "Bağlandı" → **Tümünü tekrar dene** | kuyruk boşalır | `station` notu gelir (kelime kaybolmadı) |
| 10 | Anahtar uygulamada **iptal edilir** (`subtitle.keys.revoke`) → "need" üzerine gelinir (eklenti bunu "need to" deyimi olarak açar) → **+** | "Subtitle Kartları: … (401): the access key is not valid (revoked)" | hiçbir şey yazılmaz; kelime eklentinin kuyruğunda `failed` olarak bekler (Tekrar dene için) |
| 11 | Bilinen kelimeler (eklenti bugün sormuyor; §4): eklentinin **kendi servis çalışanından**, kayıtlı anahtarıyla `/v1/me` + `/v1/known-words?lang=en` | — | paylaşım kapalıyken `features.known_words=false`, `words: []`; `subtitle.share_known_words` açılınca `true` ve yakalanan lemmalar (`key`, `pick up`, `light`, `train`, `station`) `learning` olarak |

Ses: eklenti satırın sesini yalnız Media Source Extensions ile oynayan oynatıcıların tamponundan keser (`src/audio/bufferedAudio.ts`); düz dosya oynatan taklit sayfada ses yoktur, ayrıca "Ses" seçeneği eklentide **varsayılan kapalı**. Bu denemede ses gönderilmedi; ses yükleme yolu (`/v1/media` `audio/wav`, bayt bayt saklama) `tools/subtitle-contract` ile eklentinin kendi istemcisinden zaten sınanıyor.

## 3. Bulunanlar

**Flashcard tarafında hata bulunmadı**; Rust'ta düzeltilecek bir şey çıkmadı. Ayrıntılı bakılıp hata olmadığı görülenler:

- Çeviri alanında Markdown kaçışı (`[tr] …` → alan metninde `\[tr\] …`): kart çiziminde (`cards.render`) köşeli parantezler düzgün `[tr] …` görünür; YouTube'un `[Music]`, `[laughs]` satırları için de doğru davranış.
- İkinci yakalamanın telde `201 created` + **var olan** `card_id` dönmesi: eklenti bunu "Karta eklendi" diye gösterir — doğru (yeni kart yok ama kelime saklandı).
- Uygulama kapalıyken eklentinin dediği "birazdan yeniden denenecek" (tarayıcı çevrimiçi olduğu için "çevrimdışı" değil) ve yeniden açılınca kendiliğinden teslim: kayıp yok, kopya yok.

Gözlemler (hata değil, sahibin bileceği şeyler):

- **İzin penceresi:** "Kaydet" `chrome.permissions.request({origins: ["http://127.0.0.1/*"]})` sorar. Bu denemede izin verilmedi ve her şey yine çalıştı: Flashcard eklenti kökenlerine CORS ile izin veriyor. Sahibi "İzin ver"e basarsa istekler CORS'suz gider; Flashcard `Origin`'i yine denetler, sonuç aynı olmalı (bu yol yalnız gerçek tarayıcıda denenebilir).
- **Yeni profilde "0 deste":** ilk yakalamadan önce Flashcard'da deste yoksa eklenti deste seçicisini göstermez; kelimeler `Subtitle` destesine (ya da uygulamadaki Subtitle ekranında seçilen varsayılana) gider. Sonra seçici görünür.
- **Hata metinleri yarı İngilizce:** eklenti sunucunun `error.message`'ını Türkçe ön ekinin ardına ekler ("…(401): the access key is not valid (revoked)"). Flashcard'ın API mesajları bilinçli olarak İngilizcedir; `invalid`/`revoked` ayrımı işe yarıyor. Türkçeleştirmek eklentide `error.code`'a göre yapılmalı (aşağıda).
- **Konsol:** sayfada ve arka planda hata yok; ayarlar sayfasında yalnız kesilen yazı tipi isteğinin iki `ERR_FAILED`'ı.
- **Kelimelerde sözcük türü boş:** Google sözlük yanıtında tür olduğu hâlde kelime yakalamalarında `term.pos` `null` geliyor (deyimde `phrase`). Eklenti tarafı; Flashcard boş alanı olduğu gibi saklar.

## 4. Eklenti deposunda gereken değişiklik: Flashcard'dan bilinen kelimeler

Eklenti bugün `known-words`'ü hiç çağırmıyor: istemcide `FlashcardsApi.knownWords(lang, cursor)` (`src/flashcards/api.ts`) var ama kimse kullanmıyor. `subtitle.share_known_words` açıkken altyazıdaki kelimelerin Flashcard'a göre boyanması için, Anki'nin "Anki'den oku" yolunun (`syncAnkiFx`) aynısı gerekir:

1. `src/flashcards/messages.ts` — yeni istek `{ action: "knownWords"; lang: string }`, yanıt `{ shared: boolean; words: TKnownWord[] }`.
2. `src/flashcards/background.ts` `createFlashcards().handle()` — `case "knownWords"`: kayıtlı sunucuyla (`persist:flashcardsServer`) önce `api.me()` (`features.known_words === false` → `{ shared: false, words: [] }`: "Flashcard'da paylaşım kapalı" diyebilmek için; sunucunun `shared` alanını istemci tipi `TKnownWord`/yanıt tipi okumaz), sonra `next_cursor` bitene kadar `api.knownWords(lang, cursor)` sayfaları (500'er). İstek arka planda olmalı: anahtar ve CORS orada.
3. `src/models/words/index.ts` — `flashcardsSyncRequested` + `syncFlashcardsFx`, `syncAnkiFx`'in kopyası: `status` doğrudan sunucudan (`known`/`learning`; Flashcard zaten `words.known_interval_days`, varsayılan 21 gün, ile karar verir), anahtar `wordKey(language, lemma)` (Flashcard lemması zaten sözlük hâli; Anki yolundaki `normalizeWord` + `requestWordRanks` eşlemesi yine uygulanmalı), kullanıcının kendi "biliyorum" işareti korunur, `anki: true` yerine yeni bir `flashcards: true` işareti.
4. `src/utils/words.ts` `TSavedWord` — `flashcards?: boolean`; `WordStatus.tsx`'teki `markedStatus` bu işareti Anki'ninki gibi saymalı.
5. `src/pages/content/components/Settings/Words.tsx` (satır ~139, "Anki'den oku") — öğrenme servisi `subtitle-cards` iken **"Flashcard'dan oku"** düğmesi; istenirse video açılışında kendiliğinden (günde bir).
6. Hata metinleri için: `src/flashcards/api.ts`'te 401/403 mesajı `error.code`'a göre Türkçe kurulabilir (sunucu `invalid`/`revoked` ayrımını mesajda veriyor).

Flashcard tarafı hazır ve bu denemede eklentinin kökeninden doğrulandı (§2 adım 11). Uygulama tarafında değişiklik gerekmez.

## 5. Hâlâ sahibin kendi tarayıcısını isteyenler

- **Gerçek YouTube / Netflix**, hesabıyla: gerçek oynatıcının altyazı isteği (imzalı `timedtext`, `pot` belirteçleri), Netflix'in oynatıcı API'si, reklam araları; DRM'li Netflix'te ekran görüntüsü (çerçeve alınamayabilir) ve ses (DRM sesi çözülmez).
- **"İzin ver" penceresi:** Kaydet'te çıkan izin sorusuna "İzin ver" denince bağlantı testi ve "+".
- **Ses:** "Ses" açıkken YouTube'un MSE tamponundan kesilen satır sesinin karta gelmesi.
- **Cihazdaki sözlük ve Chrome'un çevirmeni:** gerçek Chrome'da en→tr sözlük iner, kelime anlamları ondan gelir (`definition.source` `dict`); Chrome'un yerleşik çevirmeni satır çevirisini yapar.
- **Masaüstü uygulamasıyla:** sunucuyu `fc api serve` değil uygulama açar (port 28735, Ayarlar → Bağlantılar → Subtitle'daki adres); uygulama kapatılıp açılınca kuyruğun kendiliğinden boşalması.

Elle deneme listesi: Ayarlar → Bağlantılar → Subtitle'da anahtar oluştur → eklentide Öğrenme servisi = Subtitle Kartları, adres + anahtar, Kaydet (İzin ver), Bağlantıyı test et → YouTube'da bir kelimeye **+** → uygulamada Kelimelerim'de kelime, kartta cümle/resim/kaynak → aynı kelimeyi başka bir sahneden **+** → tek kart, iki örnek → uygulamayı kapat, **+**, aç → kelime gelir.

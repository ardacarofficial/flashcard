# 11 · Tasarım (v1.2)

v1.2 arayüzü akışlardan başlayarak yeniden tasarlar. **Hiçbir özellik, ekran, ayar, kısayol ya da modül anahtarı kaldırılmadı; davranış ve Rust API'si değişmedi** (CLAUDE.md kural 1). Bağlayıcı tasarım yönergesi `.orchestra/DESIGN-BRIEF.md`'dir; bu doküman yönergenin değil, **koddaki gerçek hâlin** kaydıdır.

## 0. v1.1 neden değişti

v1.1 Quizlet esinli, ferah bir arayüzdü: büyük yuvarlak kartlar, bol boşluk, açık gri + mor, kart eklemek için ağır bir form sayfası. Proje sahibi bunu açıkça reddetti: "kullanılmıyor, yapmış olmak için yapılmış, kart eklemek berbat, uygulamada rahat edemiyorum." Dürüst teşhis: v1.1 **ekranları süsledi ama akışları düşünmedi.** Her sayfa "buraya ne koyabiliriz" sorusuyla doldu; en sık iş olan kart eklemek tam bir sayfa ve fare gerektiriyordu.

v1.2 aynı tema altyapısının üstünde koyu öncelikli, sık, klavye öncelikli bir araç: Linear / Raycast / iyi bir kod editörü gibi *içinde yaşanan* bir uygulama, kâğıt dizin kartlarının sıcaklığıyla. Sahibin diğer uygulaması (Kelime) bir referanstı; kopyası değil, ondan iyisi hedeflendi.

## 1. İlkeler

1. **Önce akış.** Her ekranın testi: *kullanıcı buraya ne yapmaya geliyor ve bu kaç tuş/tıklama sürüyor?* "Bu sayfaya ne koyabiliriz" değil.
2. **Klavye öncelikli, fare de tam çalışır.** Sık işlerin hepsi tek tuşla başlar (`N`, `Ctrl+K`, `/`, `Enter`); kısayollar ekranda küçük mono ipucu olarak görünür.
3. **Sık ama sakin.** 13 px gövde, 34 px satır, 1 px kenarlık ve yüzey basamakları; gölge yok, gradyan yok, parıltı yok, krom içinde emoji yok.
4. **Tek vurgu: kehribar.** Vurgu yalnız birincil eylemi, etkin öğeyi ve odağı işaretler. Anlamlı renkler (doğru/yanlış/uyarı) yalnız anlam taşıdıklarında.
5. **Sade ≠ tasarımsız** (kural 9). Aşamalı gösterim: ikincil ayrıntılar "Diğer" altında, satır eylemleri üzerine gelince/odakta.

| Akış | Ne yapılır | Tuş |
|---|---|---|
| Kart ekle | Herhangi bir yerde `N` → ön alan → `Tab` → arka alan → `Ctrl+Enter` (pencere açık kalır, sıradaki karta geçilir) | 1 + yazı + 2 |
| Bugünü çalış | Ana sayfada `Enter` → başlatıcıda `Enter` (v1.3) → `Space` çevir → `1`/`2` (ya da `1`–`4`) | 2 + kart başına 2 |
| Kartı bul ve düzelt | `Ctrl+K` ya da Ara'da `/` → sorgu → `↓` → `Enter` düzenle → `Ctrl+Enter` kaydet | 4 + yazı |
| Toplu ekle | Oluştur → satırlarda `Tab`/`Enter` → `Ctrl+Enter` tek geri al adımı | yazı + 1 |
| İlerlemeyi gör | Kenar çubuğunda İstatistik | 1 tıklama |

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

**Kenar çubuğu** (`shell/AppSidebar.svelte`, 216 px): uygulama işareti + ad; bölümler 30 px'lik sık satırlar: **Ana sayfa, Kütüphane, Oluştur, Ara, İstatistik**. "Oluştur" artık vurgulu bir blok değil, sıradan bir öğe (kart eklemek üst çubuğun işi). Altta **Gelen kutusu** (bekleyen öğe sayısı mono), **Ayarlar**, profil menüsü. Etkin öğe `--surface-3` zemin + vurgu renkli simge. Menüler hâlâ `modules.list`'ten gelir (kural 7); modül → bölüm/grup eşlemesi tek yerde: `shell/navigation.ts` (`PLACEMENTS`). Kapalı modül her yerden (palet dahil) kaybolur.

**Üst çubuk** (`shell/TopBar.svelte`, 44 px, çalışma oturumu dışında **her zaman** görünür): solda iz (bölüm › sayfa, alt sayfada ekranın eklediği kırıntılar; `PageHeader` `back` ile otomatik), sağda:
- **"Ara ya da komut çalıştır"** alanı (`Ctrl K` ipucu): komut paletini açar; palet modülü kapalıysa Ara sayfasına gider.
- Birincil **"Kart ekle"** düğmesi (`N` ipucu): Hızlı Ekle'yi açar.

Bu girişler `shell/commands.svelte.ts`'te tek fonksiyondur: `openQuickAdd()` ve `openPalette()`. Uygulayan taraf kendini kaydeder (`setQuickAddHandler`, `setPaletteHandler`); çağıranlar hiçbir ekranı içe aktarmaz. Hızlı Ekle kayıtlı değilse (notlar modülü kapalı/yüklenmedi) kabuğun yedeği Oluştur sayfasını açar.

**Komut paleti** (`Ctrl+K`): önce Eylemler (Kart ekle, Ara, Geri al, Yinele), sonra ekranlar, sonra modüllere göre gruplu komutlar; kısayollar sağa hizalı. Taşınan her şey paletten bulunur.

**Ayarlar** tek sayfa (v1.3): solda üstte **arama kutusu**, altında görev odaklı gruplar — en sık kullanılan önce: Genel (genel, hatırlatıcılar, kısayollar, profiller ve kilit) · Çalışma ve kartlar (deste ayarları, not türleri, modlar ve okuma) · Bağlantılar (yapay zeka, **Subtitle**) · Veri (içe/dışa aktarma, başlangıç desteleri) · ayrı çizgiyle **Gelişmiş** ("teknik ayarlar: çoğu zaman gerekmez": otomasyon, MCP, yerel API, eklentiler, bakım, günlükler ve güncellemeler). 1024 px altında alt menü tek bir Select olur, arama kutusu üstünde kalır; arama sürerken sonuçlar liste olarak görünür. Arama yalnız sunumdur: sayfa adları + her sayfanın gösterdiği i18n alt ağaçları (`core/kit/settings-index.ts`) içinde tüm kelimeleri aynı metinde arar, eşleşen sayfanın altında o metni gösterir; açık sayfada eşleşen `FormRow` vurgulanır, ilk eşleşmeye kaydırılır. Genel sayfası "Ne değiştirmek istiyorsun?" sorularıyla başlar (her biri ilgili sayfayı açar).

Ayar sayfaları kutusuz (`features/core/kit`): `SettingsPage` (başlık + **tek düz cümle**: bu sayfa ne için, ne zaman dokunursun), `SettingsSection` (14 px başlık + gövde boyunda açıklama), `FormRow` (14 px etiket; **13 px açıklama** "bunu değiştirirsem ne olur?" sorusunu cevaplar; isteğe bağlı `example` → "Örnek: Günde 20 yeni kart → …"; `isDefault` → küçük "varsayılan" işareti; `onReset` + değişmiş değer → "Varsayılana dön" (`settings.reset`, geri alınabilir); ≤ 900 px pencerede ya da 560 px altı sütunda kontrol etiketin altına iner), `SettingsAdvanced` (sayfa başına "Gelişmiş" açılır bölüm: kapalı başlar, durumu cihazda hatırlanır, arama sürerken kendiliğinden açılır), `LoadState`; yıkıcı eylemler sayfanın sonunda `tone="danger"` bölümde ve onaydan önce mevcut kartlara tam olarak ne olacağını söyler.

**Sayfa kabı:** tek kap (`App.svelte` `.page`), kenar çubuğunun yanında sola hizalı, `--content-max` 1120 px, `--content-pad-x` 24 px (1024 altında 16); tablo/grafik sayfaları (Ara, İstatistik) `ScreenDef.width: 'wide'` → 1440 px. Sayfa başlığı her zaman ilk satırdır; deste seçici ve bölüm sekmeleri kabuk tarafından ekranın `PageHeader`'ına verilir (`page-chrome.ts`).

## 3. Dört temel akış (uygulandığı hâliyle)

### 3.1 Kart ekle — Hızlı Ekle ve toplu ızgara

**Hızlı Ekle** (`features/quickadd/`): `N` (yazı alanında değilken, açık bir iletişim kutusu yokken, oturum sürmüyorken), `Ctrl+N`, üst çubuktaki "Kart ekle", paletteki eylem ya da terim listesindeki "Kart ekle" açar. Pencerede:
- Deste ve not türü seçicileri (ikisi de hatırlanır; deste sayfasında açılırsa o deste),
- not türünün alanları sade girişler olarak `Tab` sırasıyla (zorunlu alan işaretli),
- etiketler (virgül/boşlukla),
- kapalı **"Diğer"**: sınıflandırma ve önizleme.

`Enter` sonraki alana geçer, son alanda ekler (`Shift+Enter` yeni satır); `Ctrl+Enter` her yerden ekler. Ekleme **pencereyi açık tutar**, alanları temizler, imleci ilk alana koyar ve "Eklendi · 3" sayacını **Geri al** ile gösterir (uygulamanın normal `oplog.undo`'su). Yinelenen kart uyarısında "Yine de ekle". `Esc` kapatır; yazılmış metin varsa ilk `Esc` sorar, ikincisi kapatır. Altta "Toplu ekle" Oluştur sayfasına geçer. Her kart sıradan bir `notes.create` komutudur. Pencerenin kodu ilk açılışta yüklenir (ilk paket bütçesi).

**Toplu ekleme ızgarası** (`features/notes/CreateScreen.svelte`, Oluştur): satırlardan önce büyük form yok. Tek sık kurulum satırı (yeni/var olan deste, başlık ya da deste, üst deste, not türü; açıklama ve sınıflandırmalar "Diğer" altında), sonra numaralı **terim | tanım** satırlarından oluşan tablo benzeri ızgara. `Tab` satırlar boyunca alandan alana, son satırın son alanında `Enter` yeni satır, `Ctrl+Enter` kaydeder. Kaydet **tek bir `notes.create_many` komutu** = tek geri al adımı (yeni deste dahil). "Yapıştırarak içe aktar" penceresi (v0.5 ayırıcı algılama) ve "AI ile üret" (modül açıksa) aynı sayfadan.

### 3.2 Bugünü çalış — Ana sayfa → oturum

**Ana sayfa** (`features/home/HomeScreen.svelte`) "şimdi ne yapayım" sorusunu cevaplar:
- İlk satır: "Bugün · **141** kart" ve mono kırılım (Vadeli · Öğreniliyor · Yeni), sağda birincil **"Çalışmaya başla"** (`Enter`).
- Gelen kutusunda bekleyen varsa tek ince satır.
- Kompakt istatistik şeridi: seri, bugün cevaplanan, hedef.
- "Bugün çalışılacak desteler": sık tablo (deste, vadeli / öğreniliyor / yeni mono, ustalık çubuğu + yüzde); satır deste sayfasını açar, satır başına "Çalış".
Tüm sayılar backend'den (`decks.summaries`, `stats.overview`, `goals.list`, `inbox.count`); modülü kapalı parça çizilmez.

**Çalışma oturumu:** oturum sürerken **kabuk tamamen kaybolur** (`ctx.setFocus(true)`): kenar çubuğu, üst çubuk, deste seçici ve sekmeler gizlenir. `features/modes/SessionFrame.svelte` tek ince üst satır çizer: menü düğmesi (kenar çubuğunu geri getirir, `html.fc-session-nav`), mod ve deste, ilerleme çubuğu, mono "12 / 40", kalan kırılım, ekstralar ve çıkış (`Esc`; Test atmadan önce sorar). Kartlar ekranında kart ortada (en çok 720 px, köşe 12), **cevap düğmeleri tam kart genişliğinde**: varsayılan 2'li (**Hâlâ öğreniyorum** = Tekrar `1`, **Biliyorum** = İyi `2`) ya da 4'lü (Tekrar/Zor/İyi/Kolay, mono sonraki aralıklarla; `study.answer_buttons`). Altında sessiz araç satırı (ipucu, geri al, bayrak, odak, daha fazla) ve küçük mono kısayol ipuçları. `F` odak modu tam ekran ve gizli sayaçlar ekler. Öğren / Test / Eşleştir aynı çerçeveyi kullanır: sık başlangıç panelleri (`SetupPanel`, `OptRow`), büyük seçenekler (`OptionTile`, `1`–`4`), renk + simge + metinle geri bildirim, Test sonunda puan halkası + yanlışlar tablosu, Eşleştir'de mono süre/en iyi süre. Oturum özeti sık, sayılar mono.

### 3.3 Kartı bul ve düzelt — Kütüphane, deste sayfası, Ara

**Kütüphane** (`features/decks/LibraryScreen.svelte`): sekmeler **Desteler | Notlar**. Varsayılan görünüm sık **ağaç-tablo** (`ListRow`, 34 px): ad, kart, vadeli, yeni, ustalık, sınıflandırma çipleri; satır eylemleri (çalış, kart ekle, ⋯) üzerine gelince/odakta; ok tuşlarıyla gezinme, katlama hatırlanır. Eski kart ızgarası ikincil görünüm olarak kalır (`fc.library.view`). "Yeni deste". Deste açılınca deste sayfası Kütüphane içinde gösterilir; geri bağlantısı ve üst çubuk izi geri döner.

**Deste sayfası** (`features/decks/DeckPage.svelte`), ekranın üst yarısında: başlık + sayılar + çipler + ⋯ menüsü; tek eylem çubuğu: birincil **"Çalış"** (`Enter`) ve çalışma modları kompakt düğmeler olarak (Kartlar, Öğren, Test, Eşleştir, Okuma; modülü kapalı olan yok). Altında daha küçük çevrilebilir önizleme kartı (`←`/`→`, `Space`), yanında Yeni / Öğreniliyor / Ustalaşıldı ilerlemesi, bugünün sayıları ve alt desteler. Aşağıda **terim listesi** (`TermList.svelte`) gerçek bir tablo: terim | tanım, seviye çipleri, ustalık, bayraklar; satır içi düzenleme (kalem ya da çift tıklama → iki alan, `Ctrl+Enter` kaydet, `Esc` vazgeç; `notes.update`), bayrak menüsü (`cards.set_flag`); "Kart ekle" bu destede Hızlı Ekle'yi açar.

**Ara** (`features/browse/BrowseScreen.svelte`, geniş): sayfanın kendisi sorgu alanıdır (gelince ve `/` ile odaklanır), sonra ince araç çubuğu, sık sanal tablo (34 px satır, mono sayılar, nokta durumlar, sütun seçici), kart bilgisi tabloyu kullanılır bırakan yan panel. Klavye: `↑`/`↓`, `Space` seç, `Shift+↑/↓` / `Shift+tık` genişlet, `Enter` notu düzenle, `Esc` paneli kapat / seçimi temizle; seçim varken yapışkan ince toplu işlem çubuğu. Sorgu dili, sıralama ve her değişiklik Rust'ta (`search.*`, `bulk.*`). Not düzenleyici: etiket üstte sıkı alanlar, sessiz biçim şeridi (`Ctrl+B`/`Ctrl+I`), `Ctrl+Enter` kaydeder.

### 3.4 İlerlemeyi gör — İstatistik

`features/stats/` (geniş). **v1.3:** ince filtre satırından sonra üç soru sırayla: **Bugün nasılım?** (tek satırlık yargı cümlesi + mono rakamlar: cevap, doğruluk, süre, seri, hedef çubuğu), **Önümde ne var?** (7 günlük çubuk grafik: her çubukta doğrudan değer, gün adları, bugün kalın etiketli, en yoğun gün "▲ en yoğun" yazısı ve çerçeveyle — renk tek işaret değil; 30 gün bir tık) ve **Neyi unutuyorum?** (sık tablo: kelime, anlam, yanlış/cevap mono, son görülme; birincil "Bunları çalış · N", satır sonunda ⋯ menüsü: durdur, etiketle). Altında "Ayrıntılar" başlığıyla dönem özeti (dört `StatTile`) ve eski paneller (ısı haritası, bellek durumları + başarı oranları, gelişim + alışkanlıklar, deste karneleri, sınıflandırma kırılımı, sülükler), her biri bir satırlık açıklamayla. Grafikler mono eksenli, ekran okuyucu için gizli tablolu. **Planlama** iki kontrolle açılır ("Günde kaç yeni kart?" + hazır sayı çipleri, "Sınav tarihim") ve canlı sonucu: büyük tek cümle, kısa olgu listesi, günlük iş yükü grafiği, haftalık mono tablo, neyi değiştireceğini söyleyen "Uygula"; sınavlar ve yük dengeleme açıklamalı, optimizasyon + hatırlama/iş yükü tablosu "Gelişmiş" altında. **Hedefler** boşluklu cümle formu. **`?`** (yazı alanında değilken), üst çubuktaki "?" ya da paletten **kısayol yardımı**: canlı `shortcuts.list` tablosundan, bağlama göre gruplu, aranabilir, kısayol düzenleyicisine bağlantılı (`features/help/`). Her sayı backend'den.

### 3.5 Diğer ekranlar

Gelen kutusu: sık inceleme listesi, gezen odak ve klavye (`J`/`K`, `A` kabul, `E` düzenle, `R` reddet, `X`/`Space` seç, `Ctrl+Enter` seçimi kabul), görünür kısayol ipuçları. AI ile üret (kaynak | seçenekler, taslaklar yan panelde), başlangıç desteleri listesi, ilk açılış rehberi, kilit ekranı (kutusuz, uygulama işareti), not türleri (sık alan tablosu), eklentiler (tek sık liste), okuma (sık liste, rahat okuma sütunu), yerel API ve **Subtitle** ayar sayfaları da aynı sistemde. Subtitle bağlantısının kendisi: [06 §5.1](06-ai-ve-entegrasyonlar.md) ve kullanıcı rehberi.

### 3.6 Kısayollar

Değiştirilebilir olanlar Ayarlar → Genel → Kısayollar'da (`palette::UI_ACTIONS`); burada varsayılanlar.

| Bağlam | Tuş | Eylem |
|---|---|---|
| Her yer | `N` | Hızlı Ekle (yazı alanında, iletişim kutusunda ve oturumda değil) |
| Her yer | `Ctrl+N` | Hızlı Ekle (`ui.new_note`) |
| Her yer | `Ctrl+K` | Komut paleti |
| Her yer | `Ctrl+F` | Ara sayfası |
| Her yer | `Ctrl+Z` / `Ctrl+Shift+Z` | Geri al / Yinele |
| Ana sayfa | `Enter` | Çalışma başlatıcısını aç (v1.3) |
| Deste sayfası | `Enter` · `←`/`→` · `Space` | Başlatıcıyı aç · önizlemede gez · önizlemeyi çevir |
| Çalışma başlatıcısı (v1.3) | `Enter` · `←`/`→` · `↑`/`↓` · `Tab` · `Esc` | Başla (odak "Devam et"te ise devam) · mod · ne çalışılacak · sayı/seçenekler · kapat |
| Hızlı Ekle | `Tab` · `Enter` · `Shift+Enter` | Sonraki alan · sonraki alan / son alanda ekle · yeni satır |
| Hızlı Ekle | `Ctrl+Enter` · `Esc` | Ekle (pencere açık kalır) · kapat (metin varsa iki kez) |
| Oluştur | `Tab` · `Enter` (son alan) · `Ctrl+Enter` | Sonraki alan · yeni satır · kaydet |
| Terim listesi | çift tık / kalem · `Ctrl+Enter` · `Esc` | Satır içi düzenle · kaydet · vazgeç |
| Kartlar | `Space` · `1`–`4` | Çevir (ön ⇄ arka, her basışta; asla puanlamaz) · Tekrar/Zor/İyi/Kolay, yalnız arka yüz görüldükten sonra (2'li düzende `1` Hâlâ öğreniyorum, `2` Biliyorum; `3`/`4` boş). `Enter` yalnız odaktaki düğmeye basar; basılı tutulan tuş tek işlem |
| Kartlar | `H` · `S` · `B` · `Z` · `Esc` | İpucu · durdur (sen açana kadar) · yarına bırak · geri al · bitir |
| Kartlar | `Ctrl+1`–`7` · `F` | Bayrak · odak modu |
| Öğren / Test | `1`–`4` | Seçenek |
| Ara | `/` · `↑`/`↓` · `Space` · `Shift+↑/↓` · `Enter` · `Esc` | Sorgu · gez · seç · seçimi genişlet · düzenle · kapat/temizle |
| Ara | `Ctrl+A` · `Ctrl+J` · `Ctrl+I` · `Delete` | Tümünü seç · askıya al · kart bilgisi · sil |
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
| `--text-1` / `--text-2` / `--text-3` | `#EDEDEF` / `#A3A3AE` / `#74747F` | `#1A1A1C` / `#5C5C63` / `#85858C` | Birincil / ikincil / yer tutucu ve süs simgesi |
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
- Ölçek 11/12/13/14/16/20/24 (+32); **gövde 13 px**, vurgu 14, sayfa başlığı 20/600; çalışma kartı metni 28–40 (uzun metinde küçülür). Satır yüksekliği 1.45 (sıkı 1.3).

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
| Ekran kitleri | `features/core/kit` (SettingsPage, SettingsSection, FormRow, SettingsAdvanced, LoadState), `features/modes/SessionFrame` (+ SetupPanel, OptRow), `features/stats` (Panel, Heatmap, BarChart) |

İlk yüklenen kod (`App.svelte`, `lib/shell/`, `*Host` bileşenleri) `$lib/ui`'yi dosya dosya alır, tembel ekranlar varilden (`tests/eager-imports.test.ts`). Sayılar/tarihler yalnız `$lib/format` (Intl, etkin dil; lint `tests/number-format.test.ts`).

### 4.5 Erişilebilirlik

- Klavye ile her şey; görünür odak halkası; ARIA rolleri (liste kutusu, radyo grubu, ağaç satırları, sekmeler, iletişim kutusu).
- Renk tek başına ipucu değildir: doğru/yanlış renk + simge + metin; ilerleme çubuğu parçaları erişilebilir adda.
- Durum duyuruları `role="status"` ile (Hızlı Ekle "Kart eklendi…", ısı haritası odak günü); grafiklerin ekran okuyucu tablosu var.
- Tüm metin çiftleri iki temada WCAG AA (testli); `prefers-reduced-motion` saygılı; 200% yakınlaştırmada taşma yok (900 px görüntülerinde denetlenir).
- **Dil:** her metin i18n anahtarı (TR/EN, `tests/no-hardcoded-text.test.ts`); terimler tutarlı ("Öğreniliyor" her yerde). Varsayılan dil sistem dilinden; mock modunda da ayarlanan dil uygulanır.

## 5. Görsel doğrulama

`apps/desktop/scripts/screens.mjs`: mock modunda uygulamayı açıp her ekranın **açık/koyu × TR/EN × 1280 ve 900 genişlik** ekran görüntüsünü `apps/desktop/screens/` (git'e girmez) altına alır; CI'da artefakt olarak yüklenir. Görsel denetçiler kodu değil bu görüntüleri değerlendirir: hiyerarşi, hizalama, yoğunluk tutarlılığı, taşma, kontrast, boş durumlar, ham kontrol kalmış mı. **Görüntüler açılıp koyu ve açık temada gözle bakılır**; otomatik bulgu sayısı tek başına yetmez.

### Nasıl çalıştırılır

```bash
pnpm -C apps/desktop screens            # tüm matris: --langs tr,en --themes light,dark --widths 1280,900
pnpm -C apps/desktop screens --quick    # yalnız EN, açık, 1280 (açıkça verilen --langs/--themes/--widths yine geçerli)
pnpm -C apps/desktop screens --strict   # bulgu varsa çıkış kodu 1 (varsayılan: yalnız raporla; betik çökerse 2)
# diğer bayraklar: --langs tr --themes dark --widths 900 --port 23456 --url <çalışan dev:mock adresi>
#                  --out <apps/desktop'a göreli klasör> --no-static
```

Çıktı `apps/desktop/screens/<dil>-<tema>-<genişlik>/<ad>.png`, `index.html` (yapılandırma başına küçük resim ızgarası, en üstte bulgu özeti) ve `report.json` (`configs`, `findings`, `static`, `totals`). Klasör her koşuda silinip yeniden yazılır. `--out` **`apps/desktop`'a göreli** bir yoldur (ör. `--out screens-dark`). Betik önce `merge-locales`'i çalıştırır, sonra vite'ı mock modunda betiğin seçtiği **rastgele bir portta** (20000–39999) `--strictPort` ile başlatır; böylece farklı worktree'lerde paralel koşular aynı geliştirme sunucusunu paylaşmaz. Sabit port için `--port N`, hazır bir sunucu için `--url` (o zaman sunucu başlatılmaz). Chromium `playwright-core` ile sürülür (`PLAYWRIGHT_CHROMIUM_EXECUTABLE`, yoksa playwright'ın kendi yolu, yoksa `~/.cache/ms-playwright` önbelleği; hiçbiri yoksa `npx playwright install chromium`). Görüntü penceresi `<genişlik> × 800`, tam sayfa değil; dil `localStorage` (`fc.locale`), tema `data-theme` ve `colorScheme` ile verilir.

**Bekleme:** her tıklamadan sonra betik ağ boşalmasını, kabuğun yükleniyor satırının (`p.loading[role=status]`, en çok 15 s) ve tembel yüklenen bir ekranın yer tutucusunun (`main .page > p.loading`, en çok 10 s) kaybolmasını, ardından 250 ms bekler. Böylece soğuk bir geliştirme sunucusunda kendi parçası (chunk) geç gelen ekranlar "Yükleniyor…" diye çekilmez ve alt gezinmeleri atlanmaz.

**Keşif:** ekranlar sabit listelenmez. Kenar çubuğundaki her düğme (`[data-testid="nav-item"]`, yoksa `nav ul button`) sırayla açılıp çekilir; ad, sıra numarası + **ilk yapılandırmanın dilindeki** etiketin kısaltmasıdır ve diğer dil/tema/genişliklerde aynı kalır (varsayılan `--langs tr,en` ile TR: `01-ana-sayfa`, `07-ayarlar`, `06-gelen-kutusu-3`). Ekran içindeki alt gezinme öğeleri (`[data-testid="subnav-item"]`, `[data-testid^="settings-group"]`, `main [role=tab]`; en çok 30) tek tek tıklanır → `<ekran>--01`, `--02`…; 1024 px altında Ayarlar'ın alt menüsü bir Select olduğundan (`settings-select`) sayfalar oradan seçilir. Her sayfada `open-dialog-*` kancalı düğmeler (en çok 8, görünür ve etkinse) tıklanır, iletişim kutusu `<ekran>--dialog-<kanca>` olarak çekilir ve Esc ile kapatılır. Çalışan bir oturumdan başka ekrana geçmeden önce Esc ile (olmazsa sayfa yenilenerek) çıkılır; kabuk oturumda gizlidir.

**Durumlar** (nav ekranlarından sonra, bu sırayla; kanca yoksa o adım atlanır, v1.0 işaretlemesine düşülür):

| Görüntü | Nasıl |
|---|---|
| `state-deck-page`, `state-deck-terms` | Ana sayfa → Kütüphane → ilk sekme → ilk `deck-row` (yoksa `.tree button.name` / `button.name`); terim listesine (`term-list`) kaydırılır |
| `state-<mod>`, `state-<mod>-active`, `state-<mod>-flipped` | deste sayfasındaki her `deck-mode-<mod>` düğmesi (cards, learn, test, match, reading): boş hâli, `study-start` sonrası, varsa `flip` sonrası |
| `state-cards-summary` | Kartlar oturumunda üç kart (Boşluk + `2`), sonra `study-end` |
| `state-test-result` | her soruda ilk `option-*` seçilir, yazılı sorulara "—" yazılır, `test-submit` |
| `state-match-finished` | `match-tile` (`data-side`) eşleşmeleri denenerek tahta çözülür, `match-result` beklenir |
| `state-library-grid` | Kütüphane'nin varsayılan olmayan görünümü (`.view-toggle` içinde seçili olmayan radyo); v1.2'de ağaç varsayılan olduğundan ızgara. Ad gösterilen görünüme göre verilir (ızgara değilse `state-library-tree`), sonra eski görünüme dönülür |
| `state-search-card-info`, `state-search-bulk` | Ara'da ilk `browse-row` tıklanır (kart bilgisi), Esc; ilk üç `browse-select` işaretlenir (toplu işlem çubuğu) |
| `state-search-edit` | Esc, tabloya (`main [role=grid]`) odaklanıp Enter: not düzenleyici |
| `state-quick-add` | Ana sayfada `N` |
| `state-palette`, `state-palette-query` | `Ctrl+K`; ardından "kart" (EN: "card") yazılır |
| `state-onboarding`, `state-lock`, `state-update`, `state-update-screen` | ayrı sayfada mock senaryoları: `?mock=fresh` (boş koleksiyon → ilk açılış rehberi), `?mock=locked` (kilit ekranı, PIN 1234), `?mock=update` (güncelleme bildirimi; `update-open` ile güncelleme ekranı) |

Oluştur'un tüm alt sekmeleri (Ekle, Yapay zeka, İçe / dışa aktar, Başlangıç desteleri) ve Ayarlar'ın her sayfası (mock'ta Subtitle dahil 16) alt gezinme olarak çekilir; kenar çubuğundaki profil öğesi (`08-…`) Ayarlar → Profiller ve kilit'i açtığından Ayarlar'ın alt gezinmesi orada bir kez daha çekilir (sırayla, aynı sayfalar). Bugünkü mock verisiyle **her yapılandırma 82 görüntü** verir, tam matris (8 yapılandırma) **656**.

**Otomatik kontroller (report.json):** `overflow-x` (sayfa yatay taşması), `outside-viewport` (görünür bir öğe yatayda pencere dışında ve kırpan bir atası yok; sabit konumlu öğeler dikeyde de), `text-clipped` (kendi metni olan öğede scrollWidth > clientWidth; `.sr-only` içindekiler sayılmaz), `raw-control` (DOM'da `[data-fc-ui]` içinde olmayan `<select>`/checkbox/radio; ayrıca `raw-control-static`: `src/lib/ui` ve `generated` dışındaki `.svelte` kaynak taraması, `--no-static` ile atlanır), `contrast` (WCAG AA — 24 px ya da 18,66 px kalın üstü 3:1, gerisi 4,5:1 — hesaplanmış renk ve birleşik arka plan; pasif öğeler ve gradyanlı zeminler atlanır), `i18n-raw-key` (ekranda ya da `placeholder`/`aria-label`/`title`'da ham `a.b.c` anahtarı), `page-error` (sayfa hatası, "Failed to load resource" dışındaki konsol hataları, başarısız istek ya da HTTP ≥ 400 — URL'siyle; `configs[].pageErrors` altında da).

Mock verisi (`src/lib/api/mock/seed.ts` ve `features/*/mock.ts`) gerçekçi görünmesi için çok desteli, Seviye/Sınav sınıflandırmalı, vadeli kartlı, istatistik geçmişli ve gelen kutusu dolu tutulur. Subtitle modülü mock'ta (gerçeğin aksine) açık başlar; biri kullanılmış, biri iptal edilmiş iki anahtar ve 12 gelen kelimeyle.

## 6. Kabul kriterleri (v1.2)

- [x] Hiçbir özellik kaldırılmadı: Rust'ın kaydettiği her rota yerleşik ve ekranlı (`shell/navigation.test.ts`, `App.test.ts`); taşınan her şey paletten bulunur; modüller hâlâ kapatılabilir (`features/module-switches.test.ts`).
- [x] Kabuk: 216 px kenar çubuğu (1024 px altında simge), oturum dışında her zaman görünen 44 px üst çubuk (iz, `Ctrl+K` arama/komut, `N` "Kart ekle"); giriş noktaları `shell/commands.svelte.ts`'te.
- [x] Hızlı Ekle: `N` / `Ctrl+N` / üst çubuk; deste ve not türü hatırlanır, `Ctrl+Enter` ekler ve pencere açık kalır, sayaç + Geri al, `Esc` kapatır (`features/quickadd/quickadd.test.ts`).
- [x] Oluştur bir ızgara: tek kurulum satırı + numaralı terim | tanım satırları, kaydet tek `notes.create_many` geri al adımı (`features/notes/create.test.ts`).
- [x] Ana sayfa "Bugün · N kart" + kırılım + `Enter` ile başla; çalışma oturumunda kabuk kaybolur (`SessionFrame`), kart en çok 720 px, cevap düğmeleri kart genişliğinde, 2'li ve 4'lü düzen korunur.
- [x] Kütüphane ağaç-tablo varsayılan, ızgara ikincil; deste sayfasında "Çalış" ve mod düğmeleri ilk ekranda, terim listesi satır içi düzenlemeli tablo.
- [x] Token'lar tasarım yönergesindeki değerlerle (koyu öncelikli, tek kehribar vurgu, sıcak nötrler), JetBrains Mono paketli; tüm metin çiftleri iki temada AA (`tests/contrast.test.ts`).
- [x] Hiçbir ekranda ham `<select>`/checkbox/radio yok (`tests/raw-controls.test.ts`, izin listesi yok); sabit metin yok (`tests/no-hardcoded-text.test.ts`); sayılar `$lib/format`'tan (`tests/number-format.test.ts`).
- [x] Davranış değişmez: ön yüz testleri geçer (44 dosya / 365 test); Rust tarafında v1.2 yalnız varsayılan hazır temanın renklerini değiştirdi (+ ayrı Subtitle modülü), `cargo check` temiz. Tam `cargo test --workspace` birleştirmeden önce CI'da koşulur.
- [x] Performans bütçesi (02 §34, ilk JS ≤ 320 kB) korunur — `pnpm build && pnpm bundle:check`: ilk JS 177,1 kB, CSS 50,0 kB (bütçe 52 kB); fontlar ve simgeler yerel.
- [x] `pnpm screens --strict` tam matris (TR/EN × açık/koyu × 1280/900): 8 yapılandırma × 82 = **656 görüntü**, otomatik denetimde **0 bulgu**; `tr-dark-1280` baştan sona, `tr-light-1280` ana ekranlar, `en-dark-900` uzun EN etiketleri için gözle incelendi.
- [ ] Gerçek Tauri penceresinde (Windows) elle gezinti — yalnız mock modunda doğrulandı.
- [ ] Subtitle tarayıcı eklentisi gerçek bir tarayıcıda uygulamaya bağlanıp kelime gönderiyor — sözleşme testleri (`tools/subtitle-contract/check.mjs`, eklentinin kendi `api.ts`'i `fc api serve`'e karşı) geçiyor, gerçek tarayıcıda denenmedi.

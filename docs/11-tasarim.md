# 11 · Tasarım (v1.1)

v1.0'ın arayüzü işlevsel ama tasarlanmamıştı: 26 maddelik düz menü, tarayıcı varsayılan kontrolleri, ana ekran yok. v1.1 arayüzü baştan tasarlar. **Davranış ve API değişmez**; iş mantığı Rust'ta kalır (CLAUDE.md kural 1).

## 1. İlham ve sınır

Esin kaynağı Quizlet'in *tasarım dili*: kart odaklı, ferah, tek bir güçlü vurgu rengi, büyük çevrilen kart, deste sayfasında çalışma modları, terim/tanım listesi. **Kopya değil:** Quizlet'in adı, logosu, maskotu, kendi mavisi (#4255FF ve tonları), yazı tipi ve birebir yerleşimleri kullanılmaz. Uygulamanın kendi kimliği vardır.

"AI slop yok" kuralı (kural 9) = **gereksiz süs yok**, tasarımsızlık değil. Yasak: gradyan arka planlar, parıltı/emoji yığınları, her yere gölge, gereksiz animasyon, "✨ AI" rozetleri, uzun pazarlama metinleri. İstenen: net hiyerarşi, cömert boşluk, tutarlı bileşenler, okunaklı tipografi.

## 2. Bilgi mimarisi

Sol kenar çubuğu (dar, simgeli + etiketli; 1024 px altında yalnız simge) **yalnız 5 ana bölüm**:

| Sıra | Bölüm | İçerik |
|---|---|---|
| 1 | **Ana sayfa** | Bugün: vadesi gelen / yeni / öğrenilen sayıları, "Çalışmaya başla", son çalışılan desteler, seri (streak), hedef ilerlemesi, gelen kutusu bildirimi (varsa) |
| 2 | **Kütüphane** | Desteler (ağaç + kart görünümü), her destede kart sayısı, vadeli sayısı, ustalık çubuğu, sınıflandırma çipleri (Seviye, Sınav…) |
| 3 | **Oluştur** (+ düğmesi, vurgulu) | Yeni deste / kart ekle; Quizlet tarzı satır satır terim–tanım editörü; yapıştırarak içe aktarma; AI ile üret (modül açıksa) |
| 4 | **Ara** | Kart tarayıcısı (sorgu dili, tablo, toplu işlemler) |
| 5 | **İstatistik** | Özet, ısı haritası, tahmin, sınıflandırma kırılımı, planlama, hedefler |

Alt kısımda: **Gelen kutusu** (rozet sayısıyla, yalnız öğe varsa), **Ayarlar** (dişli), profil avatarı (profil değiştir / kilitle).

**Ayarlar** tek sayfa, solda gruplu alt menü:
- Genel: dil, tema (açık/koyu/sistem + hazır temalar + vurgu rengi), kısayollar, hatırlatıcılar
- Çalışma: deste ayar şablonları (preset), not türleri, FSRS optimizasyonu
- Okuma ve modlar: okuma modu, öğrenme modu ayarları
- Bağlantılar: AI sağlayıcıları, MCP, yerel API + tarayıcı eklentisi, otomasyon kuralları
- Eklentiler
- Veri: içe/dışa aktarma, yedek, başlangıç desteleri, bakım, loglar ve güncellemeler
- Profiller ve kilit

Okuma modu ve İçe aktarma ayrıca deste sayfasından ve Oluştur'dan erişilir. Komut paleti (Ctrl/⌘+K) her şeye ulaşır. Menüler hâlâ `modules.list`'ten gelir (kural 7): modül → bölüm/grup eşlemesi tek bir yerde (`navigation.ts`) tutulur; kapalı modül görünmez.

## 3. Ana ekranlar

**Deste sayfası** (Quizlet set sayfası dilinde):
- Başlık: deste adı, kart sayısı, sınıflandırma çipleri, "⋯" menüsü (düzenle, ayarlar, dışa aktar, sil).
- Büyük önizleme kartı (çevrilebilir, ok tuşlarıyla gezinilir) — tıklayınca/Boşluk ile döner.
- Çalışma modu döşemeleri (4'lü ızgara, simge + ad + kısa açıklama): **Kartlar** (aralıklı tekrar), **Öğren**, **Test**, **Eşleştir**. Ek: Okuma (modül açıksa).
- İlerleme: Yeni / Öğreniliyor / Ustalaşıldı çubuğu.
- Altta **terim listesi**: iki sütun (ön | arka), satır içi düzenleme, yıldız/bayrak, seviye çipi; "Kart ekle".

**Çalışma (Kartlar):** ortada büyük kart (en çok ~720×440, yuvarlak köşe 16 px, hafif gölge), Boşluk = çevir (yumuşak 3B çevirme, `prefers-reduced-motion`'da kapalı), altında cevap düğmeleri: varsayılan 2'li (**Hâlâ öğreniyorum** = Tekrar, **Biliyorum** = İyi), ayarla 4'lü (Tekrar/Zor/İyi/Kolay + sonraki aralıklar). Üstte ince ilerleme çubuğu ve "12 / 40". Kısayollar ekranda küçük ipucu olarak.

**Öğren / Test:** soru kartı + seçenek kutuları (büyük, tıklanabilir, 1–4 tuşları), yazarak cevapta tek büyük giriş alanı; doğru/yanlış geri bildirimi renk + simge + metin (yalnız renge güvenme). Test sonu: puan halkası + yanlışlar listesi.

**Eşleştir (yeni küçük mod):** 6 terim + 6 tanım karışık döşemeler; doğru çiftler kaybolur; süre ve en iyi süre. Tur üretimi ve puanlama Rust'ta (`modes.match.*`), süre backend saatiyle; review_log'a `match` modu ile yazılır, zamanlamayı değiştirmez (pratik modu).

**Oluştur:** başlık + açıklama + deste seçimi + sınıflandırma; altında numaralı satırlar (ön | arka, görsel ekle), Tab ile sonraki alana, son satırda Enter yeni satır; "Yapıştırarak içe aktar" paneli (v0.5 ayırıcı algılama + önizleme). Kaydet tek geri al adımı.

**Ana sayfa:** "Günaydın" gibi şirin metinler yok; sade: "Bugün 42 kart" + büyük "Çalışmaya başla", altında deste kartları (son çalışılanlar), seri ve hedef.

**Boş durumlar:** her liste için tek cümle + tek eylem düğmesi (ör. "Henüz deste yok — İlk desteni oluştur").

## 4. Tasarım sistemi

`apps/desktop/src/lib/ui/` altında bileşenler; tüm ekranlar yalnız bunları kullanır (ham `<select>`, `<input type=checkbox>` yok).

**Bileşenler:** Button (primary / secondary / ghost / danger; sm/md/lg; yükleniyor durumu), IconButton, Input, Textarea, Select (özel açılır liste, klavye erişilebilir), Checkbox, Switch, Radio/SegmentedControl, Chip/Badge, Card, Tile (mod döşemesi), Dialog/Sheet, Menu (⋯), Tabs, Tooltip, Toast, ProgressBar, ProgressRing, EmptyState, Skeleton, Table (sanal kaydırma), Kbd (kısayol ipucu), Avatar, Sidebar/NavItem, PageHeader, Flashcard (çevrilebilir).

**Token'lar** (`theme/tokens.css`, mevcut tema altyapısının üstüne):
- Renk: nötr gri ölçeği (50–950), yüzeyler `--surface-0/1/2`, kenarlık `--border`, metin `--text-1/2/3`; **vurgu varsayılanı: koyu mor-indigo `#5B47E0`** (Quizlet mavisi değil), başarı `#1F9D6B`, uyarı `#C98A0B`, hata `#D64545`; her biri açık/koyu temada WCAG AA kontrastında. Hazır temalar ve özel vurgu (v1.0) korunur.
- Tipografi: **Inter** (OFL) uygulamayla birlikte paketlenir (ağdan yüklenmez); ölçek 12/14/16/20/24/32/40, satır yüksekliği 1.4–1.5, başlıklar 600–700 ağırlık. Kart metni büyük (24–32, uzun metinde otomatik küçülür).
- Boşluk: 4 px tabanlı (4, 8, 12, 16, 24, 32, 48, 64). Köşe: 8 (kontrol), 12 (kart), 16 (büyük kart), 999 (çip). Gölge: 2 seviye, çok hafif. Hareket: 150–250 ms, `prefers-reduced-motion` saygılı.
- Simgeler: tek bir açık lisanslı set (Lucide, ISC) yerel paket olarak.

**Erişilebilirlik:** klavye ile her şey; görünür odak halkası; ARIA rolleri; renge tek başına güvenilmez; 200% yakınlaştırmada taşma yok.

**Dil:** varsayılan arayüz dili sistem dilinden (TR sistemde TR); mock modunda da ayarlanan dil uygulanır.

## 5. Görsel doğrulama

`apps/desktop/scripts/screens.mjs`: mock modunda uygulamayı açıp her ekranın **açık/koyu × TR/EN × 1280 ve 900 genişlik** ekran görüntüsünü `apps/desktop/screens/` (git'e girmez) altına alır; CI'da artefakt olarak yüklenir. Görsel denetçiler kodu değil bu görüntüleri değerlendirir: hiyerarşi, hizalama, boşluk tutarlılığı, taşma, kontrast, boş durumlar, ham kontrol kalmış mı.

### Nasıl çalıştırılır

```bash
pnpm -C apps/desktop screens            # tüm TR/EN x açık/koyu x 1280/900 (yaklaşık 200+ PNG)
pnpm -C apps/desktop screens --quick    # yalnız EN, açık, 1280
pnpm -C apps/desktop screens --strict   # bulgu varsa çıkış kodu 1 (varsayılan: yalnız raporla)
# diğer bayraklar: --langs tr --themes dark --widths 900 --url <çalışan dev:mock adresi> --out <klasör>
```

Çıktı `apps/desktop/screens/<dil>-<tema>-<genişlik>/<ekran>.png`, `index.html` (küçük resim ızgarası) ve `report.json`. Mock modunda vite başlatılır; Chromium `playwright-core` ile sürülür (yerelde `~/.cache/ms-playwright` önbelleği, yoksa `npx playwright install chromium`; `PLAYWRIGHT_CHROMIUM_EXECUTABLE` ile yol verilebilir). CI'da `screens` işi klasörü artefakt yükler.

**Keşif:** ekranlar sabit listelenmez. Kenar çubuğundaki her düğme (`[data-testid="nav-item"]`, yoksa `nav ul button`) açılır; ekran içindeki alt gezinme öğeleri (`[data-testid="subnav-item"]`, `[data-testid^="settings-group"]`, `[role=tab]`) tek tek tıklanır. Dosya adları sıra numarası + EN etikettir (dil değişse de aynı kalır).

**Durum kancaları (isteğe bağlı `data-testid`):** `deck-row` (deste sayfasını açan satır), `deck-mode-<mod>` (deste sayfasındaki mod döşemeleri: her mod boş, çalışırken ve — varsa — çevrilmiş hâliyle `state-<mod>*` olarak çekilir), `study-start` ve `flip` (çalışma başlat / kartı çevir), `open-dialog-*` (tıklanınca iletişim kutusu açan düğme; Esc ile kapatılır). Kanca yoksa v1.0 işaretlemesine düşülür.

**Otomatik kontroller (report.json):** `overflow-x` (sayfa yatay taşması), `outside-viewport`, `text-clipped` (scrollWidth > clientWidth; `.sr-only` içindekiler sayılmaz), `raw-control` (DOM'da `[data-fc-ui]` içinde olmayan `<select>`/checkbox/radio; `lib/ui` bileşenleri kendilerini `data-fc-ui` ile işaretler; ayrıca `raw-control-static`: `src/lib/ui` dışındaki `.svelte` kaynak taraması), `contrast` (WCAG AA, hesaplanmış renk ve birleşik arka plan), `i18n-raw-key` (ekranda ham `a.b.c` anahtarı). Ayrıca sayfa hataları `configs[].pageErrors` altında.

Mock verisi (`src/lib/api/mock/seed.ts` ve `features/*/mock.ts`) gerçekçi görünmesi için çok desteli, Seviye/Sınav sınıflandırmalı, vadeli kartlı, istatistik geçmişli ve gelen kutusu dolu tutulur.

## 6. Kabul kriterleri (v1.1)

- [x] Kenar çubuğunda en çok 5 ana bölüm + Gelen kutusu + Ayarlar + profil; tüm eski ekranlara Ayarlar ya da ilgili bölümden ulaşılır (`navigation.test.ts`: Rust'ın kaydettiği her rota yerleşik ve ekranlı; `App.test.ts`).
- [x] Hiçbir ekranda ham tarayıcı `<select>`/checkbox yok (lint testi `tests/raw-controls.test.ts`, izin listesi yok; görsel denetimde `raw-control` 0).
- [x] Ana sayfa, Kütüphane, Deste sayfası, Kartlar, Öğren, Test, Eşleştir, Oluştur, Ara, İstatistik, Ayarlar ekranları tasarım sistemiyle; `pnpm screens --strict` TR/EN × açık/koyu × 1280/900 (≈500 görüntü) bulgusuz. Görüntüler insan gözüyle yalnız TR açık 1280/900'de tek tek incelendi; diğer yapılandırmalar otomatik denetimden geçti.
- [x] Davranış değişmez: mevcut Rust ve ön yüz testleri geçer; yeni bileşenlerin testleri var.
- [x] Performans bütçeleri (02 §34, ilk JS ≤ 320 kB) korunur — ilk JS 160,6 kB, CSS 35,8 kB; font ve simgeler yerel. İlk yüklenen kodda `$lib/ui` varili yasak (`tests/eager-imports.test.ts`).
- [ ] Gerçek Tauri penceresinde (Windows) elle gezinti — yalnız mock modunda doğrulandı.

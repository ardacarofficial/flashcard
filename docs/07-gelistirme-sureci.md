# 07 · Geliştirme Süreci

## 1. Teknoloji yığını

| Alan | Seçim | Not |
|---|---|---|
| Dil (backend) | Rust (stable, 2024 edition) | |
| Masaüstü kabuğu | Tauri 2 | Windows'ta WebView2 |
| Async | tokio | |
| Veritabanı | SQLite (rusqlite, bundled) + FTS5 | WAL modu |
| Migration | rusqlite_migration (veya eşdeğeri) | |
| Serileştirme | serde, serde_json | |
| TS tip üretimi | specta + tauri-specta | |
| Zamanlayıcı | fsrs-rs (resmi FSRS Rust kütüphanesi) | |
| Hata | thiserror (kütüphaneler), anyhow (sadece uç uygulamalar) | |
| Log | tracing, tracing-subscriber, tracing-appender | |
| Eklenti çalışma zamanı | Extism / wasmtime | |
| Yerel HTTP | axum | |
| MCP | rmcp | |
| Anahtar zinciri | keyring | |
| Parola hash | argon2 | |
| HTTP istemcisi | reqwest (rustls) | |
| Frontend | Svelte 5 + TypeScript + Vite | Değiştirilebilir |
| Paket yöneticisi | pnpm | |
| Markdown / matematik | markdown-it (veya eşdeğeri) + KaTeX | Render backend'de de yapılabilir (comrak) |
| Test (Rust) | cargo test, proptest, insta (snapshot), criterion (benchmark) | |
| Test (frontend) | Vitest, Playwright (uçtan uca) | |
| CI | GitHub Actions: Windows (öncelik), Linux | macOS sonra |

Bağımlılık sürümleri `Cargo.lock` ve `pnpm-lock.yaml` ile sabitlenir.

## 2. Kod kuralları

- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` temiz olmalı.
- `fc-core` içinde I/O yasak. `unsafe` yasak (gerekirse gerekçeli ve izole).
- Kütüphane kodunda `unwrap()`/`expect()` yok (testler hariç).
- Herkese açık API'ler belgelenir (`///`).
- Frontend: sıkı TypeScript, iş mantığı yok; backend tipleri dışında el yazması DTO yok; tüm metinler i18n anahtarından.
- Commit mesajları: Conventional Commits (`feat(core): …`, `fix(storage): …`).

## 3. Test stratejisi

| Seviye | Ne test edilir | Araç |
|---|---|---|
| Birim | fc-core saf fonksiyonları | cargo test, proptest |
| Depolama | Repository'ler, migration'lar, sorgu derleyici | Geçici SQLite |
| Servis | Komutlar, geri al, olaylar, oturum motoru | Sahte saat + geçici DB |
| Sözleşme | API girdi/çıktı şekilleri | insta snapshot |
| Simülasyon | Zamanlayıcı ve yük dengeleme | Deterministik saat, aylarca simülasyon |
| Benchmark | 100k not ölçeği | criterion |
| Frontend | Bileşenler | Vitest |
| Uçtan uca | Ana akışlar | Playwright |
| Eklenti | Örnek eklentiler, izin reddi, kaynak sınırları | Sahte host |

**Kural:** Her yeni komut için en az bir servis testi ve bir geri al testi.

## 4. Dal ve sürüm stratejisi

- `main`: her zaman derlenen, testleri geçen durum.
- Özellik dalları: `feat/<sürüm>-<konu>` (ör. `feat/v0.3-cloze`).
- Her sürüm sonunda etiket: `v0.1.0`, `v0.2.0` …
- Değişiklik günlüğü: `CHANGELOG.md`.

## 5. Paralel ajanlarla çalışma

### 5.1 İlkeler
- **Faz 1 (v0.1) sıralı ve tek elden yapılır.** API sözleşmesi, veri modeli ve iskelet burada belirlenir; paralel çalışmanın sağlıklı olması buna bağlıdır.
- Sonraki fazlarda her ajan **ayrı bir git worktree**'de, kendi özellik dalında çalışır.
- Her iz (track) için net bir sınır: hangi crate'lere/klasörlere dokunabilir, hangi API'leri ekleyebilir.
- **Ortak dosyalar** (migration listesi, kayıt defteri, `fc-api` DTO'ları) çakışma noktasıdır:
  - Migration numaraları iz başına önceden ayrılır (ör. v0.2: 0100–0199, v0.3: 0200–0299). v1.0 aralığı (`1000`–`1099`) izlere bölündü ve `fc-storage` `migrate::V1_LANE_RANGES` + `v1_migrations_stay_in_their_lane_range` testiyle zorunlu: **ux** (iz N) `1000`–`1049` (`1000_ux_state`), **release** (iz O) `1050`–`1069` (`1050_maintenance`), **tur 2** (performans/i18n/temalar) `1070`–`1089` (henüz kayıtlı değil: o iz aralığını `V1_LANE_RANGES`'e ekler), **quality** (iz Q) `1090`–`1099` (şimdilik migration yok). Tam liste docs/10 §10.
  - Kayıt defteri, modül başına ayrı dosyalardan toplanır (her modül kendi `register()` fonksiyonuna sahip).
- Orkestratör (ana ajan) her iz bittiğinde: inceler, testleri çalıştırır, `main`'e birleştirir, sonraki izlere bildirir.
- Her iz bir **görev tanımıyla** başlar: amaç, kapsam, kabul kriterleri ([04](04-yol-haritasi.md)), dokunulabilir dosyalar, yasaklar.

### 5.2 Kaynak sınırları
- Rust derlemesi CPU ve RAM yoğundur. Çekirdek başına yaklaşık bir aktif derleme hedeflenir.
- 4 çekirdek: 3-4 iz · 8 çekirdek / 16 GB: 4-5 iz · 16 çekirdek / 64 GB: 8-10 iz.
- Disk: her worktree'nin `target/` klasörü 3-5 GB tutabilir. Ortak derleme önbelleği için `sccache` önerilir.
- `CARGO_BUILD_JOBS` iz başına sınırlanarak ajanların birbirini boğması önlenir.

### 5.3 Önerilen iz dağılımı

| Faz | İz A | İz B | İz C | İz D |
|---|---|---|---|---|
| 2 | v0.2 Çalışma | v0.3 Kart türleri | v0.4 Yönetim | v0.5 İçe/dışa |
| 3 | v0.6 İstatistik | v0.7 Eklentiler | v0.8 AI (sağlayıcı arayüzü v0.1'de hazırsa) | — |
| 4 | v0.9 Dış dünya | v0.10 Öğrenme modları | Tarayıcı eklentisi | — |
| 5 | v1.0 Cila | Windows paketleme + updater | Performans | — |

## 6. Geliştirme ortamı ve VDS

### 6.1 Gereksinimler (Linux geliştirme makinesi)
- Rust (rustup, stable), Node 22+, pnpm.
- Tauri Linux bağımlılıkları: `libwebkit2gtk-4.1-dev`, `build-essential`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev` (dağıtıma göre değişebilir, Tauri belgelerine bakın).
- İsteğe bağlı: `sccache`, `ffmpeg` (v0.10 altyazı/video), `cargo-nextest`.

### 6.2 Windows
- Windows derlemesi ve kurulum dosyası GitHub Actions'ta (`windows-2025`) üretilir.
- Gerçek kullanım testleri proje sahibi tarafından Windows'ta yapılır.

### 6.3 VDS (ör. Hetzner CPX42: 8 vCPU, 16 GB RAM, 320 GB disk)
- Saatlik ~€0,095. **Hetzner'de kapatılan sunucu da ücretlendirilir; ücreti durdurmak için sunucu silinmelidir** (önce snapshot alınabilir).
- Önerilen kullanım: gün sonunda snapshot + sil, ertesi gün snapshot'tan geri aç.
- VDS üzerinde Claude Code kurulup proje klasöründe `claude remote-control` çalıştırılarak oturum Claude uygulamasından yönetilebilir.
- Repoyu klonladıktan sonra bu dokümanlar ve kök dizindeki `CLAUDE.md` ajan için başlangıç bağlamıdır.

## 7. Tanım: "Bitti"

Bir görev şu durumda bitmiştir:
1. Kabul kriterleri sağlandı.
2. Testler yazıldı ve geçiyor; clippy ve fmt temiz.
3. API değiştiyse TS tipleri yeniden üretildi ve frontend derleniyor.
4. Migration eklendiyse migration testi var.
5. İlgili doküman güncellendi.
6. Commit edildi ve push'landı.

## 8. Windows sürümü: kurulum, imzalı güncelleme, yayın (v1.0, iz O)

### 8.1 Kurulum dosyaları
- `apps/desktop/src-tauri/tauri.conf.json` → `bundle.targets = ["nsis", "msi"]`. Sürüm `tauri.conf.json`'da yazmaz: workspace `Cargo.toml` sürümünden gelir (kurulum = uygulama = `maintenance.status.version`).
- **NSIS (önerilen):** `installMode: currentUser` → yönetici izni istemez, `%LOCALAPPDATA%\Flashcard\` altına kurar; TR/EN kurulum dili. WebView2 yoksa **önyükleyici** sessizce indirir (`webviewInstallMode: downloadBootstrapper`).
- **MSI (isteğe bağlı):** WiX, makine başına (yönetici ister) — kurumsal dağıtım için. Kullanıcıya önerilen NSIS'tir.
- **Kod imzalama yok (docs/08 K8):** ilk açılışta SmartScreen "Windows kişisel bilgisayarınızı korudu" der → **Ek bilgi → Yine de çalıştır**. Bu beklenen davranıştır; güncellemelerin bütünlüğü ayrıca Ed25519 güncelleme imzasıyla korunur (aşağıda).

### 8.2 İmzalı otomatik güncelleme
- `tauri-plugin-updater`; manifest GitHub Release varlığı `latest.json` (`https://github.com/ardacarofficial/flashcard/releases/latest/download/latest.json`). Uygulama manifest'teki imzayı `tauri.conf.json → plugins.updater.pubkey` içindeki **açık anahtarla** doğrular; eşleşmeyen indirme reddedilir (`PROVIDER_UPDATE_SIGNATURE`). Akış backend'dedir (`updates.check → updates.install (iş) → updates.apply`), hiçbir şey kullanıcı tıklamadan kurulmaz; açılışta denetim varsayılan **kapalı** (`updates.check_on_start`).
- **Anahtar çifti** repo dışında üretildi; repoda yalnız açık anahtar var. Özel anahtar **asla** commit edilmez. Yeni bir çift gerekirse: `pnpm -C apps/desktop tauri signer generate -w ~/.flashcard-keys/updater.key` (parola sorar) → `.pub` içeriğini `pubkey`'e yaz. *Anahtar değişirse eski sürümler yeni güncellemeleri kabul etmez* (bir kez elle kurulum gerekir).
- **GitHub Actions sırları** (repo → Settings → Secrets and variables → Actions → *New repository secret*):
  1. `TAURI_SIGNING_PRIVATE_KEY` = özel anahtar dosyasının **içeriği** (ör. `cat ~/.flashcard-keys/updater.key` çıktısını kopyala; dosya yolu değil).
  2. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = anahtarın parolası (bu makinede üretilen çift için `~/.flashcard-keys/updater.key.password`).
  Sonra anahtar dosyalarını güvenli bir yere (parola yöneticisi) yedekle ve sunucudan sil. Sır yoksa yayın akışı yine kurulum dosyası üretir ama `latest.json` ve `.sig` olmadan (uyarı verir).

### 8.3 Yayın akışı (`.github/workflows/release.yml`)
1. `Cargo.toml` workspace sürümünü yükselt (ör. `1.0.0`), CHANGELOG'u güncelle, commit.
2. `git tag v1.0.0 && git push origin v1.0.0` (etiket sürümle eşleşmezse akış durur).
3. `windows-2025` NSIS + MSI derler, sır varsa güncelleme dosyalarını imzalar ve `latest.json` üretir, **taslak** (draft) bir Release oluşturur.
4. Taslağı incele (dosya adları, notlar) → **Publish**. Güncelleyici yalnız yayımlanmış en son sürümü görür.

### 8.4 CI duman testi (`ci.yml → windows-smoke`)
NSIS kurulumunu release modda derler, `/S` ile **sessiz, kullanıcı başına** kurar, kurulu `Flashcard.exe --smoke --smoke-report=<dosya>` çalıştırır: geçici bir klasörde (kullanıcının koleksiyonu asla) koleksiyon oluştur + migrate, deste/not, çalış + geri al/yinele, bütünlük denetimi, taslak, içinde sahte API anahtarı olan bir log satırı → log dışa aktar → anahtarın pakette **olmadığını** doğrula. Çıkış kodu 0 değilse iş kırmızı. Sonra sessiz kaldırma.

### 8.5 Elle Windows test betiği (proje sahibi; temiz bir Windows 10/11)
Ön koşul: temiz bir Windows kullanıcısı (ya da VM / Windows Sandbox), internet. Yayımlanmış iki sürüm gerekir: **A** (ör. `v1.0.0`) ve ondan yeni **B** (ör. `v1.0.1`). Her adımda beklenen sonuç ✅ ile; tutmayan adımı not al, `Bakım → Günlükler ve güncellemeler → Günlükleri dışa aktar` paketini hataya ekle.

**1. Temiz kurulum**
1. Release A'dan `Flashcard_<A>_x64-setup.exe` indir, çift tıkla. ✅ SmartScreen uyarısı → *Ek bilgi → Yine de çalıştır* ile geçilir; **UAC (yönetici) sorulmaz**.
2. Kurulum dilini seç, bitir. ✅ `%LOCALAPPDATA%\Flashcard\Flashcard.exe` var; Başlat menüsünde "Flashcard"; *Ayarlar → Uygulamalar*'da sürüm A görünür.
3. (WebView2'siz makinede) ✅ önyükleyici sessizce WebView2'yi kurar, kurulum takılmaz.

**2. İlk açılış**
4. Flashcard'ı Başlat menüsünden aç. ✅ 2 sn içinde pencere; hata kutusu yok; dil sistem diline uygun (TR/EN).
5. `%APPDATA%\app.flashcard.desktop\` aç. ✅ `collection.db`, `run.json` (uygulama açıkken), `logs\flashcard.log` var; log ilk satırında sürüm A ve `windows x86_64`.
6. Menü → *Günlükler ve güncellemeler*. ✅ "Sürüm A", "Son çalıştırma düzgün kapandı" yazmaz/ilk açılış, çökme raporu yok, veritabanı denetimi "Sorun bulunmadı".

**3. Çalışma**
7. Bir deste oluştur, 3 not ekle (biri Türkçe karakterli: `kılıç → sword`). ✅ Kartlar listede.
8. *Çalış* → 2 kartı cevapla, Ctrl+Z ile birini geri al. ✅ Kart geri gelir, sayaç doğru.
9. *Ekle* ekranında bir alana "yarım" yaz, **kaydetmeden** 2 sn bekle.
10. Görev Yöneticisi → Flashcard → **Görevi sonlandır** (çökme benzetimi).
11. Yeniden aç. ✅ "Flashcard düzgün kapanmadı" penceresi: "Veritabanı denetlendi: sorun yok", "Bitmemiş bir çalışma oturumu", "Kaydedilmemiş 1 form". *Çalışmaya devam et* → kaldığın kart. *Ekle* ekranında "yarım" geri gelir.
12. Uygulamayı normal kapat (X), yeniden aç. ✅ Kurtarma penceresi **çıkmaz**; `run.json` kapalıyken yoktur.

**4. Günlük dışa aktarma**
13. *Ayarlar → AI*'da bir sağlayıcıya sahte anahtar `sk-ant-api03-TESTTESTTESTTESTTESTTEST` gir (bağlantı başarısız olabilir, önemli değil).
14. *Günlükler ve güncellemeler → Günlükleri dışa aktar…* → Masaüstüne kaydet. ✅ "… gizli bilgi çıkarıldı" mesajı; zip'i aç: `system.txt`, `logs\flashcard.log` var; Not Defteri'nde **`sk-ant` geçmez**, kullanıcı adın yerine `<user>` yazar.

**5. Güncelleme (A → B)**
15. *Günlükler ve güncellemeler → Güncellemeleri denetle*. ✅ "B sürümü çıktı" + sürüm notları.
16. *İndir*. ✅ "İndiriliyor ve doğrulanıyor…" → "İndirildi ve doğrulandı"; iptal edilirse yeniden indirilebilir.
17. *Kur ve yeniden başlat*. ✅ Uygulama kapanır, kurulum ilerleme penceresi (pasif, soru sormaz), uygulama B sürümüyle **kendiliğinden** açılır; UAC sorulmaz.
18. ✅ Destelerin, notların, çalışma geçmişin, sınıflandırmaların aynen duruyor; **kurtarma penceresi çıkmaz** (güncelleme temiz kapanış sayılır); ekranda "Sürüm B".
19. (İsteğe bağlı, imza testi) Release B'nin `.sig` dosyası bozulmuş bir kopyasıyla yerel bir `latest.json` sunulursa ✅ "İndirilen güncelleme bu uygulamanın anahtarıyla imzalı değil" hatası, hiçbir şey kurulmaz.

**6. `flashcard://` soğuk başlatma**
20. Uygulamayı **tamamen kapat** (tepside de yok; Görev Yöneticisi'nde Flashcard yok).
21. `Win+R` → `flashcard://study?query=deck:Test` → Enter (7. adımdaki destenin adı `Test` varsayıldı). ✅ Uygulama açılır ve **Göz at ekranını açar; sorgu ekrana aktarılmaz** (kod: `shell.ts` `linkTarget`, docs/10 bilinen sınırlar) (soğuk başlatmada bağlantı kaybolmaz: pencere açılmadan gelen bağlantı kuyrukta bekler). Kurtarma penceresi çıkmaz.
22. Uygulama açıkken `Win+R` → `flashcard://add?front=merhaba&back=hello` → ✅ ikinci pencere açılmaz, mevcut pencere öne gelir, öneri *Gelen kutusu*na düşer ("Gelen kutusuna önerildi").
23. Uygulamayı yine tamamen kapat; tarayıcı adres çubuğuna `flashcard://add?front=soğuk&back=cold` yaz → ✅ "Flashcard'ı aç?" sorusu, kabul edince uygulama açılır, öneri *Gelen kutusu*na düşer, tek pencere vardır.

**7. Kaldırma**
24. *Ayarlar → Uygulamalar → Flashcard → Kaldır*. ✅ Yönetici sorulmaz, `%LOCALAPPDATA%\Flashcard` silinir. NSIS kaldırıcısındaki **"Uygulama verilerini sil"** kutusu: **işaretsizse** (varsayılan) veri klasörü `%APPDATA%\app.flashcard.desktop` kalır (yeniden kurulumda koleksiyon geri gelir); **işaretliyse** bu klasör (koleksiyon, ayarlar, loglar) kalıcı olarak silinir — geri alınamaz. Windows'un *Ayarlar → Uygulamalar* kaldırması kaldırıcıyı etkileşimli açtığı için kutu görünür; sessiz `/S` kaldırmada veri kalır.

**8. MSI (isteğe bağlı)**
25. Temiz makinede `Flashcard_<A>_x64_en-US.msi` → ✅ yönetici ister, `C:\Program Files\Flashcard\` altına kurar, 4–8. adımlar aynı geçer.

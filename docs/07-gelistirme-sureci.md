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
  - Migration numaraları iz başına önceden ayrılır (ör. v0.2: 0100–0199, v0.3: 0200–0299).
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
- Windows derlemesi ve kurulum dosyası GitHub Actions'ta (`windows-latest`) üretilir.
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

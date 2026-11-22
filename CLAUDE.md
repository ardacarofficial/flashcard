# CLAUDE.md

Bu dosya, bu repoda çalışan AI ajanları için başlangıç bağlamıdır.

## Proje

Rust + Tauri 2 ile masaüstü flash kart uygulaması. Tüm plan `docs/` klasöründedir. **Bir işe başlamadan önce `docs/README.md` ve ilgili dokümanları oku.**

- Durum: **v1.3.0: "kullanılır" sürümü** (tek çalışma başlatıcısı, bir tuş = bir iş, alan rolleri, kart eklemenin tek yeri, kendini anlatan ayarlar, AI hazır sağlayıcıları) — bkz. `docs/11-tasarim.md`, `CHANGELOG.md`. Windows WebDriver pilotu CI'da yeşil (kurulu uygulamada P, J1–J3; engellemeyen iş). Bekleyen: Windows'ta elle koşu, Subtitle eklentisinin gerçek bir tarayıcıda denenmesi.
- Öncelikli platform: **Windows** (macOS/Linux sonra).

## İletişim

- Kullanıcıyla **Türkçe**, samimi ve eğlenceli konuş.
- Karşılaştırmalarda tablo kullan ve sonunda özet ver.
- Süre sorulursa **saat bazında** cevap ver.

## Değişmez kurallar

1. **Tüm iş mantığı Rust'ta.** Frontend sadece gösterir. Frontend tamamen değiştirilse bile backend etkilenmemeli.
2. **`fc-core` saf kalır:** dosya, ağ, saat, global durum, doğrudan rastgelelik yok. Zaman `Clock`, rastgelelik `Rng` ile dışarıdan gelir.
3. **Özellik değil yapı taşı yaz.** Yeni özellikten önce hangi yapı taşlarının birleşimi olduğunu düşün (`docs/03-mimari.md`).
4. Her yazma işlemi bir **komut**tur: doğrulanır, transaction içinde çalışır, `oplog`'a tersiyle yazılır, geri alınabilir.
5. API sözleşmesi `fc-api`'dedir ve **sürümlüdür.** TS tipleri otomatik üretilir; elle DTO yazılmaz.
6. Şema değişikliği = yeni migration + migration testi.
7. Her özellik bir **modül**dür, ayarlardan kapatılabilir; arayüz menüleri `modules.list`'ten çizilir.
8. Arayüzde sabit metin yok, her şey i18n anahtarı (TR/EN).
9. AI slop yok: sade varsayılanlar, aşamalı gösterim, gereksiz süs yok. **Sade ≠ tasarımsız:** her ekran `docs/11-tasarim.md` tasarım sistemini kullanır ve görsel denetimden geçer.
10. Telemetri yok. API anahtarları sadece işletim sistemi anahtar zincirinde.

## Komutlar

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm -C apps/desktop install
pnpm -C apps/desktop check
pnpm -C apps/desktop tauri dev
pnpm -C apps/browser-extension install && pnpm -C apps/browser-extension check   # + test, build, e2e (FC_BIN=target/debug/fc)
```

## Doğrulama

Mock ekran görüntüleri (`pnpm -C apps/desktop screens`) yalnız görsel kontroldür. Bir akışın kabulü, gerçek Rust çekirdeğine dev köprüsüyle (`fc dev serve`) bağlı **yolculuklardır**: `pnpm -C apps/desktop e2e` (`apps/desktop/e2e/README.md`); yolculuk **kaydedilen sonucu** API'den geri okuyarak doğrular. Ayrıntı: `docs/11-tasarim.md` §5, `docs/07-gelistirme-sureci.md` §3.1.

## Bitti tanımı

Kabul kriterleri sağlandı · testler var ve geçiyor · clippy/fmt temiz · TS tipleri güncel · gerekiyorsa migration testi · doküman güncel · commit + push.

## Paralel çalışma

`docs/07-gelistirme-sureci.md` §5: her iz ayrı worktree ve dalda; migration numara aralıkları iz başına ayrılır; v0.1 tek elden yapılır.

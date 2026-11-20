# Flashcard tarayıcı eklentisi (Chromium MV3)

Bir sayfada kelime ya da cümle seç → sağ tık **Desteye ekle** (ya da `Alt+Shift+A`, ya da araç çubuğu düğmesi) → küçük pencerede ön/arka yüz, deste ve destenin sınıflandırmaları (ör. *Seviye*) → **Gelen kutusuna ekle**. Öğe masaüstü uygulamasının **gelen kutusuna** düşer; seçimin geçtiği cümle, sayfa başlığı ve adresi bağlam olarak gider. Ayrıntı: [docs/06 §5](../../docs/06-ai-ve-entegrasyonlar.md).

Eklenti, uygulamanın yerel HTTP API'sinin ([docs/06 §3](../../docs/06-ai-ve-entegrasyonlar.md)) sıradan bir istemcisidir: yalnız `127.0.0.1`, token ile, yalnız onaylanan kapsamlar (`inbox:write`, `decks:read`, `attributes:read`). Telemetri yok, başka adrese istek yok.

## Komutlar

```bash
pnpm -C apps/browser-extension install
pnpm -C apps/browser-extension check   # tsc (strict)
pnpm -C apps/browser-extension test    # vitest (birim + mock API)
pnpm -C apps/browser-extension build   # dist/ = paketlenmemiş eklenti
pnpm -C apps/browser-extension exec playwright install chromium   # bir kez
pnpm -C apps/browser-extension e2e     # build + Playwright (başsız Chromium, mock API)
# Gerçek uygulamaya karşı (iz I'nin `fc api serve`'ü olan bir fc ikilisiyle):
FC_BIN=$PWD/target/debug/fc pnpm -C apps/browser-extension e2e
```

## Elle kurulum ve deneme

1. `pnpm -C apps/browser-extension build`.
2. Chrome/Edge/Brave → `chrome://extensions` → **Geliştirici modu** → **Paketlenmemiş öğe yükle** → `apps/browser-extension/dist`.
3. Uygulamada **Ayarlar → Modüller → Yerel API**'yi aç (varsayılan port `28735`; değiştirdiysen eklentinin ayarlarına aynı portu yaz).
4. Eklenti düğmesi → **Bağlan** → uygulamadaki onay penceresinde **İzin ver**.
5. Bir sayfada kelime seç → sağ tık **Desteye ekle** → deste + *Seviye* seç → **Gelen kutusuna ekle** → uygulamanın gelen kutusunda görünür.
6. Uygulama kapalıyken ekle → "Kaydedildi; uygulama açılınca gönderilecek"; araç çubuğu rozetinde bekleyen sayısı. Uygulama açılınca (en geç 1 dk, ya da ayarlar → **Şimdi gönder**) gider.

## Yapı

| Dosya | Görev |
|---|---|
| `src/background/index.ts` | Servis çalışanı: sağ tık menüsü, kısayol, seçimi sayfadan okuma, rozet, 1 dakikalık gönderme alarmı, mesaj yönlendirme |
| `src/lib/service.ts` | Tüm iş akışı (`chrome.*`'sız, test edilir): eşleştirme, desteler/sınıflandırmalar (+ önbellek), ekleme, kuyruk |
| `src/lib/api.ts` | Yerel API istemcisi; ağ hatası istisna değil değer (`offline`) |
| `src/lib/outbox.ts` | Gönderilemeyenler: sıralı, `client_ref` ile tekrarsız, kalıcı ret ayrı (`failed`) |
| `src/lib/attributes.ts`, `src/popup/fields.ts` | Sınıflandırma alanları (tüm türler), son kullanılanı hatırlama |
| `src/lib/context.ts` | Seçimin cümlesi (saf) + sayfada çalışan okuma betiği |
| `src/lib/contract.ts` | Yollar, port, kapsamlar; tipler `@fc/api`'den (üretilmiş `fc-api` bağları) |
| `public/_locales/{en,tr}` | Tüm metinler (`chrome.i18n`) |
| `tests/mock-api/` | docs/06 §3'e göre yazılmış sahte yerel API (birim testlerde `fetch`, e2e'de gerçek HTTP) |

Token yalnız servis çalışanında kullanılır ve `chrome.storage.local`'da durur; açılır pencere ve ayarlar sayfası uygulamayla doğrudan konuşmaz, servis çalışanına mesaj gönderir.

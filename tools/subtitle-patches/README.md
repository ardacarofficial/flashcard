# Subtitle eklentisi için yamalar

Subtitle eklentisinin deposu (`/home/user/subtitle`) bu repodan **değiştirilmez**. Flashcard'ın
eklentide gerektirdiği değişiklikler burada `git apply` ile uygulanabilen yamalar olarak durur;
eklentinin sahibi kendi deposunda uygular ve kendi sürecinden (test, sürüm, mağaza) geçirir.

| Yama | Ne yapar | Taban (eklenti `HEAD`) |
|---|---|---|
| `0001-flashcard-known-words.patch` | "Flashcard'dan oku": Flashcard'ın bildiğin ve öğrendiğin kelimelerini okuyup altyazıyı onlara göre renklendirir | `7fe2843d9d52760bd589403e226b2fa506a8a0da` (sürüm 3.1.3, "Fix subtitle loading races and service lifecycle recovery") |

## Uygulamak

```bash
cd <subtitle deposu>
git checkout 7fe2843d9d52760bd589403e226b2fa506a8a0da      # ya da bu commit'i içeren bir dal
git apply --check <flashcard>/tools/subtitle-patches/0001-flashcard-known-words.patch
git apply <flashcard>/tools/subtitle-patches/0001-flashcard-known-words.patch
# commit olarak almak için: git am <…>.patch   (yama `git format-patch` biçiminde)
pnpm install && pnpm test && pnpm lint && pnpm build
```

## 0001 — Flashcard'dan oku

Anki'nin "Anki'den oku" yolunun (`syncAnkiFx`) Flashcard karşılığı (docs/14 §4'teki plan):

- **Nerede:** öğrenme servisi **Subtitle Kartları** iken, oynatıcının Ayarlar → Kelimeler sekmesinde
  ("Anki'den oku"nun yerinde) ve ayarlar sayfasının Öğrenme servisi bölümünde, sunucu ayarlarının
  altında **"Flashcard kelimeleri · Flashcard'dan oku"** satırı. Sonuç satırın altında ve bir
  bildirimde yazar: "N kelime okundu, M değişti", paylaşım kapalıysa "Flashcard kelimelerini
  paylaşmıyor. Flashcard'da Subtitle ekranında “Bildiğin kelimeleri Subtitle ile paylaş”ı aç, sonra
  yeniden oku.", hata olursa Türkçe nedeni (sunucu kayıtlı değil, anahtar geçersiz, sunucu kapalı…).
- **Arka plan** (`src/flashcards/background.ts`, yeni `knownWords` mesajı, `readKnownWords`): kayıtlı
  sunucu ve anahtarla (`persist:flashcardsServer`; CORS ve anahtar arka planda) önce `/v1/me`;
  `features.known_words === false` → `{ shared: false }`, kelime istenmez. Sonra `GET /v1/known-words`
  sayfaları `next_cursor` bitene kadar (en çok 200 sayfa × 500; görülmüş bir imleç döngüyü keser);
  bir sayfa `shared: false` derse yine paylaşım kapalı sayılır. Yalnız `known`/`learning` alınır.
- **Model** (`src/models/words/index.ts`): `syncFlashcardsFx` / `flashcardsSyncRequested`. Anki ile
  ortak `mergeReadWords`: kelime, altyazının baktığı sözlük hâline (`normalizeWord` + `wordRanks`)
  eşlenir; **kullanıcının "biliyorum" işareti kazanır** (bilinen kelime bilinen kalır), diğerleri
  Flashcard'ın durumunu alır ve `flashcards: true` işareti taşır (`TSavedWord.flashcards`). Okuma
  sırasında kullanıcının yaptığı düzenlemeler korunur (Anki'nin `keepEditsWhileReading` kuralı, artık
  ortak). Dil: oynayan altyazının dili; ayarlar sayfasında (altyazı yok) seviyesi ya da kayıtlı
  kelimesi olan her dil.
- **İstemci** (`src/flashcards/api.ts`): `knownWords()` artık sayfanın `shared` alanını da döndürür
  (söylemeyen sunucu paylaşıyor sayılır).
- **Yeni kalıcı depo yok:** kelimeler zaten yedeklenen `persist:words`'e yazılır; `settingsBackup`
  değişmez.
- **Playground:** sahte sunucuda `mockFlashcardsServer.shareKnownWords` (yakaladığı kelimeleri
  `learning` olarak geri verir).
- **Belgeler:** eklentinin `CLAUDE.md`'si (Known Words) ve `docs/flashcards-api.md` (`/v1/known-words`).

Bilinçli olarak yapılmayanlar: video açılışında kendiliğinden okuma (düğmeyle; günde bir otomatik
okuma sonra eklenebilir), Flashcard'dan silinen kelimenin eklentiden düşmesi (Anki yolu da yapmıyor),
401/403 mesajlarının `error.code`'a göre Türkçeleştirilmesi (docs/14 §4 madde 6).

### Nasıl sınandı

Eklenti deposunun kopyasında (`git clone`, 7fe2843):

- `pnpm test` — 137 dosya, 1528 test geçti (yeni: `src/flashcards/knownWords.test.ts` 5,
  `src/models/words/words.test.ts` +4, `src/pages/content/components/Settings/FlashcardsRow.test.tsx` 3)
- `pnpm lint` — 0 hata (18 uyarı, hepsi yamadan önce de vardı)
- `pnpm build` — geçti (`tsc --noEmit` + Vite)
- Gerçek tarayıcı: `SUBTITLE_PATCH=tools/subtitle-patches/0001-flashcard-known-words.patch node tools/subtitle-real-browser/run.mjs`
  — eklenti yamalı kopyadan derlenir, gerçek Chromium'a yüklenir, gerçek `fc api serve`'e bağlanır.
  Yalnız Flashcard'a yakalanmış "seven" kelimesiyle: paylaşım kapalıyken "Flashcard'dan oku" →
  "Flashcard kelimelerini paylaşmıyor…", `persist:words`'te Flashcard'dan gelen kelime yok, altyazıda
  "seven" boyanmamış; `subtitle.share_known_words` açılınca "Flashcard'dan oku" → "N kelime okundu",
  `en:seven` `learning` + `flashcards: true`, altyazıda "seven" `es-sub-item-learning` (turuncu).
  Ayrıntı ve sonuç: `docs/14-subtitle-gercek-tarayici.md` §4.

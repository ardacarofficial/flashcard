# 09 · Rakip Analizi

## 1. Anki ile karşılaştırma

| Konu | Anki | Bizim plan |
|---|---|---|
| 🧠 Algoritma | FSRS + SM-2 | FSRS (aynı resmi kütüphane) + takılabilir arayüz |
| 🎨 Arayüz | Eski görünümlü, karmaşık | Modern, sade varsayılanlar, tamamen değiştirilebilir frontend |
| 📚 Öğrenme eğrisi | Dik | İlk açılış rehberi, aşamalı gösterim |
| 🔌 Eklentiler | Python, çok güçlü; izin sistemi yok, sürüm güncellemelerinde sık bozulur | WASM sandbox, izin sistemi, sürümlü API |
| 🤖 AI | Yerleşik değil | Yerleşik sağlayıcılar + MCP istemci/sunucu |
| 🔊 Ses / STT / çeviri | Eklentilerle, dağınık | Standart sağlayıcı arayüzleri + model yöneticisi |
| 📥 İçe aktarma | Güçlü ama teknik | Esnek ayırıcı + canlı önizleme + otomatik algılama |
| 🌉 Dış entegrasyon | AnkiConnect (eklenti) | Yerleşik yerel API + token + gelen kutusu (+ isteğe bağlı AnkiConnect uyumu) |
| 📖 Okuma modu | Yok | Var (kelime renklendirme) |
| ↩️ Geri al | Var, sınırlı | Her komut geri alınabilir |
| ☁️ Senkronizasyon | Var (AnkiWeb) | Şimdilik yok |
| 📱 Mobil | Var | Şimdilik yok |
| 👥 Topluluk / hazır desteler | Devasa | Yok → **.apkg içe aktarma bu yüzden kritik** |

## 2. Anki'den alınanlar

- FSRS kişisel optimizasyonu, hedef hatırlama oranı
- Deste ayar şablonları (preset)
- Kolay günler, yük dengeleme
- Kart bilgi ekranı, kart tarayıcısı
- Veritabanı kontrolü
- Not / kart ayrımı, not türleri ve kart şablonları, cloze, resim üzerinde gizleme
- Kardeş gömme, leech tespiti
- .apkg / .colpkg içe ve dışa aktarma
- Arama sözdizimi fikri (`deck:`, `tag:`, `is:`, `prop:`)

## 3. Diğer uygulamalardan alınanlar

| Uygulama | Alınan |
|---|---|
| Quizlet | Esnek ayırıcılı içe aktarma; **Öğren modu** (çoktan seçmeliden yazarak cevaplamaya ilerleme); **test oluşturucu** |
| RemNote | Not içinde satır içi kart oluşturma (`soru :: cevap`); kavram bağlantıları |
| SuperMemo | Artımlı okuma ve özüt (extract) → kart akışı |
| LingQ / Migaku | Okuma modunda kelime durumu renklendirme; bilinen kelime yüzdesi |
| Brainscape | Güven bazlı (1-5) puanlama seçeneği |
| Mochi / Obsidian SR | Markdown öncelikli kart içeriği; `[[bağlantı]]` |
| Language Reactor | Altyazıdan cümle madenciliği (cümle + ses + ekran görüntüsü) |
| Duolingo | Konuşma/dinleme alıştırması fikri (eklenti olarak); oyunlaştırma alınmadı |
| Obsidian | Komut paleti, eklenti ekosistemi yaklaşımı |

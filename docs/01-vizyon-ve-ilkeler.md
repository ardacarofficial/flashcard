# 01 · Vizyon ve İlkeler

## 🎯 Vizyon

Anki'nin gücünü (aralıklı tekrar, esneklik, eklentiler) modern bir deneyimle, sağlam bir mimariyle ve yerleşik AI desteğiyle birleştiren bir masaüstü flash kart uygulaması.

İlk kullanıcı projenin sahibidir. Uygulama önce onun ihtiyaçlarını (özellikle dil öğrenimi: İngilizce ↔ Türkçe) eksiksiz karşılamalı, ama tasarımı genel amaçlı kalmalıdır.

## 👤 Hedef kullanıcı

| Kullanıcı | İhtiyacı |
|---|---|
| Dil öğrenen | Kelime, cümle, telaffuz, okuma, altyazıdan öğrenme |
| Sınava hazırlanan öğrenci | Sınav tarihine göre planlama, resimli sorular, formüller |
| İleri kullanıcı / geliştirici | Eklenti yazma, sorgu dili, otomasyon, yerel API |
| Anki'den gelen kullanıcı | .apkg içe aktarma, tanıdık kavramlar, daha iyi deneyim |

## 🧭 Temel ilkeler

### 1. 🛡️ Backend sağlam, frontend değiştirilebilir
- İş mantığının **tamamı** Rust tarafındadır. Frontend hesap yapmaz, karar vermez, sadece gösterir ve istek gönderir.
- Frontend ile backend arasında **sürümlü, tiplenmiş, sabit bir API sözleşmesi** vardır.
- Frontend tamamen silinip yeniden yazılsa bile backend'de tek satır değişmez.
- Ölçüt: Çekirdek, Tauri olmadan da (CLI, test, ileride mobil) çalışabilmelidir.

### 2. 🧱 Özellik değil, altyapı
- Her yeni özellik önce "Bu hangi yapı taşlarının birleşimi?" sorusuyla tasarlanır.
- Undo, kart geçmişi, istatistikler, toplu işlemler, çöp kutusu ayrı ayrı yazılmaz. Olay kaydı ve komut sisteminden doğal olarak çıkar.
- Yeni bir yetenek gerekiyorsa önce genel bir yapı taşı olarak eklenir, sonra özellik onun üstüne kurulur.

### 3. 🧮 Fonksiyonel çekirdek, ince kabuk
- Çekirdek (`core`) saf fonksiyonlardan oluşur: girdi alır, çıktı döner. Veritabanı, dosya sistemi, ağ, saat bilmez.
- Yan etkiler (I/O) sadece servis katmanında ve adaptörlerde olur.
- "Şimdiki zaman" ve rastgelelik dışarıdan verilir. Böylece her şey deterministik test edilir.

### 4. 🧩 Modülerlik
- Her özellik bir **modül** olarak kaydolur ve ayarlardan açılıp kapatılabilir.
- Kapalı modül arayüzde hiç görünmez (menü, buton, ayar sayfası).
- Eklentiler de aynı modül sistemine kaydolur.

### 5. 🧹 Sade arayüz, AI slop yok
- **Sade varsayılanlar:** İlk açılışta sadece desteler, çalış ve ekle.
- **Aşamalı gösterim:** Gelişmiş seçenekler "Gelişmiş" altında gizlidir.
- **Ekran başına tek amaç:** Bir ekran tek bir işi iyi yapar.
- **Ayarlar aranabilir:** Yüzlerce ayar olsa bile bulunur.
- Gereksiz emoji, gereksiz açıklama metni, gereksiz animasyon yok. Her öğe yerini hak etmeli.

### 6. 🔌 Genişletilebilirlik
- Kart türleri, algoritmalar, içe aktarıcılar, cevap kontrolcüleri, AI/TTS/STT/çeviri sağlayıcıları **takılabilir arayüzlerdir.**
- Yerleşik olanlar da bu arayüzleri kullanır. Yani yerleşik özellik ile eklenti arasında yetenek farkı yoktur (izinler hariç).

### 7. 🧪 Güvenilirlik
- Veritabanı şeması migration'larla yönetilir. Kullanıcının verisi asla bozulmaz.
- Tüm yazma işlemleri transaction içindedir.
- Her yapı taşının birim testi, her API komutunun entegrasyon testi vardır.

### 8. 🕵️ Gizlilik ve veri sahipliği
- Telemetri yok. Uygulama izinsiz hiçbir yere bağlanmaz.
- AI'a ne gönderildiği açıkça gösterilir.
- API anahtarları işletim sisteminin anahtar zincirinde saklanır.
- Kullanıcı tüm verisini her an açık formatlarda dışa aktarabilir.

## 🚫 Kapsam dışı (şimdilik)

| Konu | Neden |
|---|---|
| ☁️ Senkronizasyon | İçe/dışa aktarma şimdilik yeterli. Olay kaydı sayesinde altyapı ileride hazır olacak |
| 🎮 Oyunlaştırma (XP, rozet) | Odak öğrenme üzerinde |
| ♿ Erişilebilirlik modu | Sonraki aşamada |
| 📱 Mobil | v1.0 sonrası |
| 🎚️ Günlük kart limiti | Kullanıcı isteği üzerine çıkarıldı: kullanıcı istediği kadar çalışabilir |
| 💾 Otomatik periyodik yedekleme | İstenmedi. Yerine elle tam yedek + çöp kutusu var |

## 💻 Platform

- **Öncelik: Windows.**
- macOS ve Linux sonra gelir. Mimari ve bağımlılıklar baştan üç platforma uygun seçilir.

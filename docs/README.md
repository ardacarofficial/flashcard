# 📚 Flashcard: Proje Dokümantasyonu

Bu klasör, uygulamanın **tam planını** içerir. Geliştirme boyunca pusulamız budur. Bir şey burada yazmıyorsa ya henüz karar verilmemiştir ya da kapsam dışıdır.

> **Durum:** **v1.2.0** — arayüz akışlardan başlayarak yeniden tasarlandı ([11](11-tasarim.md)) ve Subtitle tarayıcı eklentisi bağlantısı eklendi ([06 §5.1](06-ai-ve-entegrasyonlar.md)). v1.0 (Windows sürümü) ve v1.1 (ilk arayüz tasarımı) tamam; bekleyen: Windows'ta elle koşu ([07 §8](07-gelistirme-sureci.md)) ve Subtitle eklentisinin gerçek bir tarayıcıda denenmesi. Ayrıntı: [04](04-yol-haritasi.md), [CHANGELOG](../CHANGELOG.md), kullanıcı rehberi [guide/](guide/README.md).

## 🗂️ Dokümanlar

| # | Doküman | İçerik |
|---|---|---|
| 01 | [Vizyon ve İlkeler](01-vizyon-ve-ilkeler.md) | Ne yapıyoruz, neden, hangi kurallara uyuyoruz |
| 02 | [Özellik Kataloğu](02-ozellik-katalogu.md) | Onaylanan tüm özellikler, detaylarıyla |
| 03 | [Mimari](03-mimari.md) | Katmanlar, yapı taşları, veri modeli, API sözleşmesi |
| 04 | [Yol Haritası](04-yol-haritasi.md) | Sürüm sürüm ne yapılacak, kabul kriterleri |
| 05 | [Eklenti Sistemi](05-eklenti-sistemi.md) | Eklenti altyapısı, izinler, sağlayıcılar, örnek eklentiler |
| 06 | [AI ve Entegrasyonlar](06-ai-ve-entegrasyonlar.md) | AI sağlayıcıları, MCP, yerel API, tarayıcı eklentisi, derin bağlantı |
| 07 | [Geliştirme Süreci](07-gelistirme-sureci.md) | Teknoloji yığını, klasör yapısı, test, paralel ajan çalışması, VDS |
| 08 | [Açık Kararlar](08-acik-kararlar.md) | Henüz karar verilmemiş konular |
| 09 | [Rakip Analizi](09-rakip-analizi.md) | Anki ve diğer uygulamalarla karşılaştırma, alınan fikirler |
| 10 | [Sözleşme ve sürüm kayıtları](10-v0.1-sozlesme.md) | v0.1 sözleşmesi ve sonraki sürümlerin uygulama/doğrulama kayıtları |
| 11 | [Tasarım (v1.2)](11-tasarim.md) | Akışlar, bilgi mimarisi, tasarım sistemi (token'lar, bileşenler), görsel doğrulama |

## ⚡ Tek cümlede

**Rust + Tauri** ile, iş mantığının tamamı sağlam bir Rust çekirdeğinde duran, frontend'i istendiği an tamamen değiştirilebilen, eklentilerle sınırsız genişletilebilen, FSRS tabanlı, yerel çalışan bir flash kart uygulaması.

## 🧭 Altın kurallar (özet)

1. 🛡️ **Frontend değişse de backend patlamaz.** Tüm iş mantığı Rust'ta.
2. 🧱 **Özellik değil, yapı taşı yazılır.** Özellikler yapı taşlarının birleşimidir.
3. 🧩 **Her özellik bir modüldür.** Açılır, kapanır, arayüzü doldurmaz.
4. 🧹 **AI slop yok.** Sade varsayılanlar, aşamalı gösterim.
5. 🧪 **Test edilmeyen kod ana dala girmez.**
6. 🕵️ **Gizlilik:** Telemetri yok, veri kullanıcının bilgisayarında.
- [11 · Tasarım (v1.1)](11-tasarim.md)

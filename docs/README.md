# 📚 Flashcard: Proje Dokümantasyonu

Bu klasör, uygulamanın **tam planını** içerir. Geliştirme boyunca pusulamız budur. Bir şey burada yazmıyorsa ya henüz karar verilmemiştir ya da kapsam dışıdır.

> **Durum:** **v1.4.0** — "kelime uygulaması" sürümü: kaybolamayan veri (doğrulanan akışlı yedek), "hangi kelimeleri biliyorum" sorusuna tek cevap (ortak kelime dizini, sıklık/CEFR, dil sorguları, bilinen kelimeler zamanla), Subtitle yakalaması bir karşılaşma (en öğretici sahne, zor kart onarımı), Test (bir kelime, dört anlam) ve Sınav, ölçülmüş hız ([12](12-performans.md)); arayüzde dört yerli kenar çubuğu (Ana sayfa · Kelimelerim · İlerleme · Ayarlar), tek "Gelişmiş" anahtarı, soru soru Temel ayarlar ve sade Günlük tempo ([11](11-tasarim.md)). Bekleyen: Windows'ta elle koşu ([07 §8](07-gelistirme-sureci.md)) ve Subtitle eklentisinin gerçek bir tarayıcıda denenmesi. Ayrıntı: [04](04-yol-haritasi.md), [CHANGELOG](../CHANGELOG.md), kullanıcı rehberi [guide/](guide/README.md) (v1.3'te ne değişti: [TR](guide/v1.3-tr.md) · [EN](guide/v1.3-en.md)).

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
| 11 | [Tasarım (v1.4)](11-tasarim.md) | Akışlar (başlatıcı, klavye modeli, kart ekleme, ayar kiti, kelime kartları), bilgi mimarisi, tasarım sistemi, doğrulama (yolculuklar, Windows pilotu, ekran görüntüleri) |
| 12 | [Performans (v1.4)](12-performans.md) | Ölçülmüş hız: 100k not / 2M tekrar + film izleme yükü (20k karşılaşma, ~1 GiB medya), makine, debug/release, düzeltilenler ve bilinen sınırlar |
| 13 | [Yapay zekâ gerçek sağlayıcıyla (v1.4)](13-ai-canli-deneme.md) | OpenRouter ile ilk gerçek deneme: ne koşuldu, ne çalıştı, bulunan ve düzeltilen hatalar, denenmeyenler, `FC_AI_LIVE=1` ile yeniden koşma |
| 14 | [Subtitle gerçek tarayıcıda (v1.4)](14-subtitle-gercek-tarayici.md) | Derlenmiş Subtitle eklentisi gerçek Chromium'da gerçek `fc`'ye karşı: kurulum, geçen adımlar, eklentide gereken değişiklik, sahibin tarayıcısını isteyenler |

## ⚡ Tek cümlede

**Rust + Tauri** ile, iş mantığının tamamı sağlam bir Rust çekirdeğinde duran, frontend'i istendiği an tamamen değiştirilebilen, eklentilerle sınırsız genişletilebilen, FSRS tabanlı, yerel çalışan bir flash kart uygulaması.

## 🧭 Altın kurallar (özet)

1. 🛡️ **Frontend değişse de backend patlamaz.** Tüm iş mantığı Rust'ta.
2. 🧱 **Özellik değil, yapı taşı yazılır.** Özellikler yapı taşlarının birleşimidir.
3. 🧩 **Her özellik bir modüldür.** Açılır, kapanır, arayüzü doldurmaz.
4. 🧹 **AI slop yok.** Sade varsayılanlar, aşamalı gösterim.
5. 🧪 **Test edilmeyen kod ana dala girmez.**
6. 🕵️ **Gizlilik:** Telemetri yok, veri kullanıcının bilgisayarında.

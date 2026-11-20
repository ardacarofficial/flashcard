# 08 · Açık Kararlar

Henüz karar verilmemiş konular. Karar verildikçe bu dosya güncellenir ve ilgili dokümana işlenir.

| # | Konu | Seçenekler | Varsayılan (karar gelene kadar) | Ne zaman gerekli? |
|---|---|---|---|---|
| K1 | 🏷️ Uygulamanın adı | — | Kod içinde `fc` öneki, çalışma adı "Flashcard" | v1.0 öncesi (derin bağlantı şeması, kurulum dosyası) |
| K2 | 📜 Lisans / açık kaynak | Açık kaynak (MIT/Apache-2.0, AGPL) · kapalı | Repo özel, lisans dosyası yok | Eklenti SDK'sı yayınlanmadan önce |
| K3 | 🏪 Eklenti mağazası kaynağı | Kendi sunucu · GitHub tabanlı liste · topluluk deposu | Sadece dosyadan kurulum | v0.7 sonrası |
| K4 | 🔐 Veritabanı şifreleme | Yok · SQLCipher ile isteğe bağlı | Yok (sadece uygulama kilidi) | v1.0 sonrası |
| K5 | 🖼️ Frontend çerçevesi | Svelte · React · diğer | **Svelte 5** (mimari değiştirmeye izin verir) | v0.1 |
| K6 | 🔌 Varsayılan yerel API portu | — | Sabit bir port, ayarlanabilir | v0.9 |
| K7 | 🧠 FSRS optimizasyonu kapsamı | Preset bazında · deste bazında · global | Preset bazında | v0.6 |
| K8 | 🧾 Kod imzalama (Windows) | Sertifika al · imzasız | İmzasız (SmartScreen uyarısı çıkar) | v1.0 |
| K9 | ☁️ Senkronizasyon | Kendi sunucu · dosya tabanlı (Dropbox vb.) · CRDT | Kapsam dışı | v1.0 sonrası |
| K10 | 🧠 FSRS model sürümü | FSRS-6 (21 ağırlık, tek kararlılık) · FSRS-7 (34 ağırlık, ek "hızlı" kararlılık izi → `cards`'a sütun/yan tablo gerekir) | **FSRS-6** (`fsrs-rs` 7 ile; v0.2) | v0.6 kişisel optimizasyonu ile birlikte |

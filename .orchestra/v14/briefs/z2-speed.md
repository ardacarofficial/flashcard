# Z2 — Hız: tam ölçek bütçelerini gerçekten tut

Önce `.orchestra/v14/COMMON.md`, `docs/12-performans.md`, `.orchestra/v14/STATE.md` oku.

## Durum
Z1 ölçtü (release, 4 çekirdek, tam ölçek dünya): dört bütçe kırmızı.

| Ölçüm | Sonuç | Bütçe |
|---|---|---|
| Açılış | 2235 ms | 2000 |
| study.start | 1140 ms | 500 |
| Tarayıcı sayfası | 108 ms | 100 |
| Kırılım istatistiği | 2887 ms | 1500 |

Z1'in gdb profili: açılışın çoğu `decks.tree` → `today_counts`; her deste düğümü kartlarını yeniden okuyor, v1.3'ten kalma boş kart denetimi de var. Yedekte tüm döküm bellekte (2M tekrarda tepe ~2,7 GB).

## İş
1. Dört kırmızı bütçeyi **bütçeyi gevşetmeden** yeşile çek. Önce profil al, tahminle değil ölçümle ilerle.
   - `today_counts`: desteler ağacı için kartları tek geçişte oku/grupla (deste başına sorgu yok). "Gösterilen sayı = verilen kart" garantisi **bozulmaz**: sayı ile oturumun verdiği kartlar aynı kod yolundan/aynı kuraldan çıkmalı; bunu kanıtlayan mevcut testler kalır, gerekirse eşdeğerlik testi ekle (eski yavaş yol test içinde referans olarak kalabilir, rastgele dünyalarda yeni yol = eski yol).
   - `study.start`, kırılım istatistiği, tarayıcı sayfası: aynı yöntem (profil → tek geçiş / eksik indeks / gereksiz tekrar okuma). Şema değişirse migration aralığın **1422–1429** + migration testi.
2. Yedek: dökümü akıtarak yaz ki tepe bellek tekrar sayısıyla büyümesin (hedef: 2M tekrarda < 1 GB). Biçim sürümü ve geri okunabilirlik değişmez; eski yedekler okunur.
3. Tam ölçek perf testini **en sonda bir kez daha baştan sona koş** ve `docs/12-performans.md` tablosunu o koşunun sayılarıyla güncelle (önce/sonra sütunu). Kırmızı kalan olursa neden kaldığını dürüstçe yaz; bütçe değiştirme.

## Sınırlar
- `fc-core` saf kalır. Davranış değişmez: aynı kartlar, aynı sıra, aynı sayılar.
- Arayüze dokunma (tasarım ayrı izde).
- Bitti: fmt, clippy, `cargo test --workspace`, gen-ts --check, desktop check/test, e2e 33/33, Subtitle sözleşme denetimi; CHANGELOG + docs; commit (push yok).

## Rapor
Türkçe, kısa: önce/sonra tablosu, ne değişti, hangi denetimleri gerçekten koştun, açık kalan.

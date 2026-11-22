# D3 — Oluştur, Ara, İstatistik prototype → `prototypes/v14/olustur-ara-istatistik.html`
Read `.orchestra/v14/briefs/d-common.md` first. One file, three areas, plus the left navigation so the owner
sees the whole information architecture.
The owner: "Statistics are confusing, Search is a bit odd, Create is a bit odd."
Look hard at what these screens are today (screens PNGs + code under `apps/desktop/src/lib/features/`) and at
the navigation: Ana sayfa, Kütüphane, Oluştur, Ara, İstatistik, Gelen kutusu, Ayarlar, plus the "Kart ekle"
button. Ask of each: does it deserve to be a top-level place?
- **Oluştur** vs **Kart ekle** vs **Kütüphane**: today "Kart ekle" (tek kart / toplu / yapıştır / dosyadan)
  and "Oluştur" (AI generation etc.) overlap in the user's head. Propose ONE place to add things (variant A),
  and the best alternative (variant B). Show where AI generation, reading/subtitle mining and import go.
- **Ara**: for a normal user this is "Kelimelerim": a list he can type into to find a word, see its meaning,
  status (yeni / öğreniyorum / biliyorum) and edit it. The query language, columns, bulk actions, card info are
  advanced. Maybe it lives inside Kütüphane. Prototype it.
- **İstatistik**: first view = three answers: "Bugün ne yaptım?", "İlerliyor muyum?" (bildiğim kelime sayısı
  over time), "Yarın ne var?". One sentence each in words, one small chart each. Everything else (heatmap,
  retention, intervals, hourly, leeches, forecast) under "Daha fazla" with a one-line explanation per chart.
Deliver the navigation proposal as its own A/B question (e.g. 4 places instead of 7).

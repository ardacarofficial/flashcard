# 06 · AI ve Entegrasyonlar

## 1. AI sağlayıcıları

### 1.1 Yapı
- `AiProvider` arayüzü: `complete(request) -> response`, `stream(request)`, `capabilities()` (araç kullanımı, görsel girdi, JSON çıktı, bağlam uzunluğu), `estimate_cost(...)`.
- Yerleşik sağlayıcılar: **Anthropic (Claude)**, **OpenAI**, **OpenAI uyumlu uç nokta** (OpenRouter, LM Studio vb.), **Ollama** (yerel).
- Birden fazla sağlayıcı tanımlanabilir; **özellik başına** sağlayıcı/model seçilebilir (ör. kart üretimi için güçlü model, cevap kontrolü için hızlı/ucuz model).
- Model listesi sağlayıcıdan çekilir ya da elle girilir. Model adları kodda sabit tutulmaz.
- Tüm istekler arka plan işi veya iptal edilebilir görevdir; zaman aşımı ve yeniden deneme politikası vardır.
- Yapılandırılmış çıktı: AI'dan kart istenirken JSON şeması verilir, yanıt doğrulanır; geçersizse onarım denemesi yapılır.

### 1.2 Güvenlik ve gizlilik
- API anahtarları **işletim sistemi anahtar zincirinde.** Veritabanında, ayar dosyasında, logda asla yok.
- AI'a gönderilecek içerik, ilk kullanımda ve "gelişmiş" görünümde **önizlenir** ("Şu 20 kart gönderilecek").
- Kullanıcı AI modülünü tamamen kapatabilir.

### 1.3 Kullanım ve harcama takibi
- Her çağrı `ai_usage` tablosuna: sağlayıcı, model, özellik, girdi/çıktı token, tahmini maliyet.
- Ayarlar ekranında: bu ay / bu hafta toplam, özelliğe ve modele göre dağılım.
- İsteğe bağlı aylık bütçe uyarısı ("Bu ay 5$ sınırına yaklaştın").
- Fiyat tablosu ayarlardan düzenlenebilir (sağlayıcılar fiyat değiştirir).

### 1.4 AI özellikleri

| Özellik | Açıklama |
|---|---|
| 📄 Kaynaktan kart üretme | Metin, PDF, web sayfası, görsel → seçilen not türüne uygun kartlar. Önce önizleme ve düzenleme, sonra ekleme |
| 🧑‍🏫 Bunu açıkla | Çalışırken kartın konusunu açıklama, örnek verme |
| ✍️ Akıllı cevap kontrolü | Yazarak/sesle cevaplamada anlam bazlı değerlendirme (`AnswerChecker` uygulaması) |
| 🪄 Kart iyileştirme | Belirsiz/kötü yazılmış kartı netleştirme önerisi (fark gösterilir) |
| 🐌 Zor kartı yeniden yazma | Leech kartlar için farklı açıdan yeni kart önerisi, anımsatıcı (mnemonic) |
| 🎲 Çeldirici üretme | Çoktan seçmeli kartlar için mantıklı yanlış seçenekler |
| 💬 AI ipucu | Cevabı vermeden yönlendiren ipucu (`HintStrategy`) |
| 📝 Alan doldurma | Kelime yazınca anlam, örnek cümle, telaffuz alanlarını doldurma |
| 📖 Okuma modunda anlam | Bağlama göre kelime anlamı |

Her özellik için kullanılan istem (prompt) şablonları ayarlardan görülebilir ve özelleştirilebilir (gelişmiş).

## 2. MCP

### 2.1 MCP sunucusu (uygulama → dış AI'lar)
Claude Desktop, Claude Code veya başka bir MCP istemcisi uygulamaya bağlanıp işlem yapabilir.
- Taşıma: stdio (uygulamanın yanında gelen küçük köprü çalıştırılabilir dosyası, çalışan uygulamaya yerel API üzerinden bağlanır) ve/veya yerel HTTP (Streamable HTTP).
- **Araçlar kayıt defterinden otomatik üretilir:** MCP'ye açık olarak işaretlenmiş komut/sorgular (ör. `list_decks`, `search_cards`, `create_notes`, `get_stats`, `start_custom_study`).
- Kaynaklar (resources): desteler, not türleri, istatistik özetleri.
- İzinler: MCP istemcisi bir "bağlantı" olarak kaydedilir; okuma/yazma izinleri ayrı ayrı verilir. Yazma işlemleri isteğe bağlı olarak gelen kutusuna düşer (onaylı mod).
- Örnek: "Claude, bu makaleden 20 kart çıkar ve İngilizce destesine ekle."

### 2.2 MCP istemcisi (uygulama → dış MCP sunucuları)
- Kullanıcı MCP sunucusu ekler (komut veya URL).
- Uygulamadaki AI özellikleri bu sunucuların araçlarını kullanabilir (ör. Notion'dan not çekip kart üretme, dosya sisteminden okuma).
- Her araç çağrısı kullanıcıya gösterilir; hassas araçlar onay ister.

Uygulama için resmi Rust MCP SDK'sı (`rmcp`) kullanılır.

## 3. Yerel HTTP API

- `127.0.0.1:<port>` (varsayılan sabit port, ayarlanabilir). Sadece yerel bağlantı.
- **Kimlik doğrulama:** Bearer token. Yeni bir istemci ilk bağlandığında uygulamada onay penceresi: "Chrome eklentisi bağlanmak istiyor. İzin ver / Reddet." Onaylanınca istemciye özel token verilir. Token'lar ayarlardan görülür, iptal edilir.
- **İzin kapsamları:** istemci başına `inbox:write`, `notes:write`, `decks:read`, `study:read` vb.
- **Uç noktalar:** Kayıt defterindeki HTTP'ye açık komut/sorgular, `POST /v1/<komut>` biçiminde. Ayrıca kolaylık uç noktaları: `POST /v1/inbox` (hızlı ekleme), `GET /v1/decks`.
- **CORS:** Sadece izin verilmiş tarayıcı eklentisi kökenleri.
- **Hız sınırı** ve istek boyutu sınırı.
- AnkiConnect uyumluluk katmanı (isteğe bağlı, eklenti olarak): Anki için yazılmış mevcut araçların bir kısmı doğrudan çalışabilir.

## 4. Gelen kutusu

- Dış kaynaklardan (HTTP API, tarayıcı eklentisi, MCP, pano izleme, derin bağlantı) gelen öğeler önce `inbox`'a düşer.
- Öğe: ham veri + kaynak + bağlam (sayfa adresi, cümle, zaman).
- Gelen kutusu ekranı: tek tek veya toplu onayla → not türü ve deste seç → ekle. AI ile alan doldurma seçeneği.
- Kaynak bazında "doğrudan ekle" kuralı (otomasyon kurallarıyla birleşir).

## 5. Tarayıcı eklentisi

- Chromium tabanlı tarayıcılar (Chrome, Edge, Brave) için; Firefox ileride.
- Seçili metin → sağ tık → "Desteye ekle" veya kısayol. Küçük açılır pencere: ön/arka, deste seçimi.
- Bağlam: seçimin içinde geçtiği cümle, sayfa başlığı ve adresi otomatik eklenir.
- Uygulama kapalıysa istek kuyruğa alınır, uygulama açılınca gönderilir.
- Eklenti, yerel HTTP API'nin sıradan bir istemcisidir (özel yetkisi yoktur).

## 6. Derin bağlantılar

- Şema: `flashcard://` (uygulama adı kesinleşince değişebilir, bkz. açık kararlar).
- `flashcard://deck/<id>` · `flashcard://note/<id>` · `flashcard://study?deck=<id>` · `flashcard://study?query=<sorgu>` · `flashcard://add?deck=<id>&front=...&back=...` · `flashcard://import?url=...`
- Yazma yapan bağlantılar (add, import) onay ister ya da gelen kutusuna düşer.
- Tek örnek (single instance): uygulama açıksa bağlantı mevcut pencereye iletilir.
- Kart içi `[[bağlantılar]]` da dahili olarak aynı yönlendiriciyi kullanır.

## 7. Global hızlı ekleme ve pano izleme

- **Global kısayol** (ör. `Ctrl+Alt+N`): uygulama arka plandayken küçük, her zaman üstte bir ekleme penceresi. Panodaki metin önceden doldurulabilir.
- **Pano izleme** (varsayılan kapalı): kopyalanan metin filtrelere uyuyorsa (uzunluk, dil, desen) köşede küçük bir öneri: "Kart olarak ekle?" Görmezden gelinirse kaybolur. Gizlilik için parola yöneticisi gibi uygulamalardan gelen içerik (işletim sistemi işaretliyse) yok sayılır.

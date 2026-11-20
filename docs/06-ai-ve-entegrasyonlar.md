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

#### Uygulanan hâli (v0.8, `fc-mcp` + `fc-app` `mcp` modülü)

- **Taşıma:** stdio. `fc --db <collection.db> mcp serve [--read-only]` koleksiyon dosyasını **doğrudan** açar (SQLite WAL; masaüstü açıkken de çalışır, ama masaüstü MCP'nin yaptığı değişikliği oplog konumunu yoklayarak ≤ 2 sn içinde ya da pencereye odaklanınca görür ve ekranlarını yeniler (`data_changed`); **masaüstünde Ctrl/⌘+Z oplog'un son adımını geri alır — bu Claude'un yaptığı adım da olabilir**). Sunucu şemayı **yükseltmez**: eski şemalı koleksiyonu reddeder, önce uygulamayı açmak gerekir. Çalışan uygulamaya yerel HTTP API üzerinden bağlanan köprü v0.9'da (yerel API ile) gelir. Dosya yoksa sunucu açılmaz (yanlış yolda boş koleksiyon yaratmaz); stdout yalnızca protokole aittir, hatalar stderr'e.
- **Araçlar** (`fc_app::modules::mcp::tools::catalog`, `mcp.tools.list` ile de görülür): kayıt defterinden otomatik değil, **elle seçilmiş** ince sarmalayıcılar — her biri yalnız **public** v1 metotları çağırır; insan referanslarını (deste yolu, not türü/alan adı, sınıflandırma anahtarı/adı, seçenek etiketi) kimliğe çevirir:

  | Araç | Okur/Yazar | Metotlar | Not |
  |---|---|---|---|
  | `list_decks` | okur | `decks.tree` | yol, kimlik, kart sayıları |
  | `list_note_types` | okur | `notetypes.list` | alan anahtarları; varsayılan Klasik |
  | `list_classifications` | okur | `attributes.effective` | destede geçerli (miraslı) sınıflandırmalar, seçenekler sırayla |
  | `search_cards` | okur | `search.cards` | sorgu dili (`deck:İngilizce seviye>=B1 is:due`), konumlu hata |
  | `get_note` | okur | `notes.get` | alanlar, etiketler, sınıflandırmalar etiketle |
  | `create_deck` | yazar | `decks.create` | varsa olanı döndürür (idempotent) |
  | `add_notes` | yazar | `batch.execute` + `notes.create` | ≤100 not, `{"Seviye":"B1"}`; **hepsi ya da hiçbiri, tek geri al adımı** |
  | `update_note` | yazar (değiştirir) | `notes.update` | verilmeyen alan/etiket/sınıflandırma korunur |
  | `set_classification` | yazar (değiştirir) | `search.ids` + `bulk.set_attribute` | **`confirm: true` olmadan yalnız önizler** (kaç not değişecek + `confirm_token`); uygulamak için aynı çağrı `confirm: true` ve bu belirteçle tekrarlanır (eşleşen küme değiştiyse reddedilir; en çok 2000 kart) |

  Silen araç yok. Araç hataları MCP "tool error" olarak uygulamanın `ApiError` JSON'u (`code`, `message`, `details`; ör. `note_index`) döner, model okuyup düzeltebilir; bilinmeyen araç protokol hatasıdır. Araç ek açıklamaları: `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint=false`.
- **İzinler:** aktör `Actor::Mcp(<clientInfo.name>)` (oplog'da `{"kind":"mcp","id":"claude-ai"}`). Bu aktör yalnız `MCP_PERMISSIONS`'ı taşır (`decks/notes/attributes:read+write`, `notetypes/search/settings/modules/mcp:read`, `batch:write`); ayar/modül değiştirme, geri al, iç metotlar, `mcp:write` (dış sunucu kaydetme = uygulamaya komut çalıştırtma) **reddedilir** — `batch.execute` içinde de. İzin tek başına yetmez: MCP aktörü yalnız araç kataloğunun çağırdığı metotlara erişir; silme/çöp boşaltma (`notes.delete`, `decks.delete`, `browse.purge_trash`) `notes:write` izniyle kapsansa da kapalıdır. `mcp:write` Plugin/Automation/LocalApi aktörlerine de verilmez (yalnız kullanıcı ve CLI). Yazmalar sıradan komuttur: doğrulanır, transaction, oplog (+ ters), uygulamadan geri alınır.
- **Açma/kapama:** modül `mcp` (kapalıysa araç listesi boş, çağrılar `CONFLICT_MODULE_DISABLED`); ayar `mcp.server.allow_writes` (app, varsayılan açık; kapalıysa yazma araçları listelenmez ve `PERMISSION_MCP_READ_ONLY`); `--read-only` bayrağı aynısını süreç için yapar. Ayar ve modül her çağrıda veritabanından tazelenir (masaüstündeki değişiklik çalışan sunucuya yansır).

#### Kurulum

`fc` ikilisi `PATH`'te değilse tam yolunu yaz (Windows'ta ör. `C:\Program Files\Flashcard\fc.exe`; JSON içinde ters eğik çizgiler `\\` yazılır). Koleksiyon dosyası masaüstü uygulamasının veri klasöründeki `collection.db`'dir.

**Claude Code:**

```bash
claude mcp add flashcard -- fc --db "/path/to/collection.db" mcp serve
# yalnız okuma:
claude mcp add flashcard-ro -- fc --db "/path/to/collection.db" mcp serve --read-only
```

**Claude Desktop** (`claude_desktop_config.json`; Windows `%APPDATA%\Claude\`, macOS `~/Library/Application Support/Claude/`):

```json
{
  "mcpServers": {
    "flashcard": {
      "command": "fc",
      "args": ["--db", "/path/to/collection.db", "mcp", "serve"]
    }
  }
}
```

Uygulamadaki **MCP** ekranı aynı parçacıkları, araç listesini ve yazma anahtarını gösterir.

### 2.2 MCP istemcisi (uygulama → dış MCP sunucuları)
- Kullanıcı MCP sunucusu ekler (komut veya URL).
- Uygulamadaki AI özellikleri bu sunucuların araçlarını kullanabilir (ör. Notion'dan not çekip kart üretme, dosya sisteminden okuma).
- Her araç çağrısı kullanıcıya gösterilir; hassas araçlar onay ister.

#### Uygulanan hâli (v0.8: iskelet)
- Kayıtlar: `mcp.servers.list/add/update/remove` (geri alınabilir; iç ters `mcp.servers.restore`), tablo `mcp_servers` (migration `0750`): ad (tekil), `stdio` (komut + argümanlar) ya da `http` (URL; saklanır, bağlanma henüz yok), açık/kapalı, **onay** (migration `0751`: kullanıcının onayladığı başlatma özeti; kaydı arayüzde komut gösterilip onaylanarak eklemek/kaydetmek onaylar, yedekten gelen ya da elle değişen satır onaysızdır). **Ortam değişkeni saklanmaz** (sırlar yalnız anahtar zincirinde).
- `fc_mcp::McpClient`: kapalı ya da onaysız kaydı hiç başlatmaz (`McpError::NotAllowed`); aksi halde stdio sunucuyu başlatıp el sıkışır, araçları listeler (`ExternalTool`), araç çağırır, kapatır. Onay politikası `ExternalTool::needs_confirmation`: dış sunucunun ipuçları güvenilmez sayılır, yalnız politikayı sıkılaştırabilir — açıkça `readOnlyHint=true` ve yıkıcı olmayan araç dışında her çağrı kullanıcı onayı ister. AI özelliklerine (iz G) bağlama ve çağrıların arayüzde gösterilmesi sonraki adım.
- CLI: `fc mcp tools <kayıtlı ad>` ya da `fc mcp tools -- <komut> <argümanlar…>` araçları listeler (`!` = onay ister).

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

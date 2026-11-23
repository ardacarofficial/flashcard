# 06 · AI ve Entegrasyonlar

## 1. AI sağlayıcıları

### 1.1 Yapı
- `AiProvider` arayüzü: `complete(request) -> response`, `stream(request)`, `capabilities()` (araç kullanımı, görsel girdi, JSON çıktı, bağlam uzunluğu), `estimate_cost(...)`.
- Yerleşik sağlayıcılar: **Anthropic (Claude)**, **OpenAI**, **OpenAI uyumlu uç nokta** (OpenRouter, LM Studio vb.), **Ollama** (yerel).
- Birden fazla sağlayıcı tanımlanabilir; **özellik başına** sağlayıcı/model seçilebilir (ör. kart üretimi için güçlü model, cevap kontrolü için hızlı/ucuz model).
- Model listesi sağlayıcıdan çekilir ya da elle girilir. Model adları kodda sabit tutulmaz.
- Tüm istekler arka plan işi veya iptal edilebilir görevdir; zaman aşımı ve yeniden deneme politikası vardır.
- Yapılandırılmış çıktı: AI'dan kart istenirken JSON şeması verilir, yanıt doğrulanır; geçersizse onarım denemesi yapılır.

#### Uygulanan hâli (v1.3): ön ayarlar, model listesi, bağlantı testi

- **Ön ayarlar bir yapı taşıdır, motor değil** (`crates/fc-ai/src/presets.rs`): Claude, OpenAI, OpenRouter, Google Gemini (OpenAI uyumlu uç nokta), Groq, Mistral, DeepSeek, Ollama (yerel), LM Studio (yerel), "Özel (OpenAI uyumlu)". Her biri dört protokolden birini (`anthropic`, `openai`, `openai_compatible`, `ollama`) kullanır ve şunları taşır: ad, varsayılan adres, anahtar gerekir mi, anahtarın alındığı sayfa, indirme sayfası (yereller), yerel/ücretsiz bayrakları, önerilen modeller. `ai.providers.list` → `presets` arayüzün galerisini çizer. Adresler ve anahtar sayfaları Kasım 2026'da üreticilerin belgeleriyle karşılaştırıldı; model önerileri bilerek az: asıl liste sağlayıcıdan çekilir.
- **Ön ayar saklanmaz, adresten tanınır** (`presets::detect(kind, base_url)`): adres değişince eski bir "bu OpenRouter" bayrağı kalmaz, v1.3 öncesi sağlayıcılar göç gerekmeden doğru kuralı alır. `AiProviderDto.preset` tanınan ön ayardır.
- **Anahtar kuralı** (`presets::needs_key`): barındırılan servisler (OpenRouter dahil) anahtar ister; bilinmeyen bir OpenAI uyumlu adres bu bilgisayardaysa ya da yerel ağdaysa (`localhost`, `127.x`, `::1`, özel IPv4 aralıkları, `*.local`, `*.lan`, noktasız ev ağı adları) anahtar isteğe bağlıdır, değilse gerekir. v1.2'deki hata buydu: OpenAI uyumlu sağlayıcı hiç anahtar istemediği için OpenRouter'ın anahtar alanı gizleniyordu. `ai.run` anahtarsız bir "anahtar gerekir" sağlayıcısına hiçbir şey göndermez.
- **Durum** (`AiProviderDto.status`, Rust hesaplar): `ready` · `needs_key` · `needs_model` · `test_failed` (bu ayarın son testi başarısızsa).
- **Model listesi**: `ai.providers.refresh_models` → `ai.models` işi; sonucu tipli `AiModelList`, liste sağlayıcı satırında önbelleklenir (`models`). Anthropic `/v1/models` (sayfalı), OpenAI uyumlu `/models` (`data[].id` ya da çıplak dizi), Ollama `/api/tags`. Seçici aranabilir; listede olmayan bir ad da yazılabilir.
- **Bağlantıyı dene**: `ai.providers.test` → `ai.test` işi → `AiConnectionTest { outcome, model, answered_model, http_status, message, latency_ms, checked_at }`. Tek ve en küçük istek: seçili modele "Reply with OK." (en çok 16 çıktı token'ı; model yoksa ücretsiz model listesi isteği). Sonuç ham HTTP durumu ve gövdesinden sınıflanır (`fc_ai::diagnose::classify`): `ok`, `no_key` (hiçbir şey gönderilmedi), `wrong_key`, `no_credit` (402; OpenAI 429 `insufficient_quota`; Anthropic "credit balance"; OpenRouter'ın 200 içindeki hata nesnesi), `model_not_found`, `wrong_address` (HTML ya da başka bir API yanıt verdi), `rate_limited`, `service_down`, `cannot_reach`, `tls` (sertifika/araya giren vekil), `failed`. Sağlayıcının kendi mesajı anahtardan arındırılarak gösterilir; reddedilen anahtar yanıtının gövdesi hiç tutulmaz. Test 30 sn'de keser, kullanım tablosuna yazılmaz (birkaç token) ve sonucu yalnızca bu uygulama oturumu boyunca bellekte tutulur; anahtar, adres ya da model değişince unutulur.
- **Arayüz** (`features/ai/ProvidersPanel.svelte`): sayfa AI'ın ne işe yaradığını ve kurulana kadar kapalı olduğunu söyleyen tek cümleyle başlar. "Sağlayıcı ekle" → ön ayar galerisi (logosuz kartlar: ad, tek satır açıklama, "ücretsiz seçenek var" / "yerel · ücretsiz") → tam üç adım: 1 anahtarı yapıştır (anahtar sayfası bağlantısı; yerel/özel için adres ve isteğe bağlı anahtar) → 2 model seç → 3 bağlantıyı dene. Sağlayıcı 1. adım onaylanınca oluşturulur (anahtar ancak bir sağlayıcı kimliğiyle anahtar zincirine yazılabilir); model seçilmeden kapatılırsa silinir. Liste yoğun satırlar (durum, varsayılan, yerel), satırda "Bağlantıyı dene"; ayrıntıda anahtar, model seçici ve "Gelişmiş" (adres, ad). Varsayılan sağlayıcı, özellik başına yönlendirme ve fiyatlar ayrı başlıklarda kaldı.
- **İki yer, iki iş** (X2, `features/ai/AiScreen.svelte` yerine göre çizer): **Ayarlar › Bağlantılar › Yapay zeka** = sağlayıcılar (`ProvidersPanel`) + kullanım (`UsagePanel`), alt başlık AI'ın nerede kullanıldığını söyler ve "Kart üret" bağlantısı verir; **Oluştur › Yapay zeka** = yalnız "Yapay zekayla kart üret" (`GeneratePanel`; deste formdaki bir alan, sağlayıcı yoksa "Sağlayıcı bağla" Ayarlar'a götürür). Metotlar ve davranış aynı kaldı.
- **Eklentiler:** `ai.test` işinin sonucu (sonuç, modeller, HTTP durumu, sağlayıcının arındırılmış mesajı) sağlayıcının `last_test`'i gibi **AI yapılandırmasıdır**, not metni içermez: eklenti onu `jobs:read` + `ai:read` ile görür (`plugins/redact.rs`). `ai.models` ve `ai.request` işleri not metni taşıyabildiği için ayrı (AI + notlar) sınıftadır. Testi başlatmak (`ai.providers.test`, `ai_requests:write`) eklentilere kapalıdır.
- Bilinen sınır: masaüstü kabuk dış bağlantıları kendisi açmayabilir (opener eklentisi yok); bu yüzden anahtar/indirme sayfası adresi metin olarak da gösterilir ve kopyalanabilir.
- Bilinen sınır: gerçek sağlayıcı anahtarlarıyla uçtan uca deneme yapılmadı; tanıma, anahtar kuralı ve sınıflandırma, sağlayıcıların belgelerindeki yanıt biçimlerini taklit eden yerel bir sahte sunucuya karşı test edildi (`crates/fc-ai/src/tests_presets.rs`).

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
| 🎯 Bu cümledeki anlam (v1.4) | Yakalanan cümlede kelimenin hangi anlamı ("pick up the kids"); uygulanınca kabul edilen cevaplar |

Her özellik için kullanılan istem (prompt) şablonları ayarlardan görülebilir ve özelleştirilebilir (gelişmiş).

#### Uygulanan hâli (v1.4): bu cümledeki anlam ve kabul edilen cevaplar

Gerçek ihtiyaç: *"Can you pick up the kids?"* cümlesinde "pick up"ın hangi Türkçe anlamı? Dil modeli burada iyi; **AI'a verilmeyenler** aynı kaldı: zamanlama (FSRS), kesin cevap kontrolü, bir kelimeyi "bilinen" ilan etme.

- **Görev `meaning_in_context`** (`AiTask::MeaningInContext`, `AiFeature::MeaningInContext`, istem `ai.prompt.meaning_in_context`): girdi cümle, kelime (`lemma`), cümledeki biçimi (`surface`) ve isteğe bağlı karakter konumu, cevap dili (`target_language`, yoksa uygulama dili) ve sözlük/yakalama adayları (`candidates`). `note_id` verilirse eksikler notun **alan rollerinden** gelir (terim, görünen biçim, örnek cümle) ve notun şimdiki anlamı da aday olarak eklenir; kaynak/başlık gönderilmez. Çıktı (katı JSON şeması, doğrulanır, geçersizse **bir** onarım): `meaning` (bu cümleye uyan anlam), `phrase` (dayandığı sözcükler — cümlede tam sözcük olarak geçmek zorunda, yoksa onarım), `expression` (deyim / öbek fiil mi), `also_correct` (aynı anlamın en çok 4 başka doğru karşılığı), `not_this` (bu cümleye **uymayan** en çok 3 anlam), `confidence` (0–1). Her sağlayıcıyla aynı istemci üzerinden çalışır (Claude zorunlu araç, OpenAI/uyumlu `json_schema`, Ollama `format`; yerel modelin kod çiti soyulur).
- **Hiçbir şey kendiliğinden yazılmaz.** Sorma, her AI isteği gibi `ai.preview` → `ai.run` (bütçe önizlemede görünür); iş sonucu `AiOutput::MeaningInContext` yalnızca gösterilir, `original` notun şimdiki (kaynağın) anlamıdır.
- **Uygulamak bir komuttur** (`ai.meaning.apply`, tek geri alma adımı): anlam notun alanlarından **ayrı** `note_meanings` satırına yazılır (ifade, dayanak, diğer karşılıklar, uymayanlar, güven, model, düzenlendi mi); `original` ilk uygulamadan önceki alan değeridir ve sonraki uygulamalarda korunur. Anlam alanı yalnız `replace_field: true` ile değişir (iç içe `notes.update`). Alan kaynağın ifadesini koruyorsa uygulanan anlam da kabul edilen cevap olur; kullanıcının seçtiği `accepted` karşılıklar eklenir (iç içe `accepted_answers.add`). `ai.meaning.get` notun anlamını ve kabul edilen cevaplarını döndürür.
- **Önizlemesiz öneri yalnız açık rızayla:** ayar `ai.meaning.auto_suggest` (varsayılan **kapalı**). Açıkken `ai.meaning.suggest {note_id}` isteği önizlemesiz kuyruğa koyar; aylık bütçe dolduysa reddeder (`CONFLICT_AI_BUDGET_EXCEEDED`), iş her gönderimden önce bütçeyi yeniden denetler. Sonuç **öneri** (`status: suggested`) olarak saklanır: not değişmez, uygulanmış bir anlamın üstüne yazılmaz, geri alma geçmişine girmez. Subtitle yakalaması için kanca: `fc_app::modules::ai::suggest_after_capture(ctx, note_id)` (kapalı/bütçe dolu/sağlayıcı yoksa `None`, yakalamayı asla bozmaz).
- **Kabul edilen cevaplar** (modül `accepted_answers`, kapatılabilir): not + alan başına liste (`accepted_answers.list/add/remove`, geri alınabilir; en çok 20, 300 karakter). Çalışmadaki yazılı cevap kontrolü (`study.check_answer`) kartın beklediği cevap hangi alanın metniyse o alanın listesini **çevrimdışı** kabul eder (`fc_study::answer::check_accepting`: her seçenek aynı denetleyiciyle, en iyi sonuç kazanır) — "give up" için `vazgeçmek` de `bırakmak` da geçer, çalışma anında model çağrılmaz. Ters kart (anlamdan kelimeye) kelime alanını beklediği için anlam listesini almaz. Modül kapalıyken listeler durur ama kontrol onları yok sayar.
- **Saklama:** migration `1450_meaning_in_context` (`note_meanings`, `note_accepted_answers`); not çöpe gidince satırlar kalır, not kalıcı silinince tetikleyiciyle gider.
- **Sağlayıcı reddi:** Claude zorunlu araç yerine metin/`refusal` döndürürse ya da OpenAI `refusal` alanı gelirse metin olarak uygulamaya geçer: doğrulama nedenini söyler, bir kez daha sorar, harcanan token'lar kullanım tablosuna yazılır.
- Bilinen sınır: gerçek bir anahtar yoktu; tüm sağlayıcı çağrıları yerel sahte sunucuya karşı test edildi (`crates/fc-app/src/modules/ai/tests_meaning.rs`: iyi JSON, bozuk JSON sonra iyi, ret, zaman aşımı, modül/sağlayıcı kapalı, bütçe aşımı). Practice test ve "öğren" modlarının yazılı aşaması (`modules/modes`) henüz listeyi okumuyor.

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
  | `words_lookup` | okur | `words.lookup` | v1.4: bir metin (kelimelere bölünür) ya da kelime biçimleri → anahtar (lemma: *went* → *go*) + durum (`new/learning/known/ignored`), `new_words` listesi ve sayılar: "bu metinde hangi kelimeler benim için yeni?" |
  | `words_list` | okur | `words.list` (+ deste için `decks.list`, `search.cards`, `words.lookup`) | v1.4: duruma, dile ya da **desteye** göre kelimeler ("Breaking Bad'den neler öğreniyorum?" = `deck: "Breaking Bad", status: learning`); destesiz sayfalı (`cursor`), desteli en çok 5000 not okunur (`truncated`). Deste kelimeleri notların sıralama alanından bulunur: sıralama alanı terim olmayan not türlerinde eksik kalabilir |
  | `words_stats` | okur | `words.stats` | v1.4: dil başına durum sayıları, "bilinen" eşiği, dili bilinmeyen notlar |
  | `create_deck` | yazar | `decks.create` | varsa olanı döndürür (idempotent) |
  | `add_notes` | yazar | `batch.execute` + `notes.create` | ≤100 not, `{"Seviye":"B1"}`; **hepsi ya da hiçbiri, tek geri al adımı** |
  | `update_note` | yazar (değiştirir) | `notes.update` | verilmeyen alan/etiket/sınıflandırma korunur |
  | `set_classification` | yazar (değiştirir) | `search.ids` + `bulk.set_attribute` | **`confirm: true` olmadan yalnız önizler** (kaç not değişecek + `confirm_token`); uygulamak için aynı çağrı `confirm: true` ve bu belirteçle tekrarlanır (eşleşen küme değiştiyse reddedilir; en çok 2000 kart) |

  **v1.4'te bilinçli olarak MCP'ye açılmayanlar:** yalnız `words_lookup/list/stats` eklendi; v1.4'ün diğer yöntemleri (yedek `io.backup.*`, `subtitle.*` karşılaşma/tanı, `modes.quiz.*`, `ai.meaning.*`, `accepted_answers.*`, `examples.*`, `stats.leech_repairs.*`, `media.origins`/`media.audio.fill*`, `words.data.*`, `words.index.rebuild*`, `plugins.word_decks.*`) hiçbir aracın yöntemi değildir, varsayılan-ret kuralıyla (`method_reachable`) MCP aktörüne kapalıdır. Silen araç yok. Araç hataları MCP "tool error" olarak uygulamanın `ApiError` JSON'u (`code`, `message`, `details`; ör. `note_index`) döner, model okuyup düzeltebilir; bilinmeyen araç protokol hatasıdır. Araç ek açıklamaları: `readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint=false`.
- **İzinler:** aktör `Actor::Mcp(<clientInfo.name>)` (oplog'da `{"kind":"mcp","id":"claude-ai"}`). Bu aktör yalnız `MCP_PERMISSIONS`'ı taşır (`decks/notes/attributes:read+write`, `notetypes/search/settings/modules/mcp:read`, v1.4'ten `words:read`, `batch:write`); ayar/modül değiştirme, geri al, iç metotlar, `mcp:write` (dış sunucu kaydetme = uygulamaya komut çalıştırtma) **reddedilir** — `batch.execute` içinde de. İzin tek başına yetmez: MCP aktörü yalnız araç kataloğunun çağırdığı metotlara erişir; silme/çöp boşaltma (`notes.delete`, `decks.delete`, `browse.purge_trash`) `notes:write` izniyle kapsansa da kapalıdır. `mcp:write` Plugin/Automation/LocalApi aktörlerine de verilmez (yalnız kullanıcı ve CLI). Yazmalar sıradan komuttur: doğrulanır, transaction, oplog (+ ters), uygulamadan geri alınır.
- **Açma/kapama:** modül `mcp` (kapalıysa araç listesi boş, çağrılar `CONFLICT_MODULE_DISABLED`); v1.4: bir aracın yöntemlerinden birinin modülü kapalıysa araç listelenmez (`words` kapalıyken `words_*` görünmez); ayar `mcp.server.allow_writes` (app, varsayılan açık; kapalıysa yazma araçları listelenmez ve `PERMISSION_MCP_READ_ONLY`); `--read-only` bayrağı aynısını süreç için yapar. Ayar ve modül her çağrıda veritabanından tazelenir (masaüstündeki değişiklik çalışan sunucuya yansır).

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

Uygulamadaki **MCP** ekranı (Ayarlar › Gelişmiş › MCP; v1.3: "Asistanını bağla" ve "Dış MCP sunucuları" ayrı sekmeler) istemci seçtirir (Claude Desktop, Claude Code, diğer: tek satırlık komut), aynı parçacıkları **bu profilin gerçek yollarıyla** doldurur, kopyalatır ve bağlantının asistanda nasıl deneneceğini adım adım yazar (deneme uygulamadan çağrı yapmaz): `mcp.setup` sorgusu (yalnız kullanıcı/CLI; MCP, eklenti ve yerel API aktörlerine kapalı) `db_path` (masaüstünün açtığı profil klasöründeki `collection.db`; masaüstü başlatmadıysa `null`), `fc_path` (çalışan programın yanındaki, yoksa `PATH`'teki ilk `fc`/`fc.exe`; bulunamazsa `null` — ekran o zaman yolu kullanıcıya sordurur, Windows kurulum paketi şimdilik `fc.exe` içermez) ve `platform` döndürür. Ekran ayrıca araç listesini (okur/değiştirir) ve "Yalnızca okuyabilsin" anahtarını (`mcp.server.allow_writes`'un tersi) gösterir.

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

#### Uygulanan hâli (v0.9, `crates/fc-http` + `fc-app` `local_api`/`inbox` modülleri) — **sözleşme**

Tarayıcı eklentisi (iz K) ve diğer yerel istemciler bu bölüme göre yazılır. Tipler `fc-api`'den üretilir (`apps/desktop/src/lib/api/generated/`): elle DTO yazılmaz.

**Sunucu**
- Adres: yalnız `127.0.0.1` (ya da `[::1]`), varsayılan port **`28735`** (`local_api.port`). Yapılandırma tipi geri döngü dışı adresi **ifade edemez**; bağlanan soket ayrıca denetlenir.
- Modül `local_api` **varsayılan kapalı** (Ayarlar → Modüller → "Yerel API"). Kapalıyken port dinlenmez. Açılınca masaüstü kabuğu sunucuyu başlatır; `fc --db <dosya> api serve [--port N]` aynı sunucuyu masaüstü olmadan başlatır (geliştirme/test).
- `Host` başlığı `127.0.0.1:<port>`, `localhost:<port>` ya da `[::1]:<port>` olmalı (DNS yeniden bağlama koruması) → aksi `403 PERMISSION_ORIGIN_NOT_ALLOWED`.
- **Origin:** `Origin` başlığı olmayan istek (curl, betik, yerel uygulama) kabul edilir. `Origin` varsa yalnız tarayıcı eklentisi şeması (`chrome-extension://`, `moz-extension://`, `extension://`) kabul edilir; web sayfası kökenleri (`http(s)://…`, `null`) her uçta `403 PERMISSION_ORIGIN_NOT_ALLOWED`. Eşleştirmede gelen `Origin` istemci kaydına yazılır; o istemcinin token'ı **yalnız aynı Origin'den ya da Origin'siz** geçer (MV3 servis çalışanı bazı isteklerde Origin taşımaz); başka bir Origin (web sayfası, başka eklenti) her zaman `403`. CORS başlıkları (`Access-Control-Allow-Origin: <aynı köken>`, `Allow-Headers: authorization, content-type, x-flashcard-client`, `Allow-Methods: GET, POST, OPTIONS`) yalnız eklenti kökenlerine döner; MV3 servis çalışanı `host_permissions: ["http://127.0.0.1/*"]` ile CORS'a zaten takılmaz.
- **CSRF (Origin'e ek katman):** `GET /v1/ping` dışındaki **her** istek (eşleştirme ve yoklama dahil; kullanıcı verisi döndüren `GET`'ler dahil) `X-Flashcard-Client: <istemci adı>` başlığını taşımalı (1–100 yazdırılabilir karakter, içeriği önemsiz) ve her `POST` `Content-Type: application/json` (`; charset=…` olabilir) olmalı; aksi `400 VALIDATION_INVALID_PARAMS` (`details.reason`: `client_header` ya da `content_type`). Bir web sayfası bu başlığı/içerik türünü ön uçuşsuz gönderemez, ön uçuşu da yalnız eklenti kökenleri geçer; böylece `Origin` göndermeyen bir tarayıcı isteği (ör. `no-cors` `GET`) bile kullanıcı verisine ulaşamaz. Token tek başına yetmez. Eşleştirme durumu bu yüzden `GET` değil **`POST`** ile yoklanır (aşağıda).
- **Gövde sınırı** 256 KiB → `413 VALIDATION_PAYLOAD_TOO_LARGE`. **Hız sınırı:** token başına saniyede 10 istek (anlık 30); eşleştirme isteği tüm sunucuda dakikada 5 ve **aynı istemci adı** (köken başına, büyük/küçük harf ve baştaki/sondaki boşluk önemsiz) 10 dakikada 3 → `429 CONFLICT_RATE_LIMITED` + `Retry-After` (saniye).
- **Bekleyen eşleştirmeler:** aynı anda en çok 5; dolunca **en eskisi düşer** (kilitlenme yok: bir akın gerçek isteği itebilir ama eşleştirmeyi kapatamaz). Aynı ad + köken yeniden isterse önceki isteğinin yerini alır. Sunucu her açılışta bekleyen tüm istekleri **bırakır** (token'ları yalnız belleğinde yaşıyordu; yeniden başlatmadan sonra onaylanamaz, `404 NOT_FOUND_API_CLIENT`). Kararlanmış (onaylı/reddedilmiş/iptal) istemciler korunur.
- **Gelen kutusu kotası (istemci başına):** bekleyen öğe sayısı ve metin boyutu sınırlıdır; aşılırsa `409 CONFLICT_INBOX_QUOTA` (`details.limit`: `items`/`bytes`, `details.max`) — kullanıcı birkaçını kabul/ret edince yeniden gönderilebilir. Sınırlar kullanıcının ayarıdır: `local_api.inbox_max_items` (varsayılan 500), `local_api.inbox_max_kib` (varsayılan 4096). Bilinen `client_ref` ile yeniden gönderim yeni öğe sayılmaz. Kullanıcının kendi önerileri sınırlanmaz.
- Kimlik: `Authorization: Bearer <token>`. Token yalnız istemciye bir kez verilir; veritabanında yalnız SHA-256 özeti tutulur. Aktör `Actor::LocalApi(<client_id>)` (oplog'da `{"kind":"local_api","id":"<client_id>"}`); yazmalar sıradan, geri alınabilir komuttur.

**Uç noktalar**

| Yöntem + yol | Token | Gövde → yanıt (TS tipi) | Not |
|---|---|---|---|
| `GET /v1/ping` | hayır | → `PingResponse` `{app: "flashcard", api_version: "v1", version}` | Uygulama açık mı? (kapalıysa bağlantı reddi → eklenti kuyruğa alır) |
| `POST /v1/pair` | hayır | `PairRequest` `{client_name, client_kind: "browser_extension"\|"app"\|"script", scopes: string[]}` → `201 PairResponse` `{request_id, status: "pending", expires_at}` | Uygulamada onay penceresi açılır: "<ad> bağlanmak istiyor: <izinler>. İzin ver / Reddet". Kullanıcı izinleri daraltabilir. 10 dk içinde karar yoksa `expired` |
| `POST /v1/pair/{request_id}` | hayır | `{}` (boş gövde; `Content-Type: application/json` + `X-Flashcard-Client` gerekir) → `PairStatusResponse` `{status: "pending"\|"approved"\|"denied"\|"expired", client_id?, token?, scopes}` | `token` **yalnız ilk** `approved` yanıtında döner (sonra hiç). Origin eşleştirmeyi başlatanla aynı **ya da yok** olmalı (`request_id` yalnız isteyende olan tahmin edilemez bir sırdır). `GET /v1/pair/…` artık yoktur (`404`). Token sunucu belleğinde bekler: uygulama arada kapanırsa istek `expired` olur, yeniden eşleştirilir. Bilinmeyen kimlik → `404 NOT_FOUND_PAIRING` |
| `GET /v1/auth` | evet | → `ApiClientDto` (kendi kaydı: `id, name, kind, scopes, status, …`) | Token hâlâ geçerli mi |
| `GET /v1/methods` | evet | → `LocalApiMethodList` `{methods: [{name, kind, permission}]}` | Bu token'ın çağırabileceği metotlar |
| `GET /v1/decks` | `decks:read` | → `DeckList` (`decks.list`: düz, `path` sıralı; `DeckDto.id/path/name`) | kolaylık |
| `POST /v1/inbox` | `inbox:write` | `AddInboxItem` → `201 InboxItemDto` (aynı `client_ref` ile yeniden gönderimde `200` + mevcut öğe) | kolaylık = `inbox.add` |
| `POST /v1/<metot>` | metodun izni | metodun istek tipi → metodun yanıt tipi | yalnız aşağıdaki **açık liste**; ör. `POST /v1/attributes.effective {"deck_id": "…", "target": "note"}` |

Token isteyen uçlar (`/v1/ping` ve eşleştirme dışındakiler) `Authorization: Bearer <token>` + yukarıdaki CSRF başlıklarını birlikte ister.

Başarı: `200` (oluşturmada `201`), gövde metodun yanıtıdır (zarf yok). Hata: gövde her zaman `ApiError` JSON'u (`code`, `message_key`, `message`, `details`, `retryable`); durum kodu kategoriden: `VALIDATION_*` 400 (`VALIDATION_PAYLOAD_TOO_LARGE` 413), `PERMISSION_UNAUTHENTICATED` 401 (token yok, bilinmiyor, reddedilmiş, iptal edilmiş ya da onay bekliyor — `details.reason`: `missing`/`invalid`/`pending`/`denied`/`revoked`), diğer `PERMISSION_*` 403, `NOT_FOUND_*` 404 (bilinmeyen ya da listede olmayan metot `NOT_FOUND_METHOD`), `CONFLICT_RATE_LIMITED` 429, `CONFLICT_MODULE_DISABLED` 503 (ör. `inbox` modülü kapalı), diğer `CONFLICT_*` 409 (`CONFLICT_INBOX_QUOTA` dahil), `PROVIDER_*`/`IO_*`/`INTERNAL*` 500.

**İzin kapsamları** (istemci ister, kullanıcı onaylarken daraltabilir; başkası verilemez): `inbox:write`, `inbox:read`, `decks:read`, `decks:write`, `notetypes:read`, `notes:read`, `notes:write`, `attributes:read`, `tags:read`, `search:read`. Bilinmeyen/verilemez kapsam isteği `400 VALIDATION_API_SCOPE`.

**HTTP'ye açık metotlar** (açık liste `fc_app::modules::local_api::HTTP_METHODS`; izin tek başına yetmez): `inbox.add`, `inbox.list`, `decks.list`, `decks.tree`, `decks.create`, `notetypes.list`, `notes.get`, `notes.create`, `attributes.effective`, `tags.list`, `search.cards`. **Hiçbir zaman:** iç metotlar, geri alınamaz/replay edilemez komutlar, `io.*`, `plugins.*`, `mcp.*`, `ai.*` (anahtarlar dahil), `oplog.*`, `settings.*`/`modules.*` yazmaları, `local_api.*` yönetimi, silme ve toplu işlemler.

**`AddInboxItem`** (`inbox.add`):

```json
{
  "front": "porsuk",
  "back": "badger",
  "fields": {},
  "deck_id": "0193…",
  "notetype_id": null,
  "tags": ["web"],
  "attributes": { "Seviye": "B1", "Sınav": ["YDS", "TOEFL"] },
  "context": { "url": "https://…", "title": "Sayfa başlığı", "sentence": "The badger dug a hole." }
}
```

- `front`/`back` kısayoldur: Klasik'te ön/arka, v1.3'ten beri rolü olan not türlerinde `term`/`meaning` alanları (§4); başka alanlar için `fields` (`alan_anahtarı → değer`). En az bir alan dolu olmalı. Alan başına en çok 20 000 karakter, en çok 50 etiket.
- `attributes`: anahtar = sınıflandırmanın **anahtarı, adı ya da kimliği**; değer = seçenek **etiketi ya da kimliği** (çoklu seçimde dizi), sayı/bool/metin türünde düz değer. Öğe eklenirken yalnız biçim denetlenir; deste ve seçenek **kabul edilirken** (o anki desteye göre) çözülür.
- `client_ref` (1–100 karakter, isteğe bağlı): istemci başına tekil yeniden gönderim anahtarı; aynı anahtarla gelen ikinci istek yeni öğe yazmaz, ilkini döndürür.
- `deck_id`, `notetype_id`, `context`, `tags`, `fields`, `attributes` isteğe bağlı. Kaynak (`source`) istemci tarafından **verilmez**: sunucu aktörden yazar (`{kind: "local_api", client_id, client_name}`; masaüstünde `clipboard`, `deep_link`, `user`). `via` alanı yalnız uygulamanın kendisine açıktır (istemci gönderirse `400`). Bilinmeyen alan → `400 VALIDATION_INVALID_PARAMS`.
- Yanıt `InboxItemDto` `{id, source, status: "pending", item: AddInboxItem, note_ids: [], created_at, updated_at}`.

**Olaylar** (masaüstü): `inbox_changed` (`{pending}`), `local_api_pairing` (`{client: ApiClientDto}` — bekleyen istemcinin kaydı: `id, name, kind, origin, requested_scopes, …`; onay penceresini açar, kullanıcı penceredeki kapsam onay kutularıyla izinleri daraltabilir), `local_api_clients_changed`.

## 4. Gelen kutusu

- Dış kaynaklardan (HTTP API, tarayıcı eklentisi, MCP, pano izleme, derin bağlantı) gelen öğeler önce `inbox`'a düşer.
- Öğe: ham veri + kaynak + bağlam (sayfa adresi, cümle, zaman).
- Gelen kutusu ekranı: tek tek veya toplu onayla → not türü ve deste seç → ekle. AI ile alan doldurma seçeneği.
- Kaynak bazında "doğrudan ekle" kuralı (otomasyon kurallarıyla birleşir).

#### Uygulanan hâli (v0.9, `fc-app` `inbox` modülü)
- Tablo `inbox` (0001) + migration `0800`: durum değişim zamanı, oluşan not kimlikleri, gönderen istemci ve `client_ref` (istemci başına tekil). Öğe = `AddInboxItem` (alanlar/ön-arka, deste, not türü, etiketler, sınıflandırmalar, bağlam) + kaynak (`InboxSource`: `user`, `clipboard`, `deep_link`, `local_api {client_id, client_name}`, `other {actor}`) — kaynak **her zaman aktörden** yazılır.
- `inbox.add` (geri alınabilir; tersi iç `inbox.remove`), `inbox.list` / `inbox.count`, `inbox.accept` (her öğe için iç `notes.create`; **hepsi ya da hiçbiri, tek geri al adımı**; deste/not türü/alanlar/etiketler/sınıflandırmalar incelemede değiştirilebilir; hata `details.item_index`; benzer not `allow_duplicates` olmadan `CONFLICT_NOTE_DUPLICATE`), `inbox.reject` (tersi iç `inbox.restore`). Sınıflandırmalar **kabulde** hedef desteye göre çözülür (anahtar/ad/kimlik → seçenek etiketi/kimliği; MCP ile aynı çözümleyici).
- Kaynak bazında "doğrudan ekle" otomasyon izinde (iz J); gelen kutusu `inbox_changed` olayını yayar.

#### v1.3: bağlam kabulde korunur
- **Kısayol rollere gider:** `front` → not türünün `term` rolündeki alanı, `back` → `meaning` rolündeki alanı (ikisi birden olmayan türlerde, Klasik gibi, ilk ve ikinci alan). Açıkça verilen `fields` (anahtar ya da alan adıyla) her zaman önce gelir.
- **Bağlam yazılır** (`inbox::add_context`): `context.sentence` not türünün `example` rolündeki alanına (zengin metinse terim bulunduğunda **kalın**), sayfa başlığı/adresi `source` rolündeki alanına yazılır; yalnızca o alanlar varsa ve boş bırakılmışsa. v1.2'de cümle Klasik nota hiç girmiyordu, düzenlenerek kabul edilen öğe de cümlesini kaybediyordu.
- **Varsayılan not türü kuralı** (`inbox::default_notetype`): ne öğe ne kabul bir not türü söylüyorsa Klasik; ama öğe bir cümle taşıyorsa ve açık alan değeri yoksa, cümleyi tutabilen yerleşik **Altyazı kartı** (o not türü silinmişse yine Klasik). Otomasyonun gelen kutusu süzgeci aynı kuralı kullanır (öğenin hangi not türüne gideceğine göre eşleşir).
- Arayüz: her satır hangi kart türüne dönüşeceğini söyler; destesi olmayan satır göndermeden önce deste ister; ön/arka önerinin alanlarına düşer; düzenlenmiş kabul diğer alanları korur.
- Tarayıcı eklentisi ve yerel API sözleşmesi (§3) değişmedi: `AddInboxItem.context` zaten vardı, artık kabulde kullanılıyor.

## 5. Tarayıcı eklentisi

- Chromium tabanlı tarayıcılar (Chrome, Edge, Brave) için; Firefox ileride.
- Seçili metin → sağ tık → "Desteye ekle" veya kısayol. Küçük açılır pencere: ön/arka, deste seçimi.
- Bağlam: seçimin içinde geçtiği cümle, sayfa başlığı ve adresi otomatik eklenir.
- Uygulama kapalıysa istek kuyruğa alınır, uygulama açılınca gönderilir.
- Eklenti, yerel HTTP API'nin sıradan bir istemcisidir (özel yetkisi yoktur).

#### Uygulanan hâli (v0.9, iz K, `apps/browser-extension`)

- **Biçim:** Chromium MV3 (Chrome/Edge/Brave ≥ 116), TypeScript + Vite, çerçevesiz DOM. İzinler: `contextMenus`, `storage`, `activeTab`, `scripting`, `alarms`; ana makine izni **yalnız** `http://127.0.0.1/*` (CSP `connect-src` de öyle). Telemetri yok, başka adrese istek yok.
- **Akış:** seçim → sağ tık **Desteye ekle** / `Alt+Shift+A` / araç çubuğu düğmesi → servis çalışanı seçimi sayfadan okur (`chrome.scripting`, tüm çerçeveler; erişilemeyen sayfada menünün `selectionText`'i) ve seçimin geçtiği **cümleyi** (`. ! ? …` ve CJK sonlarına, satır sonlarına göre; uzunsa seçimin çevresi) çıkarır → açılır pencere: ön yüz (seçim), arka yüz (isteğe bağlı), deste (`GET /v1/decks`; boş = gelen kutusunda seç), destenin **sınıflandırmaları** (`POST /v1/attributes.effective {deck_id, target:"note"}`; tüm türler: sıralı/sırasız/çoklu seçim, sayı, metin, tarih, evet/hayır; zorunlular işaretli; öneri/varsayılan dolu; **deste ve değerler deste başına hatırlanır**), bağlam (cümle + başlık + adres, kapatılabilir) → `POST /v1/inbox` (`AddInboxItem`; `attributes` anahtarı sınıflandırma **kimliği**, değeri seçenek **kimliği** — uygulamada ad değişse de bozulmaz; `source` gönderilmez).
- **Eşleştirme:** **Bağlan** → `POST /v1/pair` (`client_kind: "browser_extension"`, kapsamlar `inbox:write`, `decks:read`, `attributes:read`) → pencere açıkken 1,5 sn'de bir `POST /v1/pair/{id}` (POST: Chromium `Origin`'i gönderir, token eşleştirme kökenine bağlı kalır); token **ilk** onaylı yanıtta gelir, servis çalışanı hemen `chrome.storage.local`'a yazar. Reddedildi/süresi doldu/uygulama yeniden başladı (404) mesajı; herhangi bir 401'de token unutulur (kuyruk korunur), pencere yeniden bağlanmayı önerir. `ping` dışındaki her istek `X-Flashcard-Client: flashcard-browser-extension`, her POST `Content-Type: application/json` taşır (iz I'nin CSRF koruması; Faz 4 tur 2). Kalan GET'ler (`/v1/auth`, `/v1/decks`) Origin'siz gelebilir; token zaten gereklidir.
- **Kuyruk (uygulama kapalıyken):** gönderilemeyen öğe (bağlantı yok, `503` modül kapalı, `429`, 5xx, token yok) `chrome.storage.local`'da sıraya girer; her dakika (alarm), tarayıcı açılışında, pencere açılınca ve ayarlardaki **Şimdi gönder** ile **sırayla** gönderilir; yeni ekleme eskilerin arkasına girer. Her öğe bir `client_ref` (UUID) taşır: yanıt kaybolup yeniden gönderilse de gelen kutusunda tek öğe olur. Kalıcı ret (ör. deste silinmiş) öğeyi `failed` yapar, sırayı tıkamaz; ayarlarda listelenir, yeniden denenir ya da silinir. Üst sınır 500 öğe. Uygulama kapalıyken pencere son bilinen desteleri/sınıflandırmaları gösterir. Rozet bekleyen sayısını gösterir.
- **Tek sahip:** token, kuyruk ve eşleştirme yalnız servis çalışanında (`src/lib/service.ts`); açılır pencere ve ayarlar sayfası ona mesaj gönderir, uygulamayla doğrudan konuşmaz. Mesajlar yalnız eklentinin kendi sayfalarından kabul edilir (içerik betiği yok).
- **Ayarlar sayfası:** port (varsayılan `28735`), bağlantı ve eşleştirme durumu, **Bu bağlantıyı unut**, bekleyen/reddedilen öğeler.
- **Metinler:** `public/_locales/{en,tr}/messages.json` (`chrome.i18n`, tarayıcı diline göre). Hata metinleri koda göre (`error_<kod>`), yoksa kategoriye göre.
- **Tipler:** `@fc/api` = `apps/desktop/src/lib/api/generated` (yalnız tip içe aktarımı); yerel API tipleri (`PingResponse`, `PairRequest/Response`, `PairStatusResponse`, `AddInboxItem`, `InboxItemDto`, `ApiClientDto`) de oradan gelir — elle yazılmış DTO yok (Faz 4 tur 2'de `local-api-types.ts` silindi; `pnpm check` eklentiyi üretilmiş tiplere karşı doğrular).
- **Testler:** vitest (`tests/unit`: cümle bulma, sayfa betiği, sınıflandırma alanları, API istemcisi, kuyruk, servis akışları, TR/EN anahtar eşitliği ve sabit metin yokluğu) — docs/06 §3'e göre yazılmış sahte API'yle (`tests/mock-api`: Host/Origin, token+kapsam, bilinmeyen alan reddi, `client_ref` tekilleştirme). Playwright (`tests/e2e`): paketlenmemiş eklenti **başsız Chromium**'da, gerçek HTTP sahte API'ye karşı: sayfada seç → yakala → eşleştir → deste + Seviye → gelen kutusu; uygulama kapalıyken kuyruk → açılınca teslim; Türkçe arayüz; `host_permissions` dışı kökende (izin yok) menü metniyle yakalama. Test kancası yalnız `pnpm e2e` (`--mode e2e`) çıktısındadır. `real-app.spec.ts` aynı akışı `FC_BIN` verilince gerçek `fc api serve`'e karşı koşar ve öğeyi `fc call inbox.list` ile geri okur; CI'nin `extension` işi `fc`'yi derleyip `FC_BIN` verir, yani bu test her CI koşusunda çalışır. Kurulum ve elle deneme: `apps/browser-extension/README.md`.

### 5.1 Subtitle eklentisi (video altyazıları)

Kullanıcının ayrı bir tarayıcı eklentisi olan **Subtitle**, video altyazısında bir kelimeye **+** basılınca bir *yakalama* (kelime, kök biçim, anlam, cümle, çeviri, ekran görüntüsü, ses, video bağlantısı) gönderir. Sözleşme eklentinin deposundadır: `docs/flashcards-api.md` ("Subtitle Kartları API v1"); istemci `src/flashcards/api.ts`. Uygulama bu sözleşmeyi **değiştirmeden** karşılar; eklenti tarafında hiçbir şey değişmez.

#### Uygulanan hâli (v1.2, `fc-app` `subtitle` modülü + `crates/fc-http` `subtitle.rs`)

**Sunucu**
- Aynı yerel HTTP sunucusu (§3: yalnız geri döngü, `local_api.port`, varsayılan `28735`), yol öneki **`/subtitle`**: kullanıcı eklentiye sunucu adresi olarak `http://127.0.0.1:28735/subtitle` yazar; uçlar `/subtitle/v1/me`, `/subtitle/v1/decks`, `/subtitle/v1/media`, `/subtitle/v1/captures`, `/subtitle/v1/captures:batch`, `/subtitle/v1/known-words`. Önek şart: yerel API'nin `GET /v1/decks`'i başka biçimde yanıt verir ve değişmez.
- Modül `subtitle` **varsayılan kapalı** (Ayarlar → Modüller). Kapalıyken `/subtitle/*` her uçta `503` + `Retry-After: 300`, gövde yalnız `{"error":{"code":"module_disabled","message"}}` (anahtara bakılmadan; veri yok, anahtarın geçerli olup olmadığı da söylenmez). `404` değil: eklenti `/v1/media`'da `404`'ü "sunucuda medya yok" sayıp resim/sesi kalıcı olarak keser, diğer `4xx`'leri kalıcı hata sayar; `503`'te kuyruk bekler ve yeniden dener, modülü bir süre kapatan kullanıcı ne medya ne kelime kaybeder. Masaüstü kabuğu sunucuyu `local_api` **ya da** `subtitle` açıkken başlatır (`sync_api_server`); `fc --db <dosya> api serve` de ikisinden biri açıkken çalışır. `local_api` kapalıyken `/v1/*` hiçbir şey açmaz: `ping` dahil `503 CONFLICT_MODULE_DISABLED`.
- `Host` ve `Origin` denetimi §3'teki gibi önce çalışır: web sayfası kökeni her uçta `403`, `Host` geri döngü değilse `403`. CORS yalnız eklenti kökenlerine (`chrome-extension://…` vb.) döner; ön uçuş `authorization, content-type, accept, idempotency-key, x-capture-id, x-media-kind` başlıklarına izin verir, yanıtlar `Retry-After`'ı açar. `/v1/*`'ın `X-Flashcard-Client` CSRF başlığı burada **istenmez** (eklenti gönderemez); yerine her istek geçerli bir Bearer anahtar ister: bir web sayfası anahtarı bilemez, kökenler arası `Authorization` başlığı da ön uçuşsuz gönderilemez (ön uçuşu yalnız eklenti kökenleri geçer). Anahtar kökene bağlanmaz (eklenti yeniden kurulunca kimliği değişebilir).
- Sınırlar: JSON gövdesi 256 KiB (`captures:batch` için 50 × 256 KiB: dolu bir toplu istek tek tek geçerli yakalamalarla da sığar, yoksa `413` hepsini kalıcı olarak düşürürdü), ekran görüntüsü 2 MiB, ses 5 MiB (`413`); anahtar başına anlık 1000, sürekli saniyede 100 istek (`fc_http` `subtitle::KEY_BURST` / `KEY_RATE_PER_SEC`; birikmiş birkaç yüz kelimenin gönderimi — tur başına 20 yakalama × 2 yükleme + 1 toplu istek — hiç `429` almaz, kontrolden çıkmış bir istemci yine sınırlıdır; aşılınca `429` + `Retry-After`). Uygulama kilitliyken `503` + `Retry-After: 30` (eklenti yeniden dener, kaybetmez).
- Hata gövdesi sözleşmenin biçimidir: `{"error": {"code", "message"}}`. Kodlar: `unauthorized` (401), `forbidden` (403), `not_found` (404/405), `module_disabled` (503), `invalid_capture` (422), `invalid_media` (415 yanlış tür / 422), `invalid_request` (422), `too_large` (413), `rate_limited` (429), `locked` (503), `server_error` (500). `409` hiç dönmez (eklenti 409'u "sunucuda var" sayar).

**Erişim anahtarları**
- Uygulamada oluşturulur (`subtitle.keys.create`, ad 1–60 karakter): `sk_fc_` + 32 rastgele bayt (base64url). Anahtar **yalnız bu yanıtta** bir kez görünür; veritabanında yalnız SHA-256 özeti, gösterim için ilk 10 karakteri (`prefix`), oluşturma/son kullanım (dakikada en çok bir kez yazılır)/iptal zamanı tutulur (tablo `subtitle_keys`, migration `1200`). `subtitle.keys.revoke` anında geçersiz kılar; satır kalır (oplog'daki aktör kimliği adlanabilsin). Anahtar işlemleri geri al adımı değildir (`undo: false`). Yedekten geri yükleme her anahtarı iptal eder.
- Yerel API'nin istemci/kapsam düzeni kullanılmadı: eşleştirme onayı, kapsamlar ve köken bağı burada anlamsız; ayrı tablo daha sade.
- Aktör `Actor::Subtitle(<anahtar kimliği>)` (oplog'da `{"kind":"subtitle","id":"…"}`). Bu aktörün **hiç izni yoktur** (`Registry::permits` → `false`): yalnız modülün iç komutlarına, modülün kendi taşıma servisleri anahtarı doğruladıktan sonra güvenilir çağrıyla ulaşır. Kilitliyken arka plan aktörü sayılır.

**Yakalama → not**
- Doğrulama sözleşmenin JSON şemasına göre (`modules/subtitle/capture.rs`; bilinmeyen alanlar ve bilinmeyen medya türleri yok sayılır, kullanılamayan video zamanı `null` olur); hata `VALIDATION_SUBTITLE_CAPTURE` → `422`, mesaj alanı ve nedeni söyler (`term.lemma is required`).
- Not **doğrudan bir destede** oluşur (gelen kutusuna değil): yakalamanın `deck_id`'si (canlı bir desteyse), yoksa `subtitle.default_deck` ayarı (Subtitle ekranında seçilir), yoksa ilk kullanımda oluşturulan üst düzey **`Subtitle`** destesi. Bilinmeyen `deck_id` hata değildir, varsayılana düşer (kelime kaybolmaz).
- Not türü yerleşik **Altyazı kartı** (`builtin: "subtitle"`): alanlar `word` (kök), `surface` (geçtiği hâli), `pos`, `meaning`, `sentence` (terim `term_offset`'in UTF-16 aralığıyla, yoksa geçtiği yer aranarak **kalın**), `translation`, `image` (ekran görüntüsü), `audio`, `source` (`Başlık · 1:23`, videonun o anına `http(s)` bağlantı). Ön yüz (v1.3, migration `1301`): yalnız kelime — geçtiği hâli büyük, farklıysa sözlük hâli küçük yanında (`{{unless_same:surface:word}}`); cümle ilk ipucudur (kelime vurgulu). Arka: anlam, sözcük türü, örnek (cümle + çeviri), resim, ses, kaynak. Metinler HTML kaçışlı; altyazıda sık görülen Markdown işaretleri (`- ` diyalog çizgisi, `*`, `_` …) etkisizleştirilir. Hiçbir alan tekil değil. Etiketler uygulanır (geçersiz olan atlanır). Saf kısımlar `fc_core::subtitle`'da.
  - **v1.3:** alanların rolleri `word` = `term`, `surface` = `surface`, `meaning` = `meaning`, `sentence` = `example`, `translation` = `example_translation`, `source` = `source`; kartın `answer_field`'ı `meaning` (yazılı cevapta ve Testte "gitmek" yeter); arka yüz etiketli parçalardır: anlam, örnek (cümle + çevirisi), kaynak (docs/03 §4.1; var olan koleksiyonlarda migration 1300). İleri kartın ön yüzünün yalnızca kelimeyi göstermesi ve ilk ipucunun cümle olması v1.3'ün son düzeltmesidir (docs/02 §6.2).
- Komut iç `subtitle.captures.create`: not iç içe sıradan `notes.create` ile oluşur, deste gerekiyorsa `decks.create`, eşleme satırı `subtitle_captures`'a yazılır (tersi iç `subtitle.captures.forget`, onun tersi `subtitle.captures.restore`). Hepsi **tek geri al adımı** ("Subtitle: key eklendi"); geri alınınca not, açılan deste ve eşleme gider; aynı yakalama yeniden gelirse yeni kart olur. Yineleme de eşlemeyi geri getirir.
- Tekrar: aynı `capture_id` → `200 {"status":"duplicate","card_id"}` (aynı kart); aynı `lang.target` + lemma + `sha256(lower(collapse_spaces(sentence)))` → yine duplicate (`fc_core::subtitle::dedup_key`, `subtitle_captures.dedup_key` tekil). Aynı lemma **başka bir cümleden** → v1.3'te yeni not; v1.4'ten beri aynı anlamdaysa mevcut notun başka bir örneği (aşağıda), `subtitle.same_word = always_new_card` ile yine yeni not. Yeni → `201 {"status":"created","card_id"}`; `card_id` notun kartının kimliğidir. Notu kullanıcı tarafından silinmiş (çöpte ya da hiç yok) bir eşleme **bayattır**, tekrar sayılmaz: kelime yeni kart olarak gelir, bayat eşleme yenisiyle değişir (aynı geri al adımında; geri alınınca eski eşleme olduğu gibi döner).
- **v1.4 (W1) — yakalama bir KARŞILAŞMADIR, kart değil.** Her yakalama kaynağıyla birlikte bir *karşılaşma* olarak saklanır (`subtitle_captures` satırı; migration `1490_subtitle_encounters`): kimlik, dil, lemma + `surface`, `pos`, cümle (düz metin) ve terimin UTF-16 konumu (`term_start`/`term_end`), çeviri, `kind` (word/phrase/sentence), tanım + kaynağı (`definition_source`: dict/ai/mt), `video_id`, başlık, site, URL, `t_start_ms`–`t_end_ms`, ekran görüntüsü/ses özetleri, alınma zamanı (`created_at`) ve birincil örnek olduğunda notun göstereceği alanlar (`example_fields`). Bu kullanıcının kendi yerel verisidir; metin burada **saklanır** (tanı kaydının tersine). v1.4 öncesi yakalamalar notta hâlâ duranlarla doldurulur (`surface`, resim/ses özeti, `example_fields`, başlık `note_origins`'ten); cümle, tanım, zamanlar, tür, video kimliği ve URL o zaman ayrıca tutulmadığı için **NULL kalır** (uydurulmaz). Bir notun en eski karşılaşması birincildir.
  - **Not = terim + anlam; karşılaşmalar onun örnekleri.** Bir notun karşılaşmalarından tam biri **birincildir** (`is_primary`, not başına tekil dizin): notun `surface`, `sentence`, `translation`, `image`, `audio`, `source` alanları onu gösterir; diğerleri notun yeni `examples` alanında listelenir (her biri cümle + çeviri + kaynak; `fc_core::subtitle::examples_html`) ve API'de liste olarak döner. Not türüne tek değişiklik: `examples` alanı (`rich_text`, en sonda) ve arka yüzde kaynaktan önce `{{#examples}}…{{/examples}}` parçası; alan boşsa hiçbir şey çizilmez, var olan kartlar aynı görünür (migration 1490 yalnız arka yüz hâlâ v1.3'te gönderildiği gibiyse değiştirir; alanı her yerleşik Subtitle türüne ekler).
  - **Aynı kelime yeniden gelince** (ayar `subtitle.same_word`, app, `add_example` varsayılan / `always_new_card` = v1.3 davranışı): aynı dilde aynı lemmanın canlı bir Subtitle notu varsa ve **anlam aynıysa**, yakalama o notun **başka bir örneği** olur: yeni kart yok, kartın kimliği, planı ve geçmişi değişmez (yalnız `examples` alanı güncellenir; kullanıcının düzenlediği cümle alanına dokunulmaz). Kelimenin notları var ama anlam eşleşmiyorsa yakalama **ayrı bir not** olur ve **incelemeye** işaretlenir (`review_note_id` = kelimenin ilk notu); kullanıcı karar verir (`subtitle.encounters.resolve`: "aynı anlam" iki notu birleştirir, "farklı anlam" ayrı bırakır). Deste farkı eklemeye engel değildir (kelime birimdir); yeni yakalamanın etiketleri eklenen örnekte nota yazılmaz.
  - **Anlam karşılaştırma kuralı** (`fc_core::subtitle::same_meaning`, saf): iki tanım `senses`'le normalleştirilir (HTML etiketleri ve Markdown kaçışları atılır, `&amp;` vb. çözülür, küçük harf, `,` `;` `/` `|` ve satır sonunda bölünür, her parça boşluk ve `.!?:()"'`'tan arındırılır) ve **en az bir ortak anlam** varsa AYNI sayılır ("almak" ~ "Almak.", "almak, kaldırmak" ~ "kaldırmak"). Ortak anlam yoksa ya da bir taraf boşsa **EMİN DEĞİL**: ayrı not + inceleme (yanlışlıkla iki anlamı birleştirmek fazladan bir karttan kötüdür). Karşılaştırılan: yeni tanım ↔ notun `meaning` alanı ve o nota daha önce yakalanmış her tanım.
  - **HTTP yanıtı değişmedi:** başka örnek olarak eklenen yakalama telde `201 {"status":"created","card_id":<var olan kart>}` döner (saklandı; eklentinin istemcisi `duplicate` dışındaki her şeyi `created` okur), yeniden gönderimi `200 duplicate`. Uygulama içi yanıt `SubtitleCaptureOutcome` `attached` / `needs_review` bayraklarını taşır; tanı satırı yeni sonuç `attached`'ı yazar (`SubtitleAttemptResult::Attached`; migration 1490 `subtitle_capture_log` CHECK'ini genişletir). `tools/subtitle-contract/check.mjs` bunu eklentinin kendi istemcisiyle sınar.
  - **Komutlar** (herkese açık, `subtitle:read`/`subtitle:write`, hepsi geri alınabilir; tersleri iç `subtitle.encounters.put` = satırların önceki hâli + iç içe not düzenlemeleri): `subtitle.encounters.list {note_id | lang+lemma | video_id | needs_review: true}` (tam bir seçici), `subtitle.encounters.set_primary {capture_id}` (kartta görünen örnek değişir; kart kimliği, vade, stabilite ve tekrar geçmişi aynı kalır — testle), `subtitle.encounters.move {capture_id, to_note_id | null}` ("bu başka bir anlam": var olan bir Subtitle notuna ya da `null` ile yeni nota/karta; bir notun tek karşılaşması taşınamaz → `merge_notes`), `subtitle.encounters.merge_notes {a, b}`, `subtitle.encounters.resolve {capture_id, same_meaning}`. Hata kodları `NOT_FOUND_SUBTITLE_ENCOUNTER`, `VALIDATION_SUBTITLE_ENCOUNTER` (`details.reason`).
  - **Birleştirme yalnız aynı kelimede:** iki not kelime dizinine göre aynı dilde aynı kelime değilse (dizinde yoksa karşılaşmalarının dil + lemması) `merge_notes` `CONFLICT_SUBTITLE_MERGE_WORDS` ile reddeder (`details.a`/`b` = `<dil>|<anahtar>`, TR/EN mesaj); `force: true` yine birleştirir. `resolve` "aynı anlam" zorlamaz (işaretli not aynı kelimeden doğdu).
  - **Birleştirme kuralı:** **daha çok tekrarı olan not kalır** (eşitlikte eski olan). Diğer notun karşılaşmaları kalan nota birincil olmayan örnek olarak taşınır, not kartlarıyla birlikte **çöpe** gider; tekrar kaydı yeniden bağlanmaz, **arşivlenir**: `review_log` kartından bağımsız yaşadığı için (migration 0501) istatistik ve optimizer onu saymaya devam eder, kalan kartın FSRS durumu karışmaz; geri al notu geri getirir. Yanıt kalan/arşivlenen notu, taşınan karşılaşma ve arşivlenen tekrar sayısını söyler.
  - **Diğer örneklerin medyası:** `examples` alanında her örnek kendi resmini (`<img src="media:…">`) ve sesini (`<audio controls src="media:…">`) taşır; API'de `subtitle.encounters.list` satırları `facts.screenshot_hash`/`audio_hash` ve `example_fields.image`/`audio` ile. Referanslar notun alanında olduğu için not o dosyaları kullanır sayılır (sahipsiz medya temizliği silmez).
  - **Yedek ve dışa aktarım:** karşılaşmalar tablo dökümüyle `.fcbackup`'a girer ve geri yüklenir; test: temiz profile geri yükleme birincil sesi ve diğer örneğin sesini bayt bayt aynı getirir. `.apkg` ve `.fcpack` dışa aktarımı (testte dışa aktarılıp temiz profile geri alınır) birincil örneği her zamanki alanlarda, diğerlerini `examples` alanında, üç medya dosyasıyla taşır. **Kaybolan:** karşılaşma satırları (video kimliği, zaman aralığı, tanım ve kaynağı, tür, birincil seçimi, inceleme işareti — paketler yalnız notu taşır; içe aktarılan not tek örnekli sıradan bir not olur); Anki'de ayrıca alan anahtarları görünen adlardan yeniden türer (`kelime`, `cümle`…) ve resim/ses alanı aynı dosyaya `<img>`/`<audio>` etiketi olarak döner.
  - **En iyi sahne (v1.4 W3, modül `examples`):** `subtitle.encounters.rank {note_id}` notun karşılaşmalarını puan + gerekçe listesi + kopyalarıyla sıralar (`fc_core::encounter_score`); `subtitle.encounters.suggest_primary` karttakinden en az 5 puan iyi olanı **önerir**, değiştirmez (uygulamak = `set_primary`). Ayar `subtitle.auto_best_example` (app, varsayılan kapalı): yakalama başka bir örnek olarak eklendiğinde öneri `set_primary` ile hemen uygulanır — yakalamanın geri al adımının içinde, kartın takvimi ve geçmişi aynı. Puanlama ayrıntısı docs/03 §4.4.
  - **Kelime dizini ve sorgular:** dizinin `subtitle_captures` tetikleyicileri (`note_id`, `lang`) değişmedi; taşıma/birleştirme `note_id`'yi güncelleyerek iki notu da kirli işaretler. `karşılaşma:`/`seen:` artık **karşılaşma** sayar: kelimenin not sayısı (`word_index.notes`; okuma modu ve elle notlar eskisi gibi birer) + Subtitle notlarının birden fazla sahnesi (`subtitle_encounter_counts`'tan `encounters − 1`); SQL ve bellek içi değerlendirici (`LangFacts::extra_encounters`) aynı kural, sayım = liste = çalışma testi 3 ve 1 karşılaşmalı notlarla.
  - **Medyanın kökeni ve eksik ses (W4):** yüklenen resim/ses `original` kökenle kaydedilir; `subtitle.encounters.list` her satırda `facts.audio_origin`/`screenshot_origin` döndürür (`original`/`clip`/`tts`/`user`). `media.audio.fill` sesi olmayan Altyazı notunun sesini birincil karşılaşmaya da yazar (`audio_hash` + `example_fields.audio`), yerel videodan kesit yalnız karşılaşmanın `t_start_ms`–`t_end_ms`'i varsa (docs/03 §13).
  - **Test modu (C1):** seçenekler notların `meaning` alanlarıdır; eklenen örnek not olmadığından hiçbir zaman seçenek olmaz, aynı kelimenin iki notu (iki anlam) birbirinin yanlış seçeneği olmaz (yakalamalardan yapılan notlarla test edildi).
- `POST /subtitle/v1/captures:batch`: en çok 50 (fazlası `422`), her yakalama ayrı değerlendirilir ve ayrı geri al adımıdır; sonuç `results[{capture_id, status: created|duplicate|error, card_id?, error?{code, message, retryable}}]`, geçersiz olan `retryable: false`, diğer hatalar `true`.

**Medya**
- `POST /subtitle/v1/media`: ham bayt, `Content-Type` `image/jpeg` (`X-Media-Kind: screenshot`) ya da `audio/wav` (`audio`), `X-Capture-Id` yakalamanın UUIDv7'si. İçerik koklanır, başlıkla uyuşmazsa `422`; başka tür `415`. Baytlar mevcut medya deposuna gider (içerik adresli, `media` satırı), yanıt `201 {"upload_id": "up_…"}`; aynı yakalama + tür yeniden yüklenirse **aynı kimlik** döner (`subtitle_uploads`, `(capture_id, kind)` tekil). Yükleme geri al adımı değildir; onu kullanan not adımdır. Yakalama yükleme kimliklerini anarsa (aynı yakalamanın, aynı türün yüklemesiyse) `media:<hash>` olarak `image`/`audio` alanına girer; bulunamayan yükleme yalnız o medyayı düşürür.
- Bir günden eski yüklemeler sonraki yüklemede unutulur, medya satırı da (hiçbir not kullanmıyorsa); dosyaları sahipsiz medya bakımı temizler.

**Bilinen kelimeler**
- **Paylaşım anahtarı (v1.4, varsayılan KAPALI):** Subtitle yalnız bir yakalama aracıdır; bildiğin kelimeleri ona geri göndermek ayrı bir karardır: ayar `subtitle.share_known_words` (app, Bool, `false`). Kapalıyken `GET /subtitle/v1/known-words` yine `200` döner ama dizinden hiçbir şey okumaz: `{"words": [], "next_cursor": null, "shared": false}` (eklentinin istemcisi `words ?? []` okur, bilinmeyen alanı yok sayar: boş son sayfa; 4xx/5xx yeniden deneme ya da hata sayılırdı), `/me` `features.known_words: false` der; `subtitle.status` anahtarı `share_known_words` ile bildirir. Açıkken aşağıdaki gibi, `shared: true`. Sözleşme denetimi (`tools/subtitle-contract/check.mjs`) iki durumu da eklentinin kendi istemcisiyle sınar.
- `GET /subtitle/v1/known-words?lang=en&cursor=…`: v1.4'ten beri **ortak kelime dizininden** (docs/03 §4.2) okunur, yanıt biçimi aynı: nereden gelirse gelsin (Subtitle, okuma modu, elle, Anki) dizindeki `known` (elle "biliyorum" işareti ya da aralığı ≥ `words.known_interval_days`, varsayılan 21 gün olan bir kart) ve `learning` (başka kart) kelimeler; `new` ve `ignored` sözleşmede olmadığı için listelenmez. `lang` birincil alt etikettir; `interval_days` kartların en uzun aralığı, `updated_at` son tekrar (yoksa işaret, yoksa ilk not zamanı). Sayfa 500 kelime, dil ve lemmaya göre; `next_cursor` opak.

**Uygulama API'si ve ekran**
- **Tanı (v1.4, "son yakalamama ne oldu?"):** her yakalama denemesi (`POST /captures` ve toplu gönderimin her öğesi) yerel bir satır bırakır: zaman, sonuç (`created` / `duplicate` / `rejected` + neden: sözleşme hata kodu ve alan adı, ör. `invalid_capture:term.lemma` / `media_missing` + neden: `screenshot:upload_unknown`, `audio:upload_mismatch`, `…:file_missing`), eşleşen kart, süre. **Cümle, kelime, anlam, URL ya da video başlığı asla yazılmaz**; `capture_id` yalnız geçerli bir UUIDv7 ise tutulur. Son 500 deneme kalır (`subtitle_capture_log`, migration `1430`; oplog'a girmez, iç komut `subtitle.captures.log`; yedek dosyasının tablo dökümünde bulunur ama **geri yüklemeyle gelmez**: geri yüklenen profilin listesi boş başlar, v1.4 A6). Okuma: `subtitle.captures.recent {limit?}` (varsayılan 50; `fc call`), log paketi (`maintenance.logs.export`) `subtitle_captures.txt` içerir. Modül kapalıyken kayıt yok. Kimlik doğrulama/hız sınırı/boyut hataları HTTP katmanında kalır, kaydedilmez.
- Herkese açık: `subtitle.keys.list`, `subtitle.keys.create`, `subtitle.keys.revoke`, `subtitle.status` (`address`, port, varsayılan deste, gelen kelime sayısı ve son zamanı, etkin anahtar sayısı), `subtitle.default_deck.set` (iç içe `settings.set`/`settings.reset`; geri alınabilir). Tipler `fc-api` `v1::subtitle`'dan üretilir; sözleşmenin tel tipleri de (`SubtitleCapture`, `SubtitleMe`, `SubtitleBatchResult`, `SubtitleKnownWords`, `SubtitleError` …) TS'ye çıkar. Olay `subtitle_changed`.
- Ekran: Ayarlar → Bağlantılar → **Subtitle** (`features/subtitle/`): kopyalanabilir sunucu adresi, üç kurulum adımı, anahtar listesi (oluştur → anahtar bir kez pencerede, iptal et), deste seçici, gelen kelime sayısı.
- Eklentiler `subtitle` alanını hiç alamaz (anahtarlar kullanıcının kararı).
- **Sözleşme denetimi:** `tools/subtitle-contract/check.mjs`, eklentinin kendi `api.ts` istemcisini (değiştirmeden) gerçek `fc api serve`'e karşı çalıştırır: `FC_BIN=target/debug/fc SUBTITLE_REPO=<subtitle deposu> node --experimental-transform-types tools/subtitle-contract/check.mjs`. `crates/fc-http/tests/subtitle.rs` aynı akışı ham HTTP ile sınar.
- **Kesintili teslim (v1.4 sürüm kabulü):** `check.mjs` sözleşmeden sonra `interrupted.mjs`'yi koşar: eklentinin **kendi kuyruğu** (`outbox.ts`: kuyruk, geri çekilme, yeniden deneme) ve istemcisi, değiştirilmeden, gerçek `fc api serve`'e araya giren bir vekil (`harness.mjs` `Proxy`) üzerinden bağlanır; vekil tek bir alışverişi bilerek bozar. Sınanan ve belgelenen sonuçlar:
  1. medya yüklendi, yakalama sunucuda işlenirken hat koptu → kuyruk yeniden dener → **tek kart**, resim ve ses birer kez (yeniden yükleme aynı `upload_id`'yi alır);
  2. sunucu kaydetti ama yanıt kayboldu (soket sıfırlandı) → yeniden deneme `200 duplicate`, **aynı `card_id`**;
  3. sunucu süreci toplu gönderimin ortasında `SIGKILL` ile öldü, yeniden başladı → kuyruk boşalır, **kopya yok, kayıp yok** (öldürme anı turdan tura toplu isteğin ortasına kaydırılır; her turda 20 yakalama, medyalı olanların resim/sesi birer kez);
  4. yakalama uygulamada **geri alındı**, sonra aynı `capture_id` yine geldi (eklentinin bekleyen yeniden denemesi) → **yeni kart** (`201 created`, yeni `card_id`; geri al yakalamayı unutturur, eski kart geri gelmez); sonraki gönderim yine `duplicate`;
  5. her şeyden sonra `io.backup.create` → **yeni bir profile** `io.backup.restore` → not, kart, medya sayıları aynı; bir ses dosyası eklentinin gönderdiği baytlarla **birebir**; geri yüklenen profil yakalama kimliklerini bilir (yeni anahtarla yeniden gönderim `duplicate`, aynı kart), eski anahtar `401`, tanı listesi boş başlar (geri yükleme tanı satırlarını düşürür; bu kabulün bulduğu açık, `subtitle::forget_keys`);
  6. `subtitle.captures.recent` her denemeyi doğru anlatır: yakalama komutuna ulaşan her deneme bir satır (`created` bir kez, sonra `duplicate`, aynı kart); gövdesi tamamlanmadan kesilen istek hiçbir şey yazmaz; süreç kayıt ile tanı satırı arasında öldürülürse o deneme satırsız kalır ve yeniden deneme `duplicate` olarak görünür.
  Rust tarafı aynı sunucu davranışını `crates/fc-http/tests/subtitle_interrupted.rs`'de sabitler (okunmayan yanıt, yarım gövde, aynı yakalamanın ve yüklemenin eşzamanlı yeniden gönderimi, geri al + yeniden gönderim). CI: `extension` işi, `SUBTITLE_REPO_TOKEN` sırrı (eklentinin özel deposunu okuma izni) tanımlıysa depoyu çekip `check.mjs`'yi koşar; tanımlı değilse uyarı verir ve atlar.

## 6. Derin bağlantılar

- Şema: `flashcard://` (uygulama adı kesinleşince değişebilir, bkz. açık kararlar).
- `flashcard://deck/<id>` · `flashcard://note/<id>` · `flashcard://study?deck=<id>` · `flashcard://study?query=<sorgu>` · `flashcard://add?deck=<id>&front=...&back=...` · `flashcard://import?url=...`
- Yazma yapan bağlantılar (add, import) onay ister ya da gelen kutusuna düşer.
- Tek örnek (single instance): uygulama açıksa bağlantı mevcut pencereye iletilir.
- Kart içi `[[bağlantılar]]` da dahili olarak aynı yönlendiriciyi kullanır.

#### Uygulanan hâli (v0.9)
- Ayrıştırma saf: `fc_core::inbox::deeplink` (şema büyük/küçük harfe duyarsız, sondaki `/` yok sayılır, yüzde kodlama + `+`, en çok 8 KiB). `flashcard://add` ayrıca `notetype=`, `tags=a,b`, `field.<anahtar>=`, `attr.<sınıflandırma>=<etiket>` (tekrar = çoklu seçim) alır.
- Anlam `deep_links.open` komutunda (modül `deep_links`): `deck`/`note`/`study` kimlikleri ve sorguyu denetleyip **hiçbir şey yazmadan** hedef döner; `add` **asla doğrudan not yazmaz**, kaynağı `deep_link` olan bir gelen kutusu önerisi olur (sıradan, geri alınabilir `inbox.add`); `import` şimdilik `VALIDATION_DEEP_LINK` (`reason: unsupported`).
- Kabuk (`src-tauri/src/outside.rs`): `tauri-plugin-single-instance` **ilk** eklenti (ikinci başlatma argümanlarını çalışan örneğe verip çıkar), `tauri-plugin-deep-link` şeması `tauri.conf.json`'da (`flashcard`; kurulum dosyası kaydeder, geliştirme derlemesi çalışırken kaydeder). Açılıştaki bağlantı argümanlardan okunur; sonuçlar arayüz alana kadar kuyrukta bekler (`take_deep_links`), pencere öne gelir.

## 7. Global hızlı ekleme ve pano izleme

- **Global kısayol** (ör. `Ctrl+Alt+N`): uygulama arka plandayken küçük, her zaman üstte bir ekleme penceresi. Panodaki metin önceden doldurulabilir.
- **Pano izleme** (varsayılan kapalı): kopyalanan metin filtrelere uyuyorsa (uzunluk, dil, desen) köşede küçük bir öneri: "Kart olarak ekle?" Görmezden gelinirse kaybolur. Gizlilik için parola yöneticisi gibi uygulamalardan gelen içerik (işletim sistemi işaretliyse) yok sayılır.

#### Uygulanan hâli (v0.9)
- **Hızlı ekleme** (modül `quick_add`): kısayol ayarı `quick_add.shortcut` (varsayılan `CommandOrControl+Alt+N`, yeniden başlatınca), her zaman üstte küçük pencere (`index.html#quick-add`), Klasik not ekler (`notes.create`, geri alınabilir), Esc gizler. `quick_add.prefill_clipboard` (varsayılan açık) panoyu **yalnız kısayola basılınca** okur; parola/anahtar gibi görünen metni doldurmaz.
- **Pano önerileri** (modül `clipboard`, **varsayılan kapalı**): kapalıyken kabuk panoyu **hiç okumaz**. Açıkken saniyede bir bakar; Windows'ta işletim sisteminin gizli işaretlediği içerik (`ExcludeClipboardContentFromMonitorProcessing`, geçmiş/bulut dışı) okunmadan atlanır; açıldığı anda panoda olan önerilmez; yalnız son metnin parmak izi tutulur. Karar `clipboard.check`'te (saf süzgeç `fc_core::inbox::clipboard`): uzunluk (`clipboard.min_chars`/`max_chars`), en çok 3 satır, URL/e-posta/dosya yolu değil, **sır gibi görünmüyor** (anahtar önekleri, JWT, Luhn'lu kart numarası, üç karakter sınıfı karıştıran tek "kelime"). Öneri kabul edilirse `inbox.add` (`via: clipboard`). Dil/desen süzgeci henüz yok.

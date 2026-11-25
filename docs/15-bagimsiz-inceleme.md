# 15 — Yazma yolunun bağımsız incelemesi (v1.4.2, J1)

Soru tek: kartların ya da çalışma geçmişin nerede kaybolabilir, bozulabilir ya da yanlış değişebilir? Kodu yazmamış biri olarak üç alan okundu: SQLite yazmaları, komutlar ve geri alma, eklentiler. Her düzeltmenin önce kırmızıya düşen bir testi var; test duruyor.

## Ne okundu

| Alan | Okunan | Nasıl |
|---|---|---|
| SQLite yazmaları | `fc-storage/src/store.rs`, `sqlite/mod.rs`, `migrate.rs`, `interrupt.rs`, `rescue.rs`, `media.rs` tamamı; `sqlite/content.rs` medya yazma ve artık dosya temizliği; `sqlite/interop.rs` geri yükleme (`JsonLoad`) | Satır satır |
| Komutlar ve geri alma | `fc-app/src/registry.rs` tamamı; `app.rs` açılış, `invoke*`, `run_command`, gruplar, `write_oplog`; `modules/core.rs` geri al/yinele ve `batch.execute`; iç içe çağrının hatasını yutan dört yer (`ai/meaning.rs`, `ai/notes.rs`, `notes.rs`, `modes/inline_cards.rs`); medya ekleme (`modules/media.rs`, `io/media.rs`) | Satır satır |
| Eklentiler | `fc-plugins/src/package.rs`, `models.rs` tamamı; `modules/plugins/host.rs` host çağrısı dağıtımı, dosya diyalogları, anahtar-değer yazması, süre sınırı; `runtime.rs` ve `limits.rs` yalnız sınır tanımları | Kısmen |

## Bulgular

Sıra: veri kaybı > yanlış veri > çökme > sağlamlaştırma.

| # | Önem | Yer | Ne oluyordu | Durum |
|---|---|---|---|---|
| 1 | Veri kaybı | `fc-storage/src/rescue.rs:194-224` | Bozuk koleksiyonda "okunabileni kopyala": etiketi olan tek bir not kaybolunca kurtarma tümüyle başarısız oluyordu (`note_tags` gibi `WITHOUT ROWID` tablolarda yetim satırın rowid'i yok, silme sorgusu hata veriyordu). | Düzeltildi, testli |
| 2 | Veri kaybı | `fc-storage/src/rescue.rs:286-399` | Ortasında bozuk bir sayfa olan tabloda bozuk sayfadan sonraki bütün satırlar geri gelmiyordu; yarı bozuk bir sayfa aralığı erken bitirip ~500 sağlam satırı sessizce düşürüyordu (rapor "0 kayıp" diyordu). Artık iki uçtan yürünüyor, bir aralık ancak bütün satırları geldiyse kopyalanmış sayılıyor. | Düzeltildi, testli |
| 3 | Veri kaybı | `fc-storage/src/rescue.rs:402` | `WITHOUT ROWID` tablo (ayarlar, etiket bağları, kelime dizini…) bütün olarak okunamazsa tamamı bırakılıyor ve "0 kayıp" sayılıyordu. Artık anahtar sırasıyla iki uçtan satır satır. | Düzeltildi, testli |
| 4 | Veri kaybı | `fc-storage/src/sqlite/content.rs:275` | Elektrik kesintisiyle yarım kalmış bir medya dosyası sonsuza dek bozuk kalıyordu: aynı resmi yeniden eklemek ya da yedeği geri yüklemek var olan dosyayı atlıyordu. Artık dosya ancak aynı baytları tutuyorsa korunuyor. | Düzeltildi, testli |
| 5 | Yanlış veri | `fc-storage/src/sqlite/mod.rs:101-140` | Bir yazma cümlesi süre sınırı, dolu disk ya da G/Ç hatasıyla durunca SQLite bütün işlemi kendisi geri alır ve bağlantıyı otomatik onaya döndürür. Aynı işlem tutamacından sonra yapılan yazmalar tek tek, işlemsiz kaydediliyordu: komutun yarısı (geri alma kaydı olmadan) kalabiliyordu. Artık tutamak başka yazma kabul etmiyor, `commit` `RolledBack` diyor. | Düzeltildi, testli (depo katmanında) |
| 6 | Yanlış veri | `fc-storage/src/rescue.rs:301` | Kurtarmada migration'ların ektiği satırlar (bayrak adları ve renkleri, AI fiyatları, istatistik sayacı) varsayılana dönüyordu: senin değiştirdiğin değer sessizce atılıyordu. | Düzeltildi, testli |
| 7 | Yanlış veri | `fc-storage/src/rescue.rs:313` | Kurtarma raporu, kısıt bozduğu için atılan çöp satırları kayıp saymıyordu. | Düzeltildi, testli |
| 8 | Sağlamlaştırma | `fc-plugins/src/models.rs:110-150` | Eklentinin bildirdiği model adresi `file://` ile ağ paylaşımını (`file:////sunucu/paylaşım`: Windows o sunucuya oturum açmayı dener), aygıtı ya da göreli yolu gösterebiliyordu. Artık yalnız bu bilgisayardaki normal dosya. | Düzeltildi, testli |
| 9 | Yanlış veri | `fc-app/src/app.rs:244`, `:285` | Masaüstü uygulaması, `fc` komut satırı uzun bir içe aktarma yaparken açılırsa: açılış o işi "yarıda kalmış" sayar, yazdığı parçaları geri alır ve işaretini siler; komut satırı yazmaya devam eder. Sonuç yarım bir içe aktarma olabilir. Artık açık her geri alma grubu ve çalışan her iş, süreç yaşadıkça bir işletim sistemi dosya kilidi tutuyor (`process-locks/`); açılış yalnız kilidi boşta olanı geri alır. | Düzeltildi (K1), iki gerçek süreçle testli |
| 10 | Veri kaybı (dar pencere) | `fc-storage/src/sqlite/content.rs:296` | Medya dosyası diske zorlanmadan (`fsync`) yeniden adlandırılıyor. Yazmadan hemen sonraki elektrik kesintisinde satır kalıp dosya boş kalabilir. 4 numara bunu yeniden eklemede onarır, ama önlemez. `fsync` her yakalamaya Windows'ta ölçülmesi gereken bir süre ekler; ölçmeden değiştirilmedi. Artık işlem onaylanmadan önce o işlemin yazdığı medya dosyaları diske zorlanıyor (çok dosyada birkaç iş parçacığıyla). Bedeli aşağıda. | Düzeltildi (K1), ölçülerek; elektrik kesintisi sahnelenemedi |
| 11 | Sağlamlaştırma | `fc-storage/src/migrate.rs:124-145` | Aynı anda açılan iki süreç aynı migration'ı uygulamaya kalkarsa ikincisi bir kez açılamaz (ikinci deneme işlemiyle birlikte geri alınır, veri bozulmaz). Artık ikinci süreç yazıcıyı bekliyor, migration'ın kendi işleminde yeniden bakıp yapılmış olanı atlıyor. | Düzeltildi (K1), testli |
| 12 | Sağlamlaştırma | `fc-storage/src/rescue.rs:286` | Bozuk sayfadan çıkan, bütün kısıtları tesadüfen sağlayan çöp bir satır kopyalanır. Yapısal: ayırt etmenin yolu yok. | Açık |

## Kapanış turu (v1.4.2, K1)

Açık kalan dört işin her biri önce kırmızıya düşen bir testle (mümkün olan yerde) gösterildi, sonra düzeltildi.

| # | Test ne gösterdi | Düzeltme | Kanıt |
|---|---|---|---|
| 9 | İki gerçek süreç: çocuk süreç `fc` gibi bağlanıp 120 satırı 10'arlık parçalarla içe aktarırken 3. parçadan sonra bekletildi; test masaüstünün tam açılışını yaptı. Açılış çocuğun grubunu sildi (işaret 1 yerine 0), parçalarını geri aldı, işini "yarıda kaldı" yaptı. | `support/live_lock.rs`: grup açan ve iş çalıştıran, `process-locks/<tür>-<kimlik>.lock` dosyasını kilitli tutar (`flock` / `LockFileEx`; süreç nasıl biterse bitsin kilidi sistem bırakır, süreç numarası ya da saat yok). Açılış kilidi alabiliyorsa sahibi ölmüştür: işareti **yeniden okur** (az önce bitmiş olabilir), geri alır. Alamıyorsa dokunmaz. İşin kilidi, işi sahiplenen işlem onaylanmadan alınır. | `modules/io/two_process_tests.rs`: canlı süreç (grup, parçalar, iş yerinde; içe aktarma tam biter, tek geri al adımı) ve ölen süreç (`abort`; parçalar gider, iş "yarıda kaldı"). Windows'ta yalnız CI doğrular. |
| 10 | Elektrik kesintisi testle sahnelenemez; ölçüldü. Bu makinede (ext4, sanal disk) ham bedel: dosya başına `fsync` ~4 ms, klasörüyle ~8 ms. Ölçekli takım (debug, paylaşımlı makine; önce 8, sonra 5 koşu, p95 aralığı ve ortancası): kurulumdaki yakalama 6,0–14,7 (10,5) → 8,3–22,4 (20,5) ms, bütçe 300; cevap kaydedilirken yakalama 20–51 (33) → 29–62 (38,5) ms, bütçe 600; o sırada cevap 13,9–27,7 → 14,8–40,1 ms, bütçe 400; yedek (264 medya) oluştur 15,6–20,3 → 16,8–21,4 sn, doğrula 11,7–14,9 → 11,5–16,2 sn. Bütçeler tutuyor; medyalı yakalamaya ~10 ms ekleniyor. Tam boy (release) takım koşulmadı: son ölçülen yakalama p95 12–23 ms'ye ~10 ms eklenince 50 ms bütçesinin altında kalması beklenir. Windows'taki bedel (`FlushFileBuffers`) yalnız CI'da görülür. | Dosya yeniden adlandırıldıktan sonra işlemin listesine girer; `commit` önce dosyaları (`fsync` / `FlushFileBuffers`), Unix'te klasörleri bir kez diske zorlar, sonra `COMMIT`. 16 dosyadan itibaren 8 iş parçacığı (geri yükleme, içe aktarma). Sıra önemli: satır, dosyası diskte olmadan kalıcı olamaz. | `tests/content.rs::commit_forces_the_media_files_of_its_transaction_to_disk` (yol: az dosya, çok dosya, silinmiş dosya, hata); bütçeler ölçekli takımda. |
| 11 | Bağlantı düzeyinde: ikinci açan "ne bekliyor?" diye baktıktan sonra birincisi hepsini uyguladı → ikincisi `table first already exists` ile düştü. Birincinin işlemi meşgul süresinden uzun sürünce ikincisi `Busy` ile düştü. Altı eşzamanlı açılış: yeni dosyada `Busy`, eski dosyada yedek adı çakışması. | Migration kendi `BEGIN IMMEDIATE` işleminde kayıtlı mı diye yeniden bakar, kayıtlıysa atlar. Şeması geride olan açılış yazıcıyı `MIGRATION_WAIT` (180 sn) bekler. Yeni dosyada WAL'a geçiş reddedilirse yeniden istenir. Yedek adı alınmışsa sıradaki ad; hiçbir şey uygulamayan açılış kendi yedeğini siler. | `tests/migrate_race.rs` (3 test; altı açılışlı olan 60 kez arka arkaya geçti). |
| `ai/meaning.rs` | Uygulama katmanında dolu disk (SQLite'ın kendi `max_page_count` sınırı, her seferinde bir sayfa sonra): `notes.create`, 4 komutluk `batch.execute`, `media.add`, `notes.update` her noktada **bütün olarak** düştü, `IO_DISK_FULL` ile, döküm bayt bayt aynı, uygulama çalışmaya devam etti. Yani 5 numaranın uygulama katmanındaki sonucu sağlam; üretim kodunda kırılan çıkmadı. Kanca ise her hatayı yutuyordu (depo hatası dahil). | `suggest_after_capture` artık yalnız reddi (kurulu değil, bütçe bitti…) yutar; depo hatasını geri verir. Kancayı bugün **çağıran yok** (yakalama yolu bağlamamış): gizli bir tuzaktı, canlı bir hata değil. Aynı kalıbın diğer üç yeri (`ai/notes.rs`, `notes.rs`, `modes/inline_cards.rs`) yalnız "kopya not" reddini yutuyor, depo hatasını geçiriyor: değişmedi. | `cut_short_tests.rs` (2 test). |

Yan etkiler: `File::try_lock` Rust 1.89'da geldi; `rust-version` 1.86'da bırakıldı (yükseltmek bütün kodda yeni lint'leri açıyor), `live_lock.rs` bu lint'i kendi içinde kapatır; derleme sabit araç zinciriyle (1.100) yapılır. Koleksiyon klasöründe yeni `process-locks/` klasörü (boş kalır; ölen sürecin dosyasını sonraki açılış siler). Kuyrukta bekleyen (henüz sahiplenilmemiş) işin sahibi yoktur: açılış onu eskisi gibi "yarıda kaldı" yapar.

## Bulgu çıkmayan yerler

- **İşlem sınırları:** her komut tek `BEGIN IMMEDIATE` içinde; oplog kaydı, kelime dizini ve otomasyon gözlemi aynı işlemde. Eklenti kancaları işlemin dışında, ayrı çağrılar olarak koşuyor. Olaylar ve bellek önbellekleri onaydan sonra.
- **Arka plan WAL denetim noktası:** yalnız `PASSIVE`, kendi bağlantısında; kapanışta önce o bağlantı kapanıyor. Sorun görülmedi.
- **Meşgul/kilitli:** `busy_timeout` 5 sn, `Busy` yeniden denenebilir hata olarak dönüyor. `COMMIT` hatasında işlem geri alınıyor.
- **Metinden kurulan SQL:** kurtarma ve geri yüklemede tablo/sütun adları şemadan geliyor ve tırnaklanıyor; değerler parametre.
- **İki süreç:** ikincil süreçler (`fc api serve`, `fc mcp serve`, `fc` komutları) açılış işlerini yapmıyor (`attach_state`), migration'ı izinsiz çalıştırmıyor. Artık medya temizliği yazma kilidi altında, başka sürecin yarım eklemesiyle çakışmıyor. 9 numara kapanış turunda düzeltildi.
- **Geri al/yinele:** saklı tersler çalıştırılmadan önce izin listesinden geçiyor, hepsi tek işlemde. İç içe çağrının hatasını yutan dört yer de yazmadan önce reddedilen durumları yutuyor.
- **Geri yükleme:** tablolar tek işlemde boşaltılıp dolduruluyor; yarıda kalırsa eskisi duruyor.
- **Eklenti paketi:** yalnız bellekte okunuyor, girdi boyutları sınırlı, kimlik ve yollar doğrulanıyor, diske eklentinin seçtiği bir yola yazılmıyor. Dosya okuma/yazma yalnız kullanıcının açtığı diyalogla.

## Okunmayanlar

- `fc-storage/src/sqlite/` altındaki alan dosyalarının çoğu (notlar, kartlar, çalışma, arama, istatistik, kelimeler…): tek tek cümleleri okunmadı.
- `fc-app` modüllerinin komutları tek tek okunmadı. Geri alma değişmezi v1.4.1'in rastgele komut testine (`undo_fuzz_tests.rs`) bırakıldı; o testin üretmediği komutlar (`NOT_GENERATED`) burada ayrıca denenmedi.
- Eklenti çalışma zamanının (`runtime.rs`) gövdesi, izin modeli (`permission.rs`), redaksiyon (`redact.rs`) ve `host.rs`'in `invoke` yolu.
- İşler (`jobs.rs`), yedek yazma, Anki aktarımı, HTTP katmanı, masaüstü kabuğu (`apps/desktop/src-tauri`).
- 5 numaranın uygulama katmanındaki sonucu kapanış turunda SQLite'ın sayfa sınırıyla (gerçek `SQLITE_FULL`) dört komutta denendi; süre sınırıyla kesilme uygulama katmanında ayrıca denenmedi (depo katmanında kanıtlı).

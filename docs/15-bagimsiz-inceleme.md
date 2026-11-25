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
| 9 | Yanlış veri | `fc-app/src/app.rs:244`, `:285` | Masaüstü uygulaması, `fc` komut satırı uzun bir içe aktarma yaparken açılırsa: açılış o işi "yarıda kalmış" sayar, yazdığı parçaları geri alır ve işaretini siler; komut satırı yazmaya devam eder. Sonuç yarım bir içe aktarma olabilir. | Açık. Yalnız okunarak bulundu, testle doğrulanmadı |
| 10 | Veri kaybı (dar pencere) | `fc-storage/src/sqlite/content.rs:296` | Medya dosyası diske zorlanmadan (`fsync`) yeniden adlandırılıyor. Yazmadan hemen sonraki elektrik kesintisinde satır kalıp dosya boş kalabilir. 4 numara bunu yeniden eklemede onarır, ama önlemez. `fsync` her yakalamaya Windows'ta ölçülmesi gereken bir süre ekler; ölçmeden değiştirilmedi. | Açık. Testle doğrulanmadı |
| 11 | Sağlamlaştırma | `fc-storage/src/migrate.rs:124-145` | Aynı anda açılan iki süreç aynı migration'ı uygulamaya kalkarsa ikincisi bir kez açılamaz (ikinci deneme işlemiyle birlikte geri alınır, veri bozulmaz). | Açık. Yalnız okunarak, testsiz |
| 12 | Sağlamlaştırma | `fc-storage/src/rescue.rs:286` | Bozuk sayfadan çıkan, bütün kısıtları tesadüfen sağlayan çöp bir satır kopyalanır. Yapısal: ayırt etmenin yolu yok. | Açık |

## Bulgu çıkmayan yerler

- **İşlem sınırları:** her komut tek `BEGIN IMMEDIATE` içinde; oplog kaydı, kelime dizini ve otomasyon gözlemi aynı işlemde. Eklenti kancaları işlemin dışında, ayrı çağrılar olarak koşuyor. Olaylar ve bellek önbellekleri onaydan sonra.
- **Arka plan WAL denetim noktası:** yalnız `PASSIVE`, kendi bağlantısında; kapanışta önce o bağlantı kapanıyor. Sorun görülmedi.
- **Meşgul/kilitli:** `busy_timeout` 5 sn, `Busy` yeniden denenebilir hata olarak dönüyor. `COMMIT` hatasında işlem geri alınıyor.
- **Metinden kurulan SQL:** kurtarma ve geri yüklemede tablo/sütun adları şemadan geliyor ve tırnaklanıyor; değerler parametre.
- **İki süreç:** ikincil süreçler (`fc api serve`, `fc mcp serve`, `fc` komutları) açılış işlerini yapmıyor (`attach_state`), migration'ı izinsiz çalıştırmıyor. Artık medya temizliği yazma kilidi altında, başka sürecin yarım eklemesiyle çakışmıyor. İstisna: 9 numara.
- **Geri al/yinele:** saklı tersler çalıştırılmadan önce izin listesinden geçiyor, hepsi tek işlemde. İç içe çağrının hatasını yutan dört yer de yazmadan önce reddedilen durumları yutuyor.
- **Geri yükleme:** tablolar tek işlemde boşaltılıp dolduruluyor; yarıda kalırsa eskisi duruyor.
- **Eklenti paketi:** yalnız bellekte okunuyor, girdi boyutları sınırlı, kimlik ve yollar doğrulanıyor, diske eklentinin seçtiği bir yola yazılmıyor. Dosya okuma/yazma yalnız kullanıcının açtığı diyalogla.

## Okunmayanlar

- `fc-storage/src/sqlite/` altındaki alan dosyalarının çoğu (notlar, kartlar, çalışma, arama, istatistik, kelimeler…): tek tek cümleleri okunmadı.
- `fc-app` modüllerinin komutları tek tek okunmadı. Geri alma değişmezi v1.4.1'in rastgele komut testine (`undo_fuzz_tests.rs`) bırakıldı; o testin üretmediği komutlar (`NOT_GENERATED`) burada ayrıca denenmedi.
- Eklenti çalışma zamanının (`runtime.rs`) gövdesi, izin modeli (`permission.rs`), redaksiyon (`redact.rs`) ve `host.rs`'in `invoke` yolu.
- İşler (`jobs.rs`), yedek yazma, Anki aktarımı, HTTP katmanı, masaüstü kabuğu (`apps/desktop/src-tauri`).
- 5 numaranın uygulama katmanındaki sonucu (bir komutun yarım kalması) gerçek dolu diskle denenmedi; depo katmanında süre sınırıyla kanıtlandı.

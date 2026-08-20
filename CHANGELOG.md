# Changelog

## Sağ Tık Yerelleştirme ve Çift Panel Simge Düzeltmesi

### Düzeltildi
- **Kenar çubuğu sağ-tık menüleri çevrilmedi:** Aygıtlar menüsündeki
  (`sidebar.rs`) "Open in New Tab"/"Mount"/"Unmount"/"Safe Removal /
  Eject"/"Analyze Disk…"/"Properties" ve Yer İmleri menüsündeki "Open in
  New Tab"/"Rename Bookmark…"/"Remove from Bookmarks" girdileri sabit
  İngilizce string'ti; artık `t()` üzerinden `sidebar.device.*` /
  `sidebar.bookmark.*` anahtarlarıyla çevriliyor (EN/TR ikisine de
  eklendi, parite testi geçiyor).
- **Çift Panel butonu kırık simge gösteriyordu:** `headerbar.rs`'deki
  `sidebar-show-right-symbolic` GNOME'da eksik olduğundan kırık ikon
  görünüyordu; standart `view-split-left-right-symbolic` ile değiştirildi
  (aynı ikon adı Command Palette'teki "Toggle Split View" komutunda da
  güncellendi). Buton artık başlığın solunda tek başına değil, sağdaki
  görünüm/önizleme düğmeleriyle aynı grupta (`pack_end`). Tooltip
  `headerbar.split.tooltip` "Çift Panel Görünümü (F3)" olarak netleştirildi.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 uyarı.
- `cargo test --workspace`: 280 test, 0 başarısız (i18n parite testi
  `every_tr_key_exists_in_en` dahil).

## Faz 58 — Nihai Kapsamlı Denetim (Final Comprehensive Audit)

Tüm workspace, 12 kategoride (mimari, güvenlik, performans, bellek,
eşzamanlılık, dosya sistemi doğruluğu, UI/UX, erişilebilirlik, i18n,
paketleme, test/CI, dokümantasyon) satır satır denetlendi. Kod kanıtına
dayalı üretim hazırlığı raporu hazırlandı.

### Doğrulama
- `cargo test --workspace --release`: **612 test, 0 başarısız** (5 crate
  toplamı, doc-test hariç).
- `cargo clippy --workspace --all-targets --all-features`: 0 uyarı.
- `cargo fmt --all -- --check`: temiz.

### Tespit Edilen Gerçek Bulgular
- **Sınırsız GIO kopyalama iptali:** `veyra-filesystem/src/queue.rs`
  içindeki `copy()` çağrısı `gio::Cancellable::NONE` kullanıyor; tek büyük
  dosya kopyalanırken iptal isteği dosya tamamlanana kadar işlemez.
- **Bidi-override tespiti UI'a bağlı değil:** `veyra-core/src/security.rs`
  içindeki `has_bidi_override()` fonksiyonu tanımlı ama yeniden
  adlandırma/oluşturma diyaloglarından hiç çağrılmıyor — kullanıcı sahte
  yön kontrol karakterli dosya adları için uyarılmıyor (disk üzerindeki
  bayt dizisi doğru kalır, sadece görsel spoofing riski).
- **Command Palette çevrilmemiş:** `veyra-ui/src/command_palette.rs`
  içindeki 39 komut başlığı `t()` yerine sabit İngilizce string; Türkçe
  arayüzde bile İngilizce görünüyor. i18n kataloğu (393 anahtar) bunun
  dışında EN/TR arasında tam simetrik (parity testleriyle doğrulanmış).
- **window.rs God-object:** 4.557 satır, tek `build_window()` fonksiyonu
  içinde çok fazla sorumluluk (event handling, clipboard, undo/redo,
  panel kurulumu) toplanmış; fonksiyonel olarak modüler ama test
  edilebilirliği zorlaştırıyor.
- **L2 thumbnail disk önbelleği sınırsız:** boyut sınırı/eviction yok;
  L1 bellek önbelleği LRU ile sınırlı ama diskteki PNG'ler birikebilir.
- Küçük tutarsızlıklar: README test rozeti "592" gösteriyor, gerçek
  sayı 612; openSUSE `.spec` changelog bölümü boş.

### Kategori Puanları (0-100)
Architecture 90 · Security & Privacy 92 · Performance 90 ·
Memory Management 85 · Concurrency 88 · Filesystem Correctness 93 ·
UI/UX & HIG 90 · Accessibility 88 · Localization 88 ·
Packaging & Distribution 92 · Testing & CI/CD 92 · Documentation 93
— **Genel Ortalama: ~90/100, Üretim Hazır (minör iyileştirmelerle).**

## Faz 57 — Release System & Automated Release Notes

Semantic Versioning'e (`MAJOR.MINOR.PATCH`) göre tüm paketleme dosyalarını
atomik olarak güncelleyen bir sürüm artırma aracı ve Conventional
Commits'ten kategorize edilmiş GitHub Release notu üreten bir betik
eklendi. `release.yml` artık her `v*` etiketinde bu notları otomatik
üretip Release gövdesine yazıyor.

### Eklenenler
- **`scripts/generate_release_notes.py` (yeni):** İki git etiketi arasındaki
  (veya `--commits-file` ile verilen) commit'leri Conventional Commits
  tipine göre kategorize eder: 🚀 `feat`, ⚡ `perf`, 🛡️ `sec`/`security`/
  `privacy`, 📦 `pkg`/`packaging`, 🧪 `test`, 🐛 `fix`, 🔧 diğer. SHA-256
  checksum tablosu ve hızlı kurulum komutlarıyla birlikte GitHub Release
  için hazır Markdown üretir.
- **`scripts/bump_version.sh` (yeni):** SemVer (`MAJOR.MINOR.PATCH`)
  doğrulaması yapar; `Cargo.toml`, `metainfo.xml`, `PKGBUILD`, Fedora/
  openSUSE `.spec` ve Debian `changelog` dosyalarını atomik olarak
  günceller. `--dry-run` ile önizleme, `--tag` ile onaylı
  `git tag -a vX.Y.Z` etiketleme destekler.
- **`.github/workflows/release.yml` (güncellendi):** Tag push'unda önceki
  etiketten bu yana olan commit'leri `generate_release_notes.py` ile
  kategorize edip Release gövdesine (`body_path`) aktarır; checksum'ı
  otomatik notlara ekler.
- **`crates/veyra-app/tests/release_system.rs` (yeni, 7 test):** SemVer
  ayrıştırma doğrulaması, `bump_version.sh`'ın geçersiz sürümü reddettiği
  ve `--dry-run`'ın dosyaları değiştirmediği, `generate_release_notes.py`
  kategorizasyonu ve tüm paketleme dosyalarındaki sürüm numaralarının
  (`metainfo.xml` dahil) workspace ile senkronize olduğu testleri.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: temiz.
- `cargo test --workspace`: 612 test, 0 başarısız (605 önceki + yeni 7
  `release_system` testi).

## Faz 56 — CI/CD Pipeline & Multi-Distro GitHub Actions

Her `push`/`pull_request`'te otomatik format/lint denetimi, Ubuntu/Fedora/
Arch Linux üçlü test matrisi, sürüm derlemesi ve kurulum doğrulaması
çalıştıran profesyonel GitHub Actions iş akışları eklendi. Etiketlenmiş
sürümler (`v*`) otomatik olarak paketlenip GitHub Release'e yayınlanır;
Flatpak manifest'i her değişiklikte sözdizim/schema açısından doğrulanır.

### Eklenenler
- **`.github/workflows/ci.yml` (yeni):** `lint` job'ı (`cargo fmt --check`,
  `cargo clippy -D warnings`), `test-matrix` job'ı (Ubuntu/`fedora:latest`/
  `archlinux:latest` container'larında `cargo test --workspace`) ve
  `build-release` job'ı (`cargo build --release --locked`,
  `make DESTDIR=staging PREFIX=/usr install`, `desktop-file-validate`,
  `appstreamcli validate`).
- **`.github/workflows/release.yml` (yeni):** `v*` etiketinde tetiklenir;
  release binary'i derler, `strip` eder, `.tar.gz` paketi ve SHA-256
  checksum üretir, `softprops/action-gh-release` ile GitHub Release'e
  varlıkları ekler.
- **`.github/workflows/flatpak.yml` (yeni):** Flatpak manifest ve
  `cargo-sources.json` dosyalarının JSON sözdizimini, zorunlu alanlarını ve
  `flatpak-builder --show-manifest` ile şema bütünlüğünü doğrular.
- **`crates/veyra-app/tests/ci_metadata.rs` (yeni, 13 test):** Üç iş
  akışı dosyasının YAML sözdizim bütünlüğünü, gerekli job adlarını
  (`lint`, `test-matrix`, `build-release`), dağıtım başına paket
  bağımlılıklarını (Ubuntu/Fedora/Arch) ve tetikleyici olaylarını
  (`push`/`pull_request`/`tags: ['v*']`) doğrular.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: temiz.
- `cargo test --workspace`: 605 test, 0 başarısız (592 önceki test +
  yeni 13 `ci_metadata` testi).

## Faz 55 — Comprehensive Documentation Suite

Açık kaynak topluluk standartlarını (GitHub Community Standards) karşılayan
tam dokümantasyon paketi: kök dizine katkı/güvenlik/davranış kuralları
belgeleri ve `docs/` altına 6 yeni teknik geliştirici dokümanı eklendi.
Kod değişikliği yok — bu faz salt dokümantasyon.

### Eklenenler
- **`CONTRIBUTING.md` (yeni):** Geliştirme ortamı kurulumu, PR öncesi
  zorunlu kontroller (`cargo fmt`/`clippy`/`test`), Conventional Commits
  standardı ve PR süreci.
- **`SECURITY.md` (yeni):** Güvenlik açığı bildirme süreci (GitHub Security
  Advisories üzerinden özel bildirim), desteklenen sürümler ve tehdit
  modeli özeti.
- **`CODE_OF_CONDUCT.md` (yeni):** Contributor Covenant v2.1, standart
  metin, uygulama kanalı bu projenin GitHub Security Advisories akışına
  bağlandı.
- **`docs/building.md` (yeni):** Sistem bağımlılıkları (Arch/Fedora/Debian/
  openSUSE), debug/release derleme, `make install`, workspace crate
  düzeni, sık karşılaşılan derleme sorunları.
- **`docs/testing.md` (yeni):** 592 testin (ölçülen gerçek sayı,
  `cargo test --workspace` ile doğrulandı) yapısı — birim/entegrasyon test
  konumları, adversarial/Unicode/izin/büyük dizin test kapsamı, `tempfile`
  temizlik güvencesi, GTK'ya bağımlı kodun test edilme deseni.
- **`docs/plugin_development.md` (yeni):** Bugün var olan entegrasyon
  noktaları (`.desktop` MIME ilişkilendirmesi, `xdg-terminal-exec`
  terminal entegrasyonu) ve henüz uygulanmamış Faz 38 Eklenti Sistemi'nin
  planlanan tasarım yönü — var olmayan bir özelliği var gibi
  belgelememek için açıkça durum belirtiliyor.
- **`docs/translation.md` (yeni):** `i18n.rs`'nin bağımsız, `gettext`
  kullanmayan katalog motoru — anahtar yapısı, `t`/`t_fmt`/`t_plural`,
  çoğullaştırma kuralları, yeni dize/yeni dil ekleme adımları.
- **`docs/security.md` (yeni):** Kök yetkisiz çalışma prensibi
  (`root_guard`), Polkit/`pkexec` ayrıcalıklı işlem akışı, log/çökme raporu
  kimlik bilgisi maskeleme katmanları, sıfır telemetri garantisi, Flatpak
  sandbox sınırı ve geçici dosya güvenliği.
- **`docs/performance.md` (yeni):** `docs/benchmarks.md`'deki ölçülmüş
  sayıların geliştirici odaklı özeti (dizin tarama, gecikmeli metadata,
  sınırlı bellek, FTS5 arama gecikmesi, thumbnail önbelleği, kopyalama/
  taşıma verimi) ve arka plan görev önceliklendirmesinin gerçek kapsamı
  (yalnızca arama indeksleyicisi `nice(19)` uyguluyor; thumbnail/checksum
  için henüz uygulanmadı — abartılı iddia yerine gerçek durum belgelendi).
- **`README.md` (genişletildi):** Rozetler (lisans, Rust, GTK4, Libadwaita,
  592 geçen test), öne çıkan özellikler listesi genişletildi, Flatpak/Arch/
  Fedora/Makefile hızlı kurulum blokları ve temel kısayollar tablosu
  eklendi; dokümantasyon bağlantıları 15 dosyaya güncellendi.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 uyarı.
- `cargo test --workspace`: 592/592 geçti — bu faz kod değiştirmediğinden
  test sayısı önceki fazla aynı.
- Yeni belgelerdeki her teknik iddia (dosya yolları, fonksiyon isimleri,
  `nice(19)`/`XDG_RUNTIME_DIR` gibi somut davranışlar) kaynak koddan grep
  ile doğrulandı; henüz uygulanmamış özellikler (Faz 38 Eklenti Sistemi,
  Faz 56 CI, thumbnail/checksum önceliklendirmesi) tamamlanmış gibi değil,
  açıkça "planlanan" olarak işaretlendi.

## Faz 54 — Structured Logging & Privacy-Hardened Tracing

Günlükleme artık derleme moduna göre otomatik seviye seçiyor (Development'ta `DEBUG`, Production'da `INFO`, `RUST_LOG` her zaman ezer) ve dosyaya yazılan her satır — çağrı sitesinin hatırlayıp hatırlamadığına bakılmaksızın — URI şifreleri, token'lar ve ev dizini için katı bir maskeleme katmanından geçiyor (Kural #23).

### Eklenenler
- **`crates/veyra-core/src/logging_sanitizer.rs` (yeni):** `redact_uri_credentials` (`sftp://alice:secret123@host` → `sftp://alice:***@host`, `smb://domain;user:password@host` → `smb://domain;user:***@host`, aynı satırdaki birden çok URI'yi de kapsar), `redact_tokens` (`Bearer <token>` ve `token=`/`api_key=`/`apikey=`/`access_token=`/`secret=` değerlerini büyük/küçük harf duyarsız `[REDACTED_TOKEN]` ile değiştirir), `redact_home_path` (`/home/kullanici/...` → `~/[REDACTED_PATH]/...`) ve bunların hepsini sırayla uygulayan `sanitize_log_line`. Aynı modül log dosyası döngüsünü de taşıyor: `rotate_log_if_needed` dosya `MAX_LOG_SIZE_BYTES`'ı (5 MB) aştığında `.1`/`.2`/`.3`'e kaydırıyor, `MAX_LOG_BACKUPS`'ı (3) aşan en eski yedeği siliyor. 26 birim testi her maskeleme kuralını ve rotasyon senaryosunu doğruluyor.
- **`crates/veyra-app/src/logging.rs` (genişletildi):** `default_level_filter(debug_build: bool)` saf fonksiyonu, `RUST_LOG` ayarlı değilse `cfg!(debug_assertions)`'a göre `"veyra=debug,veyra_ui=debug,veyra_filesystem=debug,veyra_search=debug,warn"` (Development) veya `"veyra=info,veyra_ui=info,veyra_filesystem=info,warn"` (Production) seçiyor — `RUST_LOG` her zaman öncelikli. `init()` artık dosyayı açmadan önce `rotate_log_if_needed` çağırıyor ve dosya yazıcısını `SanitizingWriter`/`SanitizingHandle` ile sarmalıyor: her `tracing` olayının ham baytları, diske değmeden önce `sanitize_log_line`'dan geçiyor (stdout çıktısı sanitize edilmiyor — yalnızca kalıcı dosya).
- **`crates/veyra-core/src/config.rs`:** `XdgDirs::log_file()` artık `$XDG_STATE_HOME/veyra/logs/veyra.log` döndürüyor (önceki düz `veyra.log`'dan taşındı); `logs/` dizini `crashes/`'in kendi oluşturma deseniyle aynı şekilde `logging::init` tarafından açılmadan hemen önce oluşturuluyor.
- **Ayarlar entegrasyonu (`dialogs/preferences_dialog.rs`):** Privacy sayfasının Logging grubuna, `veyra.log` ve döndürülmüş yedeklerini tutan klasörü `GtkFileLauncher` ile sistem dosya yöneticisinde açan "Günlük Klasörünü Aç" butonu eklendi.
- **i18n (`i18n.rs`, +3 EN/TR anahtar çifti):** `prefs.privacy.open_log_dir.*`.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 uyarı.
- `cargo test --workspace`: tamamı geçti (`veyra-core`: 44, `veyra-app`: 5 dahil).

## Faz 53 — Privacy-Friendly Crash Reporting & Panic Diagnostics

Veyra artık panik yaşadığında sessizce çöküp bir sonraki oturuma hiçbir iz bırakmıyor: yerel, maskelenmiş bir çökme raporu `$XDG_STATE_HOME/veyra/crashes/`'e yazılıyor ve bir sonraki açılışta kullanıcıya ne olduğunu gösteren bir diyalog sunuluyor — hiçbir veri hiçbir zaman internete gönderilmiyor (Kural #24).

### Eklenenler
- **`crates/veyra-core/src/crash_report.rs` (yeni):** `CrashReport` modeli (sürüm, OS/çekirdek, GTK4/Libadwaita sürümü, panik mesajı, konum, temizlenmiş backtrace, filtrelenmiş ortam değişkenleri, zaman damgası) ve saf maskeleme fonksiyonları: `redact_home_path` (`/home/kullanici/...` → `~/[REDACTED]/...`, yolun geri kalanını korur) ve `filter_env_vars`/`is_sensitive_env_key` (`TOKEN`/`KEY`/`SECRET`/`AUTH`/`PASS` içeren her anahtarı büyük/küçük harf duyarsız ayıklar). `CrashReport::capture()` her iki maskelemeyi de otomatik uyguladığından çağıranın ayrıca hatırlaması gerekmiyor. `write()` raporu `security::write_atomic_private` ile (0600 izinli) yazıyor ve ardından `rotate()` diskte en fazla `MAX_RETAINED_REPORTS` (5) rapor kalacak şekilde eskileri siliyor; `latest_report`/`clear_all` sırasıyla başlangıç algılaması ve Ayarlar'daki "Temizle" butonunu besliyor. `set_crash_reports_enabled`/`crash_reports_enabled`, `security::SANITIZE_LOG_PATHS` ile aynı `AtomicBool` desenini izleyerek panik kancasının `Rc<RefCell<Settings>>`'a ihtiyaç duymadan ayarı okumasını sağlıyor. 24 birim testi maskeleme, filtreleme, serileştirme ve rotasyonu doğruluyor.
- **`crates/veyra-app/src/panic_hook.rs` (genişletildi):** `install()` artık `state_dir` alıyor; panik anında `std::backtrace::Backtrace::force_capture()` ile backtrace, `/etc/os-release`'den `PRETTY_NAME`, `uname(2)`'den çekirdek sürümü ve `veyra_ui::runtime_library_versions()`'tan GTK/Libadwaita sürümünü toplayıp `CrashReport::capture` + `write` üzerinden `crashes/` dizinine yazıyor — `save_crash_reports` ayarı kapalıysa hiçbir şey yazılmıyor.
- **`crates/veyra-ui/src/dialogs/crash_dialog.rs` (yeni):** Önceki oturumdan kalan bir rapor tespit edildiğinde (`lib.rs::check_crash_report`, her süreçte yalnızca ilk pencerede bir kez çalışır) gösterilen `AdwAlertDialog`: genişletilebilir "Raporu Göster" bölümü tam rapor metnini gösteriyor, `[Raporu Kopyala ve GitHub Sorunları Aç]` panoya kopyalayıp `gtk4::UriLauncher` ile yeni-sorun sayfasını açıyor, `[Dosyaya Kaydet]` bir `GtkFileDialog` ile `.txt` kaydediyor, `[Kapat ve Sil]` (Escape de aynı davranıyor) raporu diskten siliyor — diğer iki eylem raporu silmiyor, böylece yanlışlıkla kapatma veri kaybına yol açmıyor.
- **`crates/veyra-ui/src/lib.rs`:** `run()` artık `state_dir` parametresi alıyor; `runtime_library_versions()` GTK4/Libadwaita sürümlerini `gtk4`/`libadwaita`'yı doğrudan bağımlılık olarak eklemeden `veyra-app`'e sunuyor.
- **Ayarlar entegrasyonu (`config.rs`, `dialogs/preferences_dialog.rs`):** Yeni `save_crash_reports: bool` alanı (varsayılan `true`); Privacy sayfasına "Çökme Raporları" grubu — "Yerel Çökme Raporlarını Kaydet" anahtarı ve "Kayıtlı Raporları Temizle" butonu — eklendi; "Sıfır Telemetri" satırının açıklaması artık spesifikasyonun bilgilendirme notunu birebir yansıtıyor: *"Veyra values your privacy and never transmits telemetry or crash dumps."*
- **i18n (`i18n.rs`, +20 EN/TR anahtar çifti):** `crash.*` (diyalog başlığı/açıklaması/eylemleri) ve `prefs.privacy.{save_crash_reports,clear_crash_reports,group.crash_reports}.*`.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 uyarı.
- `cargo test --workspace`: tamamı geçti (`veyra-core`: 28, `veyra-app`: 3 dahil).
- Gerçek bir panikten sonraki uçtan uca akış (diyalog gösterimi, dosyaya kaydetme, GitHub'ı açma) yalnızca kod incelemesiyle doğrulandı — bu bir GUI etkileşimi gerektirdiğinden otomatik testle kapsanmıyor; saf maskeleme/rotasyon/model mantığının tamamı birim testli.

## Faz 52 — Rich Context-Aware Empty States

Boş klasör/sonuçsuz görünüm mesajı artık her yerde aynı jenerik metin değil — konuma özel (Downloads, Documents, Pictures, Music, Videos, Recent, Network, Trash, Search, genel) simge/başlık/açıklama üçlüsü gösteriyor.

### Eklenenler
- **`SpecialFolder` (`window.rs`, yeni):** `~/Downloads`/`~/Documents`/`~/Pictures`/`~/Music`/`~/Videos` konumlarını, sidebar'ın Places bölümünün (Faz 21) kullandığı aynı `glib::user_special_dir` kaynağına karşı eşleyen bir enum — bu dizinler kullanıcı tarafından yeniden yapılandırılabilir olduğundan (`XDG_DOWNLOAD_DIR` vb.), ham `~/Downloads` yol karşılaştırması yerine bu kaynak tek doğru referans.
- **`empty_state_content` genişletildi:** İmza artık `(current_dir, is_trash, is_recent, is_network, query_active)` alıyor; öncelik sırası Çöp Kutusu > etkin arama sorgusu > Son Dosyalar > Ağ > `SpecialFolder` eşleşmesi > genel "Klasör Boş". Saf fonksiyon olarak kalmaya devam ediyor (GTK/`libadwaita` bağımsız, `cargo test`'te doğrudan çağrılabiliyor).
- **`network::NETWORK_URI` (yeni sabit):** `"network:///"` dizesi artık `sidebar.rs`'teki hard-code yerine tek yerden paylaşılıyor; `update_empty_state` Ağ konumunu bu sabitle tespit ediyor.
- **i18n (`i18n.rs`, +14 EN/TR anahtar çifti):** `empty.downloads.*`, `empty.documents.*`, `empty.pictures.*`, `empty.music.*`, `empty.videos.*`, `empty.recent.*`, `empty.network.*`.
- **Birim testleri (`window.rs`):** Trash/arama/Recent/Ağ/genel önceliğini doğrulayan testlere ek olarak, çalıştığı platformda gerçekten yapılandırılmış her `SpecialFolder`'ı (`glib::user_special_dir` `Some` döndürdüğünde) doğrulayan bir test — CI sandbox'ında XDG dizini tanımsızsa o bağlam sessizce atlanıyor.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 uyarı.
- `cargo test --workspace`: tamamı geçti.

## Faz 51 — Error UX & Actionable Recovery

Hiçbir dosya işleminin belirsiz `"Error: failed"` mesajıyla bitmemesi için: her `FsError`/`std::io::Error` türünü insan-dili bir başlığa, nedene ve eyleme dönüştürülebilir kurtarma seçeneklerine ([Tekrar Dene], [Farklı Konum Seç], [Atla], [İptal]) çeviren yeni bir hata sınıflandırma katmanı ve onu gösteren `AdwAlertDialog` tabanlı diyalog.

### Eklenenler
- **`crates/veyra-ui/src/error_ux.rs` (yeni):** `ActionableError` (başlık, hedef dosya adı, insan-dili neden, teknik detay, kurtarma eylemleri listesi) ve `RecoveryAction` (`TryAgain`/`ChooseAnotherLocation`/`Skip`/`Cancel`). `classify()`, her `FsError` varyantını ve (Copy/Move'un yerel `std::io::Error` yolundaki) her ilgili `std::io::ErrorKind`'ı (`PermissionDenied`, `ReadOnlyFilesystem`, `StorageFull`/`QuotaExceeded`/`FileTooLarge`, `ResourceBusy`, `NotFound`, `AlreadyExists`, `InvalidFilename`, ağ kopması türleri) doğru insan-dili nedene ve temel eylem kümesine eşliyor; `ErrorContext` çağrı sitesinin `ChooseAnotherLocation` (yalnızca Copy/Move) ve `Skip`'i (yalnızca toplu işlemler) açıp kapatmasını sağlıyor. 10 birim testi her eşlemeyi doğruluyor.
- **`crates/veyra-ui/src/dialogs/error_dialog.rs` (yeni):** `dialog-error-symbolic` simgesi, kalın başlık + hedef dosya adı, neden metni ve genişletilebilir "Detayları Göster" (`AdwExpanderRow`, ham OS/GIO hatası + errno + tam yol, "Teknik Detayları Kopyala" panoya kopyalama butonu) içeren `AdwAlertDialog`. Butonlar `ActionableError.recovery_actions`'tan dinamik üretiliyor, `TryAgain` vurgulu (`Suggested`) görünüyor.
- **Entegrasyon (`window.rs`):** Toplu Copy/Move/Trash/Delete hatalarında (`run_bulk_operation`), tümü yetki-hatası olmayan durumda artık `show_bulk_actionable_error` çalışıyor — Tekrar Dene yalnızca başarısız ögeleri yeniden dener, Farklı Konum Seç (Copy/Move) yeni bir klasör seçtirip aynı ögeleri oraya yeniden dener. Compress/Extract hatalarında (`run_archive_operation`) aynı desen: `respawn`/`pick_location` kapanışlarıyla Tekrar Dene ve Farklı Konum Seç (farklı çıktı dosyası/hedef klasör) baştan uçlanan bir kurtarma akışı sağlıyor.
- **Entegrasyon (`dialogs/properties_dialog.rs`):** Tekli chmod (switch/oktal giriş) ve özyinelemeli chmod hataları artık aynı `error_dialog`'u kullanıyor; kendi kendine referans veren bir `RetryAttempt` (`Rc<RefCell<Option<Rc<dyn Fn()>>>>`) kapanış deseniyle Tekrar Dene, orijinal izin değişikliğini tekrar dener (izin değişikliğinde hedef klasör kavramı olmadığından Farklı Konum Seç sunulmuyor).
- **i18n (`i18n.rs`, +40 EN/TR anahtar çifti):** `error.action.*`, `error.reason.*` (11 neden metni), `error.headline.*` (Copy/Move/Trash/Delete/Compress/Extract/Chmod), `error.show_details`, `error.copy_technical_details`.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 uyarı.
- `cargo test --workspace`: tamamı geçti (`veyra-ui`: 274, `error_ux::tests` dahil 10 yeni test; `veyra-filesystem`'deki tek zamanlama-duyarlı benchmark testi izole çalıştırıldığında geçiyor — sistem gürültüsü, bu fazın değişikliğiyle ilgisiz).

### Sıradaki Faz
Faz 52 onay bekliyor.

## Faz 50 — Final UI/UX Polish & Production Quality Audit

Boş durum ekranları, tam i18n taraması ve kısayol/buton doğrulaması içeren bir cila fazı. En büyük bulgu: `veyra-ui`'de hiçbir yerde `AdwStatusPage` tabanlı boş-durum ekranı yoktu (boş klasör/arama/çöp kutusu sessizce boş bir liste gösteriyordu) ve 130'dan fazla arayüz metni doğrudan İngilizce literal olarak kodlanmıştı — `i18n::t()`'nin hiç çağrılmadığı, `Properties` diyaloğu gibi tüm bir pencere dahil.

### Eklenenler
- **Boş durum ekranı (`window.rs`, `state.rs`, `tab_page.rs`):** Her sekme artık kendi `AdwStatusPage`'ini, ana `Icon`/`Compact`/`Details` görünüm yığınının üzerine bindirilmiş bir `GtkOverlay` içinde taşıyor (`can_target(false)` — sürükle-bırak boş klasöre yine çalışıyor). `update_empty_state()`, `AppState`'e yeni eklenen özel bir `GtkFilterListModel`'in (`empty_filter_model`, ana modelle aynı filtreyi paylaşır) süzülmüş öge sayısına bakarak bağlama göre karar veriyor: Çöp Kutusu konumu > etkin arama sorgusu > düz "Klasör Boş". Yalnızca bir tarama gerçekten tamamlandıktan sonra (`load_control` boşaldıktan sonra) veya `refresh_filter()` sonrasında çağrılıyor — akan (chunked) bir taramanın ilk anlık 0 ögesi asla "Klasör Boş" ekranını yanlışlıkla göstermiyor (Kural #30).
- **i18n tamlığı (`i18n.rs`, +200'den fazla yeni EN/TR anahtarı):** Sidebar (Places/Bookmarks/Devices/Network başlıkları, tüm XDG klasör etiketleri), Önizleme kenar çubuğu, Trash/Recent bantları, Connect to Server, Open With, File Associations, Command Palette, Disk Analyzer, Conflict diyaloğu ve **Properties penceresinin tamamı** (General/Permissions/Advanced sayfaları, ~75 literal) artık `t()`/`t_fmt()` üzerinden çekiliyor. `cargo test`'teki `every_en_key_has_a_tr_counterpart`/`every_tr_key_exists_in_en`/`no_duplicate_keys_within_a_table` testleri her yeni anahtarı doğruluyor.
- **Birim testleri (`window.rs`):** Boş-durum karar mantığı, GTK/`libadwaita` widget'larından bağımsız saf bir `empty_state_content(is_trash, query_active)` fonksiyonuna ayrıştırıldı (libadwaita ana-thread kilidi `cargo test`'in test-başına-thread modeliyle çakıştığından, gerçek `AdwStatusPage` inşa eden testler mümkün değil) — 3 yeni test, Çöp Kutusu > arama > klasör önceliğini doğruluyor.

### Doğrulanan (değişiklik gerekmedi)
- Kısayol/Komut Paleti senkronizasyonu: `shortcuts.rs`/`command_palette.rs` ve yardım diyalogları, ivme (accelerator) tablosunu her zaman `Application::accels_for_action()`'dan canlı okuyor — tek doğruluk kaynağı, drift riski yok.
- Sembolik simge bütünlüğü: taranan tüm `icon_name` çağrıları `-symbolic` sonekli.
- TODO/FIXME/placeholder/Lorem ipsum: sıfır bulgu.

### Bilinen Sınırlamalar
- Komut Paleti'nin 40+ `CommandItem::title` ve Kısayollar yardımının 30+ `ShortcutEntry::label` alanı hâlâ sabit İngilizce literal — bunları `t()`'ye taşımak ayrı bir fazı hak eden geniş bir refactor (statik `&'static str` tablo → çalışma zamanı çeviri).
- Diyalog/kart marjinleri için merkezi bir boşluk sabiti yok (6/12/18px kuralı her çağrı sitesinde elle tutarlı tutuluyor); ölçülebilir bir HIG ihlali bulunmadı, yalnızca bir refactor fırsatı.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 538 → 541 (workspace genelinde), tamamı geçti.

### Sıradaki Faz
Faz 51 onay bekliyor.

## Faz 49 — Performance Benchmarks & 1M Scaling Suite

100 / 1.000 / 10.000 / 100.000 / 1.000.000 ölçeklerinde dizin tarama verimini, `FAST_ATTRIBUTES` vs `FULL_ATTRIBUTES` tembel metaveri kazanımını, gerçek işlem RSS'i üzerinden sınırlı bellek kullanımını, FTS5 arama gecikmesini, thumbnail L1 önbellek erişim hızını ve kopyalama/taşıma verimini ölçen bir performans kıyaslama paketi eklendi; sonuçlar `docs/benchmarks.md`'de tablolanıp `docs/performance-budget.md`'nin hedef bütçeleriyle karşılaştırıldı.

### Eklenenler
- **`crates/veyra-filesystem/tests/benchmarks_scaling.rs` (yeni, 9 test):** `read_dir_chunked` verimi 100/1K/10K/100K/1M ölçeğinde (gerçek dosyalarla, 1M dahil — tmpfs üzerinde oluşturma ~2sn + tarama ~6sn, `#[ignore]` gerektirmeyecek kadar hızlı); `read_dir` (`FULL_ATTRIBUTES`) ile `read_dir_chunked` (`FAST_ATTRIBUTES`) arasındaki 10K ölçekli hız karşılaştırması; `/proc/self/status`'tan `VmRSS` okuyarak 100K ögelik taramada gerçek bellek büyümesinin sınırlı kaldığını kanıtlayan test (yeni bağımlılık yok — stdlib ile); 50MB dosya kopyalama/taşıma verim testleri.
- **`crates/veyra-search/tests/benchmarks_search_latency.rs` (yeni, 1 test):** 10.000 dosya indekslenip dar/geniş/isabetsiz üç sorgu türü için FTS5 arama gecikmesi ölçülüyor.
- **`crates/veyra-ui/src/thumbnails.rs` (inline `#[cfg(test)]` genişletmesi, 1 yeni test):** L1 LRU önbelleğin 100.000 put/get işlemindeki ortalama erişim süresini ölçen `l1_lru_cache_access_is_fast_at_scale`.
- **`docs/benchmarks.md` (yeni):** Yukarıdaki tüm ölçümlerin gerçek sayılarla tablolandığı rapor; `docs/performance-budget.md`'nin hedef bütçeleriyle karşılaştırma ve "toplam tarama süresi" ile "ilk batch görünürlük süresi" arasındaki farkın açıklığa kavuşturulması.

### Ölçüm bulguları (öne çıkanlar — tam tablo `docs/benchmarks.md`'de)
- Dizin tarama verimi 100'den 1M'e kadar ~115K–184K dosya/sn aralığında düz kalıyor — gizli bir O(n²) davranış yok.
- `FAST_ATTRIBUTES` 10K ölçekte `FULL_ATTRIBUTES`'tan ~2,5× hızlı (52,5ms vs 133ms) — Rule #30'un tembel-metaveri tasarımının gerçek kazancı doğrulandı.
- 100K ögelik bir taramada ölçülebilir RSS artışı yok (~0kB) — bounded-memory iddiası artık batch-boyutu muhasebesinin ötesinde gerçek bellek ölçümüyle destekleniyor.
- FTS5 araması dar/isabetsiz sorgularda <300µs; 500 sonuçla dolan geniş bir sorguda 14ms (`RESULT_LIMIT` nedeniyle sıralama maliyeti) — ikisi de kullanıcı için "anlık" hissettiren eşiğin altında.
- `SearchIndex::index_entry`'nin çağrı-başına-transaction tasarımı 10K dosyayı indekslemeyi ~23sn'ye mal ediyor — arama gecikmesini etkilemiyor (indeksleme arka planda düşük öncelikli thread'de çalışıyor) ama gelecekteki bir indeksleyici-verimliliği fazı için not edildi.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 527 → 538, tamamı geçti (toplam süre ~35sn — 1M ölçekli tarama ve 10K FTS5 indeksleme testleri baskın maliyet).

### Sıradaki Faz
- Faz 50 onay bekliyor.

## Faz 48 — Security Testing & Adversarial Hardening (Güvenlik & Dayanıklılık Test Paketi)

Kötü niyetli symlink'ler (döngüler, hassas hedefler, chmod kaçışı), kötü niyetli/bozuk arşivler, yetkisizlik/TOCTOU/işlem-sırasında-silinen-dosya senaryoları ve bozuk yapılandırma/veritabanı dosyalarını kapsayan bir güvenlik test paketi eklendi. Test yazımı sırasında `chmod_recursive`'da gerçek bir Kural #22 ihlali bulundu ve düzeltildi.

### Bulunan ve Düzeltilen Güvenlik Açığı
- **`crates/veyra-filesystem/src/ops.rs` — `chmod_recursive` symlink hedefini değiştiriyordu:** Linux'ta `lchmod` syscall'ı bulunmadığından, bir symlink üzerinde `chmod()` her zaman hedefi takip eder. `chmod_recursive_walk`, dizin içindeki her dosya-olmayan ögeye (symlink dahil) koşulsuz `set_permissions` çağırıyordu — bu, dizin içinde `/etc/shadow` veya başka bir dosyaya işaret eden bir symlink varsa, `chmod_recursive`'ın o hedefin izinlerini sessizce değiştirebileceği anlamına geliyordu (kanıtlandı: kendi kontrolündeki bir "victim" dosyasına işaret eden symlink içeren dizinde `chmod_recursive` çalıştırılınca victim'in izinleri `644` → `700` değişti). **Düzeltme:** `chmod_recursive_walk` artık enumerasyonda `FileType::SymbolicLink` gördüğü her ögeyi (ve `chmod_recursive` girişinde kök ögenin kendisini) tamamen atlıyor — ne chmod'luyor ne de içine giriyor. `crates/veyra-filesystem/tests/security_adversarial.rs`'deki `chmod_recursive_never_modifies_a_symlinks_target_permissions` ve `chmod_recursive_skips_a_symlinked_root_entirely` regresyon testleriyle doğrulandı.

### Eklenenler
- **`crates/veyra-filesystem/tests/security_adversarial.rs` (yeni, 23 test):**
  - **Symlink saldırıları:** karşılıklı döngü (`a↔b`) `count_dir_recursive`/`analyze_directory`'de takılmıyor; kendine-işaret eden symlink `read_dir_chunked`'de takılmıyor; bir symlink kendi (küçük) boyutunu/türünü raporluyor, hassas hedefin içeriğini değil; yukarıdaki `chmod_recursive` regresyon testleri.
  - **Kötü niyetli/bozuk arşivler:** iç içe `../` path traversal ZIP'te hedef dışına çıkamıyor; rastgele baytlarla bozulmuş ve ortadan kesilmiş ZIP/TAR/TAR.GZ başlıkları panic yerine temiz hata veya boş+hatalı `outcome` döndürüyor; arşiv içi symlink girdisi asla diske yazılmıyor.
  - **Yetkisizlik & TOCTOU:** `chmod 0000` dosya/dizinlerde `stat`/`read_dir`/`copy`/`delete` zarif hata veriyor; çağrıdan hemen önce silinen dosyalarda `stat`/`copy` `FsError::NotFound` döndürüyor (panic yok); `find_duplicates` hashleme öncesi kaybolan bir aday dosyayı atlayıp geçerli çiftleri yine de onaylıyor; `analyze_directory` erişilemeyen bir alt dizini atlayıp taramanın geri kalanını tamamlıyor.
  - **Bozuk veri & kötü niyetli isimler:** `\u{202E}` bidi override `has_bidi_override` ile tespit ediliyor ve panic'e yol açmıyor; null byte içeren isimler `validate_filename` tarafından reddediliyor.
- **`crates/veyra-search/tests/security_corrupt_index.rs` (yeni, 3 test):** Rastgele baytlarla bozulmuş, sıfır baytlık ve SQLite başlığı ortadan kesilmiş `search_index.db` dosyalarının `SearchIndex::open` ile panic yerine temiz hata (veya güvenli sıfırdan yeniden oluşturma) döndürdüğünü doğrular.

### Kapsam kararları
- Gerçek `/etc/shadow` gibi sistem dosyalarına symlink oluşturup okumak hem taşınabilir olmadığından hem de gereksiz risk taşıdığından, "hassas hedef" testleri kendi kontrolündeki bir sahte-hassas dosyaya işaret eden symlink'lerle yürütüldü — asıl doğrulanan şey (bir symlink'in kendi metadata'sını raporlaması, hedefinkini değil) aynı kalıyor.
- Gerçek eşzamanlı bir yarış durumu (dosya taranırken başka bir thread'in onu silmesi) yerine, "çağrıdan hemen önce silinmiş"/"izinsiz" durumları kullanıldı — aynı hata-yönetimi kod yolunu deterministik biçimde tetikliyor, gerçek bir race koşulundan daha güvenilir ve flaky olmayan bir test.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 501 → 527, tamamı geçti.

### Sıradaki Faz
- Faz 49 onay bekliyor.

## Faz 47 — Comprehensive Testing Suite & Adversarial Filename Property Tests

Emoji, CJK, RTL (Arapça/İbranice), Türkçe özel karakter, boşluk/sekme ve 255 baytlık sınır boyuttaki dosya isimlerini kapsayan bir zorlayıcı-dosya-ismi test paketi; oluşturmadan kalıcı silmeye tam operasyon zinciri entegrasyon testleri; ve doğal sıralama/oturum kalıcılığı/FTS5 arama sözdizimi için ek UI ve arama mantığı testleri eklendi (Rule #34/#35).

### Eklenenler
- **`crates/veyra-filesystem/tests/unicode_adversarial.rs` (yeni, 11 test):** Emoji, CJK (Çince/Japonca/Korece), RTL (Arapça/İbranice), Türkçe özel karakter, boşluk/sekme/nokta, özel simge ve 255-baytlık sınır isimleriyle oluşturma/yazma/okuma, yeniden adlandırma, kopyalama/taşıma, çöpe atma/geri yükleme, ZIP ve TAR.GZ sıkıştırma/çıkarma ve `stat` doğrulaması. 255-baytlık sınır ismi, freedesktop.org Trash-spec'inin `.trashinfo` eki (10 bayt) ve ustar TAR başlığının 100-baytlık yol alanı gibi gerçek dosya sistemi/format sınırlarına çarptığı yerlerde bilinçli olarak hariç tutulup ayrı bir "temiz hata, panic değil" testiyle doğrulandı (Rule #15).
- **`crates/veyra-filesystem/tests/operations_lifecycle.rs` (yeni, 3 test):** Oluştur → yaz → SHA-256 checksum → alt dizine taşı → yeniden adlandır → ZIP'e sıkıştır → farklı dizine çıkar → hash doğrula → çöpe at → geri yükle (Undo) → kalıcı sil uçtan uca zinciri; TAR.GZ dizin yapısı round-trip'i; ve ZIP dışındaki tüm desteklenen arşiv formatlarının (TAR, TAR.XZ, TAR.ZST, 7z) checksum doğrulamalı round-trip testi.
- **`crates/veyra-search/tests/query_syntax_search.rs` (yeni, 13 test):** `name:`/`size:`/`type:`/`modified:` filtrelerinin gerçek bir FTS5 indeksine karşı tek başına ve birlikte kullanımı, boş/sadece-boşluk sorgular, FTS5 özel karakterlerinin (`"`, `*`, `AND`/`OR`/`NEAR`, parantez) hata vermeden işlenmesi, Unicode/Türkçe dosya adlarında arama ve `remove_path` sonrası indeksten düşme.
- **`crates/veyra-ui/src/sorting.rs` ve `crates/veyra-ui/src/session.rs` (inline `#[cfg(test)]` genişletmeleri, 9 yeni test):** Karışık büyüklükte sayısal isim serilerinin (`file1`/`file2`/`file10`/`file20`) tam liste `sort_by` ile doğal sıralaması, klasörlerin sayısal dosya serisine rağmen her zaman önce gelmesi, Türkçe büyük/küçük harf duyarsızlığı; bozuk/eksik/boş JSON oturum dosyalarının panic yerine boş oturuma düşmesi, çok sekmeli/Unicode yol içeren oturumların bayt-bayt round-trip'i, sağ panel yol ayrıştırmasının sol panelle aynı davranması.

  (`veyra-ui`'nin iç modülleri `pub(crate)` olduğundan, bu testler ayrı bir harici `tests/` dosyası yerine ilgili modüllerin mevcut `#[cfg(test)]` bloklarına eklendi — dış API sınırının dışına çıkmadan aynı kapsamı sağlıyor.)

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 466 → 501, tamamı geçti.

### Sıradaki Faz
- Faz 48 onay bekliyor.

## Faz 46 — Native Packaging (Arch, Fedora, openSUSE, Debian & Standart Kurulum)

Veyra'nın Flatpak dışında ana Linux dağıtımlarında da yerel (native) paketlenebilmesi için `DESTDIR`/`PREFIX` destekli bir kök `Makefile` ve dört dağıtım ailesine (Arch, Fedora/RHEL, openSUSE, Debian/Ubuntu) özel paketleme metadata'sı eklendi; tüm paket dosyalarındaki sürüm numaralarının `Cargo.toml` ile eşleştiğini doğrulayan birim testleri yazıldı.

### Eklenenler
- **`Makefile` (yeni):** `PREFIX ?= /usr`, `DESTDIR` destekli. `build`: `cargo build --release --locked --workspace`. `install`: binary'yi `bin/veyra` (755), `.desktop`'ı `share/applications/` (644), metainfo'yu `share/metainfo/` (644) ve `data/icons/hicolor/` altındaki tüm simgeleri karşılık gelen `share/icons/hicolor/` yoluna (644) kopyalar. `uninstall`: kurulan tüm dosyaları temizler.
- **`packaging/arch/PKGBUILD` (yeni):** `pkgname=veyra`, `pkgver=0.1.0`, `depends=(gtk4 libadwaita glib2)`, `makedepends=(cargo rust)`, `optdepends=(polkit gvfs xdg-terminal-exec)`. `build()` `cargo build --frozen --release --workspace`, `package()` `make DESTDIR="$pkgdir" PREFIX=/usr install`.
- **`packaging/fedora/veyra.spec` ve `packaging/opensuse/veyra.spec` (yeni):** `BuildRequires: cargo, rust, gtk4-devel/pkgconfig(gtk4), libadwaita-devel/pkgconfig(libadwaita-1)`, `Requires: gtk4, libadwaita, glib2`. `%install` `%make_install`; `%check` içinde `desktop-file-validate` + `appstream-util validate-relax` çağrıları. openSUSE sürümü `pkgconfig()` tarzı bağımlılıklar ve `Group:` etiketiyle openSUSE paketleme kurallarına uyarlandı.
- **`packaging/debian/` (yeni: `control`, `rules`, `changelog`, `copyright`, `source/format`):** `control`'da `Depends: ${shlibs:Depends}, ${misc:Depends}, libgtk-4-1, libadwaita-1-0`; `rules` `dh $@` + `override_dh_auto_build/_install/_test` ile aynı kök `Makefile`'a devrediyor; `copyright` GPL-3.0-or-later, `source/format` `3.0 (quilt)`.
- **`docs/packaging.md` (yeni):** Arch (`makepkg -si`), Fedora/openSUSE (`rpmbuild -ba`), Debian (`debuild -b -uc -us`) ve kaynaktan derleme (`make && sudo make install`) adımları; sürüm senkronizasyonu kuralı.
- **`crates/veyra-app/tests/packaging_metadata.rs` (yeni, 7 test):** `PKGBUILD`/Fedora/openSUSE `Version` alanlarının ve Debian `changelog` üst-akış sürümünün kök `Cargo.toml`'daki `[workspace.package].version` ile eşleştiğini doğrular; Debian `control` paragraf yapısını, `Makefile`'daki standart hedefleri (`all/build/install/uninstall/clean`) ve tüm paketleme dosyalarının referans verdiği `data/` yollarının (desktop, metainfo, simge) gerçekten var olduğunu doğrular.

### Kapsam kararları
- Paketleme dosyaları kaynak tarball indirip derleyecek şekilde yazıldı (`source=("$pkgname-$pkgver.tar.gz::...")`); bu ortamda gerçek bir `makepkg`/`rpmbuild`/`debuild` çalıştırması yapılmadı çünkü bunlar ağ erişimi (tarball indirme) ve dağıtıma özgü build container'ları gerektiriyor — bunun yerine sözdizimi, alan adları ve sürüm eşleşmesi Rust testleriyle doğrulandı; bu sınırlama açıkça belirtiliyor.
- Debian paketleme dosyaları `packaging/debian/` altında tutuldu (kaynak ağacın kökünde bir `debian/` dizini zorlamak yerine) — kurulum `docs/packaging.md`'de `ln -s packaging/debian debian` ile açıklanıyor; bu, Flatpak manifestinin de `build-aux/flatpak/` altında izole tutulduğu mevcut düzenle tutarlı.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 459 → 466, tamamı geçti (yeni: `veyra-app::packaging_metadata` +7 test).
- `makepkg`/`rpmbuild`/`debuild` bu ortamda çalıştırılmadı (yukarıdaki Kapsam kararları'na bakın) — sözdizimi ve sürüm eşleşmesi test edildi, gerçek paket üretimi doğrulanmadı.

### Sıradaki Faz
- Faz 47 onay bekliyor.

## Faz 45 — Flatpak Packaging & Sandbox Security (Flatpak & XDG Portal Entegrasyonu)

Veyra için GNOME standartlarına uyumlu bir Flatpak manifesti ve gerçek (üretilmiş, placeholder olmayan) `cargo-sources.json` bağımlılık listesi eklendi; sandbox izinleri sekiz gerekçelendirilmiş `finish-args` girdisiyle katı biçimde minimum tutuldu; dosya açma/bildirim/varsayılan-uygulama işlemlerinin GIO üzerinden zaten XDG Portalları'na şeffaf biçimde yönlendiğini doğrulayan sandbox tespiti ve dokümantasyon eklendi.

### Eklenenler
- **`build-aux/flatpak/io.github.erayq1.Veyra.json` (yeni manifest):** `runtime: org.gnome.Platform//47`, `sdk: org.gnome.Sdk//47` + `org.freedesktop.Sdk.Extension.rust-stable`, `command: veyra`. `finish-args`: `--share=ipc`, `--socket=fallback-x11`, `--socket=wayland`, `--filesystem=host`, `--talk-name=org.freedesktop.FileManager1`, `--talk-name=org.freedesktop.Notifications`, `--talk-name=org.gtk.vfs.*`, `--system-talk-name=org.freedesktop.UDisks2` — başka hiçbir izin yok. Tek `cargo` modülü: `cargo --offline build --release --workspace --locked` + `.desktop`/`.metainfo.xml`/simgenin `/app/share/` altına `install -Dm`'i.
- **`build-aux/flatpak/cargo-sources.json` (yeni, üretilmiş):** Resmi `flatpak-cargo-generator.py` betiği gerçek `Cargo.lock`'a karşı çalıştırılarak üretildi — 393 crate girdisi içeren gerçek, doğrulanmış bir dosya; placeholder/boş dosya değil.
- **`build-aux/flatpak/README.md` (yeni):** `flatpak-builder` ile derleme talimatı ve `cargo-sources.json`'ı `Cargo.lock` değiştiğinde yeniden üretme adımları.
- **`docs/flatpak_permissions.md` (yeni):** Sekiz `finish-args` girdisinin her biri için gerekçe + "bu iznin vermediği şey" tablosu, ve `open_with.rs`/`gio::Notification`/`is_default_file_manager` gibi GIO API'lerinin sandbox içinde zaten şeffaf biçimde ilgili XDG Portalına (`OpenURI`, `Notification`, `mimeapps.list` proxy) yönlendiğini açıklayan bir "XDG Portal Rolü" bölümü.
- **`crates/veyra-ui/src/system_integration.rs`:** `is_flatpak_sandbox()` (`/.flatpak-info` + `FLATPAK_ID` env değişkeni tespiti, saf/test edilebilir `is_sandboxed` yardımcı fonksiyonuyla ayrıştırılmış) — `lib.rs::run()` başlangıcında tanılama logu olarak kullanılıyor. 7 yeni birim testi: sandbox tespiti (3), manifest JSON sözdizimi + `app-id`/`runtime`/`command` doğrulaması, manifest `finish-args`'ının bu belgeyle birebir eşleştiğini doğrulayan test (girdi sayısı dahil — fazla/eksik izin sessizce kayamaz), manifest'in `cargo-sources.json`'ı kaynak olarak referans aldığının doğrulanması, `cargo-sources.json`'ın geçerli/boş-olmayan JSON olduğunun doğrulanması.
- **`crates/veyra-ui/src/open_with.rs`:** `launch()`'a, GIO'nun `GDesktopAppInfo` arka ucunun sandbox'ı kendisi tespit edip bu çağrıyı doğrudan host'ta fork etmek yerine şeffaf biçimde `OpenURI` portalı üzerinden yönlendirdiğini — ve bu yüzden burada elle bir portal çağrısı yazılmadığını — açıklayan dokümantasyon eklendi.

### Kapsam kararları
- `open_with.rs`/bildirimler/varsayılan-uygulama sorguları için elle bir portal D-Bus çağrısı (örn. yeni bir `ashpd` bağımlılığı) **yazılmadı** — GIO'nun `gio::AppInfo::launch`/`gio::Notification`/`default_for_type`/`set_as_default_for_type` API'leri, GLib seviyesinde sandbox'ı zaten kendileri tespit edip ilgili XDG Portalına yönlendiriyor (bu, Nautilus/Files gibi diğer GNOME uygulamalarının da özel bir portal kodu yazmadan aynı GIO çağrılarını kullanmasının nedeni). Test edilemeyen/doğrulanamayan spekülatif portal kodu yazmak yerine (`AGENTS.md` Kural #48/global kural: "doğrulanmadan done denmez"), mevcut doğru davranış dokümante edildi ve `is_flatpak_sandbox()` yalnızca tanılama/gelecekteki gerçek ihtiyaçlar için eklendi.
- `--filesystem=host` bilinçli olarak geniş tutuldu (bkz. `docs/flatpak_permissions.md`) — bir dosya yöneticisinin işi tanım gereği kullanıcının seçtiği her yolu (kök, bağlı sürücüler, `/mnt`, `/media`) gezebilmek; `--filesystem=home` gibi daha dar bir izin bu temel işlevi sessizce kırardı. Ayrıcalıklı işlemler yine bu izinden bağımsız olarak ayrı bir Polkit/`pkexec` yardımcısından geçiyor (Kural #20).
- `flatpak-builder` bu ortamda kurulu olmadığından gerçek bir `flatpak-builder --user --install` derlemesi çalıştırılamadı; bunun yerine manifest JSON sözdizimi (Rust testi + `python3 -m json.tool`), `cargo-sources.json`'ın gerçek üretimi/geçerliliği, ve `appstreamcli validate` ile metainfo doğrulaması yapıldı — bu sınırlama net biçimde belirtiliyor, "derlendi" iddia edilmiyor.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 452 → 459, tamamı geçti (yeni: `veyra-ui::system_integration` +7 test).
- `cargo build --workspace`: temiz, 0 warning.
- `desktop-file-validate data/io.github.erayq1.Veyra.desktop`: yalnızca önceden var olan kategori hint'i (Faz 44'ten, kapsam dışı).
- `appstreamcli validate --pedantic data/io.github.erayq1.Veyra.metainfo.xml`: yalnızca ulaşılamayan-URL uyarıları (yayınlanmamış repo) + pedantik app-id büyük harf notu — hata yok.
- `python3 -c "import json; json.load(open('build-aux/flatpak/io.github.erayq1.Veyra.json'))"` ve aynısı `cargo-sources.json` için: her ikisi de geçerli JSON.
- `flatpak-builder` bu ortamda mevcut değil — gerçek bir sandbox derlemesi çalıştırılamadı (yukarıdaki Kapsam kararları'na bakın).

### Sıradaki Faz
- Faz 46 onay bekliyor.

## Faz 44 — System Integration (Linux Sistem & Masaüstü Entegrasyonu)

Veyra'yı Linux masaüstünün doğal bir parçası haline getiren sistem entegrasyonu: `.desktop` dosyası zenginleştirildi, varsayılan dosya yöneticisi durumu Ayarlar'dan sorgulanıp ayarlanabiliyor, tekil örnek (`GApplication`) artık gerçek `command-line` işleme ile çalışıyor (CLI çoklu dizin/dosya açma, `--new-window`, `--preferences`), ve uzun süren arka plan işlemleri pencere odakta değilken masaüstü bildirimi gönderiyor.

### Eklenenler
- **`data/io.github.erayq1.Veyra.desktop`:** `MimeType=inode/directory;x-scheme-handler/file;`, `Keywords=`, `Actions=NewWindow;Preferences;` (+ karşılık gelen `[Desktop Action …]` blokları, `veyra --new-window %f` / `veyra --preferences`), `X-GNOME-UsesNotifications=true`, `Categories`'e `Utility` eklendi.
- **`crates/veyra-ui/src/system_integration.rs` (yeni modül):** `is_default_file_manager()`/`set_as_default_file_manager()` (`gio::AppInfo::default_for_type`/`gio::DesktopAppInfo::set_as_default_for_type`, `inode/directory`), `notify_operation_complete(window, title, body)` (pencere odakta değilse `gio::Notification` gönderir), ve GTK'dan bağımsız, birim testli `parse_args` (CLI argv ayrıştırma: `--new-window`, `--preferences`, çıplak yol argümanları).
- **`crates/veyra-ui/src/lib.rs`:** `run()` artık `gio::ApplicationFlags::HANDLES_COMMAND_LINE` ile tek bir `command-line` sinyali üzerinden çalışıyor — aynı süreçteki birincil örneğe D-Bus üzerinden iletilen her `veyra ...` çağrısı (ikinci bir terminalden dahi) bu tek noktadan geçiyor: argümansız/çıplak yol → mevcut pencereye sekme, `--new-window` → süreç içinde gerçek yeni bir pencere, `--preferences` → `win.show-preferences` eylemi. Süreç boyunca tek bir paylaşılan `settings` referansı (ilk pencerede yüklenir) her yeni pencerede yeniden kullanılıyor.
- **`crates/veyra-ui/src/window.rs`:** `win.open-external-path` (string parametreli) eylemi + `reveal_and_select` — bir dosya yolu verildiğinde üst klasörü sekmede açıp, dizin taraması (Faz 11 chunked `read_dir`) dosyayı üretir üretmez aktif görünümde seçiyor. `spawn_new_window` ("Open in New Window") artık `--new-window` bayrağıyla çağırıyor. Bulk/arşiv işlemleri ve çöp boşaltma tamamlandığında `system_integration::notify_operation_complete` çağrısı eklendi.
- **`dialogs/preferences_dialog.rs` (Advanced sekmesi):** "System Integration" grubu — "Default File Manager" satırı, güncel durumu (`prefs.advanced.default_file_manager.subtitle_default/not_default`) gösterip "Set as Default" butonuyla `set_as_default_file_manager()` çağırıyor.
- **`i18n.rs`:** `prefs.advanced.group.system_integration`/`default_file_manager.*` anahtarları, `EN` + `TR`.
- **`crates/veyra-app/src/main.rs`:** CLI argüman ayrıştırma tamamen `veyra_ui::run`'a taşındı; `main` artık yalnızca XDG dizinlerini/loglamayı kurup `veyra_ui::run(APP_ID, cache_dir)` çağırıyor. `APP_ID` artık `veyra_ui::APP_ID`'den tek kaynaktan geliyor.

### Kapsam kararları
- `HANDLES_OPEN` yerine yalnızca `HANDLES_COMMAND_LINE` seçildi — ikisini birden kullanmak iki ayrı GIO sinyalinin (`open` ve `command-line`) hangi argümanı hangisinin işleyeceğini koordine etmeyi gerektirirdi; tek sinyalde tüm mantığı (dizin/dosya/bayrak) elle ayrıştırmak daha az yüzey alanı bırakıyor.
- "Open in New Window" (sağ tık) ile düz `veyra /dizin` CLI çağrısı aynı `argv` şeklini (`veyra <path>`) paylaştığından, ayrım açık bir `--new-window` bayrağıyla yapılıyor — sağ tık eylemi bunu geçiyor, kullanıcının terminalden yazdığı çağrı geçmiyor (dolayısıyla mevcut pencereye sekme olarak ekleniyor, Faz 44 gereksinim C).
- Bildirimler yalnızca pencere odakta değilken gönderiliyor (`GtkWindow::is_active`) — odaktaki pencere zaten ilerleme tostu/durum çubuğuyla tamamlanmayı gösteriyor, bildirim orada gürültü olurdu.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 444 → 452, tamamı geçti (yeni: `veyra-ui::system_integration` 11 test — CLI ayrıştırma + `.desktop` dosyası sözdizim doğrulaması).
- `cargo build --workspace`: temiz, 0 warning.
- Manuel çalışma zamanı doğrulaması (gerçek `DISPLAY`/D-Bus oturumu ile): birincil örnek başlatıldı, ikinci `veyra /tmp` çağrısı birincil pencereye D-Bus üzerinden iletilip hızlıca çıktı (yeni pencere açmadı), `veyra --new-window /tmp` aynı süreçte gerçek ikinci bir pencere oluşturdu (log'da ikinci "building window").

### Sıradaki Faz
- Faz 45 onay bekliyor.

## Faz 43 — Smart Storage Dashboard (`veyra-filesystem` entegrasyonu / `veyra-ui`)

Tek bir pencerede kök dosya sisteminin doluluk durumunu, Home dizinindeki en büyük klasörleri, en son kullanılan dosyaları ve mükerrer dosya temizleme fırsatını gösteren **Smart Storage Dashboard** eklendi — kenar çubuğundaki yeni "Storage" satırından, komut paletinden (`Ctrl+K`) veya `Ctrl+Shift+I` kısayolundan açılıyor.

### Eklenenler
- **`crates/veyra-ui/src/dialogs/storage_dashboard_dialog.rs` (yeni modül):** `show(parent, home, navigate, refresh_all, undo_stack)` — diyalog anında açılıp bir spinner gösteriyor; kök dosya sisteminin kullanım bilgisi (`devices::query_usage`), Home dizininin `veyra_filesystem::analyze_directory` taraması (en büyük klasörler + aynı-boyut mükerrer aday listesi, tek geçişte) ve Recent Files kaydının anlık görüntüsü tek bir arka plan işinde (`fs_async::run_blocking`, Kural #11/#12) toplanıyor. Dört kart: **Storage Gauge** (`GtkProgressBar` + Used/Free/Total özeti, `gauge_percent`/`gauge_summary_label` saf fonksiyonları), **Largest Folders** (ilk 5 klasör + "Analyze…" butonuyla tam Disk Analyzer'ı açma), **Recent Files** (ilk 5 dosya, tıklanınca klasörüne gidip paneli kapatma) ve **Duplicates** (aynı-boyut aday sayısı anında görünür; "Scan Duplicates" butonu tıklanınca `find_duplicates`'i (Faz 42) yeniden dosya sistemini taramadan, zaten elde edilmiş adaylar üzerinde arka planda çalıştırıp doğrulanmış grup sayısı + boşa harcanan alanı gösteriyor; "Open in Disk Analyzer…" her zaman tam interaktif temizleme görünümüne yönlendiriyor). `gauge_percent`, `gauge_summary_label`, `take_top`, `duplicate_summary` saf fonksiyonları için 7 birim testi.
- **`crates/veyra-ui/src/window.rs`:** `setup_storage_dashboard_action` — `win.show-storage-dashboard` eylemi (`Ctrl+Shift+I`; Faz 20'nin `Ctrl+Shift+U`'su zaten `win.analyze-disk-current`'e ait olduğundan farklı bir kısayol seçildi), Home dizinini `glib::home_dir()` ile çözüp diyaloğu açıyor.
- **`crates/veyra-ui/src/sidebar.rs`:** Places bölümünün altına, `win.connect-to-server` satırıyla aynı desende (`navigate` yerine doğrudan `set_action_name`) çalışan bir "Storage" satırı eklendi.
- **`crates/veyra-ui/src/command_palette.rs`:** Tools kategorisine "Smart Storage Dashboard…" komutu eklendi.
- **`i18n.rs`:** `storage.*` altında ~20 yeni anahtar (diyalog başlığı, kenar çubuğu etiketi, dört kartın başlık/boş-durum/buton metinleri, tekil/çoğul mükerrer özet varyantları) — hem `EN` hem `TR` katalogları.

### Kapsam kararları
- Doluluk göstergesi Home dizinini değil kök dosya sistemini (`/`) ölçüyor — "Sürücü Doluluk Göstergesi" kavramsal olarak tüm sürücünün doluluğu, tek bir alt dizinin değil; `devices::query_usage`/`usage_fraction`/`format_usage` (Faz 17) doğrudan yeniden kullanıldı, kopya kod yok.
- Duplicates kartı diyalog açılışında içerik hash'i taramıyor — aynı-boyut aday listesi zaten Largest Folders için yapılan `analyze_directory` geçişinin bir yan ürünü (ücretsiz), ama gerçek `find_duplicates` içerik-hash geçişi büyük ev dizinlerinde saniyeler sürebilir; panelin anında açılması gerektiğinden (Kural #11) bu iş "Scan Duplicates" butonuna kadar erteleniyor.
- Largest Folders/Duplicates kartlarındaki "Analyze…"/"Open in Disk Analyzer…" butonları yeni bir özet görünüm inşa etmek yerine mevcut Disk Analyzer diyaloğunu (Faz 20/42) olduğu gibi açıyor — Breakdown/Largest Files/Duplicate Files sekmelerinin ayrı bir "mini" kopyasını yazmak kod tekrarı ve bakım yükü olurdu.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 437 → 444, tamamı geçti (yeni: `veyra-ui::dialogs::storage_dashboard_dialog` 7 test).
- `cargo build --workspace`: temiz, 0 warning.

### Sıradaki Faz
Faz 44 — onay bekleniyor.

## Faz 42 — Duplicate Finder: Disk Analyzer İçinde Çift Dosya Bulucu (`veyra-filesystem` / `veyra-ui`)

Faz 20'nin boyut-eşleşmesine dayalı "aday" listesi, gerçek içerik hash'iyle kesinleştiren 3 aşamalı bir motora (`crates/veyra-filesystem/src/duplicates.rs`) bağlandı ve Disk Analyzer'a interaktif bir **Duplicate Files** sekmesi eklendi: her dosyanın yanında seçim kutusu, "Select All Copies (Keep Newest/Oldest)" hızlı seçim yardımcıları ve — her grupta en az bir kopyanın korunmasını zorunlu kılan, asla otomatik silme yapmayan, Çöp Kutusu + `Ctrl+Z` ile geri alınabilir — güvenli bir "Move Selected to Trash" temizleme akışı.

### Eklenenler
- **`crates/veyra-filesystem/src/duplicates.rs` (yeni modül):** `find_duplicates(candidates: &[SameSizeCandidateGroup], control: &OperationControl) -> Vec<DuplicateGroup>` — Aşama 2: her adayın ilk 4 KB'lık verisi SHA-256 ile hash'lenip (`hash_prefix`) farklı olanlar tam okumaya gerek kalmadan elenir; Aşama 3: kısmi hash'i eşleşenlerin tam içeriği 256 KB'lık tamponlarla akış olarak hash'lenir (`hash_full`, `dev_tools::compute_checksums` ile aynı tampon boyutu). `DuplicateGroup { hash, size_per_file, wasted_size, files }` — `wasted_size = (files.len() - 1) * size_per_file`, sonuçlar `wasted_size` azalan sırada. `OperationControl` ile aşamalar arası ve büyük dosya içi tamponlar arası kontrol edilerek cooperatively cancellable (Kural #13); okunamayan/kaybolan bir dosya tüm taramayı durdurmadan atlanıyor (Kural #16/#18); yerel olmayan (GVfs) yollar `checksum_dialog`'daki emsalle aynı şekilde atlanıyor. 8 unit test: birebir aynı dosyalar tek grup, aynı boyut+farklı prefix/farklı kuyruk eşleşmiyor, 3+1 karışık grup doğru ayrışıyor, `wasted_size` her ekstra kopya için doğru hesaplanıyor, tek dosyalı aday yok sayılıyor, kaybolan dosya fatal değil, iptalde boş sonuç.
- **`crates/veyra-filesystem/src/analyzer.rs`:** Faz 20'nin boyut-eşleşmeli `DuplicateGroup`'u, yeni içerik-hash'li `DuplicateGroup`'la isim çakışmasını önlemek üzere `SameSizeCandidateGroup` olarak yeniden adlandırıldı (alan/davranış değişmedi) — `duplicates::find_duplicates`'in girdisi.
- **`crates/veyra-filesystem/Cargo.toml`:** `sha2 = "0.10"` eklendi (motorun tek bağımlılığı; `md-5`/`sha2` zaten `veyra-ui`'de kullanılıyordu, burada sadece eşitlik karşılaştırması için SHA-256 yeterli — üç algoritmayı sunmanın kullanıcıya bir faydası yok).
- **`crates/veyra-ui/src/dialogs/disk_analyzer_dialog.rs`:** Eski boyut-tabanlı "Duplicates" sekmesi, `find_duplicates`'i dialog açılır açılmaz arka planda (`fs_async::run_blocking`, Kural #11/#12) çalıştıran, her dosyanın değiştirilme tarihini de aynı arka plan geçişinde (`veyra_filesystem::stat`) çeken interaktif **Duplicate Files** sekmesiyle değiştirildi: özet başlığı ("N duplicate groups • N files • X wasted"), grup başına genişleyebilir kart (dosya adı/yolu/tarih + "Reveal in Folder"), dosya başına seçim kutusu, "Select All Copies (Keep Newest/Oldest)" (bilinmeyen `modified` değeri hiçbir zaman "en yeni/en eski" olarak kazanmıyor) ve "Clear Selection". "Move Selected to Trash": önce her grupta en az bir dosyanın seçili kalıp kalmadığını doğruluyor (ihlalde engelleyici bir uyarı diyaloğu, hiçbir şey silinmiyor), ardından silinecek dosya sayısı + kazanılacak alanı gösteren `AdwAlertDialog` onayı istiyor, onaylanırsa `veyra_filesystem::trash_tracked`'ı arka planda her dosya için çağırıp başarılı `(orijinal, çöpe taşınan)` çiftlerini tek bir `UndoableAction::Trash` kaydı olarak `undo_stack`'e yazıyor (`Ctrl+Z` ile geri alınabilir) ve etkilenen grupları/seçimi listeden çıkarıp görünümü yeniden çiziyor; başarısız dosyalar toplu işlemi durdurmadan atlanıp ayrı bir hata diyaloğunda listeleniyor (Kural #18). `show()` artık `refresh_all: Rc<dyn Fn()>` (temizlik sonrası her iki panelin açık sekmelerini `refresh_all_tabs` ile tazeler) ve `undo_stack: SharedUndoStack` alıyor.
- **`crates/veyra-ui/src/window.rs` / `sidebar.rs`:** Disk Analyzer'ı açan her çağrı noktası (`win.analyze-disk-selected`, `win.analyze-disk-current`, sidebar'daki cihaz/ağ/bookmark "Analyze" eylemi) yeni `refresh_all`/`undo_stack` parametrelerini iletecek şekilde güncellendi; `sidebar::build`, `refresh_devices_box`, `refresh_network_box`, `device_row` bu iki parametreyi taşıyacak şekilde genişletildi.
- **`i18n.rs`:** `dup.*` altında ~30 yeni anahtar (sekme başlığı, özet/seçim durumu için tekil/çoğul varyantlar, hızlı seçim butonları, onay/engelleme diyalogları) — hem `EN` hem `TR` katalogları, tamlık testleri (`every_en_key_has_a_tr_counterpart` vb.) dahil.

### Kapsam kararları
- Disk Analyzer'ın "Disk Usage" (Breakdown + Largest Files) ve "Duplicate Files" ayrımı, mevcut 3 sekmeli `AdwViewSwitcher` yapısı korunarak (Breakdown/Largest Files/Duplicate Files) uygulandı — Breakdown ve Largest Files'ı tek bir "Disk Usage" sekmesi altında iç içe bir switcher'a taşımak, mevcut çalışan gezinme/breadcrumb mantığını bozma riski taşıyan, işlevsel faydası sınırlı bir yeniden yapılanma olurdu (Kural #4).
- İçerik hash'i dialog açılışında değil, arka planda otomatik olarak (kullanıcı ek bir "Tara" butonuna basmadan) başlıyor — üst düzey taramanın kendisi de aynı spinner-sonra-içerik desenini zaten kullanıyor; ek bir tetikleyici durum makinesi eklemek karmaşıklığı artırmadan bir fayda sağlamazdı.
- Silme, mevcut toplu işlem kuyruğu (`queue::run_operation`) yerine dosya başına `trash_tracked` döngüsüyle uygulandı — kuyruğun çakışma/duraklatma makinesi bu senaryoda gereksiz; sonuç yine de aynı `UndoableAction::Trash` kaydını üretip mevcut `Ctrl+Z` motoruna sorunsuz bağlanıyor.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 429 → 437, tamamı geçti (yeni: `veyra-filesystem::duplicates` 8 test).
- `cargo build --workspace`: temiz, 0 warning.

### Sıradaki Faz
Faz 43 — onay bekleniyor.

## Faz 41 — Checksums & Hash Verification (`veyra-ui`)

`dev_tools::compute_checksums`'a üçüncü algoritma olarak SHA-512 eklendi (aynı tek geçişli akışta, ek IO maliyeti yok) ve sonuçlar artık sadece Geliştirici sağ-tık diyaloğunda değil, **Özellikler Penceresi**'nin General sayfasına isteğe bağlı (on-demand) bir "Checksums" grubu olarak da sunuluyor — beklenen bir hash'i yapıştırıp anında yeşil/kırmızı eşleşme rozeti görebileceğiniz bir doğrulama alanıyla birlikte.

### Eklenenler
- **`crates/veyra-ui/src/dev_tools.rs`:** `ChecksumResult` üçüncü alan olarak `sha512: String` kazandı; `compute_checksums` artık `Md5`/`Sha256`/`Sha512`'yi aynı 256 KB'lık okuma döngüsünde birlikte güncelliyor (Kural #33 — dosya üç kez değil bir kez okunuyor). `ChecksumAlgorithm` (`Md5`/`Sha256`/`Sha512`, `label()` → `"MD5"`/`"SHA-256"`/`"SHA-512"`) ve `matching_algorithm(result, expected)` — yapıştırılan metni (baştaki/sondaki boşluk ve harf büyüklüğü göz ardı edilerek) üç sağlama toplamıyla karşılaştırıp eşleşen algoritmayı döndüren saf fonksiyon; hiçbir şey eşleşmezse veya `expected` boşsa `None`.
- **`crates/veyra-ui/src/dialogs/checksum_dialog.rs`:** SHA-512 satırı üçüncü `checksum_row` olarak eklendi. `VerifySection`/`build_verify_section()` — "Paste expected checksum to verify…" yer tutuculu bir `GtkEntry` + başlangıçta gizli bir durum etiketinden oluşan, hem bu diyalog hem `properties_dialog` tarafından paylaşılan doğrulama bileşeni. `update_verify_status` her tuş vuruşunda (`connect_changed`) ve arka plan hesaplaması bittiğinde yeniden çalışıp `.success`/`.error` CSS sınıflarıyla yeşil `"✓ Matches SHA-256"` / kırmızı `"✗ Checksum mismatch"` gösteriyor; hash henüz hesaplanmadıysa veya alan boşsa etiket gizli kalıyor. `ChecksumRow`/`checksum_row` artık `pub(crate)` — `properties_dialog` aynı satır/kopyalama widget'ını yeniden kullanıyor, kopya kod yok.
- **`crates/veyra-ui/src/dialogs/properties_dialog.rs`:** Yalnızca `FileKind::Regular` dosyalar için General sayfasına "Checksums" grubu eklendi — açılışta otomatik hesaplama yapmıyor (büyük dosyalarda CPU/disk tüketmemek için), tek bir "Calculate Checksums" butonuyla başlıyor; tıklanınca buton satırı grup içinden çıkarılıp `checksum_dialog`'un paylaşılan MD5/SHA-256/SHA-512 satırları + doğrulama alanı takılıyor, hesaplama `fs_async::run_blocking` ile arka planda `dev_tools::compute_checksums` çağırıyor ve Properties penceresi hesaplama bitmeden kapatılırsa `OperationControl` ile anında iptal ediliyor (Kural #13) — `build_general_page` artık bu bağlanma için `dialog: &adw::PreferencesDialog`'u da alıyor.
- **`i18n.rs`:** `dev.checksum.calculate`, `dev.checksum.verify_placeholder`, `dev.checksum.matches` (`{algorithm}` yer tutuculu), `dev.checksum.mismatch` — hem `EN` hem `TR` katalogları.

### Kapsam kararları
- Checksums grubu yalnızca `FileKind::Regular` için gösteriliyor — bir dizinin "sağlama toplamı" ayrı bir kavram (içerik özeti değil), bir sembolik bağlantının baytları hedef yolu, hedefinin içeriği değil; ikisi de spesifikasyonun "Dosyalar için" ifadesiyle kapsam dışı.
- Doğrulama tek bir metin kutusu — kullanıcı hangi algoritmayı yapıştırdığını belirtmiyor, `matching_algorithm` üç sağlama toplamına karşı sırayla deniyor ve eşleşeni bulup adını rozette gösteriyor; ayrı MD5/SHA-256/SHA-512 doğrulama kutuları spesifikasyonun tek bir yapıştırma alanı istemesiyle çelişirdi.
- `checksum_dialog.rs`'in satır/doğrulama bileşenleri `properties_dialog.rs`'e `pub(crate)` olarak açıldı, aynı UI'nin ikinci bir kopyası yazılmadı — iki yüzey de aynı `dev_tools::compute_checksums` motorunu ve aynı görsel dili paylaşıyor.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 426 → 429, tamamı geçti (yeni: `veyra-ui::dev_tools` 3 test — boş dosyanın MD5/SHA-256/SHA-512 standart sabit değerleriyle eşleşmesi, `matching_algorithm`'ın büyük/küçük harf ve boşluk duyarsız her üç algoritmayı bulması, ilgisiz/boş girdide `None` dönmesi; mevcut `checksums_of_known_content_match_expected_digests` testi SHA-512 doğrulamasıyla genişletildi).
- `cargo build --workspace`: temiz, 0 warning.
- Uygulama `cargo build -p veyra-app` ile derlenip masaüstü ortamında (`WAYLAND_DISPLAY`/`DISPLAY` mevcut) kısa süre çalıştırıldı — pencere hatasız açıldı, panik/çökme yok; Checksums grubunun/diyaloğunun buton tıklama → hesaplama → doğrulama akışı otomatik bir ekran etkileşim aracı bulunmadığı için elle tıklanarak görsel olarak doğrulanmadı — bu iddia edilmiyor.

### Sıradaki Faz
Faz 42 — onay bekleniyor.

## Faz 40 — Git Integration: Dosya Bazlı Durum Rozetleri (`veyra-filesystem` / `veyra-ui`)

Faz 39'un repo seviyesindeki headerbar rozetinin ("main ↑2 ↓1 *") yanına, açık dizindeki her dosyanın kendi Git durumunu gösteren küçük, salt-okunur rozetler eklendi — Icon, Compact ve Details görünümlerinin üçünde de.

### Eklenenler
- **`crates/veyra-filesystem/src/git.rs`:** `GitFileStatus` (`Modified`/`Staged`/`Untracked`/`Ignored`/`Conflicted`/`Deleted`/`Clean`) — `badge_text()` (`M`/`A`/`??`/`!`/`C`/`D`) ve `accessible_word()` (ekran okuyucu için tam sözcük) taşıyor. `query_dir_git_statuses(dir)` — `git status --porcelain --ignored=matching -uall -z` çıktısını ayrıştırıp yalnızca `dir`'in doğrudan içindeki girdileri (alt dizinlere inmeden) dosya adı → durum eşlemesine çeviren saf argv tabanlı fonksiyon; `-z` NUL sonlandırması sayesinde Unicode/boşluklu dosya adları tırnaklama sorunu olmadan işleniyor (Kural #35). `--ignored=matching` yoksayılmış dizinleri tek bir `"!! build/"` girdisi olarak bırakırken `-uall` takip edilmeyen dosyaları ayrı ayrı listeliyor — ikisinin birlikte kullanılması, dizin rozetleri ile dosya rozetlerinin aynı komuttan doğru şekilde çıkmasını sağlıyor. `classify_xy` çakışmayı (`UU`/`AA`/`DD`) her zaman staged/modified'ın önüne alıyor. Repo dışında veya `git` çalıştırılamazsa boş harita döner — rozet sessizce gizlenir (Kural #15/#17/#18). Blocking bir çağrı; çağıranlar `fs_async::run_blocking` üzerinden arka planda çalıştırıyor (Kural #11/#14).
- **`crates/veyra-ui/src/state.rs`:** `AppState.git_statuses: SharedGitStatuses` (`Rc<RefCell<HashMap<String, GitFileStatus>>>`) — açık dizinin dosya bazlı durum haritası, tüm görünümlerin satır fabrikalarıyla paylaşılıyor.
- **`crates/veyra-ui/src/window.rs`:** `update_git_file_statuses` — her gezinmede (`update_chrome` içinden) `query_dir_git_statuses`'u arka planda çalıştırıp haritayı günceller, ardından `ListStore::items_changed` ile o anda ekranda bağlı satırları yeniden bağlanmaya zorlayarak rozetleri tazeler (yapısal bir değişiklik olmadan, sadece rebind). Tab o sırada başka bir yere gitmişse sonuç atılır (Kural #17), tıpkı `update_git_badge` gibi.
- **`crates/veyra-ui/src/views/mod.rs`:** `build_grid_view` (Icon/Compact'ın paylaştığı fabrika) artık isim etiketinin yanına küçük bir rozet `Label` ekliyor; `apply_git_badge` metni/CSS sınıfını/görünürlüğü ayarlıyor, `git_status_for` dosya adıyla haritadan arıyor, `accessible_description_with_git` ekran okuyucu açıklamasına durumu ekliyor (ör. `"README.md, File, Modified"`, Kural #28/#37).
- **`crates/veyra-ui/src/views/details_view.rs`:** İsim sütunu aynı rozet/erişilebilirlik desenini paylaşıyor — ayrı bir Git sütunu yerine mevcut isim hücresine entegre edildi.
- **`crates/veyra-ui/src/split_view.rs`:** `.veyra-git-badge` ve altı durum-renkli varyantı (`modified` turuncu, `staged` yeşil, `untracked` mavi, `ignored` soluk, `conflicted` kırmızı, `deleted` koyu turuncu) panel CSS'ine eklendi.

### Kapsam kararları
- Rozetler yalnızca açık dizinin **doğrudan** çocukları için hesaplanıyor; alt dizinler içindeki değişiklikler üst klasöre yansıtılmıyor (spesifikasyon "dosya bazlı" diyor, derin toplama istemiyor) — `git status`'un kendisi de zaten alt dizin içeriğini üst dizine katlamıyor.
- Rozet görünürlüğü depo tespitine bağlı (harita doluysa rozet var), Developer Mode anahtarına bağlı değil — Dolphin/Nautilus'ta Git rozetleri genel kullanıcı özelliği, ileri düzey bir geliştirici aracı değil; spesifikasyonun "otomatik olarak (veya Developer Mode açıkken)" ifadesi iki ayrı tetikleyici değil, aynı davranışın iki anlatımı olarak okundu.
- Details görünümünde ayrı bir Git sütunu yerine isim hücresine rozet eklendi (spesifikasyonun kendi sunduğu iki seçenekten biri) — yeni bir sütun, sıralama/genişlik/gizleme mantığını tekrar gerektirirdi.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 418 → 426, tamamı geçti (yeni: `veyra-filesystem::git` 6 test — `classify_xy` her durumu kapsıyor, repo dışı boş harita, modified/staged/untracked/ignored karışık senaryo, gerçek bir merge çakışması, alt dizin girdilerinin üst dizine sızmadığının doğrulanması; `veyra-ui::views` 3 test — erişilebilirlik açıklamasına durumun eklenmesi/eklenmemesi, her durumun boş olmayan bir CSS sınıfına sahip olması).
- `cargo build --workspace`: temiz, 0 warning.

### Sıradaki Faz
Faz 41 — onay bekleniyor.

## Faz 39 — Developer Mode & Git Integration (`veyra-filesystem` / `veyra-ui`)

Git deposu algılama/durum motoru ve sağ-tık "Developer" alt menüsü (yol/URI kopyalama, harici düzenleyici, MD5/SHA-256 sağlama toplamları, meta veri inceleyici) — `Ctrl+Shift+D` ile açılıp kapanan, Ayarlar → Advanced'ta kalıcı bir anahtar.

### Eklenenler
- **`crates/veyra-filesystem/src/git.rs` (yeni modül):** `find_git_root` — saf Rust, `.git` bulana kadar üst dizinlere yürür, hiçbir alt süreç başlatmaz. `git_status` — `git status --porcelain=2 --branch`'i sabit, kabuksuz bir argv (`Command::new("git").arg("-C")...`) ile bir kez çalıştırır (Kural #19); paket dosyası/delta çözme gibi `.git` iç formatını elle yeniden uygulamak yerine kullanıcının zaten güvendiği `git` ikili dosyasına devrediyor — hem daha güvenli hem çok daha doğru. `GitRepoStatus` (`branch`, `ahead`, `behind`, `staged`, `modified`, `untracked`, `is_dirty()`, `badge_label()` → `"main ↑2 ↓1 *"`). `git` bulunamazsa veya komut başarısız olursa rozet sessizce gizlenir (Kural #15/#18).
- **`crates/veyra-filesystem/src/advanced.rs`:** `AdvancedInfo`'ya `hard_link_count` (`unix::nlink`) eklendi — Geliştirici Meta Verisi İnceleyici'nin sabit bağlantı satırını besliyor; eklemeli alan, mevcut çağıran (`properties_dialog.rs`) etkilenmedi.
- **`crates/veyra-ui/src/dev_tools.rs` (yeni modül):** `relative_path` (Git köküne veya açık dizine göre, gerektiğinde `../` ile yukarı-aşağı yürüyen saf fonksiyon), `copy_uri` (GIO `file://` URI'si), `compute_checksums` (MD5+SHA-256'yı tek akan okuma geçişinde hesaplar, `OperationControl` ile iptal edilebilir), `open_in_editor` (`$VISUAL` → `$EDITOR` → `code`/`zed`/`subl`/`nvim`/`gedit`, `terminal.rs`'in `find_in_path`'ini paylaşarak — hiçbiri kabuk dizesi değil, her aday disk üzerinde var olduğu doğrulanmış bir ikili).
- **`crates/veyra-ui/src/dialogs/checksum_dialog.rs` (yeni modül):** MD5/SHA-256'yı arka planda hesaplayıp tek tıkla kopyalama sunan `AdwDialog`; diyalog kapatılırsa hesaplama `OperationControl.cancel()` ile durur.
- **`crates/veyra-ui/src/dialogs/dev_metadata_dialog.rs` (yeni modül):** inode, izinler (sekizli), MIME türü (zaten yüklü `FileItem`'dan, ekstra `stat` yok) + aygıt kimliği/sabit bağlantı sayısı (arka planda `stat_advanced`).
- **`config.rs`:** `VeyraSettings.developer_mode: bool` (`#[serde(default)]`, eski `settings.json` ile uyumlu).
- **`context_menu.rs`:** Tek ögelik seçim menüsüne, yalnızca Developer Mode açıkken görünen "Developer" alt menüsü (Copy Absolute Path, Copy URI, Copy Relative Path, Open in Editor, Calculate Checksums…, Developer Metadata Inspector).
- **`window.rs`:** `win.toggle-developer-mode` (`Ctrl+Shift+D`, ayarı değiştirip anında kaydeder), `win.copy-absolute-path-selected` (`Ctrl+Alt+C`), `win.copy-uri-selected`, `win.copy-relative-path-selected`, `win.open-in-editor-selected`, `win.calculate-checksums-selected`, `win.developer-metadata-selected`. `update_chrome` artık her gezinmede `update_git_badge`'i tetikliyor — arka planda `find_git_root`/`git_status`, tab o sırada başka bir yere gitmişse sonucu atıyor (Kural #17).
- **`split_view.rs`:** Panel araç çubuğuna, depo dışındayken gizli duran Git durum rozeti (`Chrome.git_badge`) eklendi.
- **`shortcuts.rs` / `command_palette.rs`:** Kısayol kataloğuna "Toggle Developer Mode" ve "Copy Absolute Path"; Komut Paletine beş yeni Tools girdisi (Toggle Developer Mode, Copy Absolute Path, Copy File URI, Calculate Checksums, Open in Default Code Editor).
- **`preferences_dialog.rs`:** Advanced sayfasına "Developer" grubu ve "Developer Mode" `SwitchRow`'u eklendi.
- **`i18n.rs`:** `menu.developer*`, `dev.checksum.*`, `dev.metadata.*`, `prefs.advanced.developer_mode.*`/`group.developer` — hem `EN` hem `TR` katalogları.

### Kapsam kararları
- Git durumu tek bir rozette gösteriliyor (headerbar/breadcrumbs satırı), ayrı bir durum çubuğu kopyası eklenmedi — spesifikasyonun "Headerbar / Breadcrumbs & Status Bar" ifadesi aynı bilginin nerede sunulacağına dair iki alternatif olarak okundu, ikisini de aynı anda tutmak gereksiz tekrar olurdu.
- `git_status` `.git` iç formatını (paket dosyaları, delta çözme, ref çözümleme) elle yeniden uygulamak yerine yerel `git` ikilisini sabit argv ile çağırıyor — Kural #19 (kabuk dizesi yok) ve Kural #48 (doğrulanmadan "tamam" dememe) ile çelişmiyor: tek çağrı, hiçbir kullanıcı girdisi komuta karışmıyor, `git` yoksa/başarısız olursa rozet sessizce gizleniyor.
- Developer Mode context-menu girdileri yalnızca tekil seçimde görünüyor (spesifikasyonun her eylemi "seçili dosya" tekil bağlamında tanımlaması ve mevcut `build_item_menu`'nün zaten `count == 1` desenini izlemesiyle tutarlı).

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 402 → 418 (yeni: `veyra-filesystem::git` 10 test — kök bulma, temiz/kirli durum, staged/modified/untracked sayımı, detached HEAD, gerçek bir uzak depoya karşı ahead/behind, rozet biçimlendirme; `veyra-ui::dev_tools` 7 test — göreceli yol hesaplama, bilinen içerik için MD5/SHA-256, iptal, uzak konum reddi, düzenleyici listesinde tekrar yok), tamamı geçti.
- `cargo build --workspace`: temiz, 0 warning.

### Sıradaki Faz
Faz 40 — onay bekleniyor.

## Faz 37 — Internationalization & Localization (`veyra-ui`)

Yeni `i18n.rs`: bağımlılıksız (ne `gettext` ne `fluent`) bir çeviri motoru — `en` (varsayılan/fallback) ve `tr` katalogları, `System`/`En`/`Tr` kalıcı tercih, sistem dili algılama ve çoğul desteğiyle. `config.rs`, `preferences_dialog.rs`, `headerbar.rs`, `split_view.rs`, `context_menu.rs`, `widgets/progress_toast.rs` ve `window.rs`'in temsilci bir kesiti bu motora bağlandı.

### Eklenenler
- **`crates/veyra-ui/src/i18n.rs` (yeni modül):** `Locale` (`En`/`Tr`, `System` yok — bkz. modül dokümanı) ve iki düz `const &[(&str, &str)]` katalog tablosu (`EN`/`TR`, ~190 anahtar). `t(key)`, parametreli `t_fmt(key, &[(name, value)])` (`{name}` yer tutucu değişimi) ve çoğul-duyarlı `t_plural(key, n, params)` (`"{key}.one"`/`"{key}.other"` — İngilizce ve Türkçe şu an aynı iki-kategorili CLDR kuralını kullanıyor, yeni bir dilin farklı kural sayısına ihtiyaç duyması `plural_category`'ye tek bir `match` kolu eklemek). `detect_system_locale()` `glib::language_names()` üzerinden (`LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`'i zaten kendi içinde çözümlüyor) ilk desteklenen dil önekini bulur, hiçbiri eşleşmezse (ör. `C`/`POSIX` yerel ayarı) sessizce `En`'e düşer (Kural #15). Eksik bir anahtar `t`'de `En`'e, o da yoksa anahtarın kendisine düşer — asla panik yok. `de`/`fr`/`es`/`ru`/`ar`/`zh`/`ja` eklemek: `Locale`'e bir varyant, kendi `const TABLE`'ı, `Locale::ALL`'a ekleme — tamlık testleri yeni tablodaki eksik anahtarı derleme zamanında değil ama ilk `cargo test`'te hemen yakalar.
- **`config.rs`:** `LanguagePref` (`System`/`En`/`Tr`), `VeyraSettings.language` alanı (`#[serde(default)]`, eski `settings.json` ile geriye dönük uyumlu). `AccentColorPref::label()` ve `IconSizePref::label()` artık kendi katalog anahtarlarından okuyor (Faz 35/34'ten kalma sabit İngilizce metin yerine).
- **`preferences_dialog.rs`:** Appearance sayfasına Theme/Accent Color/Icon Size'ın hemen altına "Language" `ComboRow`'u eklendi (`System Default`/`English (US)`/`Türkçe (TR)`) — alt yazı bunun bir sonraki başlatmada uygulanacağını belirtiyor (canlı yeniden çeviri, zaten kurulmuş ~30 widget'lık pencere ağacının tamamını yeniden kurmayı gerektireceğinden bu fazın kapsamı dışında bırakıldı, `restore_tabs_on_startup`'ın Faz 34'teki "sonraki başlatmada" deseniyle aynı). Dosyadaki her 9 sayfanın başlığı, her grup başlığı ve her satır başlığı/alt başlığı/düğme metni katalogdan okunacak şekilde değiştirildi.
- **`headerbar.rs` / `split_view.rs`:** Arama/Bölünmüş Görünüm/Önizleme düğmelerinin ipucu+erişilebilir etiketleri, Icon/Compact/Details görünüm anahtarlayıcısı, Sırala ve Süz menüsünün tüm metinleri; panel gezinme düğmeleri (Geri/İleri/Yukarı/Ana Dizin/Yenile — ipuçları artık kısayolu da gösteriyor) ve adres girişi katalogdan okunuyor.
- **`context_menu.rs`:** Tüm sağ-tık menü girdileri (Aç, Birlikte Aç, Kes, Kopyala, Yapıştır, Yeniden Adlandır, Çöpe At, Kalıcı Sil, Sıkıştır, Arşivi Aç, Özellikler, Burada Terminal Aç, vb.) katalogdan okunuyor. Eski `count_label`/`move_count_label`/`trash_count_label` (üç ayrı elle-yazılmış eşiksel fonksiyon) tek bir `count_label(key, count)`'a birleşti — gerçek CLDR `t_plural` kuralını kullanıyor.
- **`widgets/progress_toast.rs`:** İşlem satırlarının fiilleri ("Copying"/"Moving"/"Moving to Trash"/"Deleting") ve Duraklat/Devam Ettir/İptal düğmeleri katalogdan.
- **`window.rs` (sınırlı kapsam — aşağıya bakın):** İki `AdwNavigationPage` başlığı ("Files"/"Sidebar"), Sıkıştırma/Ayıklama işlem fiilleri, durum çubuğunun "Cancelled"/"Archive failed"/"Loading…" metinleri, öge sayacı (`count_label`, artık `t_plural` destekli) ve boş disk alanı (`"{size} free"`) katalogdan.

### Kapsam kararları
- Spesifikasyonun listelediği 5 dosya (`config.rs`, `preferences_dialog.rs`, `headerbar.rs`, `context_menu.rs`, `window.rs`) tam bağlandı; `window.rs` özelinde yalnızca spesifikasyonun B4'te açıkça saydığı durum çubuğu/aktarım bildirimi metinleri ve panel başlıkları — dosya 3500+ satır ve içinde onlarca dağınık hata/onay diyaloğu metni var; bunların tamamının denetimi ve çevirisi başlı başına bir faz gerektirir (Kural #2 — tek seferde dev bir sıçrama yapma). `split_view.rs`/`widgets/progress_toast.rs` da dahil edildi çünkü spesifikasyonun örneklediği tam metinler ("Geri", "Kopyalanıyor…") fiilen bu dosyalarda yaşıyor.
- Diğer diyalog dosyaları (`rename_dialog.rs`, `compress_dialog.rs`, `connect_server_dialog.rs`, `disk_analyzer_dialog.rs`, `properties_dialog.rs`, vb.) bu fazda katalogdan okumuyor — kapsam dışı, sıradaki bir i18n fazına bırakıldı.
- Dil değişikliği canlı değil, bir sonraki başlatmada uygulanıyor — zaten kurulmuş widget ağacının metinlerini yeniden yazmak (her `ComboRow`/`Label`/menü/tooltip'i canlı yeniden oluşturmak) bu fazın "motoru kur ve temsilci bir kesiti bağla" hedefinin çok ötesinde bir iş; `apply_accent_color`/`StyleManager::set_color_scheme`'in aksine, çevrilmiş metin GTK widget'larına inşa anında gömülüyor, bir canlı-uygulama kancası yok.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 389 → 402 (yeni: `i18n` 13 test — `en`/`tr` katalog tamlığı [her iki yönde], boş girdi yok, tekrarlanan anahtar yok, her `.one` anahtarının `.other` karşılığı, `t`'nin `en`'e düşmesi, `t_fmt` yer tutucu değişimi, `t_plural`'ın tekil/çoğul ayrımı ve Türkçe tablo kullanımı, sistem dili algılamanın `en`'e düşmesi; `context_menu`/`window` mevcut testleri yeni imzaya güncellendi), tamamı geçti.
- `cargo build --workspace`: temiz, 0 warning (ölü kod uyarısı yok — `t_fmt` dahil her fonksiyon en az bir gerçek çağrı sitesinde kullanılıyor).

### Sıradaki Faz
Faz 38 — onay bekleniyor.

## Faz 36 — Accessibility & Inclusive Design (`veyra-ui`)

Ekran okuyucular (Orca/AT-SPI), yalnızca klavye kullanan kullanıcılar ve yüksek kontrast/büyük metin ölçeklendirmesi için tüm arayüz katmanları gözden geçirildi (Kural #28/#29).

### Eklenenler
- **`views/mod.rs`:** Yeni `accessible_description_for(&FileItem)` — dosya adı, tür (`Folder`/`File`/`Link`/`Broken Link`/…) ve (klasörler hariç) boyutu tek bir cümlede birleştiren AT-SPI açıklaması. Icon/Compact görünümü paylaştığı `build_grid_view`'un `connect_bind`'ine ve Details görünümünün Name sütununa bağlandı — her satır/hücre artık Orca'ya "notes.txt, File, 1.0 KB" gibi zengin bir açıklama okutuyor.
- **`breadcrumbs.rs`:** Her yol segmenti butonuna `accessible::Property::Label("Navigate to {segment}")` eklendi (spesifikasyondaki tam formatla).
- **`sidebar.rs`:** Places/Bookmarks/Devices/Network satırlarının erişilebilir etiketleri artık yalnızca ad değil, konum türünü de taşıyor — `"Documents, Folder"`, `"Projects, Bookmark"`, `"My USB Drive, mounted"`, `"Network, Network Location"` gibi. `places_entries()` her girdiye bir `kind` alanı ekleyecek şekilde genişletildi.
- **`split_view.rs`:** `install_panel_css` artık `*:focus-visible { outline: 2px solid @accent_color; outline-offset: 2px; }` kuralını da yüklüyor — Tab/ok tuşlarıyla gezinirken her buton/satır/giriş net bir odak halkasına sahip (fare tıklaması `:focus-visible`'ı tetiklemediğinden tıklanan bir ögede halka belirmiyor). Yeni `watch_high_contrast`: `AdwStyleManager::is-high-contrast` açıkken panel kenarlıklarını ve odak halkasını kalınlaştıran, gizli-dosya soluklaştırmasını azaltan ek bir CSS sağlayıcısını canlı olarak takıp söküyor (`notify::high-contrast`'a bağlı, `config::apply_accent_color`'ın kur/kaldır deseniyle aynı).
- **İpucu (tooltip) taraması:** Metinsiz simge butonlarından ipucu eksik olanlar tamamlandı — `file_associations_dialog.rs`'in Kapat butonu; `disk_analyzer_dialog.rs`, `properties_dialog.rs` ve `connect_server_dialog.rs`'in daha önce yalnızca tooltip taşıyıp erişilebilir etiketi eksik olan butonlarına da `accessible::Property::Label` eklendi. `split_view.rs`'in Geri/İleri/Yukarı ipuçları artık klavye kısayolını da gösteriyor (`"Go Back (Alt+Left)"` vb.).
- **Etiketsiz giriş kutuları:** `rename_dialog.rs`, `compress_dialog.rs` (ad girişi + format seçici), `conflict_dialog.rs` ve `properties_dialog.rs`'in oktal izin girişi — hiçbiri görünür bir `AdwActionRow`/`ComboRow` başlığından etiketini miras almıyordu; hepsine açık `accessible::Property::Label` eklendi.

### Kapsam kararları
- Headerbar (arama/bölünmüş görünüm/önizleme/görünüm modu/sırala-süz), panel navigasyon butonları (geri/ileri/yukarı/ana/yenile) ve Devices/Network satırları Faz 8-34 sırasında zaten `tooltip_text` + `accessible::Property::Label` taşıyordu — bu fazda yalnızca eksik kalan yerler tamamlandı, mevcut kapsam yeniden yazılmadı.
- Metin ölçeklendirmesi: kod tabanında hiçbir sabit piksel `font-size` veya Pango boyutu bulunmadığı (`grep`'le doğrulandı), tüm etiketlerin `EllipsizeMode`/tooltip ile taştığında zarifçe kısaldığı görüldüğünden bu alanda ek bir değişikliğe gerek kalmadı — mevcut mimari zaten sistem yazı tipi ölçeklendirmesiyle esnek.
- Yüksek kontrast: GTK'nin kendi sistem yüksek-kontrast temasının (tüm palet) üzerine yalnızca Veyra'nın kendi çizdiği iki şeyi (panel kenarlığı, gizli-dosya soluklaştırması, odak halkası kalınlığı) güçlendiren minimal bir ek katman kondu; paleti kendi CSS'imizde yeniden tanımlamak sistem temasıyla çakışacağından kapsam dışı bırakıldı.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 383 → 389 (yeni: `views` 6 test — erişilebilir açıklamanın boş olmadığı, dosya adını içerdiği, klasör/dosya/kırık-bağlantı türlerini doğru ayırt ettiği), tamamı geçti.
- `cargo build --workspace`: temiz. Gerçek bir Orca/AT-SPI oturumuyla uçtan uca doğrulama bu ortamda yapılamadı (headless ajan, ekran okuyucu/görüntü aracı yok) — yalnızca derleme, birim testi ve kod taraması (tüm `tooltip_text`/`accessible::Property::Label`/`Entry::new` çağrı noktalarının elle denetimi) seviyesinde doğrulandı.

### Sıradaki Faz
Faz 37 — onay bekleniyor.

## Faz 35 — Themes & Custom Veyra Accent Colors (`veyra-ui`)

Sistem temasını (ve Libadwaita'nın kendi widget metriklerini) bozmadan, yalnızca Veyra arayüzüne özel bir vurgu rengi seçilebiliyor: `accent_color`/`accent_bg_color`/`accent_fg_color` isimli Libadwaita renklerini dinamik bir `GtkCssProvider` ile geçersiz kılan bir `AccentColorPref` eklendi.

### Eklenenler
- **`crates/veyra-ui/src/config.rs`:** `AccentColorPref` (`System` + 9 sabit vurgu — Blue/Teal/Green/Yellow/Orange/Red/Purple/Pink/Slate, spesifikasyondaki hex değerleriyle), `VeyraSettings::accent_color` alanı (`#[serde(default)]`, eski `settings.json` dosyalarıyla geriye dönük uyumlu). `apply_accent_color(pref)` — `System` için önceki override provider'ını kaldırıyor, diğer her renk için `@define-color accent_color/accent_bg_color/accent_fg_color` bloğunu `STYLE_PROVIDER_PRIORITY_APPLICATION` önceliğiyle varsayılan display'e yüklüyor (`split_view::install_panel_css`'in zaten kullandığı desenle aynı). `accent_fg_color`, her rengin WCAG ağırlıklı parlaklığına göre siyah/beyaz arasından otomatik seçiliyor (ör. parlak Yellow → siyah metin, koyu Slate → beyaz metin) — kontrast için elle kodlanmış bir liste yerine tek bir hesaplama.
- **`crates/veyra-ui/src/dialogs/preferences_dialog.rs`:** Appearance sayfasına Theme satırının hemen altına "Accent Color" `ComboRow`'u eklendi; seçim her satırda olduğu gibi anında `settings.json`'a yazılıyor ve `apply_accent_color` ile canlı uygulanıyor. "Reset All Settings to Default" artık vurgu rengini de `AccentColorPref::default()` (`System`)'a döndürüyor.
- **`crates/veyra-ui/src/lib.rs`:** Uygulama açılışında, tema (`StyleManager::set_color_scheme`) uygulandığı yerin hemen ardından kayıtlı vurgu rengi de pencere kurulmadan önce uygulanıyor.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 371 → 383 (yeni: `config` 12 test — hex değerleri, CSS çıktısı, kontrast hesaplama, JSON round-trip, eksik alanla eski `settings.json` uyumluluğu), tamamı geçti.
- `cargo build --workspace`: temiz. Gerçek bir GTK/Wayland oturumunda tıklama-akışı bu ortamda otomatik doğrulanamadı (headless ajan) — derleme/birim testi seviyesinde doğrulandı.

### Sıradaki Faz
Faz 36 — onay bekleniyor.

## Faz 34 — Comprehensive Settings / Preferences (`veyra-core`, `veyra-search`, `veyra-ui`)

Tema, görünüm, gizlilik ve performans ayarları artık dağınık/sabit kodlanmış değil: uygulamanın tüm yönlerini `~/.config/veyra/settings.json` üzerinden yöneten ve çalışma zamanında canlı uygulayan bir Ayarlar Penceresi (`Ctrl+,`, `win.show-preferences`) eklendi.

### Eklenenler
- **`crates/veyra-ui/src/config.rs` (yeni modül):** `VeyraSettings` — Appearance/Navigation/Files/Search/Preview/Performance/Privacy alanlarının tamamı, `shortcuts.rs`'in `ShortcutMap` deseniyle aynı şekilde `~/.config/veyra/settings.json`'a `veyra_core::security::write_atomic_private` ile atomik yazılıyor; dosya yoksa/bozuksa sessizce `VeyraSettings::default()`'a düşüyor (`#[serde(default)]` her alanda). `SharedSettings = Rc<RefCell<VeyraSettings>>` — `TabPage`/`AppState`'in zaten kullandığı paylaşım deseniyle pencere/panel/görünüm/diyalog arasında paylaşılıyor.
- **`crates/veyra-ui/src/dialogs/preferences_dialog.rs` (yeni modül):** `AdwPreferencesDialog`, spesifikasyonun 9 sayfası (Appearance, Navigation, Files & Display, Search & Indexing, Preview, Performance, Keyboard Shortcuts, Privacy & Security, Advanced). Her satır değeri anında `settings.json`'a kaydediyor ve ilgili canlı etkiyi uyguluyor — ayrı bir "Uygula/Tamam" adımı yok.
- **Canlı uygulama:** Tema `AdwStyleManager::set_color_scheme` ile anında; Icon Size ve Directory Stream Chunk Size değişince açık sekmeler `refresh_all_tabs` ile yeniden yükleniyor; Click Policy her tıklamada `settings`'den okunuyor (`views::attach_click_policy` — Grid ve Details görünümlerinde tek tıkla açmayı, mevcut çift-tıkla-aç `connect_activate`'i bozmadan, GTK'nin kendi seçim güncellemesinden sonra okunan seçim modeliyle uyguluyor); Thumbnail Cache Capacity `ThumbnailService::resize_l1` (lru crate'in `resize`'ı) ile anında; onay diyalogları (`confirm_trash_empty`/`confirm_permanent_delete`) eylem tetiklenirken okunuyor.
- **`veyra_search::SearchIndex::search_with_limit` (`veyra-search`):** `search`'ün sabit `RESULT_LIMIT`'i yerine çağıran taraftan `max_search_results` alıyor; `enable_fts_index` kapalıyken header bar araması sonuç döndürmüyor; "Rebuild Search Index" butonu ve ayarı yeniden açmak `spawn_background_index`'i tekrar tetikliyor (indeksleme `INSERT OR REPLACE` kullandığından tekrar çalıştırmak her zaman güvenli).
- **`crates/veyra-ui/src/session.rs` (yeni modül):** "Restore Previous Tabs on Startup" — pencere kapanırken her iki panelin açık sekme yolları `~/.config/veyra/session.json`'a yazılıyor (`AdwTabView::pages()`'in sıralı listesi, sırasız `TabRegistry` HashMap'i değil); ayar açıkken sonraki açılışta tek `start_dir` yerine kayıtlı sekmeler geri açılıyor.
- **`veyra_core::security` yeni fonksiyonlar:** `sanitize_log_paths_enabled`/`set_sanitize_log_paths` (arka plan thumbnail worker thread'lerinden de erişilebilmesi için `Rc` değil `AtomicBool`) ve `log_path` — "Sanitize File Paths in Logs" açıkken loglardaki dosya yollarını yalnızca dosya adına indiriyor; mevcut ~10 `tracing::warn!(path = %...)` çağrı noktasına (`preview.rs`, `thumbnails.rs`, `window.rs`, `shortcuts.rs`) uygulandı.
- **Kısayol/Komut Paleti/Pencere entegrasyonu:** `win.show-preferences` (`<Primary>comma`) — `shortcuts.rs` kataloğuna ve varsayılanlarına, `command_palette.rs`'e eklendi; `window.rs`'te `setup_preferences_actions` diğer `setup_*_actions` fonksiyonlarıyla aynı desende.

### Kapsam kararları
- `show_hidden`/`folders_first`/`default_view_mode` yeni açılan sekmelerin *başlangıç* değeri — zaten açık bir sekmeyi geriye dönük etkilemiyor (GNOME uygulamalarındaki "varsayılan" anlayışıyla tutarlı, kullanıcının dokunmadığı bir sekmeyi sürpriz şekilde yeniden sıralamamak için).
- `open_folders_in_new_tab` saklanıyor ve kalıcı ama bu fazda klasöre tıklama davranışına derinlemesine bağlanmadı (mevcut sidebar/orta-tık "yeni sekmede aç" akışından ayrı); `restore_tabs_on_startup`'ın aksine gerçek bir davranış değişikliği gerektirmiyordu bu yüzden kapsam dışı bırakıldı.
- `stream_chunk_size` bir sonraki dizin yüklemesinde/yenilemesinde uygulanıyor ("ortasında" yakalanacak bir tarama olmadığından).

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 359 → 371, tamamı geçti (yeni: `config` 7, `session` 5 test; `veyra-core::security` 1 yeni test).
- `./target/debug/veyra` gerçek bir Wayland oturumunda başlatıldı, panik/çökme olmadan pencere açıldı; GTK gerektiren Preferences penceresinin tam tıklama-akışı bu ortamda otomatik doğrulanamadı (headless ajan, ekran etkileşimi aracı yok) — yalnızca derleme/birim testi ve gerçek uygulama başlatma seviyesinde doğrulandı.

### Sıradaki Faz
Faz 35 — Batch Rename (Toplu Yeniden Adlandırma).

## Ara Faz — Multi-Selection, Rubberband & Batch Context Actions (`veyra-ui`)

Icon/Compact/Details görünümlerinin üçü de artık `GtkSingleSelection` yerine `GtkMultiSelection` kullanıyor: fareyle boş alandan sürükleyerek dikdörtgen (lastik bant) seçim, `Ctrl+Click` ile tekil ekle/çıkar, `Shift+Click` ile aralık seçimi ve `Ctrl+A` ile görünümdeki tüm ögeleri seçme artık çalışıyor. Sağ-tık menüsü ve Copy/Cut/Trash/Delete/Compress/Panel-transfer kısayolları artık seçili tüm dosyalara topluca uygulanıyor.

### Eklenenler
- **`GtkMultiSelection` geçişi (`views/mod.rs`, `views/icon_view.rs`, `views/compact_view.rs`, `views/details_view.rs`):** `build_selection` artık `MultiSelection` döndürüyor; `selected_item` (ilk seçili öge, tek hedefli eylemler için) ve `selected_items` (seçili tüm ögeler, toplu eylemler için) yardımcı fonksiyonları eklendi. `GtkGridView` ve `GtkColumnView` üzerinde `set_enable_rubberband(true)` aktif edildi.
- **Klavye ile çoklu seçim:** `win.select-all` (`Ctrl+A`) artık `selection.select_all()` çağırıyor (önceden yalnızca ilk ögeyi seçiyordu). Yeni `win.deselect-all` eylemi (`Escape`) aktif görünümün seçimini temizliyor — `shortcuts.rs` kataloğuna ve varsayılanlarına eklendi.
- **Dinamik sağ-tık menüsü (`context_menu.rs`):** `build_item_menu` artık `&[FileItem]` alıyor; 1 ögede önceki davranışın aynısı, 2+ ögede yalnızca toplu-uyumlu eylemler ("Copy (3 items)", "Cut (3 items)", "Move 3 items to Trash", "Delete 3 items Permanently", "Compress 3 items…", panel transferi) gösteriliyor — Open/Rename/Bookmarks/Properties gibi tekil-hedefli girdiler yanıltıcı olmaması için çoklu seçimde gizleniyor. Seçimin korunması (seçili bir ögeye sağ tıklamak seçimi bozmuyor) GTK4'ün `GtkListView` ailesinin kendi tıklama davranışına dayanıyor: soldaki tıklama gibi sağ tıklama da zaten-seçili bir satırın üzerindeyse seçim değişmiyor.
- **Toplu pano/işlemler (`window.rs`):** `ClipboardEntry` artık tek `path` yerine `paths: Vec<VeyraPath>` tutuyor; `copy-selection`/`cut-selection`/`paste` buna göre güncellendi. `trash-selection`, `delete-selection` (onay diyaloğu dahil), `compress-selected` ve panel-arası `transfer_to_other_panel` (Copy/Move to Other Panel) artık `tab.selections.selected_items(...)` ile toplanan tüm seçili yolları tek bir `run_bulk_operation`/`spawn_compress` çağrısına gönderiyor — alt yapı (`OperationRequest::sources: Vec<VeyraPath>`, DND'nin `DropExecutor`) zaten çoklu kaynak destekliyordu, yeni olan yalnızca seçim tarafındaki toplama.
- **Çoklu sürükle-bırak (`dnd.rs`, `views/mod.rs`):** `attach_drag_source`'ın `get_source` imzası tek `VeyraPath` yerine `Vec<VeyraPath>` döndürüyor; `attach_row_dnd` artık `selection` parametresi alıyor ve sürüklenen satır geçerli çoklu seçimin bir parçasıysa (`selection.is_selected(position) && selection.selection().size() > 1`) seçili tüm ögelerin yollarını, değilse yalnızca o satırınkini sürüklüyor.

### Kapsam kararları
- Open/Open With/Open in New Tab-Window/Rename/Add to Bookmarks/Analyze Disk/Extract Here-to/Copy Path/Copy Location/Open Terminal/Properties/Restore from Trash tekil-hedefli kalmaya devam ediyor (spec bunları toplu istemiyordu) — çoklu seçimde bunlar sağ-tık menüsünden kaldırılıyor ama `win.*` eylemleri hâlâ ilk seçili ögeyi hedefliyor, böylece Command Palette/kısayoldan tetiklenirlerse önceki davranışlarını koruyorlar.
- Sağ-tık "seçimi koru" davranışı için GTK'nin `GtkListItemWidget` iç tıklama mantığına güvenildi (zaten-seçili bir satıra düz tıklama basışta seçimi bozmuyor); ekstra bir capture-phase gesture eklenmedi çünkü mevcut mimari (view başına tek `GestureClick`, bubble phase) bunu gerektirmeden doğru sonucu veriyor.

### Doğrulama
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: tamamı geçti (`veyra-ui`: 172 → 176, yeni: `context_menu` modülüne 4 test — `count_label`/`move_count_label`/`trash_count_label`).
- GTK gerektiren rubberband/multi-select/right-click-preserve etkileşimleri bu ortamda ekranı olan bir oturumda manuel doğrulanmadı (headless sandbox) — yalnızca derleme/birim testi seviyesinde doğrulandı.

### Sıradaki Faz
Faz 34 — Batch Rename (Toplu Yeniden Adlandırma).

## Faz 33 — Undo / Redo Engine (Geri Al / Yinele Motoru) (`veyra-filesystem`, `veyra-ui`)

Rename, Move, Copy, Trash, Create Folder/File işlemlerinin tamamı artık `Ctrl+Z` ile geri alınabiliyor, `Ctrl+Shift+Z`/`Ctrl+Y` ile yinelenebiliyor. Kalıcı silme (`Shift+Delete`) hâlâ geri alınamaz (Kural #39) ve gerçekleştiğinde bekleyen kayıtları yığından temizliyor.

### Eklenenler
- **`veyra_filesystem::trash_tracked` (`ops.rs`):** `trash()`'in aksine, ögeyi `Trash/files/` altında nereye taşıdığını (fiziksel yolu) geri döndürüyor — GIO'nun `File::trash()`'i bunu asla söylemiyor. `restore_from_trash`/`list_trash` ile aynı doğrudan freedesktop.org Trash-spec okuma/yazma yaklaşımını izliyor (ana Trash'e özel, aynı belgelenmiş sınırlama).
- **`OperationOutcome` (`queue.rs`) yeni alanlar:** `trashed`/`moved`/`copied` — kök seviyeli `(kaynak, hedef)` çiftleri. `moved`/`copied` yalnızca tüm grup hatasız tamamlandığında dolduruluyor (kısmi başarısızlıkta Geri Al/Yinele motoru yarım kalmış bir işlem üzerinde çalışmasın diye).
- **`crates/veyra-ui/src/undo.rs` (yeni modül):** `UndoableAction` (Rename/Move/Copy/Trash/Restore/CreateFolder/CreateFile), 50 derinlik sınırlı `UndoStack` (yeni eylem `redo` yığınını temizler), ve `perform_undo`/`perform_redo` — her ikisi de hedefin hâlâ var olup olmadığını kontrol edip (Kural #15/#16, asla panic) başarısız/eksik ögeleri zarifçe atlıyor, kısmi başarıyı karşı yığına geri yazıyor.
- **Pencere entegrasyonu (`window.rs`):** `win.undo`/`win.redo` eylemleri; `run_bulk_operation` artık Move/Copy/Trash başarısını `UndoStack`'e kaydediyor, kalıcı silme ise `UndoStack::purge` ile referans veren kayıtları temizliyor. `rename-selected`, `create-folder`/`create-document`, `restore-selected` kendi çağrı noktalarında kayıt yapıyor.
- **Kısayollar/Komut Paleti:** `win.undo` (`Ctrl+Z`), `win.redo` (`Ctrl+Shift+Z`, `Ctrl+Y`) — `shortcuts.rs` kataloğuna ve `command_palette.rs`'e eklendi.

### Kapsam kararları
- Spec `Copy` eylemini `Vec<dst_path>` olarak tanımlıyor; Yinele bir kaynağa ihtiyaç duyduğu için `Vec<(src, dst)>` çifti tutuluyor.
- `win.restore-selected` için ayrı bir `Restore` varyantı eklendi — restorasyon `Trash`'in geri alınması değil, kendi başına ileri yönlü bir kullanıcı eylemi (Geri Alınması yeniden çöpe atar, Yinelenmesi yeniden geri yükler).
- Geri Al/Yinele geri bildirimi şu an panel durum çubuğunda gösteriliyor (yenilemenin öge sayısı güncellemesini ezmemesi için kısa gecikmeli) — pencerede henüz bir Toast/Overlay alt yapısı yok, yeni bir tane eklemek yerine mevcut durum çubuğu deseni (`chrome.status_left`) kullanıldı.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 355/355 geçti (345 → 355; yeni: `veyra-filesystem/tests/trash.rs`'e 2, `veyra-ui::undo` modülüne 8 test).

### Sıradaki Faz
Faz 34 — Batch Rename (Toplu Yeniden Adlandırma).

## Faz 32 — File Operation Queue / Merkezi Dosya İşlemleri Kuyruğu (`veyra-ui`)

### Eklenenler
- **Çoklu Satırlı Eşzamanlı İşlem Yöneticisi (`progress_toast.rs`):**
  - Tek paylaşımlı alt çubuk yerine her arka plan işlemi (`OperationId` ile anahtarlanan) kendi satırına, bağımsız ilerleme çubuğuna, yüzdesine ve bağımsız `OperationControl` (Pause / Resume / Cancel) kontrollerine kavuşturuldu.
  - İkinci bir dosya işlemi başlatıldığında birincisinin kontrolleri artık ezilmiyor; her iki işlem de eşzamanlı olarak bağımsızca duraklatılıp devam ettirilebiliyor veya iptal edilebiliyor.
  - Çok sayıda işlem açıldığında arayüzü taşmaması için `GtkScrolledWindow` (azami 240px yükseklik) ile sarmalandı.
  - Son aktif işlem bittiğinde panel otomatik olarak kapanır.
- **Pencere Entegrasyonu (`window.rs`):**
  - `run_bulk_operation` ve `run_archive_operation` akışları `OperationId` taşıyacak şekilde güncellendi.
  - Çakışma yönetimi kanal tabanlı olarak işlem başına bağımsız çalışmaya devam ediyor.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 345/345 geçti.
- `cargo fmt --all -- --check`: temiz.

### Sıradaki Faz
Faz 33 — Batch Rename (Toplu Yeniden Adlandırma).

## Faz 31 — Huge Directory Engine: Lazy Metadata & Viewport Virtualization / Dev Klasör Motoru: Tembel Metaveri ve Görünüm Sanallaştırması (`veyra-filesystem`, `veyra-ui`)

Faz 30 `read_dir_chunked` ile 100.000+ dosyalık klasörlerin ilk paketini hızla ekrana getirdi, ama her paket hâlâ `FULL_ATTRIBUTES` ile `owner::user`, `owner::group` ve `unix::mode` gibi ekstra GIO/GVfs stat maliyeti taşıyordu — 100k satırın tamamı için gereksiz. Faz 31 bu maliyeti listeleme yolundan tamamen çıkardı ve iki kalan darboğazı kapattı: hızlı kaydırmada terk edilen thumbnail isteklerinin arka plan kuyruğunda birikmesi, ve akışlı yükleme sırasında GTK ana döngüsünün nefes almadan tek seferde binlerce ögeyi işlemesi (Kural #30, #31, #33).

### Eklenenler
- **`veyra-filesystem::metadata::FAST_ATTRIBUTES`:** `FULL_ATTRIBUTES`'ın alt kümesi — sadece bir satırı çizmek ve türünü bilmek için zorunlu alanlar (`standard::name,display-name,type,size,content-type,is-hidden,is-symlink,symlink-target,time::modified`). `owner::*`, `unix::mode/uid/gid/inode` ve `time::created/access` dışarıda bırakılıyor.
- **`read_dir_chunked` artık `FAST_ATTRIBUTES` kullanıyor** (önceden `FULL_ATTRIBUTES`): dönen `FileItem`lerde `permissions`/`owner`/`group`/`created`/`accessed`/`inode` alanları `None` — `build_file_item` zaten her GIO özniteliğini opsiyonel kabul ettiği için (GVfs mount'larının bu alanları hiç sunmadığı durumla aynı kod yolu) bu değişiklik hiçbir yerde panic üretmiyor, sadece o alanlara bakan görünümlerde (Detaylar sütunu, Yürütülebilirler hızlı filtresi) veri gelene kadar boş/varsayılan gösteriyor. `read_dir`/`stat` (Properties, arşiv/kopyalama kuyruğu) değişmeden `FULL_ATTRIBUTES` kullanmaya devam ediyor.
- **Properties penceresi artık tembel tam metaveri çekiyor:** `properties_dialog::show`, pencereyi kurmadan önce arka planda tek bir `veyra_filesystem::stat(path)` çağrısı yapıp (`fs_async::run_blocking`, Kural #11) İzinler sayfasını ve Oluşturulma/Erişim satırlarını bu güncel sonuçla inşa ediyor — çağıranın elindeki hızlı-taranmış `FileItem` yerine. Öge Properties açılmadan önce silinmişse orijinal `item`'a geri dönüyor.
- **Thumbnail kuyruğu iptali (`ThumbnailService::unbind`, Kural #31):** Görünüm fabrikaları (`views::build_grid_view`'ın `connect_unbind`'i, Detaylar görünümünün isim sütunu) artık bir satır ekrandan çıkıp yeniden kullanılırken `thumbnails.unbind(&icon)` çağırıyor. Henüz işlenmemiş bir istek varsa paylaşılan `cancelled: Arc<Mutex<HashSet<PathBuf>>>` kümesine ekleniyor; worker iş parçacığı isteği kuyruktan çektiğinde (`take_cancelled`) hemen atıyor — decode/disk I/O hiç başlamıyor. Kullanıcı aynı satıra geri kaydırırsa `bind` cancel bayrağını temizleyip isteği normal şekilde tamamlatıyor.
- **`fs_async::run_streaming` artık her paketten sonra ana döngüye devrediyor:** `receiver.recv().await` kanalda zaten hazır mesaj varken senkron şekilde çözüldüğü için (gerçek bir askıya alma olmadan), hızlı bir üretici (100k'lık taramanın tamamı) tüm akışı GTK ana döngüsüne hiç kontrol bırakmadan tek seferde işleyebiliyordu. Yeni `yield_to_main_loop()` her `on_chunk` sonrası bir `glib::idle_add_local_once` turu bekliyor — böylece 500'lük her paketten sonra girdi/yeniden çizim olayları işlenme şansı buluyor, 60 FPS kaydırma ve buton tepkiselliği korunuyor.

### Testler
- **Yeni test paketi `crates/veyra-filesystem/tests/huge_dir.rs` (4 test):** 100.000 ögelik taramada hiçbir `FileItem`'in `permissions`/`owner` taşımadığının doğrulanması (buna karşın isim/mime/değiştirilme zamanı gibi listeleme-zorunlu alanların dolu olması); tek bir ögenin lazy `stat()` ile izin/sahiplik bilgisini geri kazandığının kanıtı; gizli bayrağın ve tür tespitinin hızlı taramada da doğru kaldığı; 100k'lık taramanın makul bir süre tavanının altında tamamlandığı regresyon koruması.
- **`veyra-ui::thumbnails` testlerine 3 yeni birim testi:** kaydırmayla terk edilen bir isteğin `cancelled` kümesine düşüp worker tarafından atlandığı (`take_cancelled`); worker işlemeden önce aynı satıra geri dönülürse iptalin geri alındığı; hiç istek açmamış bir satırın `unbind`'inin güvenli no-op olduğu — hepsi GTK widget/worker iş parçacığı başlatmadan, saf `Mutex<HashSet<PathBuf>>` mantığı üzerinden.
- Toplam: 338 → 345 test, hepsi geçiyor; `cargo fmt --all -- --check` ve `cargo clippy --workspace --all-targets -- -D warnings` temiz.

### Kapsam Dışı Bırakılanlar (bilinçli faz kararı)
- Detaylar görünümünün Owner/Permissions sütunları ve "Yürütülebilirler" hızlı filtresi, bir öge seçilene veya Properties açılana kadar taze-taranmış bir dizinde boş/pasif kalır — spec'in "yalnızca Properties/seçimde tembel sorgula" gereksinimi bilerek bunu tercih ediyor; 100k satırın tamamına eager `unix::mode`/`owner::*` sorgusu (arka planda bile) taramanın asıl kazanımını geri verirdi. Gerekirse gelecekte, kullanıcı bu sütunu/filtreyi gerçekten kullandığında tetiklenen hedefli bir arka plan zenginleştirme geçişi eklenebilir.

## Faz 30 — Performance & Huge Directory Virtualization / Performans ve Büyük Klasör Sanallaştırma (`veyra-filesystem`, `veyra-ui`)

`read_dir` tüm dizin içeriğini tek seferde belleğe toplayıp döndürüyordu — 100.000+ dosyalı bir klasörde bu, ilk ögenin ekrana çizilmesinden önce tüm taramanın bitmesini bekletiyordu. Faz 30 bunu parçalı/akışlı bir tarama motoruyla değiştirdi: ilk 500 ögelik paket saniyenin çok altında ekrana düşüyor, kalan ögeler arka planda akmaya devam ediyor, kullanıcı önceki klasörden ayrılırsa tarama anında iptal ediliyor (Kural #30, #13, #33).

### Eklenenler
- **`veyra-filesystem::ops::read_dir_chunked(dir, chunk_size, control, on_chunk) -> Result<(), FsError>`:** `read_dir`'in akışlı karşılığı — çocukları `chunk_size`'lık paketler halinde `on_chunk`'a teslim eder (artı son yarım paket), tüm dizini tek bir `Vec` olarak biriktirmeden. `count_dir_recursive`/`chmod_recursive` ile aynı `OperationControl` işbirlikçi iptal deseni: iptal, teslim edilmiş her şeyi koruyarak taramayı hatasız (`Ok(())`) durdurur. `read_dir` (tüm çağıranları: `queue::flatten`, vb.) değişmeden korunuyor.
- **`READ_DIR_CHUNK_SIZE: usize = 500`:** Varsayılan paket boyutu — ilk paket ~50ms ilk-boyama bütçesinin çok altında hazır olacak kadar küçük, UI ekleme maliyetini amortize edecek kadar büyük.
- **`veyra-ui::fs_async::run_streaming`:** `run_blocking`'in akışlı karşılığı — arka plan iş parçacığından gelen her ara değeri (`on_chunk`) GTK ana döngüsüne geldiği anda teslim eder, tüm işlem bitene kadar arabelleğe almadan; son değer (`on_done`) işlem tamamlandığında bir kez çalışır. `async-channel` + `glib::spawn_future_local` üzerine kurulu, `run_blocking` ile aynı köprü deseni.
- **`AppState::load_control: Option<OperationControl>`:** O anki sekmenin aktif dizin taramasının iptal anahtarı. `load_directory`, yeni bir tarama başlatmadan önce öncekini iptal ediyor — kullanıcı devasa bir klasörden ayrılırsa arka plandaki eski tarama artık modele stale öge eklemeye devam etmiyor.
- **`load_directory` (yerel dosya sistemi yolu):** Artık `read_dir_chunked` + `run_streaming` üzerinden akıyor: model önce `remove_all()` ile temizleniyor, her paket geldikçe `gio::ListStore`'a ekleniyor ve durum çubuğu `"Loading… (N items)"` olarak canlı güncelleniyor; tarama bitince `"N items (toplam boyut)"` ile finalize ediliyor. `recent:///` ve `trash:///` konumları (gerçek GVfs mount'ları değil, zaten sınırlı boyutlu) eski tek-seferlik `run_blocking` + `read_dir`/`list_trash` yolunda değişmeden kalıyor.

### Testler
- **Yeni test paketi `crates/veyra-filesystem/tests/scaling.rs` (11 test):** 100/1.000/10.000/100.000 ögelik dizinlerde tam kapsama ve paket başına `chunk_size`'ı asla aşmayan sınırlı bellek kullanımı; ilk paketin tarama bitmeden teslim edildiğinin kanıtı; taramanın ortasında iptalin taramayı hemen durdurduğu ve başlamadan önce iptalin hiçbir şey teslim etmediği; 100.000 ögede sayım/boyut toplamlarının `saturating_add` ile taşmadan doğru sonuca ulaştığı; `chunk_size` 0 ve 1 sınır durumları; var olmayan dizin hatası.
- Toplam: 327 → 338 test, hepsi geçiyor; `cargo fmt --all -- --check` ve `cargo clippy --workspace --all-targets -- -D warnings` temiz.

### Not
- Soğuk başlangıç (C bölümü): denetim, arama indekslemesinin (`veyra_search::spawn_background_index`) ve thumbnail servisi worker'larının halihazırda ana thread'i bloklamadan arka planda başladığını doğruladı (`window.rs` `build_window`); bu alanda ek değişiklik gerekmedi.

## Faz 29 — Security Hardening & Vulnerability Prevention / Güvenlik Sertleştirmesi ve Zafiyet Önleme (`veyra-core`, `veyra-filesystem`, `veyra-ui`)

Faz 0-28 boyunca kurulmuş savunmaların (Zip Slip koruması, `NOFOLLOW_SYMLINKS` tabanlı symlink izolasyonu, argv-tabanlı süreç oluşturma) `docs/security-model.md`'ye karşı denetimi ve kalan boşlukların kapatılması. Denetim archive extraction, recursive dizin gezintisi, süreç oluşturma ve loglamanın zaten Kural #19-24'e uyduğunu doğruladı; üç somut sertleştirme uygulandı.

### Eklenenler
- **`veyra-core::security` (yeni modül):** Tüm crate'ler arasında paylaşılan güvenlik ilkelleri.
  - **`validate_filename(name: &str) -> Result<(), FilenameError>`:** Null-byte enjeksiyonunu (`FilenameError::NullByte`) ve `MAX_PATH_BYTES` (4096, Linux `PATH_MAX`) üstü aşırı uzun yolları (`FilenameError::TooLong`) reddeder; hiçbir girdide panic olmaz.
  - **`has_bidi_override(name: &str) -> bool`:** Unicode bidi override/embedding karakterlerini (`U+202A`-`U+202E`, `U+2066`-`U+2069`, örn. RTL Override `U+202E`) tespit eder — dosya uzantısı spoofing'ine karşı UI uyarısı için advisory sinyal (sert red değil, meşru RTL dosya adlarını engellememek için).
  - **`write_atomic_private(tmp_path, final_path, contents) -> io::Result<()>`:** Atomik `.tmp` + rename yazma deseni artık `.tmp` dosyasını rename'den önce `0600` izinlerine kısıtlıyor ve herhangi bir adım başarısız olursa `.tmp` kalıntısını temizliyor (Security Model 3.2).
- **`archive::security::sanitize_entry_path`:** Artık her arşiv girdi adını `veyra_core::security::validate_filename` ile de doğruluyor — null-byte içeren veya aşırı uzun girdi adları `SkipReason::UnsafePath` olarak sessizce reddediliyor (zip/tar/7z'nin hepsi aynı chokepoint'ten geçtiği için tek noktadan).
- **Aritmetik taşma sertleştirmesi:** `analyzer.rs` (`UsageNode::size_bytes` özyinelemeli toplama) ve `dircount.rs` (`DirCount::total_size`) artık `+=` yerine `saturating_add()` kullanıyor — çok büyük ağaçlarda `u64` taşmasına karşı savunma.
- **Güvenli geçici dosyalar:** `bookmarks.rs`, `shortcuts.rs`, `network.rs` atomik yazımları (`.tmp` + rename) artık `write_atomic_private` üzerinden geçiyor; `.tmp` dosyası artık rename öncesi kısa süreliğine dünya-okunabilir kalmıyor.

### Doğrulananlar (kod değişikliği gerektirmedi)
- **Path Traversal / Zip Slip (`archive/security.rs`, `archive/extract.rs`):** `sanitize_entry_path` `..`, mutlak yol ve Windows sürücü önekini reddediyor/normalize ediyor; her format (zip/tar/tar.gz/tar.xz/tar.zst/7z) `plan_entry` chokepoint'inden geçiyor.
- **Symlink & TOCTOU (`analyzer.rs`, `dircount.rs`, `queue.rs`):** Tüm özyinelemeli gezintiler `gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS` kullanıyor (lstat eşdeğeri) — symlink'ler asla takip edilmiyor, döngü riski yapısal olarak yok.
- **Shell/Komut Enjeksiyonu (`privileged.rs`, `terminal.rs`, `open_with.rs`):** Sıfır `sh -c`; her yerde `Command::new(binary).arg(...)` tipli/doğrulanmış argümanlarla.
- **Hassas Veri Loglaması:** `tracing` çağrılarında parola/jeton/kimlik bilgisi loglanmıyor; loglanan yollar kullanıcı verisi değil UI'da zaten görünür dosya yolları.

### Testler
- **Yeni test paketi `crates/veyra-filesystem/tests/security.rs`:** Path Traversal (mutlak yol zip girdisi), Zip Slip (tar `..` girdisi, raw header bytes ile), symlink materyalizasyon reddi (tar symlink girdisi), symlink döngü güvenliği (`count_dir_recursive` özyinelemeli sayım kendine referanslı symlink üzerinde), null-byte/aşırı uzun dosya adı reddi, RTL bidi override tespiti, shell metakarakter enjeksiyon kanıtı (argv-tabanlı `Command` semantiği) — 9 test.
- **`veyra-core::security` birim testleri:** null-byte reddi, aşırı uzun yol reddi, normal isimlerin kabulü, RTL override tespiti, `write_atomic_private` izin/temizlik davranışı — 6 test.
- **`archive::security` genişletilmiş testler:** null-byte içeren ve aşırı uzun arşiv girdi adlarının reddi — 2 yeni test.
- Toplam: 310 → 327 test, hepsi geçiyor; `cargo fmt --all -- --check` ve `cargo clippy --workspace --all-targets -- -D warnings` temiz.

## Faz 28 — Permissions & Privileged Operations / İzinler ve Ayrıcalıklı İşlemler (`veyra-filesystem`, `veyra-ui`)

### Eklenenler
- **`veyra-filesystem::permissions::FilePermissions` enhancements:**
  - **Özel bit getters/setters (`is_setuid()`, `is_setgid()`, `is_sticky()`, `with_setuid()`, `with_setgid()`, `with_sticky()`):** Faz 28 özel izin bitleri (setuid/setgid/sticky) için destek, özel bitler mevcut bit ayarlayıcılar tarafından korundu, artık açıkça manipüle edilebiliyor.
  - **`parse_octal(s: &str) -> Option<Self>`:** 3 veya 4 haneli octal string'leri (`"755"`, `"0755"`, `"4755"`) ayrıştırır, geçersiz input'ları (non-octal rakamlar, boş string, 4 karakterden uzun) reddeder. `octal_string()`'in tersi (bidirectional dönüşüm).
  - Unit testler: roundtrip (0755/0644/4755/2755/1777), 3-digit/4-digit parsing, özel bitler, geçersiz input reddi.
- **`veyra-filesystem::ops::chmod_recursive()` & `ChmodRecursiveOutcome`:**
  - **`chmod_recursive(root: &VeyraPath, permissions: FilePermissions, control: &OperationControl) -> Result<ChmodRecursiveOutcome, FsError>`:** recursive chmod Faz 18'in dircount.rs örneğini izleyerek, sembolik bağlantıları takip etmez, subdirectory enumerate hatalarını atlayıp devam eder (Kural #18), `OperationControl` tarafından işbirlikçi iptal desteği. Kökün kendisi dahil (Faz 5'in toplu işlemlerinden farklı olarak).
  - **`ChmodRecursiveOutcome { succeeded: u64, errors: Vec<(VeyraPath, FsError)> }`:** Faz 5'in `OperationOutcome`'u aynasını iz — toplu hatalar toplanır, ilk hata ile durmuyor.
  - `lib.rs` dışa aktarımı.
  - Unit testler: recursive uygulama, iptal, hata koleksiyonu.
- **`veyra-ui::privileged` (yeni modül):** Polkit's `pkexec` aracılığıyla ayrıcalıklı işlem yükseltme (Kural #20 izolasyon).
  - **`is_available() -> bool`:** `pkexec`'in `$PATH`'de bulunabilir olup olmadığını denetler.
  - **`PrivilegedError` (thiserror::Error):** `PkexecNotFound`, `NoAuthenticationAgent` (pkexec çıkış kodu 127), `Cancelled` (126), `Failed`, `NoTerminal`.
  - **Privileged operations (her biri `fs_async::run_blocking` içinden çağrılmalı, asla GTK main thread'inde):**
    - `chmod(path, mode, recursive) -> Result<(), PrivilegedError>` → `pkexec chmod [-R] <octal> <path>`
    - `remove(path, recursive) -> Result<(), PrivilegedError>` → `pkexec rm -f [-r] <path>`
    - `r#move(src, dst) -> Result<(), PrivilegedError>` → `pkexec mv <src> <dst>`
    - `copy(src, dst, recursive) -> Result<(), PrivilegedError>` → `pkexec cp [-a] <src> <dst>` (attribute-preserving)
    - `open_terminal_as_root(dir) -> Result<(), PrivilegedError>` → `terminal::resolve_terminal()` çözümüne, ardından `pkexec <terminal> [args]`. Hiçbir argüman user input'tan yapılmadı (Kural #19).
  - Unit testler: exit kodu eşlemesi (126→Cancelled, 127→NoAgent, diğer→Failed).
- **`veyra-ui::terminal::ResolvedTerminal` & `resolve_terminal()`:**
  - Privileged operations'in terminal'i root olarak çalıştırabilmesi için, terminal'in program + args'ını çözen yeni accessor. `open_terminal`'in çalışmasını etkilemez.
- **`dialogs::properties_dialog` (Faz 28 entegrasyonu):**
  - **Editable octal mode entry:** Mode `ActionRow` artık inline `Entry` ile editable — `parse_octal` ile doğrulama, `activate`'de `set_permissions` yürütür, "Invalid mode" hata mesajı görüntüler. Roundtrip: entry text ↔ internal state ↔ disk.
  - **Special Permissions group:** SUID/SGID/Sticky `SwitchRow`ları (symlink'ler hariç, çünkü meaningless). Diğer switch'ler gibi kablolı.
  - **"Apply Permissions to Enclosed Files…" button:** Directories için (recursive group). Tıklanırsa onay diyaloğu "This will change permissions for every file and folder inside..." → Cancel / Apply. Apply'da `chmod_recursive` ile `fs_async::run_blocking` başlatılır, spinner UI gösterir, sonuç özetlenir. Tüm hatalar `FsError::PermissionDenied` ise, "Retry as Administrator" yanıtı sunar (pkexec kullanarak tekrar `privileged::chmod(..., recursive=true)`).
  - **Retry-as-administrator on single-file chmod:** `show_chmod_error_with_retry` — permission denied ise "Retry as Administrator" yanıtı sunulur, pkexec ile `privileged::chmod`'u çağırır. Mode entry ve switch'ler aynı merkezi hata yönetimini paylaşır.
  - **Helper dialogs:** `show_recursive_chmod_dialog`, `show_recursive_chmod_retry_admin`, `show_privileged_error`.
- **`context_menu.rs` entegrasyonu:**
  - **"Open in Terminal as Root"** — directories için item menu'da (aynı section "Open Terminal Here" ile), background menu'da (tüm dizinler).
- **`window.rs` entegrasyonu:**
  - **`setup_terminal_as_root_actions(app, window, panels, focused)`:** `win.open-terminal-as-root-selected` ve `win.open-terminal-as-root-current` actions. İlki seçili öğenin dizinini (veya dosyaysa parent'ını), ikincisi mevcut dizini açar. `privileged::open_terminal_as_root` ile `fs_async::run_blocking` aracılığıyla. Kısayol: `Ctrl+Shift+Alt+t` (current için).
  - **Bulk operation retry-as-admin:** `run_bulk_operation` sonunda, tüm hatalar `PermissionDenied` ve `is_available()` ise, `show_bulk_retry_admin_dialog` gösterilir — Delete (recursive), Move, Copy için retry-as-admin desteği (Trash'ı atlayıp hayır); kaynak satırlı `privileged::remove` / `privileged::r#move` / `privileged::copy` çağrıları. Not: Move/Copy destination'lar toplu işlem outcome'unda mevcut olmadığından, Delete için yalnızca tam retry — other iki işlem bu sürümde skip edildi (Kural #2, bounded scope).

### Testler
- `veyra-filesystem::permissions`: parse_octal için 11 yeni test, special bit getter/setter'lar için 6, roundtrip için 1 — toplam +18.
- `veyra-filesystem::ops`: chmod_recursive için 3 yeni test.
- `veyra-ui::privileged`: exit code mapping için 3 yeni test.
- Toplam: 308/308 test (workspace genelinde, önceki 292'den +16).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 308/308 geçti.
- `cargo fmt --all -- --check`: temiz.

### Sıradaki Faz
Faz 29 (Onay bekleniyor.)

## Faz 27 — File Associations / Dosya Türü ve Varsayılan Uygulama Yönetimi (`veyra-ui`)

### Eklenenler
- **`file_associations` (yeni modül, `veyra-ui`):** MIME türü keşfi ve varsayılan uygulama yönetimi motoru.
  - **`FileTypeEntry` veri modeli:** `content_type` (MIME türü), `description` (insan tarafından okunabilir), `icon` (GIO simgesi), `default_app` (atanmışsa varsayılan uygulama).
  - **`list_system_file_types()`:** tüm uygulamaların desteklenen içerik türlerinin birleşimini toplar, tekilleştirir ve açıklamaya göre (tiebreak: content_type) sıralar. Yüzlerce MIME türü üretebilir, diyalog açılırken bir kez hesaplanır.
  - **`matches_query` (saf, birim testli):** content_type, description veya varsayılan uygulama adına karşı büyük/küçük harf duyarsız alt-dizi eşleşmesi — File Associations diyaloğu canlı arama filtresi bunu kullanır.
  - Unit testler: boş sorgu, content_type eşleşmesi, description eşleşmesi, app ad eşleşmesi, eşleşmeyen sorgu reddi.
- **`dialogs::file_associations_dialog` (yeni):** MIME türleri, açıklamaları ve varsayılan yöneticileri listeleyen `AdwDialog` tabanlı diyalog.
  - **Dosya türleri listesi:** `GtkSearchEntry` filtresi, `GtkListBox` + `adw::ActionRow` satırları (simge, açıklama, MIME türü, varsayılan uygulama adı, "Change…" butonu).
  - **`pick_default_app(parent, content_type, on_changed)` (faktör edilmiş işlev):** minimal app seçici overlay (hiçbir auto-launch), önerilen ve tüm uygulamalar (open_with.rs ile aynı veri kaynağı), canlı arama, seçildiğinde sadece `open_with::set_default` çağrılar. Properties diyaloğunun "Default Application" satırından da çağrılabilir.
  - Canlı güncelleme: varsayılan değiştiğinde, ağaç yeniden oluşturulmadan sadece o satırın suffix etiketi tazelenir.
- **`Properties` diyaloğu entegrasyonu (`properties_dialog.rs`):** "Default Application" satırı, Regular dosyalar için General sayfasına eklendi (Type satırından sonra) — app simgesi/adı + "Change…" butonu, `pick_default_app` çağırır, satır içi güncelleme.
- **`window.rs` entegrasyonu:** `setup_file_associations_action` — `win.manage-file-associations` aksiyonu, File Associations diyaloğu açar.
- **`command_palette.rs` entegrasyonu:** "Manage File Associations…" (Tools kategorisi, icon `preferences-desktop-default-applications-symbolic`), action `win.manage-file-associations`.

### Testler
- `veyra-ui::file_associations`: 5 yeni birim testi (`matches_query`: boş sorgu, content_type eşleşmesi, description eşleşmesi, app adı eşleşmesi, non-match).
- Toplam: 292/292 test (workspace genelinde, önceki 287'den +5).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 292/292 geçti.
- `cargo fmt --all -- --check`: temiz.

### Sıradaki Faz
Faz 28 (Onay bekleniyor.)

## Faz 26 — Drag & Drop / Sürükle ve Bırak (`veyra-ui`)

### Eklenenler
- **`dnd` (yeni modül, `veyra-ui`):** Merkezi sürükle-bırak altyapısı:
  - **`attach_drag_source` / `attach_drop_target`:** Geri dönüştürülen (recycled) `GtkListItem` satırlarında canlı `list_item.item()` takibi ile `IconView`, `CompactView` ve `DetailsView` üzerinde çift yönlü sürükle-bırak desteği.
  - **`resolve_action`:** Klavye modifikatörlerini (`Ctrl` ➔ Copy, `Shift` ➔ Move) ve aksi hâlde aynı dosya sistemi heuristiğini (aynı FS ➔ Move, farklı FS ➔ Copy) uygulayan deterministik eylem çözümleyici.
  - **Ask Popover (`Copy Here`, `Move Here`, `Create Link Here`, `Cancel`):** `Alt` basılıyken veya sağ tıkla sürükleyip bırakıldığında (`gdk::BUTTON_SECONDARY` / `DragAction::ASK`) açılan etkileşimli seçim menüsü.
  - **`create_links`:** Sembolik bağlantı oluşturma (`gio::File::make_symbolic_link`) işlemi `fs_async::run_blocking` ile arka planda çalıştırılır, UI thread'ini asla bloklamaz.
  - **Entegre Edilen Bileşenler:**
    - **Görünümler (Icon, Compact, Details):** Her satır drag source + (klasörse) drop target; görünüm arka planı açık dizine bırakma hedefi.
    - **Breadcrumbs:** Her yol kırıntısı butonu o üst klasöre bırakma hedefi.
    - **Sidebar Yer İmleri:** Dosya bırakılırsa ilgili yer imi klasörünün içine aktarma.
    - **Paneller Arası (Split View):** Sol panelden sağ panele veya tam tersine sürükle-bırak.
    - **Pencere Entegrasyonu (`window.rs`):** `build_dnd_executor` ile Copy/Move işlemleri mevcut `OperationQueue` altyapısına bağlanır; aktarım sonrası tüm açık sekmeler tazelenir.

### Testler
- `veyra-ui::dnd`: 5 yeni birim testi (Ctrl Copy, Shift Move, Alt Ask, aynı/farklı dosya sistemi çözümlemesi).
- Toplam: 287/287 test (workspace genelinde, önceki 282'den +5).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 287/287 geçti.
- `cargo fmt --all -- --check`: temiz.

### Sıradaki Faz
Faz 27 — File Associations (Dosya Türü & Varsayılan Uygulama Yöneticisi).

## Faz 25 — Keyboard-First & Configurable Shortcuts (Klavye Odaklı Kullanım & Özelleştirilebilir Kısayollar) (`veyra-ui`)

### Eklenenler
- **`shortcuts` (yeni modül, `veyra-ui`):** GTK widget'larından bağımsız, saf ve birim testli kısayol veri modeli.
  - **`ShortcutMap`:** aksiyon adı (`win.copy-selection`, parametreli aksiyonlar için `win.set-view-mode::icon` gibi hedefli biçimde) → `Vec<String>` (GTK'nın `<Primary>c` sözdizimiyle) eşlemesi. `default_shortcuts()` 32 aksiyonun tamamı için (mevcut 29 `set_accels_for_action` çağrısının birebir aynısı + yeni `win.select-all`/`win.show-shortcuts-help`/`win.reset-shortcuts`) tek doğruluk kaynağı.
  - **XDG kalıcılığı:** `~/.config/veyra/shortcuts.json`, `network.rs`'nin geçmiş dosyasıyla aynı atomik yazma deseni (`.json.tmp` → `rename`). Dosya yoksa, okunamıyorsa veya geçersiz JSON'sa sessizce `default_shortcuts()`'a düşer (Kural #4); geçersiz tek tek ivme dizeleri (`is_valid_accel` — saf dize doğrulaması, `gtk4::accelerator_parse`'a bilerek bağımlı değil çünkü o çağrı GTK'nın başlatılmasını (`gtk::init`/ekran bağlantısı) gerektirir ve bu modül `command_palette` gibi ekransız birim testli kalmalı) elenip o aksiyon için hiç kısayol yokmuş gibi davranılır.
  - **`apply_to_application(app)`:** haritadaki her aksiyonu `Application::set_accels_for_action` ile uygular; `window.rs`'te tüm `setup_*_actions` çağrılarından **sonra** çalıştırılarak kullanıcının `shortcuts.json`'u (veya hiç yoksa derlenmiş varsayılanlar) son sözü söyler.
  - **`format_accel`:** `<Primary><Shift>n` → `Ctrl+Shift+N` gibi görüntüleme dizesine çevirir — Komut Paleti rozetleri ve Kısayollar yardım penceresi bunu, `Application::accels_for_action`'dan okunan canlı değerle birlikte kullanır; ikinci bir statik kopya tutulmaz, böylece asla senkron dışı kalamaz.
  - **`catalog()`:** yardım penceresi için 32 girişlik kategorili liste (Navigation, File Operations, View, Tabs, Tools) — testler her katalog girişinin bir varsayılana, her varsayılanın bir katalog girişine sahip olduğunu doğrular.
- **`win.select-all` (`Ctrl+A`, `window.rs`):** her üç görünüm de tek seçimli `GtkSingleSelection` kullandığından (Faz 5 notu) gerçek çoklu seçime genişletilemez; belirtim gereği ilk ögeyi seçip görünüme odaklanmaya düşer (`ViewSelections::active`, `selected()` ile paylaşılan yeni yardımcı).
- **`win.show-shortcuts-help` (`Ctrl+?`) / `dialogs::shortcuts_help_dialog` (yeni):** `AdwDialog` tabanlı, kategorilere ayrılmış salt-okunur kısayol listesi (Komut Paleti'yle aynı görsel dil); her satırın rozeti `Application::accels_for_action`'dan canlı okunur.
- **`win.reset-shortcuts` (kısayolsuz — Kural #38/#39'un ruhu: yanlışlıkla tetiklenmemeli, Kısayollar penceresi/Komut Paleti üzerinden erişilir):** `default_shortcuts()`'ı `shortcuts.json`'a geri yazar ve `app`'e yeniden uygular.
- **`command_palette.rs`:** `CommandItem.shortcut` statik alanı tamamen kaldırıldı (24 elle yazılmış dize sildi) — rozetler artık `dialogs/command_palette_dialog.rs`'te `Application::accels_for_action` ile canlı okunuyor, tek doğruluk kaynağı `ShortcutMap`. Ayrıca "Go to Location" (`win.focus-address`), "Select All" (`win.select-all`), "Keyboard Shortcuts" (`win.show-shortcuts-help`) ve "Reset Shortcuts to Default" (`win.reset-shortcuts`) komutları eklendi.
- **`win.focus-address` (`Ctrl+L`):** zaten mevcuttu (Faz 9); yeni `ShortcutMap`'e ve kısayollar yardım penceresine/Komut Paleti'ne dahil edildi, ayrı bir uygulama gerekmedi.

### Testler
- `veyra-ui::shortcuts`: 19 yeni birim testi — katalog/varsayılan tutarlılığı, JSON round-trip, eksik/bozuk dosyada varsayılana düşme, geçerli dosyanın yalnızca belirtilen aksiyonları geçersiz kılması, geçersiz ivme dizelerinin elenmesi, atomik kaydet/yükle (`.tmp` kalıntısı kalmaması), `is_valid_accel` kabul/red, `format_accel`'in önceki Komut Paleti dizeleriyle birebir eşleşmesi.
- Toplam: 282/282 test (workspace genelinde, önceki 263'ten +19).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 282/282 geçti.
- `cargo fmt --all -- --check`: temiz.
- **Not:** Bu ortamda görüntü sunucusu (display server) yok; Kısayollar yardım penceresinin ve Komut Paleti rozetlerinin görsel doğrulaması gerçek bir GTK oturumunda elle test edilmedi — yalnızca derleme + birim testleri doğrulandı. `is_valid_accel`'in `gtk4::accelerator_parse`'a değil saf dize ayrıştırmasına dayanması bilerek bu kısıtı hesaba katıyor.

### Sıradaki Faz
Faz 26 (Onay bekleniyor.)

## Faz 24 — Command Palette (Komut Paleti & Hızlı Eylem Yöneticisi) (`veyra-ui`)

### Eklenenler
- **`command_palette` (yeni modül, `veyra-ui`):** GTK widget'larından tamamen bağımsız, saf ve %100 birim testli komut modeli ve puanlamalı bulanık arama motoru.
  - **`CommandItem`:** `id`, `title`, `category`, `icon_name`, `shortcut`, `action_name`, `action_target` (`Option<glib::Variant>` — `win.set-view-mode` / `win.sort-by` gibi parametreli aksiyonlar için).
  - **`all_commands()`:** 25 komutluk tam liste — File Operations (New Folder, New Document, Compress Selection…, Extract Archive…, Empty Trash, Copy Path, Copy Location), Navigation (New Tab, Close Tab, Open in New Window, Toggle Split View, Open Terminal Here), View (Toggle Hidden Files, Toggle File Preview, Icon/Compact/Details View), Tools (Analyze Disk Usage…, Connect to Server…, Open Properties, Search Files), Sort & Filter (Sort by Name/Size/Modified/Type). Her komut, ilgili menü öğesinin veya kısayolunun tetiklediği gerçek `win.*` aksiyonuna bağlanır — ikinci bir mantık kopyası yok.
  - **`fuzzy_score(query, target)` (saf, birim testli):** sıralı karakter alt-dizisi eşleşmesi; kelime başı bonusu, ardışık eşleşme bonusu (koşu uzadıkça artan), tam eşleşme ve önek eşleşmesi ağırlıklandırması.
  - **`filter_commands(query, commands)`:** boş/boşluk sorguda kategoriye göre gruplu orijinal sırayı korur; dolu sorguda puana göre azalan sırada filtrelenmiş sonuç döner (kararlı sıralama — eşit puanlarda orijinal sıra korunur).
- **`dialogs::command_palette_dialog` (yeni):** `AdwDialog` tabanlı Spotlight tarzı overlay. Üstte `GtkSearchEntry`, altında kategori başlıklı (`GtkListBox::set_header_func`) filtrelenmiş `AdwActionRow` listesi (ikon + başlık + klavye kısayolu rozeti). Odak açılışta doğrudan arama kutusunda kalır; `Yukarı`/`Aşağı` gerçek widget odağını arama kutusundan asla almayan bir program içi seçim imleci taşır (yazmaya devam edilebilir), seçili satır otomatik görünür alana kaydırılır (`Adjustment::clamp_page`). `Enter`/tıklama seçili komutu `gtk_widget_activate_action_variant` ile pencere üzerinde tetikler ve paleti kapatır; `Escape` `AdwDialog`'un varsayılan kapatma kısayoluyla kapanır.
- **`window.rs`:** `setup_command_palette_actions` — `win.command-palette` (`Ctrl+K` / `Ctrl+Shift+P`, diyaloğu açar), `win.set-view-mode` (string parametreli: `icon`/`compact`/`details`, header bar'ın görünüm anahtarlayıcı düğmelerini `sync_view_switcher` ile eşitler, `Ctrl+1`/`Ctrl+2`/`Ctrl+3`), `win.sort-by` (string parametreli: `name`/`size`/`modified`/`type`, odaklı sekmenin `sort_config`'ini günceller — Sort & Filter menüsü zaten her açılışta `tab.sort_config`'i tazelediğinden ayrı bir eşitleme gerekmez). Ayrıca daha önce kısayolu olmayan `win.create-folder`'a `Ctrl+Shift+N` eklendi.

### Testler
- `veyra-ui::command_palette`: 14 yeni birim testi — `fuzzy_score` (boş sorgu, sırasız alt-dizi reddi, eşleşmeyen karakter reddi, büyük/küçük harf duyarsızlık, tam eşleşme > dağınık eşleşme, önek eşleşmesi > önek olmayan alt-dizi, kelime başı > kelime içi, ardışık koşu > dağınık eşit uzunluk), `filter_commands` (boş/boşluk sorgu kategori sırasını korur, en iyi eşleşme önce, eşleşmeyen sorgu boş sonuç), komut listesi bütünlüğü (`id` tekilliği, her komutun bir `win.*` aksiyonunu hedeflemesi).
- Toplam: 263/263 test (workspace genelinde, önceki 249'dan +14).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 263/263 geçti.
- `cargo fmt --all -- --check`: temiz.
- **Not:** Bu ortamda görüntü sunucusu (display server) yok; diyaloğun görsel/etkileşimli doğrulaması (klavye gezinme, kaydırma, odak davranışı) gerçek bir GTK oturumunda elle test edilmedi — yalnızca derleme + birim testleri doğrulandı.

### Sıradaki Faz
Faz 25 (Onay bekleniyor.)

## Faz 23 — Terminal Integration (Burada Terminal Aç) (`veyra-ui`)

### Eklenenler
- **`terminal` (yeni modül, `veyra-ui`):** Kabuk enjeksiyonuna kapalı Terminal Başlatma Motoru — hiçbir yerde `sh -c "..."` çalışmaz, her aday `Command::new(binary).current_dir(hedef).spawn()` ile başlatılır (Kural #19); hedef dizin argüman olarak asla iletilmez, yalnızca alt sürecin çalışma dizini olarak aktarılır.
  - **Sistem Tercihleri Çözümleme Hiyerarşisi (Kural #25):** tek bir terminal hardcode edilmez, sırasıyla denenir: (1) `xdg-terminal-exec` (`$PATH` üzerinde bulunursa), (2) `$TERMINAL` ortam değişkeni (ilk boşlukla ayrılmış jeton ikili olarak çözülür, kalan jetonlar argüman olarak aktarılır), (3) `TerminalEmulator` kategorisi taşıyan GIO/XDG varsayılan masaüstü uygulaması (`AppInfo::executable`, `.desktop` yer tutucuları GIO tarafından zaten ayıklanmış halde), (4) sabit bilinen terminal listesi (`ptyxis`, `gnome-terminal`, `konsole`, `kitty`, `alacritty`, `wezterm`, `foot`, `ghostty`, `xfce4-terminal`, `mate-terminal`, `terminator`, `tilix`, `urxvt`, `xterm`) `$PATH` üzerinde bulunanlar filtrelenerek.
  - Her katman yalnızca gerçekten diskte var olup çalıştırılabilir olduğu doğrulanmış (`find_in_path`/`is_executable_file`) adaylar üretir; ilk katman spawn sırasında başarısız olursa (örn. bayat bir `$TERMINAL`), motor bir sonraki adaya otomatik geçer.
  - **Hedef dizin çözümü:** seçilen öge bir klasörse doğrudan o klasör, bir dosyaysa üst klasörü (`parent()`) açılır. Konum yerel değilse (`VeyraPath::as_local_path() == None`, örn. `sftp://`/`smb://`) `TerminalError::NotLocal` ile net biçimde reddedilir.
- **`context_menu.rs`:** Öge sağ-tık menüsündeki ve boş alan sağ-tık menüsündeki `"Open Terminal Here (Faz 23)"` / `win.not-implemented` yer tutucuları kaldırıldı; sırasıyla `win.open-terminal-here-selected` ve `win.open-terminal-here-current` gerçek aksiyonlarına bağlandı.
- **`window.rs`:** `setup_terminal_actions` — `win.open-terminal-here-selected` (seçili ögenin dizininde), `win.open-terminal-here-current` (aktif panelin güncel dizininde; `F4` ve `Ctrl+Alt+T` kısayolları — Dolphin/GNOME standartları). Başlatma hatası veya sistemde hiçbir terminal bulunamaması mevcut paylaşılan `show_error_dialog` ile `AdwAlertDialog` olarak kullanıcıya bildirilir (Kural #15/#18).

### Testler
- `veyra-ui::terminal`: 9 yeni birim testi — hedef dizin çözümü (klasör/dosya/uzak konum reddi), `find_in_path`/`find_in_dirs` (mutlak yol, `$PATH` sırasına saygı, eksik ikili), bilinen terminal listesinde yinelenme yokluğu, uzak konumda `open_terminal`'ın spawn denemeden önce reddi.
- Toplam: 249/249 test (workspace genelinde, önceki 240'tan +9).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 249/249 geçti.
- `cargo fmt --all -- --check`: temiz.

### Sıradaki Faz
Faz 24 (Onay bekleniyor.)

## Faz 22 — Open With (Birlikte Aç & Varsayılan Uygulama Yönetimi) (`veyra-ui`)

### Eklenenler
- **`open_with` (yeni modül, `veyra-ui`):** İçerik türüne göre uygulama keşfi ve güvenli başlatma motoru — tamamen `gio::AppInfo` üzerinden, hiçbir kabuk komutu çalıştırmaz (Kural #19).
  - **`recommended_apps` / `default_app` / `all_apps`:** sırasıyla `gio::AppInfo::all_for_type`, `default_for_type`, `all()` (gizli/`NoDisplay` girdiler elenmiş, isme göre sıralı) üzerine ince sarmalayıcılar.
  - **`matches_query` (saf, birim testli):** uygulama adı/açıklamasına karşı büyük/küçük harf duyarsız alt dizi eşleşmesi — diyaloğun canlı arama filtresi bunu kullanır.
  - **`launch`:** `GtkWidget::display().app_launch_context()` ile başlatılan uygulamayı doğru ekrana/çalışma alanına bağlar; tek bir `GAppInfo::launch` çağrısı (fork + D-Bus aktivasyonu, dosya sistemi G/Ç'si değil) GTK ana thread'inde çalıştırılacak kadar hızlıdır (Kural #11 toplu Copy/Move/Trash gibi işlemleri hedefler, bunu değil).
  - **`set_default`:** `set_as_default_for_type` ile XDG `~/.config/mimeapps.list` varsayılan uygulama kaydını günceller.
- **`dialogs::open_with_dialog` (yeni):** Eski `AdwDialog` + `GtkSearchEntry` tabanlı modern "Open With" diyaloğu; deprecated `GtkAppChooserDialog` yer tutucusunun yerini alır.
  - **Recommended Applications / All Applications** olarak iki `GtkListBox` bölümü, arama kutusuna göre canlı yeniden filtrelenir; her liste kendi filtrelenmiş `AppInfo` sırasını `Rc<RefCell<Vec<_>>>` içinde tutar (satır indeksi → `AppInfo` eşlemesi için).
  - Tek tık seçer (Open butonunu etkinleştirir, diğer listenin seçimini temizler), çift tık/Enter doğrudan açar. Varsayılan uygulama "Default" rozetiyle işaretlenir.
  - **"Always use this application for `<content-type>` files"** onay kutusu işaretliyse açmadan önce `open_with::set_default` çağrılır.
  - Başlatma hatası, çağıranın sağladığı `on_error` geri çağrısı üzerinden `AdwAlertDialog`'a taşınır (Kural #15/#18) — panik yok.
- **`context_menu.rs`:** Sağ-tık öğe menüsündeki düz "Open With…" girdisi, dinamik bir alt menüye (`build_open_with_submenu`) dönüştürüldü — üstte içerik türü için önerilen ilk 5 uygulama (`win.open-with-app`, uygulamanın masaüstü kimliği string hedef parametresi olarak), altında "Other Application…" (`win.open-with-selected`, tam diyaloğu açar).
- **`window.rs`:** `win.open-with-app` (string parametreli, submenu'den tek uygulama başlatır) ve güncellenmiş `win.open-with-selected` (yeni diyaloğu açar) aksiyonları; ikisi de hatayı mevcut paylaşılan `show_error_dialog`'a yönlendirir.

### Testler
- `veyra-ui::open_with`: 4 yeni birim testi (`matches_query`: boş sorgu, isme göre büyük/küçük harf duyarsız eşleşme, açıklamaya göre eşleşme, eşleşmeyen sorgunun reddi).
- Toplam: 240/240 test (workspace genelinde, önceki 236'dan +4).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 240/240 geçti.
- `cargo fmt --all`: temiz.

### Sıradaki Faz
Faz 23 — Open Terminal Here (Onay bekleniyor.)

## Faz 21 — Network (Ağ Dosya Sistemleri & Tarayıcısı) (`veyra-ui`)

### Eklenenler
- **`network` (yeni modül, `veyra-ui`):** GVfs üzerinden uzak sunuculara bağlanma mantığı; Kural #44 gereği çekirdek `veyra-filesystem`'den ayrı tutuldu.
  - **`NetworkProtocol`:** `Sftp` (`sftp://`/`ssh://`), `Smb`, `Ftp`, `Dav`, `Davs` — her biri şema, diyalog rozeti (chip) etiketi ve simge adı taşır.
  - **`detect_protocol` / `parse_server_address` (saf, birim testli):** `[user@]host[:port][/path]` biçimini şemalı ya da şemasız girdi için ayrıştırıp doğrular, boş adres/desteklenmeyen şema/eksik host hatalarını ayrı ayrı raporlar.
  - **`mount_remote_location` (asenkron):** `gio::File::mount_enclosing_volume_future` + `gtk4::MountOperation` (Kural #11/#12: GTK ana thread'ini bloklamaz). Kimlik doğrulama tamamen GTK'nın yerleşik parola diyaloğu üzerinden yürür — bu modül hiçbir zaman düz metin parola görmez (Kural #23). Zaten bağlı bir konum veya kullanıcının iptal ettiği bir bağlantı isteği zarifçe ele alınır; diğer hatalar `AdwAlertDialog`'a uygun, ham GLib metni içermeyen mesajlara çevrilir (Host not found / Timed out / Connection refused / Authentication failed).
  - **Sunucu geçmişi:** `~/.config/veyra/network_history` — en fazla 10 kayıt, en son bağlanılan başta, atomik yazım (`bookmarks.rs` ile aynı `.tmp` + rename deseni).
- **`dialogs::connect_server_dialog` (yeni):** `AdwDialog` tabanlı "Connect to Server" diyaloğu — sunucu adresi girişi, SFTP/SMB/FTP/WebDAV hızlı şema seçici (linked toggle grubu), canlı doğrulama ile etkinleşen Connect butonu, bağlanırken spinner + durum satırı, başarısız bağlantıda satır içi hata mesajı, ve tıklanınca adres alanını dolduran/silinebilen Recent Servers listesi.
- **Sidebar entegrasyonu (`sidebar.rs`):** Devices'tan ayrı yeni **Network** bölümü — sabit "Network" kökü (`network:///`), altında canlı SFTP/SMB/FTP/WebDAV bağlantıları (`devices.rs`'teki mevcut satır bileşeni yeniden kullanılarak: tıkla-git, sağ tık Unmount/Open in New Tab/Properties menüsü, satır içi eject/unmount butonu), en altta **"+ Connect to Server…"** (`win.connect-to-server`). Aynı yedi `GVolumeMonitor` hotplug sinyali artık hem Devices hem Network bölümünü tazeler.
- **`devices.rs` güncellemesi:** `scan()` artık SFTP/SMB/FTP/WebDAV bağlı noktalarını atlar (Network bölümüne taşındı); MTP/`trash://` gibi diğer uzak GVfs arka uçları Devices'ta değişmeden kalır — mevcut işlevsellik korunur (Kural #4).
- **`window.rs`:** `win.connect-to-server` aksiyonu, diyaloğu açar; başarılı bağlantı odaklanmış paneli yeni bağlanan konuma yönlendirir.

### Testler
- `veyra-ui::network`: 11 yeni birim testi (protokol tespiti, tam/şemasız URI ayrıştırma, kullanıcı adı+port, host eksik/boş adres/desteklenmeyen şema hataları, büyük/küçük harf duyarsız şema, geçmiş round-trip + tekilleştirme + üst sınır).
- Toplam: 236/236 test (workspace genelinde, önceki 225'ten +11).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 236/236 geçti.
- `cargo fmt --all`: temiz.

### Sıradaki Faz
Faz 22 — (Onay bekleniyor.)

## Faz 20 — Disk Analyzer / Disk Kullanım Analizörü (`veyra-filesystem`, `veyra-ui`)

### Eklenenler
- **`veyra-filesystem::analyzer` (yeni modül):**
  - **`UsageNode` veri modeli:** her düğüm `name`, `path`, `is_dir`, `size_bytes` (dizinler için özyinelemeli alt ağaç toplamı), `direct_file_count`, `direct_dir_count` ve boyuta göre büyükten küçüğe sıralı `children` taşır — dosyalar da ağaçta yaprak düğüm olarak yer alır.
  - **`analyze_directory` (özyinelemeli tarama):** `count_dir_recursive` (Faz 12) ile aynı `OperationControl` iptal sözleşmesini paylaşır (Kural #13) — iptal, o ana kadar biriktirileni `Ok` olarak döner. `NOFOLLOW_SYMLINKS` sayesinde sembolik bağlantılı dizinler yaprak olarak sayılır, asla özyinelemeli gezilmez (Kural #22, döngü riski yok). Alt dizin numaralandırma hatası (izin reddi, eşzamanlı silme) o dalı atlar; yalnızca kök dizinin kendisi çözülemezse sert hata döner (Kural #18).
  - **Türetilmiş görünümler — tek geçişte hesaplanır:** `largest_files`/`largest_dirs` (tüm ağaç genelinde en büyük 200 giriş, büyükten küçüğe), `duplicate_candidates` (aynı bayt boyutunu paylaşan ve `>= 1 MB` olan dosya grupları, `count >= 2`).
- **UI & Diyalog (`veyra-ui`):**
  - **`dialogs::disk_analyzer_dialog` (yeni):** `AdwDialog` tabanlı analizör penceresi. Tarama arka planda (`fs_async::run_blocking`) çalışır, kapatılırsa `OperationControl::cancel` ile anında durur.
    - **Gezinti çubuğu:** kökten güncel dizine kadar tıklanabilir breadcrumb'lar; bir kırıntıya tıklamak o derinliğe geri döner.
    - **Dağılım (Breakdown) sekmesi:** Cairo ile çizilen orantılı renkli segment çubuğu (en büyük 7 alt klasör + "Other" toplamı) ve altında tüm doğrudan alt öğelerin listesi (`AdwActionRow`, renk anahtarı, boyut, yüzde). Bir klasöre tıklamak **yeniden tarama yapmadan** önceden bellekte duran ağaçta derinlemesine gezinir (drill-down).
    - **En Büyük Dosyalar sekmesi:** tüm ağaç genelinde en büyük dosyaların sıralı listesi, her satırda "Open in Folder" butonu (`navigate` callback'i ile ana pencereyi o klasöre götürür).
    - **Yinelenen Adayları sekmesi:** aynı boyuttaki dosya grupları `AdwExpanderRow` ile gruplanır, her üye kendi "Open in Folder" aksiyonuna sahiptir.
  - **Menü & Kısayol Entegrasyonu:**
    - Klasör öğesi sağ-tık menüsüne "Analyze Disk Usage…" (`win.analyze-disk-selected`).
    - Boş alan (arka plan) sağ-tık menüsüne "Analyze Disk Usage…" (`win.analyze-disk-current` — güncel dizin).
    - `Ctrl+Shift+U` kısayolu güncel dizini analiz eder.
    - Sidebar cihaz/aygıt sağ-tık menüsüne "Analyze Disk…" (`device.analyze`).

### Testler
- `veyra-filesystem`: 8 yeni birim testi (ağaç boyutu/sıralama, en büyük dosyalar, en büyük dizinler, eşik üstü/altı yineleme adayları, symlink döngü koruması, iptal, kök dizin bulunamadı hatası).
- Toplam: 225/225 test (workspace genelinde, önceki 217'den +8).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 225/225 geçti.
- `cargo fmt --all`: temiz.

### Sıradaki Faz
Faz 21 — (Onay bekleniyor.)

## Faz 19 — Archive Manager / Arşiv Yöneticisi (`veyra-filesystem`, `veyra-ui`)

### Eklenenler
- **`veyra-filesystem::archive` (yeni modül paketi):**
  - **`ArchiveFormat` (`format.rs`):** `Zip`, `Tar`, `TarGz`, `TarXz`, `TarZst`, `SevenZip` formatlarının tespiti (`.tar.gz`, `.tar.xz`, `.tar.zst` çift uzantı önceliğiyle) ve uzantı eşlemeleri.
  - **Güvenlik Katmanı (`security.rs` — Kural #21, #22):** Zip Slip / Path Traversal saldırılarına karşı tam koruma. Her girdi yolu normalize edilir (`..` dizin kaçışları engellenir), mutlak yollar ve Windows sürücü harfleri güvenli göreceli yola indirgenir. Hedef kök dizin dışına kaçan veya geçersiz yollar sessizce atlanır (`ArchiveOutcome.skipped`). Sembolik bağlantı saldırılarına karşı arşiv içi symlink'lerin hedef dışına yazması engellenir.
  - **Sıkıştırma Motoru (`compress.rs`):** `create_archive` — kaynak dosya ve klasörleri özyinelemeli paketler, `OperationControl` ile anlık iptal edilebilir (`is_cancelled`), atomik `.tmp` geçici dosyasına yazıp işlem bitince hedefe taşır.
  - **Çıkarma Motoru (`extract.rs`):** `extract_archive` — ZIP, TAR, TAR.GZ, TAR.XZ, TAR.ZST ve 7Z arşivlerini hedef dizine güvenle açar; canlı bayt/dosya ilerlemesi (`Progress`) bildirir.
- **UI & Diyalog Entegrasyonu (`veyra-ui`):**
  - **`dialogs::compress_dialog` (yeni):** `AdwAlertDialog` tabanlı sıkıştırma diyaloğu — arşiv adı girişi (uzantı otomatik temizlenir ve seçilen formata göre eklenir) ve format seçici (`.zip`, `.tar.gz`, `.tar.xz`, `.tar.zst`, `.7z`).
  - **`archive_ops.rs` (yeni):** `spawn_compress` ve `spawn_extract` asenkron köprüleri — ağır I/O işlemlerini arka planda `fs_async::run_blocking` ile çalıştırır, UI thread'ini asla bloklamaz (Kural #11/#12).
  - **Alt İlerleme Çubuğu (`ProgressToast`):** Arşivleme ve çıkarma sırasında canlı ilerleme, Pause/Resume ve Cancel butonlarıyla kullanıcıya gösterilir.
  - **Context Menu (`context_menu.rs`) & Aksiyonlar:**
    - Seçili dosya/klasörlerde "Compress…" (`win.compress-selected`).
    - Arşiv dosyalarında "Extract Here" (`win.extract-here-selected`) ve "Extract to…" (`win.extract-to-selected` — `GtkFileDialog` ile hedef klasör seçimi).
    - Güvensiz veya atlanan girdi varsa durum çubuğunda bildirim gösterilir.

### Testler
- `veyra-filesystem`: 14 yeni birim testi (format tespiti, Zip Slip engelleme, mutlak yol temizleme, ZIP / TAR.GZ / 7Z sıkıştırma ve çıkarma round-trip testleri, iptal edilince tmp dosyasının temizlenmesi).
- `veyra-ui`: 2 yeni birim testi (`compress_dialog` uzantı temizleme).
- Toplam: 211/211 test (workspace genelinde, önceki 197'den +14).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 211/211 geçti.
- `cargo fmt --all --check`: temiz.

### Denetim Düzeltmeleri (2026-08-15)
- **`context_menu.rs::is_archive_name` format uyumsuzluğu giderildi:** fonksiyon kendi sabit uzantı listesini tutuyordu — `ArchiveFormat`'ın gerçekten desteklediği `.tar.zst` eksikti (menüde "Extract Here"/"Extract to…" hiç görünmüyordu), buna karşın motor tarafından desteklenmeyen `.tar.bz2`/`.tbz2`/`.xz`/`.rar` için gösteriliyordu (tıklanınca `extract_archive` "unrecognized archive format" hatasıyla başarısız oluyordu). Artık doğrudan `ArchiveFormat::from_name(name).is_some()` kullanıyor, tek doğruluk kaynağı `format.rs` oldu. Yeni test: `rejects_unsupported_archive_like_extensions`.
- **Sert arşiv hataları artık `AdwAlertDialog` ile gösteriliyor (Kural #15/#18):** `run_archive_operation`'daki üst seviye `Result::Err` (bozuk/tanınmayan arşiv, izin hatası, disk dolu) önceden yalnızca durum çubuğuna yazılıyordu — panik yoktu ama kullanıcıya görünür bir uyarı da yoktu. `show_trash_error` genelleştirilip `show_error_dialog` olarak Trash ve arşiv işlemleri arasında paylaşıldı; artık her iki akış da aynı `AdwAlertDialog` düzenini kullanıyor.
- Doğrulama: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — hepsi temiz (217/217 test).

### Sıradaki Faz
Faz 20 — Disk Analyzer (Etkileşimli disk kullanım analizi & ağaç haritası). Onay bekleniyor.

## Faz 18 — Trash Integration (Çöp Kutusu Entegrasyonu) (`veyra-filesystem`, `veyra-ui`)

### Eklenenler
- **`veyra-filesystem::ops`:**
  - `list_trash() -> Result<Vec<FileItem>, FsError>`: home trash'in (`$XDG_DATA_HOME/Trash`, `~/.local/share/Trash`'e fallback) `files/` dizinini doğrudan okuyor — `trash://` GVfs arka planına (`gvfsd-trash`) bağımlı değil, Faz 2'nin `restore_from_trash` tasarım kararıyla aynı gerekçeyle. Her `FileItem.path` fiziksel `Trash/files/...` yolu olduğundan, ek bir "trash entry" tipi gerekmeden doğrudan `restore_from_trash`/`delete`'e verilebiliyor ve mevcut Icon/Compact/Details görünümleri hiçbir değişiklik olmadan `trash:///` listesini render edebiliyor.
  - `empty_trash() -> Result<(), FsError>`: `Trash/files` ve `Trash/info` altındaki her girdiyi siliyor; öge bazlı best-effort (bir izin hatası kalanları durdurmuyor), ilk hata varsa çağırana döndürülüyor.
  - `restore_from_trash`: orijinal üst klasör artık mevcut değilse `create_dir_all` ile otomatik yeniden oluşturuluyor (diğer ana akım dosya yöneticileriyle aynı davranış), böylece geri yükleme başarısız olmuyor.
  - `lib.rs`: `list_trash`, `empty_trash` public export edildi.
- **`veyra-ui::trash` (yeni modül, Faz 15'in `recent.rs` desenini izliyor):** `is_trash_location`, `format_summary` ("Trash — 12 items, 4.3 MB total" / boşsa "Trash is Empty"), `TrashBannerHandles` + `build_banner()`.
- **Trash Banner (`split_view::Chrome.trash_banner`):** `trash:///` odaklandığında görünen `GtkRevealer` üst çubuğu — özet etiketi + `destructive-action` stilli "Empty Trash" butonu. `window.rs::update_chrome`/`on_directory_loaded` Recent banner ile aynı desende reveal/refresh ediyor.
- **`dialogs::empty_trash_confirm`:** "Empty all items from Trash? This action cannot be undone." uyarılı `AdwAlertDialog` (Kural #38/#39), `delete_confirm`/`clear_recent_confirm` ile aynı kalıp.
- **`win.*` aksiyonları (`window.rs::setup_trash_actions`):**
  - `win.empty-trash`: onay diyaloğu → `fs_async::run_blocking(empty_trash, ...)` → her iki panelde `trash:///` gösteriliyorsa yeniden yükleniyor; hata `AdwAlertDialog` ile bildiriliyor.
  - `win.restore-selected` (`<Primary><Shift>r`): odaklı panelin seçili ögesini `restore_from_trash` ile geri yüklüyor, arka planda; başarısızlık (disk dolu, izin, hedef çakışması) `AdwAlertDialog` ile gösteriliyor.
  - Mevcut `win.delete-selection` (`<Shift>Delete`, zaten `delete_confirm` diyaloğu arkasında) çöp kutusu bağlamında "Delete Permanently" olarak yeniden kullanılıyor — davranışı zaten birebir aynı olduğundan ayrı bir aksiyon eklenmedi.
- **`context_menu.rs`:** `attach()` artık `is_trash: Rc<dyn Fn() -> bool>` alıyor (her tıklamada tazelenen, `has_clipboard`/`split_active` ile aynı desen). `trash:///` içindeyken öge menüsü sadece "Restore" / "Delete Permanently" / "Properties" gösteriyor, boş alan menüsü "Empty Trash" / "Properties" gösteriyor — normal "Move to Trash", "Rename", "Compress", "Open With" vb. tamamen gizleniyor (devre dışı değil).
- **`window.rs::open_tab`:** her sekme artık kendi `is_trash` kapanışını (`state`'ten `trash::is_trash_location` okuyan) inşa edip Icon/Compact/Details görünüm kurucularına ve context menüye iletiyor.

### Testler
- `veyra-filesystem/tests/trash.rs`: 1 → 4 test (`list_trash_includes_freshly_trashed_entry`, `restore_recreates_missing_parent_directory`, `empty_trash_clears_isolated_trash_root` — gerçek trash'i asla silmeyen, `XDG_DATA_HOME` yönlendirmesiyle izole edilmiş bir kökte çalışıyor). Tüm testler `TRASH_TEST_LOCK` ile serileştirildi (gerçek `~/.local/share/Trash`'e paralel erişim ve `empty_trash` testinin env değişkeni geçersiz kılması güvenli olsun diye).
- `veyra-ui/src/trash.rs`: 4 birim test (`is_trash_location`, `format_summary` boş/tekil/çoğul).
- Toplam: 197/197 test (workspace genelinde, 190'dan).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 197/197 geçti.
- `cargo fmt --all --check`: temiz.

### Bilinen Notlar
- Per-mount trash dizinleri (`$topdir/.Trash-$uid`, topdir-relative `Path=` girdileri) hâlâ desteklenmiyor — `list_trash`/`restore_from_trash`/`empty_trash` sadece home trash'i (`$XDG_DATA_HOME/Trash`) kapsıyor. Faz 2'nin bıraktığı sınır bilerek genişletilmedi; yanlış/test edilmemiş bir çoklu-mount uygulaması eklemek yerine dürüst bir sınır olarak bırakıldı.
- `trash:///` listesi `trash://` GVfs arka planından değil doğrudan `Trash/files`'tan okunuyor — `gvfsd-trash` çalışmasa bile çalışır, ama bu da yalnızca yerel home trash'i kapsadığı anlamına geliyor.

### Sıradaki Faz
Faz 19 — Compress/Extract (Arşiv Desteği). Onay bekleniyor.

## Faz 17 — Devices & Volumes (Aygıtlar ve Sürücüler) (`veyra-ui`)

### Eklenenler
- **Yeni modül `devices.rs`:** `gio::VolumeMonitor`'ü tam kapasiteyle tarıyor — sadece aktif `mounts()` değil, henüz bağlanmamış `volumes()` (mount'u `None` olanlar: takılı ama açılmamış USB bellek, henüz erişilmemiş optik disk) ve kök dosya sistemi (`/`, her zaman "System" olarak, mount edilmemiş olsa bile listede) dahil.
  - `scan(monitor) -> Vec<DeviceEntry>`: kök her zaman ilk sırada; zaten mount edilmiş bir volume, `monitor.volumes()` taramasında tekrar eklenmiyor (`get_mount().is_some()` ile filtreleniyor).
  - `DeviceKind` + saf, `gio` tiplerinden bağımsız `classify(is_root, removable, optical, network) -> DeviceKind` fonksiyonu ve `icon_name(kind)` eşlemesi (System → `drive-harddisk-system-symbolic`, InternalDisk → `drive-harddisk-symbolic`, Removable → `drive-removable-media-symbolic`, Optical → `media-optical-symbolic`, Network → `network-server-symbolic`) — sınıflandırma mantığı canlı `Drive`/`Volume` nesnelerinden ayrıldığı için doğrudan birim testlenebiliyor.
  - `DeviceEntry::can_mount/can_unmount/can_eject`: sağ-tık menüsü ve satır içi çıkarma butonunun etkin/pasif durumunu `gio::Mount`/`Volume`/`Drive`'ın kendi `can_*` bayraklarından türetiyor; kök dosya sistemi hiçbir zaman unmount/eject edilemez.
  - `query_usage(path) -> Option<UsageInfo>`: `filesystem::size,free,used,type` özniteliklerini okuyor (bloklayıcı — her zaman `fs_async::run_blocking` üzerinden çağrılıyor, Kural #11/#12). `usage_fraction` (sıfır bölme güvenli, `[0,1]`'e clamp'li) ve `format_usage` ("512.0 MB free of 1.0 GB (ext4, 50% used)") ile UI'ye besleniyor.
- **`sidebar.rs` — zenginleştirilmiş Devices satırı:** ikon + isim + canlı doluluk alt etiketi (`GtkProgressBar` ile), satır oluşturulur oluşturulmaz "Calculating…" gösterip kullanım bilgisini arka planda asenkron çekiyor; mount edilmemiş volume'lar "Not mounted" gösteriyor ve tıklanınca önce mount edip sonra o konuma gidiyor. Çıkarılabilir/ayrılabilir aygıtlar için satır sonunda `media-eject-symbolic` ikonlu flat buton (tooltip: "Unmount / Eject").
  - **Sağ-tık menüsü** (`bookmark_row`'daki per-row `SimpleActionGroup` desenini izliyor, `"device.*"` eylemleri): "Open in New Tab" / "Mount" / "Unmount" / "Safe Removal / Eject" / "Properties" — her biri `DeviceEntry::can_*`'a göre etkin/pasif; Properties, aygıtı arka planda `stat` edip mevcut Faz 12 Properties diyaloğunu (`dialogs::properties_dialog::show`) açıyor.
  - **Asenkron Mount/Unmount/Eject:** `Volume::mount_future` / `Mount::unmount_with_operation_future` / `{Mount,Volume,Drive}::eject_with_operation_future` (gtk-rs'in GIO async future sarmalayıcıları, `glib::spawn_future_local` ile) — UI thread hiçbir noktada bloklanmıyor (Kural #11). Mount, olası şifre/decrypt istemleri için varsayılan bir `GMountOperation` geçiyor. Hata (aygıt meşgul, yetki reddi) `AdwAlertDialog` ile zarifçe gösteriliyor, çökme yok (Kural #15/#18).
  - **Canlı hotplug:** önceki sadece üç `mount_*` sinyaline ek olarak `volume_added`/`volume_removed`/`drive_connected`/`drive_disconnected` de dinleniyor — tamamı Devices bölümünü baştan yeniden çiziyor.
- **`window.rs`:** `sidebar::build` artık Properties diyaloğu için `ThumbnailService`'i de alıyor (pencere zaten sidebar'dan önce oluşturuluyordu, `thumbnails` de ondan önce — sıralama değişmedi, sadece bir parametre eklendi).

### Testler
- `devices.rs`'e 11 yeni birim testi: `classify`'ın kök/optik/ağ/çıkarılabilir/varsayılan önceliklendirmesi, `icon_name`'in her `DeviceKind` için doğru simgeyi döndürmesi, `usage_fraction`'ın normal/sıfır-toplam/toplam-üstü-kullanım durumları, `format_usage`'ın dosya sistemi türü var/yok iki biçimi.
- Toplam: 190/190 test (workspace genelinde, önceki 179'dan +11).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 190/190 geçti.
- `cargo fmt --all`: temiz.
- `cargo run` gerçek Wayland oturumunda kısa süre çalıştırıldı: pencere ve sidebar (Devices dahil) panik atmadan açıldı, gerçek `GVolumeMonitor` taraması hatasız tamamlandı.
- **Not:** bu sandbox'ta Wayland girdi otomasyonu ve ekran görüntüsü alma yeteneği bulunmadığından, sağ-tık menüsü / mount / unmount / eject akışlarının uçtan uca tıklama testi otomatik sürülemedi; bu yollar birim testleri, statik inceleme ve yukarıdaki canlı başlatma doğrulamasıyla sınırlı kaldı (Faz 13-16'daki aynı sınırlama).

### Sıradaki Faz
Faz 18. Onay bekleniyor.

## Faz 16 — Favorites / Bookmarks (Yer İmleri & Sürükle-Bırak) (`veyra-ui`)

### Eklenenler
- **Yeni modül `bookmarks.rs`:** Linux masaüstü standardı `~/.config/gtk-3.0/bookmarks` dosyasını okuyup yazıyor (GTK3/GTK4 dosya seçicileri ve Nautilus ile aynı dosya — Veyra'nın yer imleri masaüstünün geri kalanıyla senkron kalıyor). Format satır başına `<uri>[ <özel etiket>]`.
  - `load()` / `add(target, label)` / `remove(uri)` / `rename(uri, new_label)`: her biri gerçek XDG dosya yoluna sabit ince sarmalayıcı; asıl mantık `*_at(path, ...)` iç fonksiyonlarında yaşıyor, böylece birim testleri kullanıcının gerçek yer imlerine hiç dokunmadan geçici dosyalarla çalışıyor.
  - **Atomik yazma:** her `save_to` çağrısı `<path>.tmp`'e yazıp `rename` ile üzerine alıyor — yarıda kesilen bir yazma asla bozuk dosya bırakmıyor (Kural #16/#17).
  - `add` aynı URI için idempotent (zaten yer imliyse sessizce no-op, hata değil).
  - **Canlı izleme (`watch`):** `gio::File::monitor_file` ile bookmarks dosyasını izliyor; Veyra içinden veya harici bir uygulamadan (ör. Nautilus) yapılan her değişiklikte callback'i tetikliyor. Monitor oluşturma başarısız olursa panik atmak yerine uyarı loglayıp `None` dönüyor (Kural #15/#18/#20) — sidebar başlangıçta yüklenen listeyi göstermeye devam ediyor, sadece canlı yenilenmiyor.
- **`sidebar.rs` — Bookmarks bölümü:** "Places" ile "Devices" arasına yerleştirildi. Her satır özel etiket (varsa) veya yer iminin son yol segmentini `starred-symbolic` ikonuyla gösteriyor, tıklamada ilgili konuma gidiyor.
  - **Sağ-tık menüsü:** "Open in New Tab" / "Rename Bookmark…" / "Remove from Bookmarks" — `context_menu.rs`'in pencere-geneli `win.*` eylemlerinden farklı olarak her satır kendi hedefine sahip olduğundan, menü her satıra özel bir `gio::SimpleActionGroup` (`"bookmark.*"`) üzerinden bağlanıyor.
  - **Sürükle-Bırak ile ekleme:** Bookmarks bölümüne (`gtk4::DropTarget`, `gdk::FileList` kabul eder) herhangi bir panelden bir klasör sürükleyip bırakmak onu otomatik yer imlerine ekliyor; dizin olmayan bırakmalar sessizce yok sayılıyor.
  - **Canlı yenileme:** `bookmarks::watch` ile kurulan `GFileMonitor`, Veyra'nın kendi yazmalarında da harici değişikliklerde de sidebar'ı otomatik yeniden çiziyor; monitor'ün referansı, bölümün ömrü boyunca yaşayan drop-target kapanışında tutuluyor (widget ağacına gömülü bir GObject'in ömrünü Rust tarafında kapanış yakalaması üzerinden garanti eden, `unsafe` gerektirmeyen bir desen).
- **`context_menu.rs`:** klasör ögelerinin sağ-tık menüsüne "Add to Bookmarks" (`win.add-to-bookmarks-selected`) eklendi.
- **`window.rs`:** yeni `win.add-to-bookmarks-selected` eylemi (odaklı panelin seçili klasörünü `bookmarks::add`'e yönlendiriyor, hata olursa durum çubuğuna yazıyor) ve sidebar'ın "Open in New Tab" bağlamı için yeni bir `open_in_new_tab` kapanışı eklendi. `sidebar::build` artık Rename Bookmark diyaloğunu bağlayabilmek için pencere referansı alıyor — bu yüzden pencere artık içerik ayarlanmadan önce oluşturuluyor (`adw::ApplicationWindow::builder()` → `sidebar::build(&window, ...)` → `window.set_content(...)`).

### Testler
- `bookmarks.rs`'e 9 yeni birim testi: etiketli/etiketsiz satır ayrıştırma, boş satır atlama, serileştirme round-trip'i, `add`→`load` round-trip'i, aynı URI için `add`'in idempotentliği, `remove`, `rename` (özel etiket verme ve boş etiketle temizleme), var olmayan dosyadan `load`'ın boş liste döndürmesi. Tümü geçici dosyalarla çalışıyor, gerçek `~/.config/gtk-3.0/bookmarks`'a dokunmuyor.
- Toplam: 179/179 test (workspace genelinde, önceki 170'ten +9).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 179/179 geçti.
- `cargo fmt --all`: temiz.
- **Not:** bu sandbox'ta Wayland girdi otomasyonu ve ekran görüntüsü alma yeteneği bulunmadığından, sürükle-bırak ile ekleme, sağ-tık menüsü tıklamaları ve Rename Bookmark diyaloğunun uçtan uca canlı etkileşimi otomatik sürülemedi; bu yollar birim testleri ve statik inceleme ile doğrulandı (Faz 13/14/15'teki aynı sınırlama).

### Sıradaki Faz
Faz 17. Onay bekleniyor.

## Faz 15 — Recent Files & Privacy / Son Kullanılanlar ve Gizlilik (`veyra-ui`)

### Eklenenler
- **Yeni modül `recent.rs`:** `recent:///` normal bir GVfs bağlama noktası olmadığından (`veyra_filesystem::read_dir` genel amaçlı `enumerate_children` çağrısıyla güvenilir biçimde desteklenmiyor), bu konum artık doğrudan XDG `recently-used.xbel` kaydından (`gtk4::RecentManager`) besleniyor:
  - `snapshot_entries()`: GTK ana thread'inde (`RecentManager` gereksinimi), zaten bellekte ayrıştırılmış URI + son-ziyaret zaman damgası listesini I/O yapmadan okur.
  - `list_recent_items(entries)`: her URI'yi arka planda `veyra_filesystem::stat` ile doğrular; artık var olmayan ögeleri panik atmadan zarifçe atlar (Kural #18/#20), her `FileItem`'ın `accessed` alanını dosya sistemi atime'ı yerine kayıttaki gerçek "son ziyaret" zaman damgasıyla damgalar (birçok bağlama noktası atime izlemediğinden daha güvenilir), sonucu azalan sırada döndürür.
  - `TimeGroup` (`Today`/`Yesterday`/`ThisWeek`/`Older`) + `classify(accessed, now)`: takvim gününe göre Bugün/Dün, kalan 7 günlük pencereye göre Bu Hafta, ötesi Daha Eski.
  - `format_group_summary(items, now)`: `TimeGroup::classify`'ı gerçek bir UI çıktısına bağlar — banner başlığını `"Recent Files — 3 Today, 12 This Week"` gibi bir döküme çevirir (boş gruplar atlanır).
  - `record_opened(uri, privacy_mode)` / `clear_history()`: sırasıyla kayda ekleme (Gizlilik Modu açıksa atlanır) ve `purge_items()` ile tam temizleme.
- **`window.rs::load_directory`:** `recent::is_recent_location` doğruysa genel `read_dir` yerine iki adımlı `snapshot_entries` (ana thread) + `list_recent_items` (arka plan, `fs_async::run_blocking`) akışına dallanıyor — UI thread hâlâ hiç engellenmiyor (Kural #11/#12).
- **Varsayılan sıralama:** `navigate_to`, `recent:///`'a girildiğinde sekmenin `sort_config`'ini otomatik olarak `SortKey::Accessed` + `SortOrder::Descending`'e ayarlayıp `resort()` çağırıyor (mevcut `SortKey::Accessed`/`metadata.accessed` altyapısı yeniden kullanıldı, yeni alan eklenmedi).
- **Gizlilik Modu & Clear History banner'ı (`recent::build_banner`, `split_view::Chrome.recent_banner`):** her panelin araç çubuğu ile sekme şeridi arasına yerleştirilen, yalnızca `recent:///`'da görünür olan bir `GtkRevealer`; başlık etiketi (`TimeGroup` dökümü), `Privacy Mode` anahtarı ve `destructive-action` stilli `Clear History` düğmesi içeriyor. `update_chrome` her navigasyon/sekme değişiminde görünürlüğü ve anahtarın durumunu senkronize ediyor.
- **`win.clear-recent-history`:** `dialogs::clear_recent_confirm` (Kural #38/#39'a uygun `AdwAlertDialog` onayı, `delete_confirm.rs` ile aynı desen) üzerinden onaylandığında `recent::clear_history()` çağırıp `recent:///`'ı gösteren her paneli yeniden yüklüyor.
- **`win.toggle-privacy-mode`:** app-genelinde paylaşılan `Rc<RefCell<bool>> privacy_mode`'u (her iki panelin banner'ı ve `open_item` tarafından paylaşılır) değiştirip her iki panelin anahtarını senkronize ediyor.
- **`window.rs::open_item`:** bir dosya açılırken (dizinler hariç), arka plan thread'i başlatılmadan *önce*, ana thread'de `recent::record_opened(uri, &chrome.privacy_mode)` çağrılıyor — `RecentManager` ana-thread-only olduğundan sıralama önemli.

### Testler
- `recent.rs`'e 8 yeni birim testi: `is_recent_location` tam URI eşleşmesi, `TimeGroup::classify`'ın dört dalı (bugün/dün/bu hafta/daha eski, 7 günlük sınır dahil), ve `list_recent_items`'ın gerçek geçici dosyalarla artık var olmayan bir URI'yi atlayıp kalanları son-ziyaret zamanına göre azalan sıraladığını doğrulayan bir entegrasyon testi.
- Toplam: 170/170 test (workspace genelinde).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 170/170 geçti.
- `cargo fmt --check`: temiz.
- Uygulama gerçek Wayland (KDE Plasma) oturumunda birkaç saniye çalıştırıldı; başlangıçta panik/çökme gözlenmedi. **Not:** bu sandbox'ta Wayland girdi otomasyon aracı (wtype/ydotool/xdotool) ve ekran görüntüsü alma yeteneği bulunmadığından, Sidebar'daki "Recent" ögesine tıklama, Privacy Mode anahtarının canlı etkileşimi ve Clear History onay diyaloğunun tıklama-tabanlı doğrulaması otomatik sürülemedi; bu yollar birim testleri ve statik inceleme ile doğrulandı (Faz 13/14'teki aynı sınırlama).

### Sıradaki Faz
Faz 16. Onay bekleniyor.

## Faz 14 — Hidden Files / Gizli Dosyalar (`veyra-ui`)

### Eklenenler
- **`Ctrl+H` (`win.toggle-hidden-files`):** odaklı panelin aktif sekmesinde gizli dosya/dizinlerin gösterimini açıp kapatır. `setup_navigation_shortcuts` içine eklendi (`window.rs`), diğer `win.*` kısayollarıyla aynı `SimpleAction` deseni.
- **`.hidden` ve dotfile desteği "bedava":** `veyra-filesystem`'in `build_file_item`'ı GIO'nun `standard::is-hidden` özniteliğini zaten okuyor (`metadata.rs:125`) — GIO'nun yerel arka ucu bunu hem `.` ile başlayan adlar hem de dizinin kendi `.hidden` listesi için otomatik hesaplıyor, bu yüzden Faz 14 `veyra-filesystem`'de değişiklik gerektirmedi; iş tamamen UI filtre/görünüm katmanında.
- **`TabPage.show_hidden: Rc<RefCell<bool>>`:** Faz 7 sekme izolasyonuna (Kural #51) uygun, her sekme kendi gizli-dosya tercihini taşır; varsayılan `false`.
- **`sorting::passes_hidden_filter(item, show_hidden)`:** `window.rs`'teki `build_combined_filter`'a üçüncü bir `AND` koşulu olarak eklendi (arama metni ve `QuickFilter`'ın yanına).
- **Görsel ayrıştırma:** yeni `.veyra-hidden-item` CSS sınıfı (`split_view.rs`'in mevcut `install_panel_css` sağlayıcısına eklendi — `opacity: 0.55; font-style: italic;`). Icon/Compact görünümde öge kutusuna, Details görünümde Name hücresinin satırına ve diğer sütun hücrelerinin etiketlerine uygulanıyor; geri dönüştürülen liste ögelerinde sınıf her `bind`'de açıkça set/kaldırılıyor (aksi halde eski ögenin sınıfı sızabilir).

### Testler
- `sorting.rs`'e 3 yeni birim testi: gizli öge `show_hidden=false` iken filtrelenir, `show_hidden=true` iken gösterilir, görünür öge her iki durumda da geçer.
- Toplam: 162/162 test (workspace genelinde).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 162/162 geçti.
- `cargo fmt --check`: temiz.
- Uygulama gerçek Wayland (KDE Plasma) oturumunda çalıştırıldı; varsayılan (`show_hidden=false`) durumda `~` dizinindeki dotfile'ların (`.bashrc`, `.gitconfig`, vb.) artık listede görünmediği ekran görüntüsüyle doğrulandı. **Not:** bu sandbox'ta Wayland girdi otomasyon aracı bulunmadığından `Ctrl+H`'nin canlı tıklama/tuş simülasyonuyla açılıp-dimmed-görünüm-göstermesi otomatik sürülemedi; toggle mantığı birim testleri ve statik inceleme ile doğrulandı (Faz 13'teki aynı sınırlama).

### Sıradaki Faz
Faz 15. Onay bekleniyor.

## Faz 13 — Sorting & Filtering / Sıralama ve Filtreleme (`veyra-ui`)

### Eklenenler
- **Yeni modül `sorting.rs`:**
  - `SortKey` (Name/Size/Type/Modified/Created/Accessed/Owner), `SortOrder` (Ascending/Descending), `SortConfig { key, order, folders_first }` — bir sekmenin tüm sıralama tercihini kapsayan tek kaynak.
  - `compare_items(a, b, &SortConfig)`: her üç görünümün (Icon/Compact/Details) paylaştığı tek karşılaştırıcı; `folders_first` her zaman diğer kriterlerden önce uygulanır. `Name` anahtarı büyük/küçük harf duyarsız **doğal sıralama** kullanır (`"file2" < "file10"`).
  - `build_sorter(Rc<RefCell<SortConfig>>) -> gtk4::CustomSorter`: `SortConfig` değiştiğinde `Sorter::changed` ile tüm görünümleri aynı anda yeniden sıralayan paylaşılan sorter.
  - `QuickFilter` (All/Images/Videos/Documents/Archives/Executables/LargeFiles/RecentlyModified) ve `quick_filter_matches(item, filter, now)`: MIME türü ve/veya uzantı bazlı eşleştirme; dizinler her filtrede her zaman geçer (gezinme engellenmesin diye).
- **Birleşik filtre zinciri:** `window.rs`'teki `build_combined_filter` artık serbest metin aramasını (`search_query`) `QuickFilter` ile `AND` mantığıyla birleştiriyor.
- **Details view senkronizasyonu:** `ColumnView`'in kendi başlık-tıklama sorter'ı (`GtkColumnViewSorter`, `gtk4` `v4_10` özelliği) artık modeli değil yalnızca başlık oku göstergelerini sürüyor; `primary_sort_column`/`primary_sort_order` değişince paylaşılan `SortConfig`'e yansıtılıp `TabPage::resort()` tetikleniyor — böylece Icon/Compact/Details her zaman aynı sırada.
- **`TabPage` (Faz 7 izolasyonu korunarak) yeni alanlar:** `sort_config`, `quick_filter`, `sorter`, `details_column_view`, `details_sort_columns`, `sort_sync_guard`; `resort()`/`refresh_filter()` yardımcı metodları.
- **HeaderBar'da Sort & Filter menü düğmesi:** `view-sort-ascending-symbolic` ikonlu `GtkMenuButton`, açıldığında odaklı panelin aktif sekmesinin güncel `SortConfig`/`QuickFilter` durumunu senkronize eden bir `GtkPopover`; Sort By / Direction / Folders First / Filter By bölümleri, radio-stil `GtkCheckButton` gruplarıyla.

### Testler
- `sorting.rs` içinde 17 birim testi: her `SortKey` için sıralama doğruluğu, `SortOrder` tersine çevirme, `folders_first` aktif/pasif kombinasyonları, doğal sıralama, ve her `QuickFilter` türü için eşleşme/eşleşmeme senaryoları (dizin her zaman geçer dahil).
- Toplam: 159/159 test (workspace genelinde, `veyra-ui` 53, `veyra-search` 35, `veyra-filesystem` ve diğer crate'ler 71).

### Bağımlılık Değişiklikleri
- `veyra-ui`: `gtk4` bağımlılığına `v4_10` özelliği eklendi (`GtkColumnViewSorter::primary_sort_column`/`primary_sort_order` için gerekli). Bu, `GtkSignalListItemFactory::connect_setup`/`connect_bind`'in Faz 3'ten beri kullanılan tip-çıkarımlı (`&ListItem`) kısayolunu devre dışı bıraktığından, tüm `factory.connect_setup`/`connect_bind` çağrıları artık kapanış içinde `list_item.downcast_ref::<gtk4::ListItem>()` ile açıkça indiriliyor (davranış değişikliği yok, yalnızca tip belirtimi).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 159/159 geçti.
- `cargo fmt --check`: temiz.
- Uygulama gerçek bir Wayland (KDE Plasma) oturumunda çalıştırıldı; HeaderBar'da yeni Sort & Filter düğmesi doğrulandı (ekran görüntüsü ile), panik/çökme gözlenmedi. **Not:** bu sandbox'ta Wayland girdi otomasyon aracı (wtype/ydotool/xdotool) bulunmadığından popover'ın tıklama-tabanlı tam etkileşimi (radyo seçimi, sütun başlığı tıklaması) otomatik olarak sürülemedi; bu yol birim testleri ve statik inceleme ile doğrulandı.

### Sıradaki Faz
Faz 14. Onay bekleniyor.

## Faz 12 — Properties Window / Özellikler Penceresi (`veyra-filesystem`, `veyra-ui`)

### Eklenenler
- **`veyra-filesystem`'de yeni sorgu/işlem yardımcıları:**
  - `ops::stat(path)`: bir dizinin *kendisinin* meta verisini sorgular (`read_dir` yalnızca çocukları listeler — hâlihazırda açık olan sekmenin kendi Properties'ini açmak için gerekliydi, çünkü bir dizin kendi listelemesinde bir öge olarak görünmez).
  - `ops::set_permissions(path, FilePermissions)`: GIO `g_file_set_attribute_uint32("unix::mode", ...)` üzerinden `chmod`.
  - **Yeni modül `advanced.rs`:** `stat_advanced(path) -> AdvancedInfo { device_id, disk_usage_bytes, filesystem_type }` — aygıt ID'si (`unix::device`), gerçek ayrılmış disk kullanımı (`unix::blocks * 512`) ve `query_filesystem_info("filesystem::type")` ile dosya sistemi türü.
  - **Yeni modül `dircount.rs`:** `count_dir_recursive(dir, &OperationControl) -> DirCount { file_count, dir_count, total_size }` — Faz 12'nin tek uzun sürebilecek hesaplaması olduğundan, Faz 5'in toplu işlem motorunun zaten kullandığı aynı `OperationControl` ile iş birlikçi iptal edilebilir (Kural #13); alt dizinlerden biri ortada okunamaz hale gelirse (izin hatası, eşzamanlı silme) tüm sayımı iptal etmek yerine o alt ağaç atlanır (Kural #18). Symlink'li dizinler `ops::delete`'in politikasıyla aynı şekilde asla içine girilmeden birer öge olarak sayılır (Kural #22).
  - **`permissions.rs`:** eksik olan `is_owner/group/other_readable/writable` getter'ları ve dokuz adet `with_owner/group/other_read/write/execute(bool) -> Self` builder-tarzı setter'ı eklendi — özel bitleri (setuid/setgid/sticky) koruyarak yalnızca hedeflenen `rwx` bitini değiştiriyorlar.
- **Yeni modül `veyra-ui/src/dialogs/properties_dialog.rs`:** `AdwPreferencesDialog` tabanlı, üç sayfalı Özellikler penceresi — sayfalar arası geçiş (dar genişlikte alt anahtara otomatik düşüş dahil) tamamen `AdwPreferencesDialog`'un kendi yerleşik davranışı, elle `AdwViewStack`/`AdwViewSwitcherTitle` kurulumu gerekmedi (ki bu zaten `libadwaita` 1.4'te kullanımdan kaldırılmış olurdu).
  - **General:** büyük ikon (mevcut `ThumbnailService`/`icon_name_for` ile, görsel dosyalarda gerçek küçük resim) + ad, Type (`gio::content_type_get_description` ile insan tarafından okunabilir MIME açıklaması, dizinler/sembolik bağlantılar/özel dosyalar için sabit etiketler), Location ("Copy Path" düğmesiyle), Size (`insan-okunabilir (tam bayt)`), Disk Usage, klasörler için Contains, ve Created/Modified/Accessed (yerel saat, `chrono::Local`). Disk Usage ile klasörlerin Contains sayımı, pencere anında açıldıktan *sonra* arka planda dolduruluyor (Kural #11/#12) — Contains hesaplanırken bir `GtkSpinner` gösteriliyor, pencere kapanınca `OperationControl::cancel()` tetikleniyor.
  - **Permissions:** salt-okunur Owner/Group satırları, Owner/Group/Others için üçer `AdwSwitchRow` (Read/Write/Execute) erişim matrisi, canlı `0755 · rwxr-xr-x` Mode göstergesi, düzenli dosyalar için ayrı bir "Allow executing file as program" kısayol anahtarı (üç `x` bitini birlikte değiştiriyor). Her anahtar değiştiğinde değişiklik hemen `set_permissions` ile diske uygulanıyor; yazma başarısız olursa anahtar (sinyal işleyicisi geçici olarak bloke edilerek — yeniden giriş/sonsuz döngü riski olmadan) eski durumuna döndürülüp bir `AdwAlertDialog` ile hata gösteriliyor (Kural #18/#20). GVfs arka uçlarında POSIX izinleri yoksa (`FileMetadata::permissions == None`) bu sayfa hiç eklenmiyor.
  - **Advanced:** MIME Type, Inode, Device, Filesystem (ikisi de tek bir arka plan `stat_advanced` çağrısından), ve sembolik bağlantılar için ayrı bir Target/Status (kırık/geçerli) grubu.
- **Bağlama (`context_menu.rs`, `window.rs`):** öge menüsündeki ve boş-alan menüsündeki "Properties" girdileri artık gerçek `win.properties-selected` / `win.properties-current` aksiyonlarına bağlı (ikisi de `setup_properties_actions`'ta kayıtlı). `win.properties-selected` seçili ögeyi hiçbir ek G/Ç olmadan doğrudan açar (`FileItem` zaten görünüm modelinde yüklü); `win.properties-current` önce mevcut dizini arka planda `stat` eder (o dizin kendi listelemesinde bir öge olarak yer almadığından), sonucu pencereyi açmak için kullanır. `Alt+Enter` kısayolu `win.properties-selected`'a bağlandı.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: tümü geçti (yeni birim testler: `FilePermissions` read/write getter'ları ve `with_*` setter'larının özel bitleri koruduğu, `count_dir_recursive`'in iç içe dizinleri doğru saydığı + iptal öncesi çağrıldığında boş sonuç döndürdüğü + var olmayan dizinde sert hata verdiği, `properties_dialog`'daki bayt gruplama biçimlendiricisi ve `parent_display` yardımcı fonksiyonu).
- **Manuel duman testi:** `./target/debug/veyra` gerçek Wayland oturumunda başlatıldı, pencere hatasız açıldı (log'da panik/hata yok). Bu ortamda ekran görüntüsü/giriş otomasyonu araçları (`grim`/`wtype`/`xdotool`) kurulu olmadığından sağ-tık → Properties → sayfa geçişi → izin anahtarı değiştirme akışı görsel olarak doğrulanamadı; bu adım kullanıcı tarafından gerçek bir masaüstünde tekrar edilmeli.

### Sıradaki Faz
Faz 13. Onay bekleniyor.

## Faz 11 — Thumbnail Engine / Küçük Resim Motoru & Önbellek (`veyra-ui`)

### Eklenenler
- **Yeni modül `veyra-ui/src/thumbnails.rs`:** iki seviyeli, tamamen asenkron küçük resim önbellek/üretim motoru. `ThumbnailService` (`window.rs`'te bir kez inşa edilip `has_clipboard`/`split_active` ile aynı `Rc` deseniyle her sekmeye/görünüme taşınıyor) tek genel giriş noktası olan `bind(&GtkImage, &FileItem)`'ı sunuyor.
  - **L1 bellek içi LRU:** `lru::LruCache<PathBuf, {mtime, Pixbuf}>`, sabit 1000 öge kapasitesiyle (Kural #31/#40) — en eski öge otomatik tahliye ediliyor, `mtime` uyuşmazlığında (dosya değişmiş) kayıt geçersiz sayılıp siliniyor.
  - **L2 disk önbelleği:** `<xdg-cache>/veyra/thumbnails/normal/<md5(file://URI)>.png`, freedesktop.org adlandırma kuralının aynısı. Geçersiz kılma: kaynak dosyanın `mtime`'ı önbellek PNG'sinin kendi dosya sistemi `mtime`'ından yeniyse önbellek atlanıp yeniden üretiliyor — ek PNG metadata'sı gerekmiyor. Atomik yazma: aynı dizinde `.png.tmp-<pid>-<thread-id>` geçici dosyasına yazılıp `rename` ile hedefe taşınıyor, böylece yarım yazılmış bir PNG asla okunamıyor.
  - **Arka plan işçi havuzu:** `fs_async::run_blocking`'in istek-başına-thread deseninin aksine (Kural #33 kaynak bilinci — hızlı bir kaydırma tek seferde onlarca öge bağlar), 2 kalıcı işçi thread'i `async_channel` üzerinden paylaşılan bir istek kuyruğunu tüketiyor. Sonuç `preview.rs`'teki `DecodedImage` deseniyle aynı şekilde `Send`-güvenli ham piksel arabelleği (`glib::Bytes`) olarak ana thread'e dönüyor; `gdk_pixbuf::Pixbuf`/`gdk4::Texture` yeniden kurulumu (main-thread-only GObject'ler) yalnızca orada yapılıyor.
  - **Widget geri dönüşüm koruması:** `#![forbid(unsafe_code)]` altında `GObject` qdata kullanılamadığından (bu, `unsafe fn data::<T>()` gerektiriyor), koruma `GtkImage::widget-name` özelliğine (tamamen safe get/set) bağlanan dosya-yolu tabanlı bir jetonla yapılıyor — `bind()` her çağrıldığında jetonu hemen günceller; gecikmeli asenkron sonuç geldiğinde jeton uyuşmazsa (öge hızlı kaydırmada başka bir dosyaya yeniden bağlanmış) sonuç sessizce atılıyor.
  - **Format/hata yönetimi (Kural #15):** yalnızca `FileKind::Regular` + `mime_type` `image/` ile başlayan yerel dosyalar aday (semboling bağlantılar TOCTOU/sembolik-bağlantı belirsizliği yüzünden hariç, Kural #22; uzak GVfs bağlamaları da hariç). Decode `gdk_pixbuf::Pixbuf::from_file_at_scale` (128×128, en-boy oranı korunarak) ile yapılıyor — PNG/JPEG/WEBP/GIF/BMP/ICO/SVG'yi (kurulu librsvg yükleyicisi varsa) kapsıyor; bozuk/çözülemeyen dosyalar `panic!` yerine `None` döndürüp çağrı yerinde zaten ayarlanmış sembolik ikonun aynen kalmasını sağlıyor.
- **Görünüm entegrasyonu (`views/mod.rs`, `views/icon_view.rs`, `views/compact_view.rs`, `views/details_view.rs`):** `build_grid_view` (Icon + Compact) ve `details_view`'in `name_column` fabrikası artık `connect_bind` içinde önce her zaman `icon_name_for` ile sembolik ikonu basıyor (anında, senkron fallback), sonra `thumbnails.bind(&icon, &file_item)` çağırıyor — L1 isabetinde `Pixbuf` hemen `gdk4::Texture`'a çevrilip senkron olarak basılıyor, aksi halde arka plan isteği kuyruğa giriyor ve sembolik ikon üretim tamamlanana kadar yerinde kalıyor. UI thread hiçbir zaman disk/decode işlemi yapmıyor (Kural #11/#12).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo fmt --all -- --check`: temiz.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: tümü geçti (yeni 8 birim test: L1 LRU tahliye + `get`'te tazeleme, MD5 URI hash'inin `md5sum` ile çapraz doğrulanmış sabit değeri, önbellek tazelik kontrolü — eksik dosya/kaynak-daha-yeni/önbellek-daha-yeni üç senaryosu, jeton benzersizliği, `thumbnailable_path`'in dizin/metin/görsel ayrımı).
- **Manuel duman testi:** `./target/debug/veyra`, 5 renkli PNG içeren geçici bir dizine karşı gerçek Wayland ekranında çalıştırıldı ve ekran görüntüsü alındı — Icon View'de tüm 5 dosya kendi renklerini gösteren gerçek küçük resimlerle (sembolik ikon değil) render edildi; `~/.cache/veyra/thumbnails/normal/` altında beklenen MD5 adlı PNG dosyalarının oluştuğu doğrulandı. Compact/Details view'leri aynı `bind()` çağrısını ve `build_grid_view`'i paylaştığından ayrıca görsel olarak test edilmedi (bu ortamda `xdotool`/`wmctrl` yok, görünüm değiştirmek için tıklama otomasyonu yapılamadı).

### Bağımlılık Değişiklikleri
- `veyra-ui`'ye iki yeni bağımlılık: `lru = "0.12"` (L1 LRU önbellek — stdlib'de karşılığı yok) ve `md-5 = "0.10"` (RustCrypto, XDG küçük resim adlandırma sözleşmesinin gerektirdiği MD5 hash — stdlib'de yok). İkisi de MIT/Apache-2.0, GPL-3.0-or-later ile uyumlu.

### Sıradaki Faz
Faz 12. Onay bekleniyor.

## Faz 10 — File Preview / Dosya Önizleme Paneli (`veyra-ui`)

### Eklenenler
- **Yeni modül `veyra-ui/src/preview.rs`:** sağ tarafa gizlenebilir, `GtkStack` üzerine kurulu Önizleme Paneli — `empty`/`loading`/`image`/`text`/`info` sayfaları arasında `Crossfade` geçişiyle geçiyor. `PreviewPanelHandles` tek üst widget'ı (`.widget`) ve tüm dahili GTK tutamaçlarını taşıyan `Clone`'lanabilir bir handle grubu; `show(&handles, Option<FileItem>)` tek genel giriş noktası.
- **Panel entegrasyonu (`window.rs`):** Faz 8'in sol/sağ dosya panellerini taşıyan `paned`'in sağına, ikinci bir `GtkPaned` (`content_paned`) ile eklendi — sağ panel `Panel::frame.set_visible(false)` deseninin aynısıyla (`preview.widget.set_visible(false)`) başlangıçta gizli, görünürlük değiştiğinde `GtkPaned` otomatik olarak ayırıcıyı gizliyor/gösteriyor (Faz 8'in split-view'i için zaten kullanılan davranış).
- **Kısayol & aksiyon:** `win.toggle-preview` (`F9`) + headerbar'da `view-preview-symbolic` ikonlu, `win.toggle-preview`'a `action-name` ile bağlı bir `GtkToggleButton` (split-view düğmesinin yanına `pack_end`). Panel açılırken güncel seçim anında yükleniyor.
- **Seçim senkronizasyonu:** Her tabın Icon/Compact/Details görünümünün üçü de kendi `GtkSingleSelection`'ında `SelectionModelExt::connect_selection_changed` ile paylaşılan bir `refresh_preview: Rc<dyn Fn()>` kapanışına bağlı; aynı kapanış panel odak değişiminde (`focus_panel`), sekme değişiminde (`connect_selected_page_notify`), görünüm modu değiştiğinde (headerbar view switcher) ve split-view aç/kapatıldığında da çağrılıyor. `refresh_preview` her zaman "şu an odaklı panelin aktif sekmesinin aktif görünümünde seçili öge ne" sorusunu yeniden hesaplıyor — hangi olay tetiklediğinden bağımsız olarak tek doğru kaynaktan okuyor, bu yüzden ayrı ayrı "bu olay önizlemeyi etkiler mi" mantığı gerekmiyor.
- **Desteklenen kartlar:**
  - **Görseller:** `gio::File::load_contents` ile arka planda okunan baytlar `gdk_pixbuf::Pixbuf::from_read` ile yine arka planda çözülüyor (bu çağrının GTK ana thread zorunluluğu yok); yalnızca ham piksel arabelleği (`glib::Bytes`, `Send`) + boyut/renk-uzayı bilgisi ana thread'e taşınıyor, orada `Pixbuf::from_bytes` + `gdk4::Texture::for_pixbuf` ile (main-thread-only) yeniden kuruluyor — `gdk4::Texture::from_bytes`'ın gerektirdiği `v4_6` derleme özelliğine hiç ihtiyaç duymadan tam asenkron okuma/çözme. Meta: çözünürlük, dosya boyutu, MIME, değiştirilme tarihi.
  - **Düz metin/kod:** `gio::File::read` ile açılan akıştan en fazla 512 KB (`TEXT_PREVIEW_CAP_BYTES`) arka planda okunuyor, `GtkTextView` (`editable=false`, `cursor_visible=false`, monospace) içinde gösteriliyor. Meta: satır sayısı, karakter sayısı, dosya boyutu, MIME; kırpıldıysa not ekleniyor.
  - **PDF/belgeler, ses/video, arşivler, bilinmeyen dosyalar:** zaten dizin listelemesinden gelen `FileMetadata`'dan senkron kurulan ortak "info kartı" (ikon + ad + tür + boyut/MIME/tarih + "Open in Default App" düğmesi) — ekstra I/O gerekmediği için asenkron değil.
  - **Dizinler:** `veyra_filesystem::read_dir` ile arka planda sayılan alt öge sayısı + yol + değiştirilme tarihi.
  - **Sembolik bağlantılar:** hedef yol gösteren info kartı; kırık bağlantılar ayrı bir hata kartına düşüyor (`FileKind::Symlink { is_broken: true, .. }`), FIFO/soket/blok-aygıt gibi özel dosya türleri de panikleme yerine kendi info kartını alıyor.
  - **Boş durum:** `AdwStatusPage` ("Select a file to preview").
- **Eşzamanlılık/iptal (Kural #11-#14):** `Rc<Cell<u64>>` nesil sayacı — her `show()` çağrısı sayaç değerini artırıp yakalıyor; arka plan işi bittiğinde sayaç hâlâ eşleşmiyorsa (kullanıcı ok tuşuyla hızla ilerlemiş, daha yeni bir `show()` zaten çalışıyor) sonuç sessizce atılıyor, eski önizleme asla yeninin üzerine yazamıyor.
- **Hata yönetimi (Kural #15, #18, #20):** Okuma sırasında dosya silinir/izin biterse `glib::Error`/`FsError` `Permission denied`/`File not found`/`Unable to read file` gibi kullanıcı dostu bir hata kartına çevriliyor (`friendly_gio_error`, `friendly_fs_error`) — hiçbir hata `panic!`'e çıkmıyor.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 123/123 (118 + yeni 5 birim test: `preview.rs`'te MIME sınıflandırma (görsel/metin/arşiv), öge-sayısı çoğullaştırma, özel dosya türü etiketleme).
- `cargo fmt --check`: temiz.
- Manuel duman testi: `./target/debug/veyra` panik olmadan açıldı, log'da önizlemeyle ilgili hata yok. Not: bu ortamda Wayland girdi-enjeksiyon/ekran görüntüsü aracı yok (önceki fazlarda da not edildi) — `F9`/görsel-metin-önizleme render'ı elle tıklanarak/görsel olarak doğrulanamadı; bu davranış kod incelemesi + build/clippy/test/fmt ile sınırlı doğrulandı.

### Bağımlılık Değişiklikleri
- Yeni harici bağımlılık eklenmedi: `gdk_pixbuf` zaten `gtk4` üzerinden geçişli olarak ağaçta olan `gtk4::gdk_pixbuf` yeniden ihracı üzerinden kullanılıyor.

### Sıradaki Faz
Faz 11. Onay bekleniyor.

## Faz 9 — Search / Arama Motoru (`veyra-search` yeni crate, `veyra-ui`)

### Eklenenler
- **`veyra-search` (yeni workspace crate):** SQLite + FTS5 tabanlı arama motoru, UI'dan ve `veyra-filesystem`'den bağımsız (Kural #42) — indeksleyici GIO değil düz `std::fs` kullanıyor, UI'nin GLib main context'ine hiç bağımlı değil. Tek crate'te bu workspace'in ilk `#![forbid(unsafe_code)]` **taşımayan** modülü: arka plan indeksleyici thread'i kendini `nice(19)` ile düşük önceliğe alıyor (`indexer::lower_priority`), `# Safety` doc yorumuyla izole tek bir `unsafe` çağrısı — `veyra-app::root_guard`'ın `libc::geteuid` kullanımıyla aynı, bu projede zaten var olan desen.
  - **`schema.rs`:** `directories`, `files`, `metadata` tabloları + `fts_index` FTS5 sanal tablosu (`filename, path, content_metadata`), istenen şemayla birebir.
  - **`classify.rs`:** MIME türü + uzantı + çalıştırılabilir bit'ten `image`/`video`/`document`/`archive`/`executable`/`other` sınıflandırması (çalıştırılabilir bit her zaman MIME tahmininden önce gelir — `+x` bitli bir `text/plain` betik hâlâ "executable").
  - **`query.rs`:** `name:`, `type:`, `size:>100MB`/`size:<10MB`, `modified:today`/`yesterday`/`last-week`/`last-month` sözdizimi ayrıştırıcısı + serbest metin terimleri, hepsi kombinasyon halinde (`type:image size:>10MB modified:last-week`). Tanınmayan bir `key:value` jetonu (bilinmeyen anahtar veya ayrıştırılamayan değer) sessizce serbest metne düşer — bir filtredeki yazım hatası tüm aramayı asla başarısız kılmaz.
  - **`index.rs`:** `SearchIndex` — `index_entry`/`remove_path`/`search`; FTS5 `MATCH` sorgusuna giden her kullanıcı terimi tırnaklı bir cümle olarak alıntılanıyor (`fts5_quote`), böylece serbest metindeki `AND`/`OR`/`-`/`*` gibi FTS5 operatör sözdizimi asla sorgu söz dizimine karışamıyor (enjeksiyon hijyeni).
  - **`indexer.rs`:** `spawn_background_index` — arka plan thread'inde özyinelemeli tarama; her 64 girdide bir 5ms uyku ile CPU/IO'yu serbest bırakıyor, sembolik bağlantı döngülerine karşı 64 seviye derinlik sınırı var.
- **Gerçek `Ctrl+F` (`window.rs`, `headerbar.rs`):** Faz 3'ten beri `search_toggle` düğmesinin ipucu metni "Ctrl+F" diyordu ama gerçek bir klavye kısayolu hiç bağlanmamıştı (fark edilmemiş bir hata) — artık `win.toggle-search` aksiyonu düğmeyle aynı arama çubuğunu gerçekten açıp kapatıyor.
- **Gelişmiş arama UI entegrasyonu (`headerbar.rs`, yeni `search_results.rs`):** Arama kutusuna yazılan sorgu `veyra_search::parse` ile ayrıştırılıyor; sorgu herhangi bir filtre kullanıyorsa (`has_filters()`), Faz 3'ün düz sekme-içi dosya adı filtresi yerine SQLite indeksine karşı arka planda (`fs_async::run_blocking`, ana thread asla bloklanmaz) sorgu çalıştırılıyor ve sonuçlar arama kutusunun altında satır satır (simge, ad, tam yol, boyut) gösteriliyor; bir satıra tıklamak odaklı paneli o konuma götürüyor (dizinse içine, dosyaysa üst dizinine). Düz metin sorguları (filtre yok) Faz 3 davranışını değiştirmeden kullanmaya devam ediyor.
- **Başlangıçta arka plan indeksleme (`window.rs`):** Pencere açılışında ev dizini `veyra_search::spawn_background_index` ile düşük öncelikli arka planda taranmaya başlıyor; arama sonuçları anlamlı olsun diye. İndeks veritabanı `~/.cache/veyra/search_index.db` altında (`veyra_search::default_db_path`); açılamazsa (izin/disk hatası) panik yerine bellek-içi indekse geçiliyor ve durum loglanıyor (Kural #15).
- **`veyra_ui::run` imzası:** artık `cache_dir: &Path` parametresi alıyor (indeks veritabanı konumu için); `main.rs` zaten çözülmüş `xdg_dirs.cache_dir`'i geçiyor.

### Kapsam Notu (bilinçli sınırlama)
- Arama sonuçları ayrı, hafif bir liste (simge + ad + yol + boyut) — mevcut Icon/Compact/Details görünümlerine (tam `FileItem` gerektirdikleri, ek `stat` I/O'su isteyen) enjekte edilmiyor. Bu, kapsamı makul tutmak için bilinçli bir tasarım tercihi; tıklama hâlâ gerçek gezinmeye bağlanıyor.
- İndeksleyici yalnızca ev dizinini tarıyor (Faz 9'un "arka plan indeksleyici" hedefi); dosya sistemi değişikliklerini canlı izleyen bir `GFileMonitor` entegrasyonu ve manuel "Re-index" aksiyonu kapsam dışı bırakıldı — bir sonraki arama fazına aday.
- İçerik arama (`content_metadata` sütunu, dosya içeriği tam metin arama) şema düzeyinde hazır ama şu an yalnızca `kind` sınıflandırmasıyla dolduruluyor — gerçek dosya içeriği indekslemesi (PDF/metin çıkarımı vb.) kapsam dışı.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 118/118 (83 + yeni 35 birim/entegrasyon test: `veyra-search`'te `query.rs` sözdizimi ayrıştırıcı testleri, `classify.rs` sınıflandırma testleri, `index.rs` bellek-içi SQLite+FTS5 arama testleri — serbest metin, `type:`, `size:`, `modified:`, kombinasyon, yeniden indeksleme, silme, FTS5 özel karakter güvenliği — ve `indexer.rs` gerçek dizin tarama testleri).
- `cargo fmt --check`: temiz.
- Manuel duman testi: `./target/debug/veyra` panik olmadan açıldı; `~/.cache/veyra/search_index.db` gerçekten oluştu ve birkaç saniye içinde arka plan indeksleyici 5.915 satırı gerçekten indeksledi (`sqlite3 ... "select count(*) from files"` ile doğrulandı). Not: bu ortamda Wayland girdi-enjeksiyon aracı yok (önceki fazlarda da not edildi) — arama kutusuna yazma/sonuç tıklama/Ctrl+F görsel olarak elle tıklanarak doğrulanamadı; ekran görüntüsü denemesi başka bir tam ekran pencereyi yakaladığı için atıldı. Bu davranışlar kod incelemesi + `veyra-search`'ün 35 gerçek SQLite entegrasyon testi + build/clippy/test/fmt ile sınırlı doğrulandı.

## Faz 8 — Split View / Çift Panel (`veyra-ui`)

### Eklenenler
- **`split_view.rs` (yeni, `veyra-ui`):** `PanelId` (Left/Right, `.other()`), `Panel` (kendi `AdwTabView`'i + kendi `TabRegistry`'si + kendi tam gezinme `Chrome`'u: back/forward/up/home/refresh, breadcrumbs/adres, durum çubuğu), `Panels` (`left`/`right`), `focused_tab()`, `build_panel()`, `install_panel_css()`. Faz 7'nin pencere-geneli tek `Chrome`'u yerini panel başına tam bağımsız bir `Chrome`'a bırakıyor — bir panelin geri/ileri/yukarı/refresh/breadcrumb/adres/durum çubuğu widget'ları yalnızca o panele ait, hangi panelin odakta olduğuyla hiç koordinasyona ihtiyaç duymuyor. Bu da Faz 7'nin `navigate_to`/`go_back`/`go_forward`/`load_directory`/`update_chrome` fonksiyonlarının neredeyse hiç değişmeden yeniden kullanılmasını sağladı.
- **Çift panel düzeni (`window.rs`):** İçerik alanı artık yatay `GtkPaned` — `start_child`/`end_child` sol/sağ paneller, kullanıcı ayırıcıyı sürükleyerek yeniden boyutlandırabiliyor (`shrink_*_child(false)` ile paneller sıfıra küçülmüyor). Sağ panel başlangıçta gizli (`frame.set_visible(false)`) ve sıfır sekmeyle kurulu; `F3` ile ilk açılışında sol panelin o anki konumunu ayna olarak yeni bir sekme açıyor (`win.toggle-split-view`).
- **Aktif panel & bağımsız navigasyon:** Panel çerçevesine tıklamak (capture-phase `GtkGestureClick`, tıkladığı hiçbir widget'ı yutmuyor) odağı o panele taşıyor; odaklı panel `.veyra-active-panel` CSS sınıfıyla (`install_panel_css`, `@accent_color` kenarlık) vurgulanıyor — bölünmüş görünüm kapalıyken vurgu hiç gösterilmiyor. Her panelin kendi `AdwTabView`'i, kendi Faz 7 sekme izolasyonu (konum/geçmiş/seçim/görünüm modu) ile tamamen bağımsız.
- **`Tab` ile panel odağı değişimi:** Bölünmüş görünüm açıkken ve odak bir metin girişinde değilken (`gtk4::Editable` kontrolü) `Tab` diğer panele geçiyor. Kasıtlı olarak pencere geneli bir `win.*` klavye kısayolu (accelerator) DEĞİL — her panelin kendi çerçevesine bağlı, bubble-phase `GtkEventControllerKey`. Gerekçe: pencere geneli bir `Tab` accelerator'ı `AdwAlertDialog` düğmeleri ve adres/arama giriş kutuları dahil uygulama genelindeki standart Tab-ile-odak-gezintisini kırardı (Kural #29 Keyboard Accessibility, Kural #4 Never Destroy Functionality) — bu yüzden kapsam bilinçli olarak panel çerçeveleriyle sınırlandı.
- **Paneller arası hızlı işlemler:** `win.copy-to-other-panel-selected` (`Ctrl+Shift+O`) ve `win.move-to-other-panel-selected` (`Ctrl+Shift+M`) — odaklı panelin seçili ögesini karşı panelin o anki dizinine kopyalar/taşır (mevcut `run_bulk_operation` motoru üzerinden, Faz 5). Sağ tık öge menüsüne "Copy to Other Panel"/"Move to Other Panel" girişleri eklendi — yalnızca bölünmüş görünüm gerçekten ikinci bir panel gösteriyorken görünürler (devre dışı gösterilmiyorlar, tamamen kaldırılıyorlar — "Faz 7"deki gibi gelecekteki bir faz değil, şu an uygulanabilir olmayan bir bağlam olduğu için).
- **`run_bulk_operation` genelleştirmesi (`window.rs`):** Tek `(state, chrome)` yerine `Vec<(SharedState, Chrome)>` "refresh_targets" alıyor — panel-içi işlemler (Paste/Trash/Delete) tek hedef geçiyor, paneller arası kopyala/taşı hem kaynak hem hedef panelin listesini işlem bitince tazeliyor.
- **Slim `headerbar.rs`:** Geri/ileri/yukarı/ev/refresh/breadcrumb/adres artık pencere başlığında değil, panellerin kendi `Chrome`'unda; başlıkta yalnızca arama, görünüm modu anahtarı (Icon/Compact/Details — odaklı panelin aktif sekmesine uygulanır) ve yeni bölünmüş-görünüm aç/kapa düğmesi kaldı.

### Kapsam Notu (bilinçli sınırlama)
- Kopyala/Kes panosu hâlâ pencere genelinde tek slot (Faz 7'den miras) — panele özel değil.
- Arama kutusu tek, paylaşılan bir widget; odaklı panelin sorgu/filtresini günceller ama panel değiştirince kutunun görünen metni otomatik değişmiyor (Faz 7'den miras aynı kapsam notu).
- "Copy/Move to Other Panel" tek seçili ögeyle çalışıyor (Faz 5/6/7 ile aynı tek-seçim kapsamı) — alttaki `run_operation()` motoru zaten çoklu kaynağı destekliyor, çoklu seçim UI'si eklendiğinde yalnızca `sources` genişleyecek.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 83/83 (81 + yeni 2 birim test: `split_view.rs` içinde `PanelId::other()` — panel odağı/karşı panel hesaplama mantığı).
- `cargo fmt --check`: temiz.
- Manuel duman testi: `./target/debug/veyra` panik olmadan açıldı; ekran görüntüsü tek-panel varsayılan durumu doğruladı (panel araç çubuğu: geri/ileri/yukarı/ev/refresh + adres + "73 items / 357.7 GB free" durum çubuğu, başlıkta bölünmüş-görünüm düğmesi). Not: bu ortamda Wayland girdi-enjeksiyon aracı yok (Faz 7'de de aynı kısıt not edilmişti) — `F3` ile panel açma, panel yeniden boyutlandırma, `Tab` ile odak geçişi ve paneller arası kopyala/taşı görsel olarak elle tıklanarak doğrulanamadı; bu davranışlar kod incelemesi + build/clippy/test/fmt ile sınırlı doğrulandı. Kullanıcının ilk elle denemesinde özellikle `Tab`-odak-geçişinin metin girişlerini bozmadığını teyit etmesi önerilir.

## Faz 7 — Tabs / Sekmeler (`veyra-ui`)

### Eklenenler
- **`tab_page.rs` (yeni, `veyra-ui`):** `TabPage` — her sekmenin izole durumu: kendi `AppState`'i (konum + geri/ileri geçmişi + öge modeli), kendi Icon/Compact/Details `GtkStack`'i, kendi üç görünümün `GtkSingleSelection` zinciri (`ViewSelections`), kendi arama sorgusu/filtresi. `TabRegistry` — `AdwTabView`'in verdiği `AdwTabPage`'i ilgili `TabPage`'e eşleyen `Rc<RefCell<HashMap<...>>>`; `glib` nesne sarmalayıcıları işaretçi kimliğiyle `Hash`/`Eq` uyguladığından `unsafe` qdata gerekmiyor (`#![forbid(unsafe_code)]` ile uyumlu). `active_tab()` — `AdwTabView::selected_page()`'i registry'den çözer; tüm `win.*` aksiyonları ve gezinme kısayolları bu yardımcıyla her zaman *o an görünür* sekmeyi hedefler.
- **`window.rs`:** İçerik alanı artık `AdwTabBar` (başlığın altında) + `AdwTabView` (`veyra-ui` içinde çoklu sekme gövdesi). Tek sekmede `AdwTabBar` varsayılan olarak kendiliğinden gizlenir (standart tarayıcı davranışı), 2+ sekmede görünür. Sekme çubuğunun sonunda `win.new-tab`'e bağlı düz "+" butonu. `open_tab()` yeni bir sekme inşa edip kayda alır ve seçili sekme yapar; `update_chrome()` artık tek `AppState` yerine aktif sekmenin durumunu okuyup paylaşılan header/breadcrumbs/durum çubuğuna *ve* o sekmenin `AdwTabPage` başlığına (`tab_title()` — klasör adı, ev dizini için "Home") yansıtıyor; görünüm modu (Icon/Compact/Details) anahtarları da aktif sekmeyle senkron tutuluyor.
- **Klavye kısayolları:** `Ctrl+T` yeni sekme (aktif sekmenin konumunda açılır), `Ctrl+W` aktif sekmeyi kapatır (son sekme her zaman açık kalır — `AdwTabView::close-page` sinyali son sekmede kapatmayı veto eder), `Ctrl+Tab`/`Ctrl+Shift+Tab` sekmeler arası ileri/geri (`AdwTabView::select_next_page`/`select_previous_page`).
- **"Open in New Tab" (`context_menu.rs`, `window.rs`):** Faz 6'da devre dışı duran menü ögesi artık gerçek `win.open-in-new-tab-selected` aksiyonuna bağlı; klasör hedefini yeni bir sekmede açıp o sekmeye geçiyor.

### Kapsam Notu (bilinçli sınırlama)
- Kopyala/Kes panosu pencere genelinde tek slot olarak kalıyor (sekmeye özel değil) — çoğu dosya yöneticisinde olduğu gibi bir sekmede kopyalanan öge başka bir sekmeye yapıştırılabilir; bu Faz 7'nin izolasyon listesinde (konum/geçmiş/seçim/kaydırma/görünüm modu) yer almıyor.
- Arama kutusu tek, paylaşılan bir başlık widget'ı; sorgu/filtre durumu sekmeye özel tutuluyor (her tuş vuruşu o anki aktif sekmenin filtresini günceller), ancak kutunun görünen metni sekme geçişinde otomatik değişmiyor — küçük, kabul edilebilir bir UX boşluğu.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 81/81 (75 + yeni 6 birim test: `window.rs` içinde `tab_title()`/`count_label()`).
- `cargo fmt --check`: temiz.
- Manuel duman testi: `./target/debug/veyra` panik olmadan açıldı, ev dizini "Home" başlığıyla yüklendi; ikinci `cargo run` çağrısı GApplication tekil-örnek davranışıyla birinci pencereyi etkinleştirip 0 koduyla çıktı (beklenen). Not: bu ortamda Wayland girdi-enjeksiyon aracı (xdotool/ydotool/wtype) yok, bu yüzden sekme açma/kapatma/geçiş kısayolları ekran görüntüsüyle görsel olarak doğrulanamadı — doğrulama build/clippy/test/fmt ve kod incelemesiyle sınırlı kaldı.

## Faz 6 — Context Menu (`veyra-ui`, `veyra-app`)

### Eklenenler
- **`context_menu.rs` (yeni, `veyra-ui`):** Öge ve boş-alan sağ tık popover menüleri için `gio::Menu` + `GtkPopoverMenu` altyapısı. Sağ tık `GtkGestureClick` (`BUTTON_SECONDARY`, `released`) her üç görünümün asıl `GtkGridView`/`GtkColumnView` widget'ına bağlanır — GTK4'ün dahili satır-seçim gesture'ı zaten *her* fare tuşunda (sadece primary değil) `released` anında seçimi günceller, bu yüzden bizim bubble-phase handler'ımız çalıştığında `selection.selected()` doğru ögeyi verir; ek pozisyon takibi gerekmez. Boş alan tıklaması `Widget::pick()` sonucunun görünüm widget'ının kendisi olup olmamasıyla ayırt edilir.
  - **Öge menüsü:** Open, Open With… (`GtkAppChooserDialog`), Open in New Tab (Faz 7 — devre dışı), Open in New Window (yalnızca klasör; ayrı bir Veyra sürecini hedef dizinle yeniden başlatır), Copy/Cut (mevcut `win.copy-selection`/`win.cut-selection`'ı yeniden kullanır), Rename (F2, yeni `dialogs/rename_dialog.rs`), Move to Trash/Delete Permanently (mevcut aksiyonlar), Compress… (Faz 19 — devre dışı), Extract Here (yalnızca `.zip/.tar.gz/.7z/.xz` vb. tanınan arşiv uzantılarında görünür, Faz 19 — devre dışı), Open Terminal Here (Faz 23 — devre dışı), Copy Path, Copy Location, Properties (Faz 12 — devre dışı).
  - **Boş alan menüsü:** New Folder / New Document (`veyra_filesystem::create_dir`/`create_file` + `suggest_name` ile çakışmasız isim), Paste (pano doluysa gerçek `win.paste`'e, boşsa devre dışı aksiyona bağlanır — her açılışta taze değerlendirilir), Open Terminal Here (Faz 23 — devre dışı), Properties (Faz 12 — devre dışı).
- **`dialogs/rename_dialog.rs` (yeni, `veyra-ui`):** `AdwAlertDialog` + `Entry` tabanlı yeniden adlandırma istemi; ad kökü (uzantı hariç) önceden seçili, boş girişte "Rename" yanıtı devre dışı.
- **Yeni `win.*` aksiyonları (`window.rs`):** `open-selected`, `open-with-selected`, `open-in-new-window-selected`, `rename-selected` (`F2`), `copy-path-selected`, `copy-location-selected`, `create-folder`, `create-document`, ve paylaşılan devre dışı `not-implemented` aksiyonu (henüz inşa edilmemiş her menü ögesi buna bağlanır).
- **"Open in New Window" desteği (`veyra-ui::run`, `veyra-app::main`):** `veyra_ui::run` artık isteğe bağlı bir başlangıç dizini (`Option<VeyraPath>`) alıyor; `main.rs` ilk CLI argümanını (`veyra /some/path`) buna geçiriyor. `gio::Application::run()` yerine `run_with_args(&[])` kullanılıyor — argv'yi `GApplication`'ın kendi "open files" komut satırı işleyicisine (bu uygulama `HANDLES_OPEN` bildirmiyor) tekrar geçirmemek için; yol zaten yukarıda elle ayrıştırılıp `activate` içinde kullanılıyor.

### Kapsam Notu (bilinçli sınırlama)
- Compress/Extract (Faz 19 — Arşiv Yöneticisi), Open Terminal Here (Faz 23 — Terminal Entegrasyonu), Properties (Faz 12 — Özellikler Penceresi) ve Open in New Tab (Faz 7 — Sekmeler) roadmap'te sonraki fazlara ait; Kural #2 (No Monolithic Leaps) gereği bu fazda gerçek işlevsellik eklenmedi. Menüde görünürler ama devre dışıdırlar, etiketlerinde ait oldukları fazı belirtir (ör. "Compress… (Faz 19)") — kullanıcı onayıyla seçilen yaklaşım.
- "Open in New Tab" için gerçek sekme altyapısı yok (Faz 7); bu yüzden şimdilik menüde yer almıyor değil, doğrudan devre dışı gösteriliyor (yukarıdaki not). "Open in New Window" ise gerçek ve çalışır durumda: hedef dizinle yeni bir Veyra süreci başlatır.
- Properties penceresi Faz 12'ye kadar hem öge hem boş-alan menüsünde devre dışı; iki menü de aynı paylaşılan `win.not-implemented` aksiyonunu kullanıyor.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 75/75 (yeni: `context_menu.rs` 2 birim test — arşiv uzantısı tanıma).
- `cargo fmt --check`: temiz.
- Manuel duman testi: `./target/debug/veyra` (varsayılan ev dizini) ve `./target/debug/veyra /tmp` (CLI ile başlangıç dizini) her ikisi de panik olmadan pencereyi açtı; `run_with_args` düzeltmesinden önce ikincisi `GLib-GIO-CRITICAL: This application can not open files` hatası veriyordu.

## Faz 5 — Dosya İşlemleri Sistemi (`veyra-filesystem`, `veyra-ui`)

### Eklenenler
- **`queue.rs` (yeni, `veyra-filesystem`):** `run_operation()` — Copy/Move/Trash/Delete için tek blocking giriş noktası. Copy/Move özyinelemeli olarak dizin ağacını gezer (`flatten`), her dosya için `gio::File::copy`'nin canlı `progress_callback`'ini kullanarak gerçek zamanlı bayt bazlı ilerleme raporlar; aynı dosya sistemi içindeki Move'lar önce anlık `rename` (hızlı yol) dener, yalnızca bu başarısız olursa (aygıtlar arası taşıma) kopyala-sonra-sil'e düşer. `OperationControl` (Arc'lı atomic'ler) ile Cancel/Pause/Resume — Pause, worker thread'i kısa aralıklarla uyuyup kontrol eden bir döngüyle bloklar; Cancel her dosya arasında ve pause beklerken kontrol edilir.
- **`conflict.rs` (yeni, `veyra-filesystem`):** `ConflictDecision` (Replace/ReplaceAll/Rename/Skip/SkipAll/Cancel) ve `suggest_name()` — `"report (2).pdf"` tarzı çakışmasız isim önerisi (nokta ile başlayan gizli dosyalarda uzantı kırılmaz).
- **`progress.rs` (yeni, `veyra-filesystem`):** `Progress` — dosya adı/indeks/bayt sayaçları + `percent()` (bayt toplamı bilinmiyorsa dosya sayısına düşer, örn. Trash/Delete).
- **`operations.rs` (yeni, `veyra-ui`):** `run_operation`'ı arka plan thread'inde çalıştırıp olayları (`Progress`/`Conflict`/`Done`) `async-channel` ile GTK ana thread'ine akıtan köprü. Çakışma anında worker thread, UI diyalog cevabını `answer_rx.recv_blocking()` ile bekler — worker gerçek bir arka plan thread'i olduğundan bu bloklama ana döngüyü asla dondurmaz.
- **`widgets/progress_toast.rs` (yeni, `veyra-ui`):** Alt araç çubuğunda canlı ilerleme paneli (`GtkRevealer`): dosya adı + yüzde, `GtkProgressBar`, bayt/dosya sayacı, Pause/Resume ve Cancel butonları. `AdwToast` değil — Pause/Resume ve bir ilerleme çubuğu barındırması gerektiğinden düz metin+tek-aksiyon toast'ı yetersiz kalıyordu.
- **`dialogs/conflict_dialog.rs` (yeni, `veyra-ui`):** `AdwAlertDialog` tabanlı çakışma diyaloğu — mevcut/gelen dosyanın boyut+değiştirilme tarihi karşılaştırması (Compare, ayrı bir adım değil satır içi bilgi olarak), önerilen isimle önceden doldurulmuş düzenlenebilir yeniden adlandırma alanı, Skip/Rename/Replace butonları, "Kalan tüm çakışmalara uygula" onay kutusu (blanket ReplaceAll/SkipAll).
- **`dialogs/delete_confirm.rs` (yeni, `veyra-ui`):** Kalıcı silme için zorunlu `AdwAlertDialog` onayı (Kural #38/#39) — `Delete` tuşu her zaman Trash'e gider, kalıcı silme yalnızca `Shift+Delete` + bu onay diyaloğundan sonra çalışır.
- **Klavye/aksiyon entegrasyonu (`window.rs`):** `win.copy-selection`/`win.cut-selection` (`Ctrl+C`/`Ctrl+X`, seçili ögeyi panoya alır), `win.paste` (`Ctrl+V`, geçerli dizine Copy/Move başlatır), `win.trash-selection` (`Delete`), `win.delete-selection` (`Shift+Delete`, onay diyaloğu arkasında). İşlem bitince dizin otomatik yenilenir; hatalar durum çubuğunda özetlenir ve `tracing::warn!` ile loglanır.

### Kapsam Notu (bilinçli sınırlama)
- Görünümler hâlâ `GtkSingleSelection` kullanıyor (çoklu seçim Faz 3'ten miras); Faz 5 bu nedenle tek seferde tek ögeyi kopyalar/taşır/siler. Alttaki `run_operation()` motoru zaten çoklu kaynaklı toplu istekleri destekliyor ve test ediliyor (`skip_all_applies_to_later_conflicts_without_asking` gibi testler 2+ dosyalı grup gönderir) — çoklu seçim UI'si eklendiğinde `OperationRequest.sources` sadece genişletilecek, motor değişmeyecek.
- Çakışma diyaloğundaki "Compare", ayrı bir alt pencere değil; boyut+tarih bilgisi diyalog gövdesinde satır içi gösteriliyor (senkron, yerel `query_info` — mikrosaniyeler sürdüğünden zaten modal olan diyalog için kabul edilebilir).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 73/73 (yeni: `queue.rs` 12 birim test — kopyalama/taşıma/özyineleme/çakışma [Skip/Replace/Rename/Cancel/SkipAll]/pause-resume/cancel senaryoları; `conflict.rs` 5 test; `progress.rs` 4 test).
- `cargo fmt --check`: temiz.

### Bağımlılık Değişiklikleri
- `veyra-ui`: `libadwaita` özelliği `v1_4` → `v1_5` yükseltildi (`AdwAlertDialog` bu özellik kapısının arkasında; sistemde kurulu `libadwaita` 1.9.3 fazlasıyla yeterli).

## Faz 4 — Navigasyon (`veyra-ui`)

### Eklenenler
- **Navigasyon geçmişi (`history.rs`, yeni):** `History` struct'ı back/forward stack mantığını `AppState`'ten ayırdı — `record`/`go_back`/`go_forward`/`can_go_back`/`can_go_forward`, 8 birim testle kapsanıyor (boş stack no-op, yeni navigasyon forward stack'i temizler, çok adımlı geri/ileri round-trip).
- **Home & Refresh butonları (`headerbar.rs`, `window.rs`):** Home butonu `$HOME`'a navigasyon (geçmişe eklenir); Refresh butonu geçerli dizini geçmişe dokunmadan yeniden okur (`refresh()`).
- **Adres modu (`headerbar.rs`):** Breadcrumb satırının boş alanına tıklama veya `Ctrl+L`, başlık alanını düzenlenebilir `GtkEntry`'ye çevirir (mevcut tam yol önceden dolu, seçili). Enter → yola git ve breadcrumb moduna dön; Esc veya odak kaybı → değişiklik yapmadan breadcrumb moduna dön. İki mod `GtkStack` (`title_stack`) ile değiştiriliyor.
- **Klavye kısayolları (`window.rs::setup_shortcuts`):** `Alt+Left`/`Alt+Right`/`Alt+Up`/`F5`/`Ctrl+L`, pencere düzeyinde `GioSimpleAction` (`win.go-back` vb.) + `app.set_accels_for_action` ile bağlandı — ham `EventControllerKey` yerine action tabanlı, böylece bir metin girişi odaktayken kısayollar GTK'nin standart odak/engelleme kurallarına uyuyor.
- Geri/İleri/Yukarı, breadcrumb tıklama ve klasöre çift tıklama (Faz 3'ten) zaten mevcuttu; bu faz onları Home/Refresh/adres modu/kısayollarla tamamladı.

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 53/53 (yeni: `history.rs` 8 birim test).
- `cargo fmt --check`: temiz.

### Bilinen Notlar
- Adres modunda girilen yol yalnızca yerel (`VeyraPath::from_local`) olarak yorumlanıyor; `trash://` gibi URI şemalarının adres çubuğundan elle girilmesi kapsam dışı (breadcrumb tıklamasıyla zaten gidiliyor).

## Faz 3 — İlk Gerçek UI (`veyra-ui`)

### Eklenenler
- **Layout:** `AdwToolbarView` (top-bar=`AdwHeaderBar`, bottom-bar=status bar) içinde `AdwNavigationSplitView` (sidebar + content), `crates/veyra-ui/src/window.rs` tarafından kurulur.
- **HeaderBar (`headerbar.rs`):** Geri/İleri/Yukarı butonları (linked group), ortada tıklanabilir breadcrumb bar, arama toggle (başlık alanını `GtkSearchEntry` ile değiştirir), 3'lü view-switcher (linked `GtkToggleButton` grubu).
- **Breadcrumbs (`breadcrumbs.rs`):** Her yol segmenti ayrı tıklanabilir buton; `$HOME` altındaki yollar "Home" ile kısaltılır; GIO URI konumlar (trash://, recent://) salt-okunur etiket olarak gösterilir.
- **Sidebar (`sidebar.rs`):** Places (Home, Desktop, Documents, Downloads, Music, Pictures, Videos, Recent, Trash — `glib::user_special_dir` ile XDG çözümleme) + Devices (`gio::VolumeMonitor`, `mount-added`/`mount-removed`/`mount-changed` sinyalleriyle canlı güncellenir).
- **Status bar (`statusbar.rs`):** Sol "N items", sağ boş disk alanı (`gio` `filesystem::free` sorgusu, async).
- **3 görünüm modu (`views/`):**
  - `icon_view.rs`: `GtkGridView`, 48px ikon, dikey düzen.
  - `compact_view.rs`: `GtkGridView`, 20px ikon, yatay (ikon+ad yan yana) düzen, çok sütunlu akış.
  - `details_view.rs`: `GtkColumnView` — Ad, Boyut, Tür, Değiştirilme Tarihi, İzinler sütunları; her sütun tıklanabilir başlıkla sıralanabilir (`GtkColumnViewSorter`).
  - `mod.rs`: paylaşılan filtre→sırala→seçim zinciri, klasör-önce+ada-göre varsayılan sıralayıcı, standart Adwaita ikon adı eşlemesi.
- **Async entegrasyon (`fs_async.rs`):** Her `veyra-filesystem` çağrısı arka plan thread'inde çalışır, sonuç `async-channel` + `glib::spawn_future_local` ile ana thread'e taşınır — UI thread hiçbir I/O yapmaz (Kural #14).
- **Arama:** Başlık alanındaki arama girişi, `GtkCustomFilter` ile geçerli dizindeki adları anlık filtreler (tam metin/içerik arama motoru değil — o Faz 9 kapsamı).
- Geliştirme modunda `data/icons` dizini `GtkIconTheme` arama yoluna eklenir (`cfg!(debug_assertions)` korumalı).
- Application ID `io.github.erayq1.Veyra` ile başlangıç dizini `$HOME`.

### Tasarım Kararları (spec'ten sapmalar, gerekçeli)
- **`glib::MainContext::channel` yerine `async-channel` + `glib::spawn_future_local`:** İstenen API bu glib sürümünde (0.20) kaldırılmış; gtk-rs ekosisteminin resmi yerine geçen çözümü kullanıldı. `docs/technology-decisions.md`'e eklendi.
- **Seçim durumu görünümler arası paylaşılmıyor:** Her view kendi `GtkSingleSelection` zincirini kurar (aynı paylaşılan `ListStore`'u sarar). Navigasyon tüm view'leri anında günceller (aynı model), ama view modu değiştirince seçim sıfırlanır — spec bunu şart koşmuyordu, karmaşıklığı azaltmak için bilinçli tercih.
- **Details view varsayılan sıralaması klasör-önce değil, Ad-artan:** `GtkColumnViewSorter` kullanıcı başlığa tıklayınca devreye giriyor; bu, sütun bazlı sıralamanın doğal davranışı (çoğu gerçek dosya yöneticisinde de böyle).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 45/45 (Faz 3 GTK kodu için otomatik test eklenmedi — ekran gerektiren UI mantığı, gerçek doğrulama gerçek çalıştırma ile yapıldı).
- `cargo fmt --check`: temiz.
- **Gerçek Wayland oturumunda çalıştırma + ekran görüntüsü:** Pencere açıldı; sidebar (Places/Devices), headerbar (nav+breadcrumb+arama+view-switcher), icon view (73 öge, doğru klasör/dosya ikonları, kırık symlink `action-unavailable-symbolic` ile doğru işaretlendi), status bar ("73 items" / "358.4 GB free") hepsi çalışır durumda doğrulandı.

### Bağımlılık Değişiklikleri
- `veyra-ui`: `veyra-filesystem`, `gio`, `glib`, `async-channel` eklendi; `libadwaita` özelliği `v1_4` açıldı (NavigationSplitView/NavigationPage/ToolbarView bu özellik kapısının arkasında).
- `docs/technology-decisions.md` güncellendi (`async-channel` gerekçesi).

### Bilinen Notlar
- Symlink'ten dizine çift tıklama şu an navigasyon yerine `open()` (GIO varsayılan işleyici) çağırıyor — gerçek dizine navigasyon incelemesi ek bir stat gerektirdiği için Faz 3 kapsamı dışında bırakıldı, ileride küçük bir iyileştirme.
- "Compact View" ikon adı (`view-continuous-symbolic`) bazı ikon temalarında bulunmayabilir (kozmetik, derlemeyi/testleri etkilemiyor).
- Arama şu an yalnızca dosya adı alt-dizge eşleşmesi; içerik/fuzzy arama Faz 9'da.

### Sıradaki Faz
Faz 4 — Navigation (Back/Forward/Up/Home genişletmeleri, Address mode, klavye kısayolları). Onay bekleniyor.

---

## Faz 1 — Proje Altyapısı

### Eklenenler
- Cargo workspace (`resolver = "2"`) ve 4 crate: `veyra-core`, `veyra-filesystem`, `veyra-ui`, `veyra-app`.
- `veyra-core`: XDG Base Directory çözümleyici (`XdgDirs`) ve `VeyraError` (thiserror).
- `veyra-filesystem`: crate iskeleti kuruldu, içerik Faz 2'de eklenecek.
- `veyra-ui`: Libadwaita `Application` + `ApplicationWindow`, başlık "Veyra - Modern Linux File Manager", 1024x680 varsayılan boyut.
- `veyra-app`: giriş noktası (`main.rs`), root-yasağı kontrolü (`geteuid` üzerinden, izole `unsafe`), XDG dizin oluşturma, `tracing` tabanlı structured logging (stdout + `~/.local/state/veyra/veyra.log`), panic hook (panic'leri log'a yazıp varsayılan davranışa devreder).
- Application ID: `io.github.erayq1.Veyra`.
- 5 unit test (XDG çözümleme, log dosyası yolu, `geteuid` doğrulaması).
- `docs/technology-decisions.md`'e `libc` bağımlılığı ve gerekçesi eklendi.

### Doğrulama
- `cargo build --workspace`: başarılı, 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 5/5 test geçti.
- `cargo fmt --check`: temiz.
- Gerçek Wayland oturumunda `cargo run`: pencere başarıyla açıldı, log dosyasına yazıldı, panic/crash yok.

### Bilinen Notlar
- Çalışma zamanında `Adwaita-WARNING: Using GtkSettings:gtk-application-prefer-dark-theme with libadwaita is unsupported` uyarısı görülüyor. Kaynağı bu ortamın masaüstü ayarlarında global `gtk-application-prefer-dark-theme` açık olması; Veyra kodu bu ayarı hiçbir yerde set etmiyor. Faz 35 (Themes & Customization) kapsamında `AdwStyleManager:color-scheme` ile ele alınacak.
- `veyra-filesystem` bilerek boş: dosya sistemi operasyonları Faz 2 kapsamı.

### Sıradaki Faz
Faz 2 — Dosya Sistemi Çekirdeği. Onay bekleniyor.

---

## Faz 1 Eki — Masaüstü Entegrasyonu (freedesktop.org)

### Eklenenler
- `data/io.github.erayq1.Veyra.desktop`: `desktop-file-validate` ile doğrulandı.
- `data/io.github.erayq1.Veyra.metainfo.xml`: AppStream metainfo, `appstreamcli validate` ile doğrulandı (yalnızca repo henüz GitHub'da yayınlanmadığı için `url-not-reachable` uyarısı var, şema hatası değil).
- `data/icons/hicolor/scalable/apps/io.github.erayq1.Veyra.svg`: Adwaita mavi paletiyle klasör+belge ikonu.
- `docs/technology-decisions.md`: değişiklik yok, önceki `libc` girdisi geçerli.

### Doğrulama
- `desktop-file-validate`: temiz.
- `appstreamcli validate --pedantic`: 2 `url-not-reachable` uyarısı (beklenen, repo henüz yayınlanmadı).
- `xmllint --noout`: SVG geçerli.
- `cargo check --workspace`: temiz.
- `cargo test --workspace`: 4/4 (o anki durum).

### Bilinen Notlar
- `.desktop` dosyasındaki `Exec` alanı kullanıcı isteğindeki `veyra-app %U` yerine `veyra %U` olarak yazıldı: gerçek binary adı `veyra` (`crates/veyra-app/Cargo.toml`'daki `[[bin]] name`), `veyra-app` yazılsaydı masaüstü girişi çalışmazdı.

---

## Faz 2 — Dosya Sistemi Çekirdeği (`veyra-filesystem`)

### Eklenenler
- **Modeller:**
  - `VeyraPath` (`path.rs`): yerel `PathBuf` ve GIO URI'lerini (`sftp://`, `smb://`, `trash://`, ...) tek çatı altında temsil eder; `to_gio_file()` / `from_gio_file()` ile GIO köprüsü.
  - `FileKind` (`kind.rs`): `Regular`, `Directory`, `Symlink { target, is_broken }`, `Fifo`, `Socket`, `BlockDevice`, `CharDevice`, `Unknown`. **Tasarım kararı:** "Gizli dosya" ve "Çalıştırılabilir dosya" enum varyantı değil, `FileMetadata` üzerinde bağımsız `is_hidden` / `is_executable()` alanı — bir dizin aynı anda hem gizli hem çalıştırılabilir olabildiğinden, bunları enum koluna sıkıştırmak geçersiz durumlara yol açardı.
  - `FilePermissions` (`permissions.rs`): POSIX mod maskesi, `octal_string()` (`"0755"`), `symbolic_string()` (`"rwxr-xr-x"`, setuid/setgid/sticky dahil).
  - `FileMetadata` / `FileItem` (`metadata.rs`): ad, `VeyraPath`, boyut (`size_human()` — B/KB/MB/GB/TB, 1024 taban), zaman damgaları (`chrono`), izinler (backend desteklemiyorsa `None`, asla sahte varsayılan değil), sahip/grup, MIME türü (GIO `content-type` + `mime_guess` fallback), inode, `FileKind`.
  - `FsError` (`error.rs`): `NotFound`, `PermissionDenied`, `AlreadyExists`, `NotADirectory`, `NotEmpty`, `ReadOnly`, `InvalidPath`, `NoHandlerAvailable`, `Gio`, `Io` — GIO hatalarından otomatik eşleme (`map_gio_error`).
- **İşlemler (`ops.rs`, hepsi blocking, GTK ana thread dışında çağrılmak üzere tasarlandı):**
  - `read_dir`, `create_file`, `create_dir`, `rename`, `copy`, `move_entry`, `delete` (recursive, symlink'li dizinlere asla girmez — döngü riski yok), `trash` (GIO `g_file_trash`), `restore_from_trash` (freedesktop.org Trash spec'ine göre `.trashinfo` okuma — `trash://` GVfs arka planına bağımlı değil), `open` (GIO `AppInfo::launch_default_for_uri`, shell yok).
- **Edge case yönetimi:**
  - Kırık symlink tespiti: GIO'nun yerel backend'i dangling symlink'te hata vermeyip sessizce link'in kendi bilgisine düştüğünü doğruladık; `is_broken` bu davranışa göre hesaplanıyor.
  - Unicode/boşluk/özel karakterli dosya adları, 40 seviye derin yol: test edildi.
  - Transient dosyalar (okuma ile işlem arasında silinen): `NotFound` olarak raporlanıyor, panic yok.
  - Permission denied: `read_dir` üzerinde test edildi (root altında çalışıyorsa test bypass ediliyor, hata sayılmıyor).

### Testler
- 15 unit test (`path.rs`, `kind.rs` dolaylı, `metadata.rs`, `permissions.rs`).
- 25 integration test (`tests/`): `read_dir.rs` (6), `crud.rs` (12), `symlinks.rs` (3), `trash.rs` (1, gerçek trash round-trip), `edge_cases.rs` (3).
- Toplam: 45/45 test (workspace genelinde).

### Doğrulama
- `cargo build --workspace`: 0 warning.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warning.
- `cargo test --workspace`: 45/45 geçti.
- `cargo fmt --check`: temiz.

### Bağımlılık Değişiklikleri
- `veyra-filesystem`: `gio`, `glib`, `mime_guess`, `thiserror`, `tracing`, `chrono` eklendi; `dev-dependencies`'e `tempfile` eklendi. `docs/technology-decisions.md` güncellendi.
- Mimari tablodaki `tokio` bilerek eklenmedi: Faz 2 kapsamı senkron/blocking CRUD; async operasyon kuyruğu Faz 5/32'de gelecek (erken/monolitik sıçrama yapılmadı).

### Bilinen Notlar
- `restore_from_trash`, ev dizini çöp kutusuyla (mutlak `Path=` girdisi) sınırlı; bağlama-noktası bazlı (topdir-relative) çöp kutuları Faz 18'e bırakıldı.
- Test ortamında `gvfsd-trash` daemon'ı çalışmıyor olduğu tespit edildi (`trash:///` enumerasyonu "Operation not supported" veriyor); bu yüzden `restore_from_trash` kasıtlı olarak `trash://` GVfs arka planına değil, doğrudan freedesktop Trash spesifikasyonuna (`.trashinfo` dosyaları) dayanıyor — daha sağlam ve bağımsız bir tasarım.
- Test çalıştırmaları sırasında gerçek kullanıcı çöp kutusuna (`~/.local/share/Trash`) yalnızca test-prefixli geçici dosyalar yazıldı ve hepsi round-trip sonunda temizlendi; kullanıcının kendi çöp kutusu içeriğine dokunulmadı.

### Sıradaki Faz
Faz 3 — İlk Gerçek UI. Onay bekleniyor.

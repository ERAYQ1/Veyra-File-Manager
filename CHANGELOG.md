# Changelog

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

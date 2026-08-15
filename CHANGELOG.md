# Changelog

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

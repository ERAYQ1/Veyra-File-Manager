# Changelog

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

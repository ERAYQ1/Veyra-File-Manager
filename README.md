# 🦀 Veyra — Modern Linux File Manager

[![License: GPL v3](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![GTK4](https://img.shields.io/badge/GTK-4-4A86CF.svg)](https://www.gtk.org)
[![Libadwaita](https://img.shields.io/badge/Libadwaita-1-purple.svg)](https://gnome.pages.gitlab.gnome.org/libadwaita/)
[![Tests](https://img.shields.io/badge/tests-616%20passing-brightgreen.svg)](docs/testing.md)

Veyra, Linux için profesyonel seviyede zengin özellikler sunan, **Rust**,
**GTK4** ve **Libadwaita** ile geliştirilmiş; bağımsız, özgün, modern, son
derece akıcı, güvenli ve geliştirici dostu bir dosya yöneticisidir.

<p align="center">
  <img src="docs/assets/screenshot.png" alt="Veyra File Manager — Çift Panel ve Akıllı Depolama görünümü" width="900">
</p>

<p align="center"><i>Çift Panel Split View ve Akıllı Depolama Paneli — ekran görüntüsü yakında.</i></p>

---

## 🌟 Öne Çıkan Özellikler

- **Non-Blocking UI** — Tüm dosya, indeksleme, thumbnail, arşiv ve ağ
  işlemleri arka plan işçileriyle (worker pool) asenkron çalışır; arayüz
  **asla** donmaz.
- **Akışlı Dizin Tarama** — 100.000+ dosyalı klasörlerde bile 500'lük
  parçalar halinde akan, UI'ı asla bloklamayan asenkron listeleme
  (`read_dir_chunked`, ölçülen hız: **180.000+ dosya/sn**, bkz.
  [docs/performance.md](docs/performance.md)).
- **FTS5 Tam Metin Arama** — SQLite + FTS5 tabanlı, düşük öncelikli arka
  plan indeksleyici ile anlık dosya araması.
- **Çift Panel Split View** (`F3`) — yan yana iki panel, karşı panele
  doğrudan kopyala/taşı kısayolları.
- **Akıllı Depolama Paneli** — disk kullanım analizi, en büyük dosya/klasör
  keşfi, depolama içgörüleri.
- **3 Aşamalı "Open With" Dosya Bulucu** — MIME ilişkilendirmeli önerilen
  uygulamalar, tam uygulama listesi ve özel komut girişi.
- **Güvenlik Odaklı** — Path traversal engelleme, symlink/TOCTOU koruması,
  root çalıştırma yasağı ve izolasyonlu Polkit/D-Bus ayrıcalıklı işlem
  modeli.
- **Gizlilik Dostu Loglama & Çökme Teşhisi** — sıfır telemetri; loglar ve
  çökme raporları yalnızca yerelde, kimlik bilgisi/ev dizini maskelemesiyle
  saklanır (bkz. [docs/security.md](docs/security.md)).
- **Çoklu Görünüm Desteği** — Icon View, Compact View ve Details View
  (Column View).
- **Gelişmiş Navigasyon** — Tıklanabilir breadcrumbs, adres satırı modu
  (`Ctrl+L`), çoklu sekmeler (`Ctrl+T`), dinamik sağ tık context menu.
- **Zengin Boş Durumlar** — Downloads/Documents/Pictures/Music/Videos,
  Recent, Network, Trash ve arama sonucu için konuma özel boş durum
  ekranları.

> Command Palette (`Ctrl+K`) FAZ 24'te planlanıyor, henüz uygulanmadı — bkz.
> [docs/roadmap.md](docs/roadmap.md).

---

## 🏗️ Proje Mimarisi (Cargo Workspace)

Veyra modüler bir Cargo Workspace olarak yapılandırılmıştır:

```
Veyra
│
├── veyra-core         # Veri modelleri, config, loglama, temel hatalar ve trait'ler
├── veyra-filesystem   # GIO/GVfs dosya sistemi soyutlama katmanı, operasyon kuyruğu
├── veyra-search       # SQLite + FTS5 arama motoru, sorgu ayrıştırıcı, arka plan indeksleyici
├── veyra-ui           # GTK4 & Libadwaita arayüz bileşenleri, görünümler, sekmeler, split view
└── veyra-app          # Uygulama giriş noktası (binary), lifecycle, CLI ve D-Bus
```

Ayrıntılar için [docs/architecture.md](docs/architecture.md).

---

## 🚀 Hızlı Kurulum ve Başlatma

### Kaynaktan derleme

```bash
git clone https://github.com/ERAYQ1/Veyra-File-Manager.git
cd Veyra-File-Manager
cargo run --bin veyra-app
```

Gereksinimler: **Rust 1.85+**, **GTK4**/**Libadwaita** geliştirme
kütüphaneleri (`libgtk-4-dev`, `libadwaita-1-dev` veya dağıtımınızın
eşdeğer paketleri). Ayrıntılı derleme adımları ve sorun giderme için
[docs/building.md](docs/building.md).

### Tek komutla kurulum (install.sh)

```bash
git clone https://github.com/ERAYQ1/Veyra-File-Manager.git
cd Veyra-File-Manager
./install.sh
```

Dağıtımınızı algılar, derleme bağımlılıklarını kurar (`--no-deps` ile
atlanabilir), derler ve `make install` ile sisteme kurar. Kaldırmak için
`./install.sh --uninstall`.

### Makefile ile kurulum

```bash
make
sudo make install        # PREFIX=/usr varsayılan
sudo make uninstall
```

### Arch Linux (PKGBUILD)

```bash
cd packaging/arch
makepkg -si
```

### Fedora / RHEL (RPM)

```bash
rpmbuild -ba packaging/fedora/veyra.spec
```

### Flatpak

```bash
flatpak install flathub org.gnome.Platform//47 org.gnome.Sdk//47 \
    org.freedesktop.Sdk.Extension.rust-stable//47
flatpak-builder --user --install --force-clean build-dir \
    build-aux/flatpak/io.github.erayq1.Veyra.json
```

Tüm dağıtımlar (openSUSE, Debian/Ubuntu dahil) için ayrıntılı paketleme
kılavuzu: [docs/packaging.md](docs/packaging.md).

---

## ⌨️ Temel Kısayollar

| Eylem | Kısayol |
| :--- | :--- |
| Yeni Sekme | `Ctrl+T` |
| Sekmeyi Kapat | `Ctrl+W` |
| Çift Panel (Split View) | `F3` |
| Adres Satırı | `Ctrl+L` |
| Gizli Dosyaları Göster/Gizle | `Ctrl+H` |
| Klasör İçinde Ara | `Ctrl+F` |
| Yeniden Adlandır | `F2` |
| Çöp Kutusuna Taşı | `Delete` |
| Kalıcı Sil | `Shift+Delete` |
| Geri / İleri | `Alt+Left` / `Alt+Right` |
| Üst Klasöre Git | `Alt+Up` |
| Yenile | `F5` |

Tam liste ve klavye-öncelikli tasarım ilkeleri için
[docs/ui-guidelines.md](docs/ui-guidelines.md).

---

## 📚 Dokümantasyon

**Katkı sağlama:** [CONTRIBUTING.md](CONTRIBUTING.md) ·
**Güvenlik açığı bildirimi:** [SECURITY.md](SECURITY.md) ·
**Davranış kuralları:** [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) ·
**Sürüm geçmişi:** [CHANGELOG.md](CHANGELOG.md)

Teknik geliştirici dokümanları `docs/` altında:

- [docs/architecture.md](docs/architecture.md) — Sistem mimarisi ve thread sınırları
- [docs/building.md](docs/building.md) — Derleme kılavuzu (dağıtım paketleri dahil)
- [docs/testing.md](docs/testing.md) — Test yapısı, adversarial testler, temp dizin güvencesi
- [docs/packaging.md](docs/packaging.md) — Native paketleme (Arch/Fedora/openSUSE/Debian) ve Makefile
- [docs/plugin_development.md](docs/plugin_development.md) — Mevcut entegrasyon noktaları ve planlanan eklenti sistemi
- [docs/translation.md](docs/translation.md) — i18n katalog yapısı ve yeni dil ekleme
- [docs/security.md](docs/security.md) — Ayrıcalık izolasyonu, Polkit, gizlilik garantileri
- [docs/performance.md](docs/performance.md) — Ölçülen performans sayıları ve ölçekleme
- [docs/technology-decisions.md](docs/technology-decisions.md) — Teknoloji seçimleri ve bağımlılık politikası
- [docs/security-model.md](docs/security-model.md) — Tehdit matrisi ve zafiyet engelleme
- [docs/filesystem-model.md](docs/filesystem-model.md) — Dosya sistemi soyutlaması ve edge case yönetimi
- [docs/ui-guidelines.md](docs/ui-guidelines.md) — UI/UX rehberi ve klavye kısayolları
- [docs/performance-budget.md](docs/performance-budget.md) — Performans hedefleri ve bellek bütçeleri
- [docs/benchmarks.md](docs/benchmarks.md) — Ölçülmüş kıyaslama sonuçları
- [docs/flatpak_permissions.md](docs/flatpak_permissions.md) — Flatpak sandbox izin gerekçelendirmesi
- [docs/roadmap.md](docs/roadmap.md) — 60 Fazlık Master Roadmap ve 60 Geliştirme Kuralı

Yapay zeka asistanlarının uyması gereken geliştirme protokolü:
[AGENTS.md](AGENTS.md).

---

## 💬 Topluluk & İletişim

Sorularınız, öneriniz veya geri bildiriminiz mi var? Bize ulaşın:

- 💡 **GitHub Discussions:** [github.com/ERAYQ1/Veyra-File-Manager/discussions](https://github.com/ERAYQ1/Veyra-File-Manager/discussions) — fikir, soru ve genel sohbet için.
- 🐛 **Hata bildirimi:** [GitHub Issues](https://github.com/ERAYQ1/Veyra-File-Manager/issues) üzerinden.
- 💬 **Matrix:** `#veyra:matrix.org` *(yakında)*
- 🎮 **Discord:** *(yakında — davet bağlantısı eklenecek)*

Katkı sağlamadan önce lütfen [CONTRIBUTING.md](CONTRIBUTING.md) ve
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) dosyalarını inceleyin.

---

## 📜 Lisans

Bu proje **GNU General Public License v3.0 (GPL-3.0)** altında
lisanslanmıştır. Detaylar için [LICENSE](LICENSE) dosyasına
bakabilirsiniz.

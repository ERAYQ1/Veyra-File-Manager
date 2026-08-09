# 🦀 Veyra — Modern Linux File Manager

Veyra, Linux için **Dolphin** seviyesinde zengin özellikler sunan, **Rust**, **GTK4** ve **Libadwaita** ile geliştirilmiş; modern, son derece akıcı, güvenli ve geliştirici dostu bir dosya yöneticisidir.

---

## 🌟 Öne Çıkan Özellikler & Hedefler

- **Rust Güvenliği & Hızı**: Sıfır bellek sızıntısı, maksimum performans ve thread-safe mimari.
- **Modern GNOME/Libadwaita HIG Arayüzü**: GTK4 tabanlı GPU hızlandırmalı modern responsive tasarım.
- **Non-Blocking UI**: Tüm dosya, indeksleme, thumbnail, arşiv ve ağ işlemleri arka plan işçileriyle (worker pool) asenkron çalışır, arayüz asla donmaz.
- **Hızlı Arama Engine**: SQLite + FTS5 entegrasyonu ile bilgisayar kaynaklarını yormadan anlık tam metin dosya araması.
- **Çoklu Görünüm Desteği**: Icon View, Compact View ve Details View (Column View).
- **Gelişmiş Navigasyon**: Tıklanabilir breadcrumbs, adres satırı modu (`Ctrl+L`), çoklu sekmeler (`Ctrl+T`), çift panel split view (`F3`, karşı panele kopyala/taşı) ve dinamik sağ tık context menu.
- **Async Dosya İşlemleri**: Copy/Move/Delete/Trash arka plan kuyruğunda, canlı ilerleme çubuğu ve çakışma çözümleme diyaloğuyla.
- **Güvenlik Odaklı**: Path traversal engelleme, symlink/TOCTOU koruması, root çalıştırma yasağı ve izolasyonlu Polkit/D-Bus ayrıcalıklı işlem modeli.

> Command Palette (`Ctrl+K`) FAZ 24'te planlanıyor, henüz uygulanmadı — bkz. [docs/roadmap.md](docs/roadmap.md).

---

## 🏗️ Proje Mimarısı (Cargo Workspace)

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

---

## 🚀 Kurulum ve Çalıştırma

### Gereksinimler
- **Rust** (1.85 veya üzeri)
- **GTK4** & **Libadwaita** geliştirme kütüphaneleri (`libgtk-4-dev`, `libadwaita-1-dev` veya dağıtımınızın eşdeğer paketleri)
- **GIO / GLib** kütüphaneleri

### Projeyi Klonlama ve Çalıştırma
```bash
git clone https://github.com/ERAYQ1/Veyra-File-Manager.git
cd Veyra-File-Manager

# Uygulamayı derle ve çalıştır
cargo run --bin veyra-app
```

---

## 📚 Dokümantasyon

Tüm mimari kararlar ve geliştirme standartları `docs/` altında dokümante edilmiştir:

- [docs/architecture.md](docs/architecture.md) — Sistem mimarisi ve thread sınırları
- [docs/technology-decisions.md](docs/technology-decisions.md) — Teknoloji seçimleri ve bağımlılık politikası
- [docs/security-model.md](docs/security-model.md) — Güvenlik modeli ve zafiyet engelleme
- [docs/filesystem-model.md](docs/filesystem-model.md) — Dosya sistemi soyutlaması ve edge case yönetimi
- [docs/ui-guidelines.md](docs/ui-guidelines.md) — UI/UX rehberi ve klavye kısayolları
- [docs/performance-budget.md](docs/performance-budget.md) — Performans hedefleri ve bellek bütçeleri
- [docs/roadmap.md](docs/roadmap.md) — 60 Fazlık Master Roadmap ve 60 Geliştirme Kuralı

---

## 📜 Lisans

Bu proje **GNU General Public License v3.0 (GPL-3.0)** altında lisanslanmıştır. Detaylar için [LICENSE](LICENSE) dosyasına bakabilirsiniz.

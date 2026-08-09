# Veyra — Agent & AI Geliştirme Protokolü

Bu dosya, Veyra projesinde çalışan yapay zeka asistanları (Claude, Antigravity vb.) için zorunlu kılınan çalışma kurallarını ve token optimizasyon protokollerini tanımlar.

---

## 60 Veyra Development Rules (Geliştirme Kuralları)

1. **Production Quality**: Veyra ciddi, üretim kalitesinde bir Linux dosya yöneticisidir. Oyuncak/demo uygulaması yazma.
2. **No Monolithic Leaps**: Bütün özellikleri tek seferde veya bir fazda kodlamaya çalışma. Roadmap'i adım adım takip et.
3. **Inspect Architecture First**: Herhangi bir fazı uygulamadan önce mevcut mimariyi incele.
4. **Never Destroy Functionality**: Yeni bir özellik eklerken çalışan mevcut işlevselliği asla bozma veya silme.
5. **No Placeholder Code**: Açıkça dokümante edilmedikçe placeholder (geçici/taslak) kod kullanma.
6. **No TODOs for Core Functionality**: Çekirdek işlevsellik için sonraya bırakılmış TODO'lar bırakma.
7. **Clean Compiler Output**: Derleyici uyarılarını (`warnings`) asla göz ardı etme.
8. **Clean Tools**: `cargo fmt`, `cargo clippy` ve `cargo test` her zaman temiz olmalı.
9. **Prefer Safe Rust**: Safe Rust tercih et. Mutlaka gerekmedikçe `unsafe` Rust kullanma.
10. **Isolate Unsafe**: Unsafe gerekliyse, kod bloğunu izole et ve güvenlik gerekçelerini (`Safety` doc comments) eksiksiz yaz.
11. **Non-Blocking UI Thread**: GTK UI thread'ini dosya sistemi, indeksleme, thumbnail, arşiv veya ağ işlemleriyle ASLA bloklama.
12. **Async / Background Workers**: Tüm ağır işlemleri arka planda/asenkron olarak çalıştır.
13. **Handle Cancellation**: Arka plan işlemlerinin iptal edilebilirliğini (`cancellation token`) doğru yönet.
14. **Explicit Error Handling**: Hataları açıkça yönet (`Result`, `thiserror`, `anyhow`).
15. **No User Error Panics**: Normal bir kullanıcı dosya sistemi hatası nedeniyle uygulamanın `panic` olmasına izin verme.
16. **No Stale Path Assumptions**: Yolların sonsuza kadar geçerli kalacağını varsayma.
17. **Transient Files**: Dosyalar tespit ile işlem yapılması arasında silinebilir veya taşınabilir.
18. **Graceful Permissions**: Yetki hatalarını zarif bir şekilde yönet ve kullanıcıya açıklayıcı mesajlar sun.
19. **Sanitize Shell Commands**: Kullanıcı girdisi içeren kabuk komutlarını asla doğrudan çalıştırma.
20. **No Root Default**: Veyra'yı asla varsayılan olarak `root` yetkisiyle çalıştırma. Privileged operasyonları Polkit/D-Bus ile izole et.
21. **Path Traversal Protection**: Arşiv çıkarma ve yol işleme sırasında `../` vb. Path Traversal saldırılarını engelle.
22. **Symlink Attack Protection**: Symlink saldırılarına ve TOCTOU (Time-of-Check to Time-of-Use) zafiyetlerine karşı koruma sağla.
23. **No Sensitive Logging**: Log dosyalarında şifre, jeton veya hassas dosya içeriklerini/yollarını loglama.
24. **No Telemetry Without Consent**: Kullanıcı açık rızası olmadan telemetry gönderme.
25. **No Hardcoded Terminal**: Terminal emülatörünü kod içine hardcode etme. Sistem tercihlerini kullan (`xdg-terminal-exec` / `gio`).
26. **Follow Linux Desktop Standards**: XDG Base Directory ve freedesktop.org standartlarına uy.
27. **Follow GTK4 & Libadwaita HIG**: GNOME Human Interface Guidelines kurallarına uy.
28. **Accessible Controls**: Tüm etkileşimli denetimlerin erişilebilir etiketlere (`accessible labels`) sahip olmasını sağla.
29. **Keyboard Accessibility**: Önemli tüm özellikler klavye ile kullanılabilir olmalı (`Keyboard-first`).
30. **Huge Directory Virtualization**: Çok büyük klasörlerde (100.000+ dosya) UI tepkiselliğini korumak için sanallaştırma ve lazy loading kullan.
31. **Thumbnail Memory Limits**: Binlerce thumbnail'ı aynı anda belleğe yükleme; bellek sınırı ve lazy loading kullan.
32. **Resource-Aware Indexing**: Arama indeksleme işlemlerini CPU/IO kaynak bilinçli olarak çalıştır.
33. **Memory Optimization**: Bellek kullanımını sürekli izle ve optimize et.
34. **Test File Operations**: Her önemli dosya sistemi işlemi için unit ve integration testleri ekle.
35. **Test Edge Cases**: Unicode, boşluk içeren ve özel karakterli dosya isimlerini test et.
36. **Test Failures**: Yetkisizlik ve hata senaryolarını test et.
37. **Test Large Directories**: Büyük klasörleri ve eşzamanlı dosya sistemi değişikliklerini test et.
38. **Data Loss Prevention**: Kullanıcı verisini koru. Yok edici (destructive) işlemler açık onay gerektirsin.
39. **Safe Permanent Delete**: Kalıcı silme işlemi normal `Delete` tuşuyla kazara tetiklenemez olmalıdır.
40. **Modular Architecture**: Mimariyi modüler tut. Workspace crate sınırlarına sadık kal.
41. **Decouple UI and Core**: Dosya sistemi mantığını UI mantığından ayrı tut (`veyra-filesystem` vs `veyra-ui`).
42. **Decouple Search**: Arama/indeksleme sistemini UI'dan bağımsız tut.
43. **Decouple Previews**: Önizleme ve thumbnail motorunu UI'dan ayrı tut.
44. **Decouple Network**: Ağ işlevselliğini (GVfs/SFTP/SMB) çekirdek dosya sisteminden ayrı tut.
45. **Document Architecture**: Mimari kararları `docs/` altında dokümante et.
46. **License Compliance**: GPL veya uyumsuz lisanslı projelerden kod kopyalama. Lisanslara saygı göster.
47. **Verify Crate Licenses**: Harici bağımlılık eklemeden önce lisans uyumluluğunu kontrol et.
48. **Verify Before Declaring Done**: Bir özelliğin tamamlandığını test etmeden iddia etme.
49. **Step-by-Step Phase Protocol**: Claude/AI her fazda şu sırayı takip etmelidir:
    1. CURRENT STATE ANALYSIS
    2. REQUIREMENTS
    3. ARCHITECTURE
    4. IMPLEMENTATION PLAN
    5. IMPLEMENTATION
    6. TESTS
    7. SECURITY REVIEW
    8. PERFORMANCE REVIEW
    9. UI/UX REVIEW
    10. BUILD
    11. CLIPPY
    12. TEST
    13. FINAL VERIFICATION
    14. CHANGELOG
    15. NEXT PHASE
50. **No Auto-Advancing Phases**: AI bir fazı bitirdiğinde onay almadan otomatik olarak sonraki faza atlamamalıdır.

---

## Token ve Geliştirme Protokolü

- Her faz için doğrudan hedef odaklı kod üretilir.
- Kod açıklamaları kısa, net ve işlevsel tutulur.
- Gereksiz bağlam şişirilmesi engellenir.

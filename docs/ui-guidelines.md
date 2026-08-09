# Veyra — User Interface & HIG Compliance Guidelines

**Document Status:** Approved Architecture  
**Design Standard:** GNOME Human Interface Guidelines (HIG) + Libadwaita Modern Standards

---

## 1. Interface Layout Overview

Veyra uses a clean, modern responsive layout built around `AdwApplicationWindow`, `AdwHeaderBar`, and `AdwNavigationSplitView`.

```
┌────────────────────────────────────────────────────────────────────────┐
│ ← → ↑   Home > Projects > Veyra                   🔍  [Split] [⋮]      │  <-- AdwHeaderBar & Breadcrumbs
├───────────────────────────────┬────────────────────────────────────────┤
│                               │                                        │
│  PLACES                       │  📁 src               📁 tests          │
│  ⭐ Home                      │  📁 assets            📄 README.md      │  <-- Main Content Area
│  📁 Documents                 │  📄 Cargo.toml                         │      (Icon / Compact / Details)
│  📁 Downloads                 │                                        │
│                               │                                        │
│  DEVICES                      │                                        │
│  💾 System (128 GB Free)      │                                        │
│                               │                                        │
├───────────────────────────────┴────────────────────────────────────────┤
│ 6 items                                                   12.4 GB free │  <-- Status Bar
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Core UI Components & Modes

### 2.1 View Modes
1. **Icon View (`GtkGridView`):** Large scalable grid icons, adaptive spacing, name truncation with tooltip full text.
2. **Compact View (`GtkGridView`):** Smaller icons arranged horizontally in columns for quick visual scanning.
3. **Details View (`GtkColumnView`):** Data-rich tabular view displaying Name, Size, Modified Date, Type, Owner, and Permissions with sortable column headers.

### 2.2 Navigation Controls
- **Interactive Breadcrumbs Bar:** Each path component is an interactive button with drop target capability.
- **Address Bar Switching (`Ctrl + L`):** Toggles breadcrumb into editable path text entry with path auto-completion.
- **Split View (`F3`):** Side-by-side dual panel view with active panel focus indicators and direct "Copy to target panel" shortcuts.

### 2.3 Command Palette (`Ctrl + K`)
- Centered overlay modal (`AdwDialog`) providing instant fuzzy search across all application commands, shortcuts, settings, and navigation actions.

---

## 3. Keyboard Shortuts Policy (Keyboard-First Design)

All primary actions MUST be fully accessible via keyboard shortcuts:

| Action | Primary Shortcut | Secondary / Legacy |
| :--- | :--- | :--- |
| **New Tab** | `Ctrl + T` | — |
| **Close Tab** | `Ctrl + W` | — |
| **Toggle Split View** | `F3` | — |
| **Command Palette** | `Ctrl + K` | — |
| **Location Entry** | `Ctrl + L` | `Alt + D` |
| **Toggle Hidden Files** | `Ctrl + H` | — |
| **Search Directory** | `Ctrl + F` | — |
| **Rename Entry** | `F2` | — |
| **Move to Trash** | `Delete` | — |
| **Permanent Delete** | `Shift + Delete` | — |
| **Go Back** | `Alt + Left` | `Backspace` |
| **Go Forward** | `Alt + Right` | — |
| **Go Up** | `Alt + Up` | — |
| **Refresh Directory** | `F5` | `Ctrl + R` |

---

## 4. Accessibility & UI Performance

1. **Virtualization:** All directory views use GTK4 list models (`GtkDirectoryList`, `GtkSelectionModel`, `GtkSignalListItemFactory`) to virtualize UI widget creation. Directory listings with 100,000 files only instantiate UI elements for visible on-screen rows.
2. **Accessible Labels:** All icon-only buttons (`GtkButton`) must explicitly provide `accessible-role` and `tooltip-text` / `accessible-description` properties for AT-SPI screen readers.
3. **Theme & Accent Adaptation:** Automatically respects system dark/light preference via `AdwStyleManager` while supporting custom accent color highlights.

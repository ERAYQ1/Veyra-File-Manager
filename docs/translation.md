# Translation & Localization Guide

Veyra ships its own minimal, dependency-free localization engine
(`crates/veyra-ui/src/i18n.rs`) instead of `gettext`/`fluent`. `glib`
(already a `veyra-ui` dependency for system-locale detection) covers
locale resolution; a compile-time-checked key/value table covers
everything a desktop file manager's UI needs — pulling in a whole
message-catalog crate isn't warranted (see
[technology-decisions.md](technology-decisions.md)'s "avoid new
dependencies when an existing one already covers the need" policy).

## How it works

- **`Locale`** is the concrete language currently being rendered (`En` /
  `Tr` today) — never "system". The user's *preference*
  (`config::LanguagePref::System`/`En`/`Tr`) is a separate concept that
  resolves down to a concrete `Locale` via `detect_system_locale()` when
  set to `System`, so "the user asked for system language" and "the
  system happens to be Turkish" stay distinguishable.
- **`detect_system_locale()`** walks `glib::language_names()` (which
  itself resolves `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`, most-specific
  first) and returns the first entry whose two-letter prefix matches a
  language Veyra ships. Falls back to `En` — including in a `C`/`POSIX`
  locale or a headless test environment — and never panics.
- **`t(key)`** looks up `key` in the current locale's table, falling back
  to the `En` table, then to the raw key string itself, if a translation
  is missing. A missing translation is never fatal (Kural #15) — it shows
  up as the literal key string in the UI, which is easy to spot and file.
- **`t_fmt(key, &[("name", "value")])`** is `t(key)` with `{name}`-style
  placeholders substituted, e.g.
  `t_fmt("status.free_space", &[("size", "12.3 GB")])`.
- **`t_plural(key, n, params)`** looks up `"{key}.one"` or `"{key}.other"`
  (via `plural_category`) instead of a bare key, then applies `params` the
  same way. Callers conventionally pass the count itself as a `"count"`
  param too.

## Catalog structure

Each language is one flat `const &[(&str, &str)]` array (`EN`, `TR`).
Keys are dotted `<area>.<element>[.<field>]`, e.g.
`prefs.appearance.theme.title` — grepping for `prefs.appearance.` finds
every string on that one Preferences page. Plural entries are always a
`.one`/`.other` pair, never a bare key:

```rust
("menu.copy.one", "Copy"),
("menu.copy.other", "Copy ({count} items)"),
```

## Adding a translated string

1. Add the key to `EN` in the right section (sections are marked with
   `// --- Area (source_file.rs) ---` comments — put the new key in its
   matching section, or add a new section comment if it's a new area).
2. Add the same key to `TR` with the Turkish translation.
3. Call it from the widget code: `t("your.new.key")`, `t_fmt(...)`, or
   `t_plural(...)`.
4. Run `cargo test -p veyra-ui i18n` — three completeness tests
   (`every_en_key_has_a_tr_counterpart`, `every_tr_key_exists_in_en`,
   `no_duplicate_keys_within_a_table`) fail loudly if the two tables drift
   apart, so a missing translation is a test failure, not a runtime
   surprise.

## Adding a new language

1. Add a variant to the `Locale` enum (e.g. `De`).
2. Add its own `const DE: &[(&str, &str)] = &[...]` array, mirroring every
   key in `EN` — the completeness tests above apply to any table you wire
   into `Locale::table`, so a partial table fails the build's test suite
   immediately rather than silently falling back to English for whatever
   was missed.
3. Wire it into `Locale::table`, `Locale::code` (BCP-47-ish, e.g. `"de"`,
   used for both `glib::language_names()` prefix matching and the
   persisted `settings.json` value), `Locale::label` (display name in the
   Preferences language picker), and `Locale::ALL`.
4. Add the matching `LanguagePref` variant in `veyra-core::config` so it's
   selectable (not just auto-detected) from Preferences.

The roadmap's originally scoped Faz 37 language list is
`de`/`fr`/`es`/`ru`/`ar`/`zh`/`ja` — note that Arabic needs more than the
two plural categories (`one`/`other`) the current `plural_category`
function implements (CLDR defines six for Arabic); that function is kept
separate specifically so a language needing more categories is a new
match arm there, not a rewrite of `t_plural`.

## Pluralization rules

Both shipped locales (English, Turkish) use the same two-category
(`one`/`other`) CLDR plural rule today. `plural_category(locale, n)`
currently ignores `locale` and picks `"one"` only for `n == 1`, `"other"`
otherwise — correct for both current languages, but the parameter is
already there for a future language (e.g. Arabic, Russian) that needs
different category boundaries per locale.

## Testing translations

```sh
cargo test -p veyra-ui i18n::
```

covers table completeness (every key present in both languages, no
duplicate keys within one table) and the `t`/`t_fmt`/`t_plural` behavior
itself, including the Turkish-table pluralization test
(`t_plural_in_turkish_uses_the_turkish_table`). There is no runtime
locale-switching UI test beyond this — actually seeing a translated string
render correctly in a widget is verified manually by switching
Preferences → Language and checking the affected screen.

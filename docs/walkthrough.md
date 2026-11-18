# Cosmic Font Foundry (`v0.3.0`) — Walkthrough

Cosmic Font Foundry is refactored and upgraded from the upstream MIT codebase (`atunfrito/tuna-fonts` v0.2.3).

---

## Summary of Changes

### 1. Brand, Packaging, Icons & Workflows
- **Identity & Bundle:** Rebranded to **Cosmic Font Foundry** (`com.thecosmiccrafter.cosmicfontfoundry`) across [package.json](../package.json), [tauri.conf.json](../src-tauri/tauri.conf.json), [index.html](../index.html), [hooks.nsh](../src-tauri/nsis/hooks.nsh), and capabilities ([default.json](../src-tauri/capabilities/default.json), [menu.json](../src-tauri/capabilities/menu.json), [quick.json](../src-tauri/capabilities/quick.json)).
- **Icons & Licensing:** Added Cosmic SVG/PNG/ICO branding ([public/icon.svg](../public/icon.svg), [design/icon.svg](../design/icon.svg), `src-tauri/icons/`) and preserved MIT attribution in [LICENSE](../LICENSE) and [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md).
- **CI/CD & Winget:** Added [ci.yml](../.github/workflows/ci.yml), updated [release.yml](../.github/workflows/release.yml) and [winget.yml](../.github/workflows/winget.yml), and replaced `winget/` templates for `TheCosmicCrafter.CosmicFontFoundry`.

### 2. Platform-Independent Core Engine (`src-tauri/core`)
- **[reader.rs](../src-tauri/core/src/reader.rs):** Parses `fvar` variable axes, `GSUB` OpenType feature tags, `OS/2` Unicode script coverage, `name` IDs (`6`, `8`, `9`, `13`, `14`), and SHA-256 content hashes.
- **[inventory.rs](../src-tauri/core/src/inventory.rs) & [cache.rs](../src-tauri/core/src/cache.rs):** Scans system/user/watched-folder fonts and detects exact SHA-256 duplicates, style collisions, and corrupt files.
- **[sync.rs](../src-tauri/core/src/sync.rs):** Implements BYO-Sync manifests (`cosmic-manifest.json`), `.cosmicfonts.json` project detection, portable JSON import/export, and PKZIP client handoff bundles with `LICENSES.txt`.
- **[migrate.rs](../src-tauri/core/src/migrate.rs):** Non-destructive one-click **"Import from tuna-fonts"** migrator reading `%APPDATA%\es.atunfrito.tunafonts`.

### 3. Windows Backend (`src-tauri/src`)
- Refactored modules to English ([lib.rs](../src-tauri/src/lib.rs), [win.rs](../src-tauri/src/win.rs), [tray.rs](../src-tauri/src/tray.rs), [startup.rs](../src-tauri/src/startup.rs), [google.rs](../src-tauri/src/google.rs), [fonts/mod.rs](../src-tauri/src/fonts/mod.rs)).
- Added native Win32 global hotkey (`RegisterHotKey` / `UnregisterHotKey`, default `Ctrl+Shift+F`) for the Quick-Switcher overlay and pinned collection toggles in the tray popup.
- Added safe corrupt/duplicate quarantine (`quarantine_font`).

### 4. Svelte 5 Frontend (`src/`)
- Restyled [app.css](../src/app.css) with Cosmic light/dark tokens and added `en` (default) + `es` translations ([en.js](../src/i18n/en.js), [es.js](../src/i18n/es.js)).
- Upgraded [App.svelte](../src/App.svelte), [Sidebar.svelte](../src/components/Sidebar.svelte), [Specimen.svelte](../src/views/Specimen.svelte), [Compare.svelte](../src/views/Compare.svelte), [Settings.svelte](../src/views/Settings.svelte), [Menu.svelte](../src/Menu.svelte), and [QuickSwitcher.svelte](../src/QuickSwitcher.svelte).

---

## Verification Results

- `npm run vite:build`: Passed (production Svelte 5 bundle, zero errors, zero warnings).
- `cargo test --manifest-path src-tauri/core/Cargo.toml`: Passed (`11/11` unit and integration tests).
- Full Windows `src-tauri` build (`windows-sys` / `winreg`) runs on Windows locally (`cargo test --manifest-path src-tauri/Cargo.toml`, `npm run dev`) or via the `windows-latest` CI runner in `.github/workflows/ci.yml`.

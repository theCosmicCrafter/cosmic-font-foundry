<div align="center">

<img src="public/icon.svg" width="88" alt="Cosmic Font Foundry">

# Cosmic Font Foundry

**A local-first, open-source font manager for Windows 10 and 11.**
No subscriptions, no accounts, no telemetry—built to outshine bloated commercial font managers.

*Originally forked from [`atunfrito/tuna-fonts`](https://github.com/atunfrito/tuna-fonts) (MIT).*

</div>

## Features

- **Instant Inventory & Menu Decluttering:** View every system, user, and watched-folder font grouped by family in list or grid view. Deactivate fonts safely without deleting them so Illustrator, Figma, Photoshop, and Word stay fast and tidy.
- **Variable Font Playground:** Live sliders for `wght`, `wdth`, `opsz`, `slnt`, and custom `fvar` axes with one-click CSS `font-variation-settings` copy.
- **OpenType & Script Inspection:** Toggle `liga`, `dlig`, `smcp`, `onum`, `tnum`, and stylistic sets (`ss01`–`ss20`) live; inspect script/language coverage badges, foundry/designer/license metadata, and the full character map (click to copy).
- **Compare & Pairing Sandbox:** Compare 2–4 families stacked, side-by-side, or in a Heading + Body pairing sandbox on light/dark canvases.
- **Google Fonts Discover + Pre-Install Specimen:** Browse the full Google Fonts catalog and open the full Specimen inspector before installing anything.
- **Duplicate & Conflict Scanner:** Spot exact SHA-256 duplicates, family/style collisions across folders, and unreadable/corrupt font files with safe quarantine.
- **Global Quick-Switcher (`Ctrl+Shift+F`) & Tray Collections:** Search and toggle fonts or pinned collections from anywhere without losing focus on your design canvas.
- **Watched Folders & `.cosmicfonts.json` Project Auto-Activation:** Load project fonts temporarily for the session without installing them to Windows.
- **BYO-Sync & Client Handoff Packaging:** Sync collections, tags, and favorites across machines via any cloud/Syncthing folder (`cosmic-manifest.json` matched by SHA-256 + PostScript name), or export a collection `.zip` with generated `LICENSES.txt`.
- **Optional `tuna-fonts` Migration:** One-click non-destructive import from `%APPDATA%\es.atunfrito.tunafonts`.

## Privacy & System Safety

- Everything runs locally on your machine. Network traffic only happens when opening Google Fonts Discover or checking GitHub releases (configurable in Settings).
- Essential Windows system fonts (`Segoe UI`, `Arial`, `Consolas`, etc.) remain permanently locked; other system fonts are locked by default and backed up before any elevated change.

## Build From Source

Requires [Node 22+](https://nodejs.org), [Rust](https://rustup.rs) stable, and the Windows WebView2 runtime.

```bash
git clone https://github.com/theCosmicCrafter/tuna-fonts.git
cd tuna-fonts
npm ci
npm run installer
```

Run core unit tests (works on both Windows and Linux):

```bash
cargo test --manifest-path src-tauri/core/Cargo.toml
```

Run full Windows integration tests:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

## Updater Key Setup (For Maintainers)

Before publishing signed releases, generate your own Tauri updater keypair:

```bash
npx tauri signer generate -w ~/.tauri/cosmic-font-foundry.key
```

Update `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` with the public key, and add `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to your GitHub repository secrets.

## License

[MIT](LICENSE). See [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for bundled font and library notices.

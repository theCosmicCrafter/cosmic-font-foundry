<div align="center">

<img src="public/icon.svg" width="88" alt="Cosmic Font Foundry">

# Cosmic Font Foundry

**A lightweight, local-first, open-source font manager and modular foundry for Windows 10 and 11.**
No subscriptions, no accounts, no telemetry, and zero bundled ML bloat (`~8.6 MB` native binary).

*Originally forked from [`atunfrito/tuna-fonts`](https://github.com/atunfrito/tuna-fonts) (MIT).*

</div>

## Features

- **Instant Inventory & Menu Decluttering:** View every system, user, and watched-folder font grouped by family in list or grid view. Deactivate fonts safely without deleting them so Illustrator, Figma, Photoshop, and Word stay fast and tidy.
- **Tranquil Nebula UI:** Warm obsidian dark mode and alabaster light mode with semantic Lucide icons, frosted glass header, formatting sampler (`B / I / U / S / AA`), and an 8-card Standard Formatting Matrix.
- **Multi-Foundry Discover & Custom Feed Subscriptions:** Browse **Google Fonts**, **Fontshare**, **Fontsource Open Foundry**, **Adobe & Dev Originals**, and **Font Squirrel**, or subscribe to your own **RSS / Atom XML, JSON Catalogs, and GitHub Releases** with `New Release` badges.
- **Forge Studio (Modular Font Creator):** Design custom fonts on a **`7×9` or `14×18` grid** with `A–Z`, `a–z`, `ÁÉÑ`, `0–9`, and symbol glyphs, parametric DNA sliders, live `.ttf` binary compilation, export, and one-click Windows installation.
- **Smart PC Font Scanner & OS Auto-Collections:** Automatically detect OS default font directories, scan Downloads/Desktop/Documents/Adobe folders, and organize collections algorithmically (zero ML).
- **Variable Font Playground & OpenType Inspector:** Live sliders for `wght`, `wdth`, `opsz`, `slnt`, and custom `fvar` axes; toggle `liga`, `dlig`, `smcp`, `onum`, `tnum`, `ss01`–`ss20`; inspect scripts, SHA-256 duplicates, and character maps.
- **Compare & CSS / Tailwind Token Export:** Compare 2–4 families stacked, side-by-side, or in a `Heading + Body` pairing sandbox with one-click `:root` CSS & Tailwind token copy on both pairings and collections.
- **Global Quick-Switcher (`Ctrl+Shift+F`), Watched Folders & BYO-Sync:** Load project fonts temporarily via `.cosmicfonts.json`, sync collections via any cloud/Syncthing folder (`cosmic-manifest.json`), or export client `.zip` bundles with `LICENSES.txt`.
- **Optional Local MCP & Agent Control (Hidden in Settings):** Reveal **Agent Hub** in Settings for a localhost-only (`127.0.0.1:17420`) MCP JSON-RPC 2.0 + REST control plane, read-only monitoring mode, deterministic command director, or BYO-AI (Ollama / OpenAI-compatible).

## Privacy & System Safety

- Everything runs locally on your machine. Network traffic only occurs when browsing Discover, syncing custom font feeds, or checking GitHub releases.
- Essential Windows system fonts (`Segoe UI`, `Arial`, `Consolas`, etc.) remain permanently locked; system fonts require explicit user unlock and UAC elevation, with backups stored prior to modification.
- Custom feed URLs enforce `is_public_url` SSRF checks against loopback and private network addresses.

## Build From Source

Requires [Node.js 22+](https://nodejs.org), [Rust](https://rustup.rs) stable, and the Windows WebView2 runtime.

```bash
git clone https://github.com/theCosmicCrafter/cosmic-font-foundry.git
cd cosmic-font-foundry
npm ci
npm run installer
```

Run checks & tests:

```bash
cargo fmt --manifest-path src-tauri/core/Cargo.toml -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/core/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

## Contributing & Community

- [Contributing Guide](CONTRIBUTING.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Security Policy](SECURITY.md)
- [Project Governance](GOVERNANCE.md) & [Maintainers](MAINTAINERS.md)
- [Changelog](CHANGELOG.md)

## License

[MIT](LICENSE). SPDX-compliant via [`REUSE.toml`](REUSE.toml). See [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for bundled font and library notices.

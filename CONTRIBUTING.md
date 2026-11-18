# Contributing to Cosmic Font Foundry

Thank you for your interest in contributing to **Cosmic Font Foundry**! Whether you are fixing a bug, adding a font foundry parser, improving accessibility, or polishing the UI, your help is welcome.

## Development Setup

### Prerequisites
- [Node.js 22+](https://nodejs.org/)
- [Rust (stable)](https://rustup.rs/) with `rustfmt` and `clippy`
- Windows 10/11 with WebView2 runtime (for full desktop Tauri builds & Windows registry integration tests)
- Optional FOSS quality tools: `gitleaks`, `reuse`, `pre-commit`

### Clone & Run Locally

```bash
git clone https://github.com/<your-username>/cosmic-font-foundry.git
cd cosmic-font-foundry
npm ci

# Fast browser UI preview (uses demo adapter for Tauri IPC)
npm run vite

# Full Tauri desktop app in dev mode (Windows)
npm run dev
```

## Verification Checklist Before Opening a PR

All pull requests must pass formatting, linting, unit tests, and frontend build checks:

```bash
# 1. Rust formatting
cargo fmt --manifest-path src-tauri/core/Cargo.toml -- --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

# 2. Rust linting (zero warnings enforced)
cargo clippy --manifest-path src-tauri/core/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

# 3. Tests
cargo test --manifest-path src-tauri/core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml

# 4. Frontend production bundle
npm run vite:build

# 5. License & secret hygiene (if installed locally)
reuse lint
gitleaks detect --source .
```

## Commit Style

We follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat(scope): ...` — new features
- `fix(scope): ...` — bug fixes
- `style(scope): ...` — UI, CSS, or formatting updates
- `docs(scope): ...` — documentation updates
- `test(scope): ...` — new or updated tests
- `chore(scope): ...` — tooling, CI, or dependency maintenance

## Pull Request Guidelines
- Keep PRs focused on a single concern.
- Include unit or integration tests for new Rust logic (parsers, inventory guards, MCP handlers).
- Do not bundle pre-trained ML runtimes or closed-source binaries.
- Ensure no hardcoded local machine paths or credentials are introduced.

## Summary
<!-- Briefly describe what this PR changes and why. -->

## Related Issue
<!-- Closes #... -->

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] UI / styling polish
- [ ] Documentation / FOSS maintenance
- [ ] Performance or refactoring

## Verification Checklist
- [ ] `cargo fmt --manifest-path src-tauri/core/Cargo.toml -- --check` and `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` pass
- [ ] `cargo clippy --manifest-path src-tauri/core/Cargo.toml --all-targets -- -D warnings` and `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` pass with 0 warnings
- [ ] `cargo test --manifest-path src-tauri/core/Cargo.toml` and `cargo test --manifest-path src-tauri/Cargo.toml` pass
- [ ] `npm run vite:build` builds cleanly
- [ ] No secrets, hardcoded local paths, or bundled ML weights introduced

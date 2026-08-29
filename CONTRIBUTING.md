# Contributing to Cooldown Bar

Thank you for helping improve Cooldown Bar.

## Before opening an issue

1. Search existing issues and discussions.

2. Confirm the problem on a supported macOS version.

3. Remove credentials, account identifiers, file contents, and private paths from logs.

4. Include the application version, Mac architecture, provider, expected result, and observed result.

## Development setup

```bash
npm ci
npm test
npm run build
npm run tauri dev
```

Run the Rust checks before submitting a pull request.

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

## Pull requests

1. Keep each change focused.

2. Explain what changed, why it was needed, and how it was verified.

3. Add tests when behavior changes and an existing test location fits the change.

4. Preserve the bounded process model, stale data rules, and accessibility preferences.

5. Update both language trees when user facing behavior or setup changes.

6. Use clear Conventional Commit messages when practical.

By contributing, you confirm that you have the right to submit the work. A project license has not yet been selected, so contributions do not change the copyright status of the repository.

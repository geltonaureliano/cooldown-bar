# Releases

The workflow in `.github/workflows/ci-release.yml` validates the project, builds a universal macOS application, verifies the package, and can publish a GitHub Release.

## Validation

Every pull request and branch push runs JavaScript tests, Python statusline tests, TypeScript compilation, the web build, Rust formatting, Clippy, Rust tests, and workflow validation.

The macOS job builds Apple Silicon and Intel binaries. It verifies both architectures, the application version, code signing, the DMG, artifact hashes, and build provenance.

## Publication

A push to the default branch can publish the manifest version when its tag does not exist. A manual run can publish only from the default branch. Existing published releases and tags are never replaced.

The release job receives only the verified artifact from the matching build. It creates a draft, uploads all assets, checks their remote hashes, and publishes only after every verification passes.

## Release notes

Notes combine an optional file at `release-notes/<version>.md`, GitHub pull request notes, and classified commit messages. The generator works without an external AI service or provider credentials.

Use clear Conventional Commit subjects such as `feat(ui): improve liquid motion`, `fix(poller): recover after wake`, or `docs: clarify installation`.

## Version update

```bash
npm run release:version -- patch
```

An explicit version is also accepted.

```bash
npm run release:version -- 0.0.2
```

The script updates npm, Cargo, and Tauri manifests together. It does not create a commit, tag, or push.

## Apple signing

Without Apple secrets, CI creates an ad hoc signed test build. With `APPLE_SIGNING_ENABLED=true` and the documented Developer ID credentials, the release build is signed, submitted for notarization, stapled, and verified.

The required secrets are `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`.

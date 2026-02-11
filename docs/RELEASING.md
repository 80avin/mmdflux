# Releasing

This project publishes:

- Crate releases to crates.io
- Binary release assets to GitHub Releases
- Homebrew formula updates in `kevinswiber/homebrew-mmdflux`

## Release Checklist

1. Ensure `main` is green in CI.
   - For routing-default promotion, run the dedicated checklist:
   - `docs/UNIFIED_ROUTING_PROMOTION.md`
2. Bump version in `Cargo.toml` and `Cargo.lock`.
3. Commit and push the version bump.
4. Publish crate:

```bash
cargo publish --locked
```

5. Tag and push:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

6. Confirm the GitHub `Release` workflow completes and uploads assets.

## GitHub Release Assets

The release workflow publishes these artifacts:

- `mmdflux-vX.Y.Z-darwin-arm64.tar.gz`
- `mmdflux-vX.Y.Z-darwin-x86_64.tar.gz`
- `mmdflux-vX.Y.Z-linux-x86_64.tar.gz`
- `mmdflux-vX.Y.Z-windows-x86_64.zip`
- `checksums.txt`

## Homebrew Tap

Tap repository:

- [kevinswiber/homebrew-mmdflux](https://github.com/kevinswiber/homebrew-mmdflux)

Install command for users:

```bash
brew tap kevinswiber/mmdflux
brew install mmdflux
```

### Updating Homebrew Formula for a New Release

1. Clone/update tap repo:

```bash
git clone git@github.com:kevinswiber/homebrew-mmdflux.git
cd homebrew-mmdflux
```

2. Pull release checksums:

```bash
gh release download vX.Y.Z --repo kevinswiber/mmdflux --pattern checksums.txt
cat checksums.txt
```

3. Update `Formula/mmdflux.rb` with new version, URLs, and SHA256 values.
4. Commit and push the formula update.

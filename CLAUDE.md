# Agora Forum — Developer Notes

## Release Process

**The version is set in ONE place: `Cargo.toml`.** All three crates must have matching versions:
- `crates/agora-common/Cargo.toml`
- `crates/agora-client/Cargo.toml`
- `crates/agora-server/Cargo.toml`

Everything else derives from `Cargo.toml` automatically:
- `agora-server --version` uses `env!("CARGO_PKG_VERSION")`
- `agora --version` uses `env!("CARGO_PKG_VERSION")` via clap
- `SERVER_VERSION` in `db.rs` uses `env!("CARGO_PKG_VERSION")`
- The `/version` API endpoint returns `SERVER_VERSION`

**To cut a release:**
1. Bump `version` in all three `Cargo.toml` files to the new version
2. **Pre-release sanity check** — before committing, review for obvious gaps:
   - Help text and `after_help` strings mention all current commands/actions
   - `guide.rs` documents all CLI commands, flags, and features
   - README.md and SERVER-GUIDE.md are up to date
   - No stale version numbers or hardcoded strings
3. Commit the version bump (and any fixes from step 2)
4. Tag with the **exact same version**: `git tag v<version>`
5. Push with tags: `git push origin master --tags`
6. Wait for CI to finish before telling anyone to upgrade

**The git tag and Cargo.toml version MUST match.** Never tag without bumping Cargo.toml first.

**CI builds are triggered by `v*` tags** (`.github/workflows/release.yml`). The release job uses `softprops/action-gh-release` with `make_latest: true`. The install scripts download from `/releases/latest/download/` so the binary isn't available until CI finishes.

**Do not tell the user to upgrade until the CI run is complete.** Check with `gh run list` or `gh run view <id>`.

## Install Scripts

- `install.sh` — Client installer. Users curl it directly, no clone needed.
- `install-server.sh` — Server installer. Users curl it directly, no clone needed. Supports `--upgrade` for binary-only updates.
- Both download pre-built binaries from GitHub releases (`/releases/latest/download/`).
- Server docs should always show the curl one-liner, never require `git clone` or Rust toolchain.

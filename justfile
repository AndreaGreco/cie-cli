# cie-cli - Task Runner
# Install just: cargo install just
# Usage: just <task>

set shell := ["bash", "-c"]

# Show available tasks
@default:
    just --list

# === Build ===

# Build debug binaries
build:
    cargo build --bin cie --bin cie-gui

# Build release binaries
release:
    cargo build --release --bin cie --bin cie-gui

# Run all workspace tests
test:
    cargo test --workspace

# Clippy on everything, warnings are errors
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Clean build artifacts
clean:
    cargo clean
    rm -rf target/distrib target/packager

# === Install (locale) ===

# Build release and install both binaries to ~/.local/bin
install: release
    install -Dm755 target/release/cie ~/.local/bin/cie
    install -Dm755 target/release/cie-gui ~/.local/bin/cie-gui
    @echo "Installed: ~/.local/bin/{cie,cie-gui}"

# === dist (CLI: archivi + installer .pkg/.msi) ===

# Preview release plan
dist-plan:
    dist plan

# Build the artifacts this machine can build
dist-build:
    dist build --artifacts all
    @ls -lh target/distrib/ 2>/dev/null || echo "No artifacts found"

# Regenerate the dist workflow and the WiX definition
dist-generate:
    dist generate

# === cargo-packager (GUI: .dmg / .exe / .deb / .AppImage) ===

# Package the GUI for the current platform.
# Formats: linux -> deb,appimage | macos -> app,dmg | windows -> nsis
#
# NO_STRIP: lo strip che linuxdeploy si porta dietro e' vecchio e si rifiuta di
# toccare una libpcsclite con la sezione .relr.dyn. Senza, l'AppImage non esce.
package-gui FORMATS="deb,appimage":
    NO_STRIP=1 cargo packager --release -p cie-gui --formats {{FORMATS}} --verbose

# === Release ===

# Create a new release (version, changelog, tag, push)
release-create VERSION:
    ./scripts/release.sh {{VERSION}}

# Create release without running tests
release-create-skip-tests VERSION:
    ./scripts/release.sh {{VERSION}} --skip-tests

# Prepare release locally without pushing
release-prepare VERSION:
    ./scripts/release.sh {{VERSION}} --no-push

# Show current version and unreleased changes
release-check:
    @echo "Current version: $(grep '^version =' Cargo.toml | head -1 | cut -d'"' -f2)"
    @echo ""
    @echo "Unreleased changes:"
    @sed -n '/^## \[Unreleased\]/,/^## \[/p' CHANGELOG.md | head -n -1

# Undo last release (local only, before push!)
release-undo:
    #!/usr/bin/env bash
    set -euo pipefail
    LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "no-tag")
    if [ "$LAST_TAG" = "no-tag" ]; then
        echo "No tags found"
        exit 1
    fi
    echo "Deleting tag: $LAST_TAG"
    git tag -d "$LAST_TAG"
    echo "Resetting last commit"
    git reset --hard HEAD~1
    echo "Release undone (local only)"

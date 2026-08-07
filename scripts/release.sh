#!/usr/bin/env bash
set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[i]${NC} $*"; }
success() { echo -e "${GREEN}[OK]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

if [ $# -eq 0 ]; then
    error "Usage: $0 <version> [--skip-tests] [--no-push]"
fi

VERSION="$1"
SKIP_TESTS=false
NO_PUSH=false

shift
while [ $# -gt 0 ]; do
    case "$1" in
        --skip-tests) SKIP_TESTS=true ;;
        --no-push) NO_PUSH=true ;;
        *) warn "Unknown flag: $1" ;;
    esac
    shift
done

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    error "Invalid version format. Expected: X.Y.Z or X.Y.Z-prerelease"
fi

VERSION="${VERSION#v}"
TAG="v${VERSION}"

TRACKED_FILES=(Cargo.toml Cargo.lock CHANGELOG.md dist-workspace.toml .github/workflows/release.yml)

info "Preparing release ${YELLOW}${TAG}${NC}"
echo

if ! git diff-index --quiet HEAD --; then
    error "You have uncommitted changes. Please commit or stash them first."
fi

# A local underskrift override makes Cargo.lock point at a path no runner can
# resolve; the release would build something nobody else can reproduce.
if [ -f .cargo/config.toml ]; then
    error ".cargo/config.toml is active (local underskrift override). Remove it before releasing."
fi
if ! grep -q 'source = "git+.*underskrift' Cargo.lock; then
    error "Cargo.lock does not resolve underskrift from git. Run 'cargo metadata' without the local override."
fi

CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$CURRENT_BRANCH" != "main" ]; then
    warn "You're not on 'main' branch (current: ${CURRENT_BRANCH})"
    read -p "Continue anyway? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        error "Release cancelled"
    fi
fi

revert_files() {
    warn "Reverting file changes..."
    git checkout "${TRACKED_FILES[@]}" 2>/dev/null || true
}

# 1. Update the workspace version
info "Updating Cargo.toml version to ${VERSION}..."
cp Cargo.toml Cargo.toml.bak
if sed -i.tmp "s/^version = \".*\"$/version = \"${VERSION}\"/" Cargo.toml && \
   grep -q "version = \"${VERSION}\"" Cargo.toml; then
    rm -f Cargo.toml.tmp Cargo.toml.bak
    success "Updated Cargo.toml"
else
    mv Cargo.toml.bak Cargo.toml
    error "Failed to update Cargo.toml"
fi

# 2. Open a new CHANGELOG section
info "Updating CHANGELOG.md..."
TODAY=$(date +%Y-%m-%d)
cp CHANGELOG.md CHANGELOG.md.bak
if awk -v ver="$VERSION" -v date="$TODAY" '
    /^## \[Unreleased\]/ {
        print "## [Unreleased]\n"
        print "## [" ver "] - " date
        next
    }
    { print }
' CHANGELOG.md > CHANGELOG.md.tmp && mv CHANGELOG.md.tmp CHANGELOG.md; then
    rm -f CHANGELOG.md.bak
    success "Updated CHANGELOG.md"
else
    mv CHANGELOG.md.bak CHANGELOG.md
    error "Failed to update CHANGELOG.md"
fi

# 3. Tests, with the bumped manifest in place
if [ "$SKIP_TESTS" = false ]; then
    info "Running tests..."
    if cargo test --workspace --quiet; then
        success "All tests passed"
    else
        revert_files
        error "Tests failed. Fix them or use --skip-tests to skip."
    fi
else
    warn "Skipping tests (--skip-tests flag)"
fi
echo

# 4. Both binaries must build in release before anything is tagged
info "Building release binaries..."
if cargo build --release --bin cie --bin cie-gui --quiet; then
    success "Release build successful"
else
    revert_files
    error "Release build failed"
fi
echo

# 5. Refresh the dist-generated workflow
info "Refreshing dist generated files..."
if ! dist --version > /dev/null 2>&1; then
    revert_files
    error "dist is required. Install with: cargo install cargo-dist --locked --version 0.30.3"
fi
if dist generate > /dev/null 2>&1; then
    success "dist workflow regenerated"
else
    revert_files
    error "Failed to regenerate dist workflow"
fi
echo

# 6. Show and confirm
info "Changes to be committed:"
git diff --color=always "${TRACKED_FILES[@]}" | head -80
echo
read -p "$(echo -e ${GREEN}Commit these changes?${NC} [Y/n] )" -n 1 -r
echo
if [[ $REPLY =~ ^[Nn]$ ]]; then
    revert_files
    error "Release cancelled"
fi

info "Creating commit..."
git add "${TRACKED_FILES[@]}"
git commit -m "chore: bump version to ${TAG}" || error "Failed to create commit"
success "Commit created"

# 7. dist preflight: the same planning step CI runs, before the tag exists
echo
info "Running dist preflight..."
if dist host --steps=create --tag="${TAG}" --output-format=json > /dev/null; then
    success "dist preflight passed"
else
    error "dist preflight failed. Fix the reported issue before creating the tag."
fi

# 8. Tag
info "Creating tag ${TAG}..."
git tag -a "$TAG" -m "Release ${TAG}" || error "Failed to create tag"
success "Tag created: ${TAG}"

# 9. Push
echo
if [ "$NO_PUSH" = true ]; then
    warn "Skipping push (--no-push flag)"
    echo
    info "To push manually, run:"
    echo "  git push origin ${CURRENT_BRANCH}"
    echo "  git push origin ${TAG}"
    exit 0
fi

echo -e "${YELLOW}════════════════════════════════════════${NC}"
echo -e "${YELLOW}Ready to push to remote!${NC}"
echo -e "${YELLOW}════════════════════════════════════════${NC}"
echo
info "This will:"
echo "  1. Push commit to origin/${CURRENT_BRANCH}"
echo "  2. Push tag ${TAG}"
echo "  3. Trigger GitHub Actions:"
echo "     - Release (dist): archives, shell/powershell installers, .pkg, .msi"
echo "     - Bundle GUI (cargo-packager): .dmg, NSIS .exe, .deb, .AppImage"
echo
read -p "$(echo -e ${GREEN}Push to remote?${NC} [Y/n] )" -n 1 -r
echo
if [[ $REPLY =~ ^[Nn]$ ]]; then
    warn "Push cancelled. To push later, run:"
    echo "  git push origin ${CURRENT_BRANCH}"
    echo "  git push origin ${TAG}"
    exit 0
fi

info "Pushing to remote..."
if git push origin "$CURRENT_BRANCH" && git push origin "$TAG"; then
    success "Successfully pushed to remote!"
    echo
    info "Next steps:"
    echo "  1. Monitor: https://github.com/AttilioGreco/cie-cli/actions"
    echo "  2. Release: https://github.com/AttilioGreco/cie-cli/releases/tag/${TAG}"
else
    error "Failed to push to remote"
fi

echo
success "Done!"

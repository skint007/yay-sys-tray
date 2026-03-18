#!/bin/bash
set -euo pipefail

PKGBUILD="aur/PKGBUILD"

if [ $# -ne 1 ]; then
    echo "Usage: $0 <version>  (e.g. $0 0.7.0)"
    exit 1
fi

VERSION="$1"

# Ensure working tree is clean (ignoring PKGBUILD which we're about to change)
if [ -n "$(git status --porcelain -- ':!aur/PKGBUILD')" ]; then
    echo "Error: working tree has uncommitted changes"
    exit 1
fi

# Stamp version into PKGBUILD
sed -i "s/^pkgver=.*/pkgver=${VERSION}/" "$PKGBUILD"

# Commit + tag + push to GitHub
git add "$PKGBUILD"
if ! git diff --cached --quiet; then
    git commit -m "Release v${VERSION}"
fi
git tag -a "v${VERSION}" -m "v${VERSION}"
git push origin master "v${VERSION}"

echo "Released v${VERSION}"

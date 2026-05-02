#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

LOCAL=0
if [[ "${1:-}" == "--local" ]]; then
    LOCAL=1
fi

if [ "$LOCAL" -eq 1 ]; then
    if [ -n "$(git status --porcelain -- PKGBUILD)" ]; then
        echo "Error: PKGBUILD has uncommitted changes; commit or stash first" >&2
        exit 1
    fi
    if [ -n "$(git -C .. status --porcelain -- ':!aur/PKGBUILD')" ]; then
        echo "Note: git+file uses HEAD — uncommitted changes outside PKGBUILD will not be in this build" >&2
    fi

    trap 'git checkout -- PKGBUILD; rm -rf src pkg yay-sys-tray' EXIT

    REPO_ROOT="$(cd .. && pwd)"
    sed -i "s|^source=.*|source=(\"git+file://${REPO_ROOT}\")|" PKGBUILD

    rm -rf src pkg yay-sys-tray
fi

makepkg -si --noconfirm
systemctl --user restart yay-sys-tray.service

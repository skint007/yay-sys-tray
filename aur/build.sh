#!/bin/bash
# Build & install the AUR package.
#
#   ./build.sh            Build from the published GitHub repo (HEAD of master).
#   ./build.sh --local    Build from your current working tree, including
#                         uncommitted and untracked changes (respecting
#                         .gitignore) and any local PKGBUILD edits. No commit
#                         required.
set -euo pipefail

cd "$(dirname "$0")"

LOCAL=0
if [[ "${1:-}" == "--local" ]]; then
    LOCAL=1
fi

if [ "$LOCAL" -eq 1 ]; then
    REPO_ROOT="$(cd .. && pwd)"

    # Snapshot the working tree into a throwaway git repo so the git-based
    # PKGBUILD can build exactly what's on disk. We clone for the commit
    # history + tags (needed by pkgver()), then overlay the working tree on
    # top as a single commit so uncommitted/untracked edits are included.
    SNAP="$(mktemp -d)"
    # Back up the PKGBUILD so we can rewrite only its source= line and restore
    # it verbatim afterwards (preserving any uncommitted PKGBUILD edits too).
    PKGBUILD_BAK="$(mktemp)"
    cp PKGBUILD "$PKGBUILD_BAK"
    trap 'cp "$PKGBUILD_BAK" PKGBUILD; rm -f "$PKGBUILD_BAK"; rm -rf src pkg yay-sys-tray "$SNAP"' EXIT

    git clone --quiet --local --no-hardlinks "$REPO_ROOT" "$SNAP"

    # Copy modified + untracked (non-ignored) files; drop working-tree deletions.
    (
        cd "$REPO_ROOT"
        git ls-files --modified --others --exclude-standard -z \
            | rsync -a --files-from=- --from0 ./ "$SNAP/"
        git ls-files --deleted -z | while IFS= read -r -d '' f; do rm -f "$SNAP/$f"; done
    )

    git -C "$SNAP" add -A
    if ! git -C "$SNAP" diff --cached --quiet; then
        git -C "$SNAP" -c user.email=local@build -c user.name=local \
            commit --quiet -m 'local working-tree snapshot'
        echo "==> Building local snapshot (includes uncommitted changes)"
    else
        echo "==> Building local snapshot (working tree matches HEAD)"
    fi

    # Name the checkout "yay-sys-tray" so it matches the `cd yay-sys-tray`
    # references and pkgver()'s directory check in the PKGBUILD.
    sed -i "s|^source=.*|source=(\"yay-sys-tray::git+file://${SNAP}\")|" PKGBUILD
    rm -rf src pkg yay-sys-tray
fi

# -f forces a rebuild even if a package with this version was built before.
makepkg -sif --noconfirm
systemctl --user restart yay-sys-tray.service

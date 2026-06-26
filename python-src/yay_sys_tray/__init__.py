import re
import subprocess
from pathlib import Path


def _get_version() -> str:
    """Compute version from git tags (matching the PKGBUILD pkgver() formula).

    On a tagged commit (e.g. v1.0.0):         returns '1.0.0'
    After a tag (e.g. v1.0.0, 3 commits on):  returns '1.0.0.3.abc1234'
    No tags at all:                            returns 'r{count}.{short}'
    Not a git repo (installed package):        returns '0.1.0'
    """
    try:
        src = Path(__file__).parent
        desc = subprocess.run(
            ["git", "describe", "--tags", "--long", "--abbrev=7"],
            capture_output=True, text=True, cwd=src,
        ).stdout.strip()
        if desc:
            v = re.sub(r"^v", "", desc)
            v = re.sub(r"-0-g[0-9a-f]+$", "", v)               # exactly on tag
            v = re.sub(r"-(\d+)-g([0-9a-f]+)$", r".\1.\2", v)  # post-tag commits
            return v
        # No tags yet — fall back to commit counting
        count = subprocess.run(
            ["git", "rev-list", "--count", "HEAD"],
            capture_output=True, text=True, cwd=src,
        ).stdout.strip()
        short = subprocess.run(
            ["git", "rev-parse", "--short=7", "HEAD"],
            capture_output=True, text=True, cwd=src,
        ).stdout.strip()
        if count and short:
            return f"r{count}.{short}"
    except Exception:
        pass
    return "0.1.0"


__version__ = _get_version()

import subprocess
from pathlib import Path


def _get_version() -> str:
    """Compute version using the same formula as the PKGBUILD pkgver() function.

    Returns e.g. 'r28.5ee1ba9' when run from a git repo, or '0.1.0' as fallback.
    """
    try:
        src = Path(__file__).parent
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

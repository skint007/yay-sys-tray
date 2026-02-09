import json
import shutil
from dataclasses import asdict, dataclass
from pathlib import Path

CONFIG_DIR = Path.home() / ".config" / "yay-sys-tray"
CONFIG_FILE = CONFIG_DIR / "config.json"

DESKTOP_ENTRY = """\
[Desktop Entry]
Type=Application
Name=Yay Update Checker
Comment=System tray update checker for Arch Linux
Exec=yay-sys-tray
Icon=system-software-update
Terminal=false
Categories=System;
StartupNotify=false
X-GNOME-Autostart-enabled=true
"""


def _detect_terminal() -> str:
    for term in ("kitty", "alacritty", "konsole", "xterm"):
        if shutil.which(term):
            return term
    return "xterm"


@dataclass
class AppConfig:
    check_interval_minutes: int = 60
    notify: str = "new_only"  # "always" | "new_only" | "never"
    terminal: str = ""
    autostart: bool = False

    def __post_init__(self):
        if not self.terminal:
            self.terminal = _detect_terminal()

    @classmethod
    def load(cls) -> "AppConfig":
        if CONFIG_FILE.exists():
            try:
                data = json.loads(CONFIG_FILE.read_text())
                return cls(
                    **{k: v for k, v in data.items() if k in cls.__dataclass_fields__}
                )
            except (json.JSONDecodeError, TypeError):
                pass
        return cls()

    def save(self) -> None:
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        CONFIG_FILE.write_text(json.dumps(asdict(self), indent=2) + "\n")

    def manage_autostart(self) -> None:
        autostart_dir = Path.home() / ".config" / "autostart"
        desktop_file = autostart_dir / "yay-sys-tray.desktop"
        if self.autostart:
            autostart_dir.mkdir(parents=True, exist_ok=True)
            desktop_file.write_text(DESKTOP_ENTRY)
        elif desktop_file.exists():
            desktop_file.unlink()

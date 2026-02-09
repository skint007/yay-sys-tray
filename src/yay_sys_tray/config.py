import json
import shutil
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path

CONFIG_DIR = Path.home() / ".config" / "yay-sys-tray"
CONFIG_FILE = CONFIG_DIR / "config.json"

SERVICE_NAME = "yay-sys-tray.service"


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
    noconfirm: bool = False
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
        action = "enable" if self.autostart else "disable"
        subprocess.run(
            ["systemctl", "--user", action, SERVICE_NAME],
            capture_output=True,
        )

import getpass
import json
import shutil
import subprocess
from dataclasses import asdict, dataclass
from pathlib import Path

CONFIG_DIR = Path.home() / ".config" / "yay-sys-tray"
CONFIG_FILE = CONFIG_DIR / "config.json"

SERVICE_NAME = "yay-sys-tray.service"


def is_arch_linux() -> bool:
    """Check if running on an Arch-based Linux distribution."""
    return Path("/etc/arch-release").exists()


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
    animations: bool = True
    recheck_interval_minutes: int = 5
    passwordless_updates: bool = False
    # Tailscale remote checking
    tailscale_enabled: bool = False
    tailscale_tags: str = "server,arch"
    tailscale_timeout: int = 10

    def __post_init__(self):
        if not self.terminal:
            self.terminal = _detect_terminal()
        # Migrate old "tag:server,tag:arch" format to "server,arch"
        if "tag:" in self.tailscale_tags:
            self.tailscale_tags = ",".join(
                t.strip().removeprefix("tag:")
                for t in self.tailscale_tags.split(",")
                if t.strip()
            )

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
        if not is_arch_linux():
            return
        action = "enable" if self.autostart else "disable"
        subprocess.run(
            ["systemctl", "--user", action, SERVICE_NAME],
            capture_output=True,
        )

    SUDOERS_FILE = "/etc/sudoers.d/yay-sys-tray"

    def manage_passwordless_updates(self) -> bool:
        """Create or remove sudoers NOPASSWD rule for pacman. Returns True on success."""
        if not is_arch_linux():
            return False
        if self.passwordless_updates:
            username = getpass.getuser()
            rule = f"{username} ALL=(ALL) NOPASSWD: /usr/bin/pacman"
            result = subprocess.run(
                [
                    "pkexec", "bash", "-c",
                    f'printf "%s\\n" "{rule}" > {self.SUDOERS_FILE}'
                    f" && chmod 440 {self.SUDOERS_FILE}",
                ],
                capture_output=True,
            )
            return result.returncode == 0
        else:
            result = subprocess.run(
                ["pkexec", "rm", "-f", self.SUDOERS_FILE],
                capture_output=True,
            )
            return result.returncode == 0

import os
import subprocess
from dataclasses import dataclass

from PyQt6.QtCore import QThread, pyqtSignal

# Packages that require a system restart when updated
RESTART_PACKAGES = {
    "linux",
    "linux-lts",
    "linux-zen",
    "linux-hardened",
    "systemd",
    "glibc",
    "nvidia",
    "nvidia-lts",
}


@dataclass
class UpdateInfo:
    package: str
    old_version: str
    new_version: str


@dataclass
class RebootInfo:
    needed: bool
    running_kernel: str
    installed_kernel: str


@dataclass
class CheckResult:
    updates: list[UpdateInfo]
    needs_restart: bool
    restart_packages: list[str]
    reboot_info: RebootInfo | None = None


def parse_update_output(output: str) -> list[UpdateInfo]:
    """Parse 'package old_version -> new_version' lines into UpdateInfo list."""
    updates = []
    for line in output.strip().splitlines():
        line = line.strip()
        if not line or " -> " not in line:
            continue
        parts = line.split()
        if len(parts) >= 4 and parts[-2] == "->":
            updates.append(
                UpdateInfo(
                    package=parts[0],
                    old_version=parts[1],
                    new_version=parts[-1],
                )
            )
    return updates


def check_reboot_needed() -> RebootInfo:
    """Check if a reboot is needed by looking for the running kernel's modules."""
    running = subprocess.run(
        ["uname", "-r"], capture_output=True, text=True,
    ).stdout.strip()

    modules_exist = os.path.isdir(f"/lib/modules/{running}")

    # Detect which kernel package corresponds to the running kernel
    if "-zen" in running:
        pkg = "linux-zen"
    elif "-hardened" in running:
        pkg = "linux-hardened"
    elif "-lts" in running:
        pkg = "linux-lts"
    else:
        pkg = "linux"

    installed = ""
    try:
        result = subprocess.run(
            ["pacman", "-Q", pkg], capture_output=True, text=True,
        )
        if result.returncode == 0:
            parts = result.stdout.strip().split()
            if len(parts) >= 2:
                installed = parts[1]
    except Exception:
        pass

    return RebootInfo(
        needed=not modules_exist,
        running_kernel=running,
        installed_kernel=installed,
    )


class UpdateChecker(QThread):
    check_complete = pyqtSignal(object)  # CheckResult
    check_error = pyqtSignal(str)

    def run(self):
        try:
            updates = []

            # checkupdates syncs a temp database copy, so results are always fresh
            repo = subprocess.run(
                ["checkupdates"],
                capture_output=True,
                text=True,
                timeout=120,
            )
            # checkupdates: exit 0 = updates, exit 2 = no updates, exit 1 = error
            if repo.returncode == 1:
                self.check_error.emit(
                    f"checkupdates error: {repo.stderr.strip()}"
                )
                return
            if repo.returncode == 0:
                updates.extend(parse_update_output(repo.stdout))

            # Check AUR packages separately via yay
            aur = subprocess.run(
                ["yay", "-Qua"],
                capture_output=True,
                text=True,
                timeout=120,
            )
            # yay -Qua: exit 0 = updates, exit 1 = no updates
            if aur.returncode == 0 and aur.stdout.strip():
                updates.extend(parse_update_output(aur.stdout))

            restart_pkgs = [u.package for u in updates if u.package in RESTART_PACKAGES]
            result = CheckResult(
                updates=updates,
                needs_restart=len(restart_pkgs) > 0,
                restart_packages=restart_pkgs,
                reboot_info=check_reboot_needed(),
            )
            self.check_complete.emit(result)
        except FileNotFoundError as e:
            self.check_error.emit(f"Command not found: {e.filename}")
        except subprocess.TimeoutExpired:
            self.check_error.emit("Update check timed out after 120 seconds")
        except Exception as e:
            self.check_error.emit(str(e))

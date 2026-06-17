import os
import subprocess
from dataclasses import dataclass

from PyQt6.QtCore import QThread, pyqtSignal

from yay_sys_tray.config import SUBPROCESS_HIDDEN

# Kernel packages: only the flavor you're booted into requires a restart when
# updated (e.g. a mainline `linux` update shouldn't demand a reboot when you're
# running `linux-lts`).
KERNEL_PACKAGES = {
    "linux",
    "linux-lts",
    "linux-zen",
    "linux-hardened",
}

# Packages that always require a system restart when updated.
ALWAYS_RESTART_PACKAGES = {
    "systemd",
    "glibc",
    "nvidia",
    "nvidia-lts",
}

# Full set of restart-relevant packages (any flavor), for reference.
RESTART_PACKAGES = KERNEL_PACKAGES | ALWAYS_RESTART_PACKAGES


@dataclass
class UpdateInfo:
    package: str
    old_version: str
    new_version: str
    description: str = ""
    repository: str = ""
    url: str = ""
    requires_restart: bool = False


def kernel_package_for(release: str) -> str:
    """Map a `uname -r` release string to its kernel package name."""
    if "-zen" in release:
        return "linux-zen"
    if "-hardened" in release:
        return "linux-hardened"
    if "-lts" in release:
        return "linux-lts"
    return "linux"


def package_requires_restart(package: str, running_kernel_pkg: str | None) -> bool:
    """Whether updating `package` requires a restart.

    For kernel packages, only the running flavor counts. If the running kernel
    can't be determined (running_kernel_pkg is None), kernel updates are treated
    conservatively as requiring a restart.
    """
    if package in KERNEL_PACKAGES:
        return running_kernel_pkg is None or package == running_kernel_pkg
    return package in ALWAYS_RESTART_PACKAGES


def mark_restart_packages(
    updates: list["UpdateInfo"], running_kernel_pkg: str | None
) -> list[str]:
    """Set each update's requires_restart flag; return restart-requiring names."""
    names = []
    for u in updates:
        u.requires_restart = package_requires_restart(u.package, running_kernel_pkg)
        if u.requires_restart:
            names.append(u.package)
    return names


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


def fetch_descriptions(packages: list[str]) -> dict[str, str]:
    """Fetch package descriptions from the local pacman database."""
    if not packages:
        return {}
    try:
        result = subprocess.run(
            ["pacman", "-Qi"] + packages,
            capture_output=True, text=True, timeout=10, **SUBPROCESS_HIDDEN,
        )
        descriptions: dict[str, str] = {}
        name = None
        for line in result.stdout.splitlines():
            if line.startswith("Name"):
                name = line.split(":", 1)[1].strip()
            elif line.startswith("Description") and name:
                descriptions[name] = line.split(":", 1)[1].strip()
        return descriptions
    except Exception:
        return {}


def fetch_repositories(packages: list[str]) -> dict[str, tuple[str, str]]:
    """Fetch repository name and architecture from the pacman sync database."""
    if not packages:
        return {}
    try:
        result = subprocess.run(
            ["pacman", "-Si"] + packages,
            capture_output=True, text=True, timeout=10, **SUBPROCESS_HIDDEN,
        )
        repos: dict[str, tuple[str, str]] = {}
        current_repo = ""
        current_name = ""
        for line in result.stdout.splitlines():
            if line.startswith("Repository"):
                current_repo = line.split(":", 1)[1].strip()
            elif line.startswith("Name"):
                current_name = line.split(":", 1)[1].strip()
            elif line.startswith("Architecture") and current_name:
                arch = line.split(":", 1)[1].strip()
                repos[current_name] = (current_repo, arch)
                current_name = ""
        return repos
    except Exception:
        return {}


def check_reboot_needed(running: str | None = None) -> RebootInfo:
    """Check if a reboot is needed by looking for the running kernel's modules."""
    if running is None:
        running = subprocess.run(
            ["uname", "-r"], capture_output=True, text=True, **SUBPROCESS_HIDDEN,
        ).stdout.strip()

    modules_exist = os.path.isdir(f"/lib/modules/{running}")

    # Detect which kernel package corresponds to the running kernel
    pkg = kernel_package_for(running)

    installed = ""
    try:
        result = subprocess.run(
            ["pacman", "-Q", pkg], capture_output=True, text=True, **SUBPROCESS_HIDDEN,
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
            repo_packages: list[UpdateInfo] = []

            # checkupdates syncs a temp database copy, so results are always fresh
            repo = subprocess.run(
                ["checkupdates"],
                capture_output=True,
                text=True,
                timeout=120,
                **SUBPROCESS_HIDDEN,
            )
            # checkupdates: exit 0 = updates, exit 2 = no updates, exit 1 = error
            if repo.returncode == 1:
                self.check_error.emit(
                    f"checkupdates error: {repo.stderr.strip()}"
                )
                return
            if repo.returncode == 0:
                repo_packages = parse_update_output(repo.stdout)
                updates.extend(repo_packages)

            # Check AUR packages separately via yay
            aur = subprocess.run(
                ["yay", "-Qua"],
                capture_output=True,
                text=True,
                timeout=120,
                **SUBPROCESS_HIDDEN,
            )
            # yay -Qua: exit 0 = updates, exit 1 = no updates
            if aur.returncode == 0 and aur.stdout.strip():
                aur_packages = parse_update_output(aur.stdout)
                for u in aur_packages:
                    u.repository = "aur"
                    u.url = f"https://aur.archlinux.org/packages/{u.package}"
                updates.extend(aur_packages)

            descs = fetch_descriptions([u.package for u in updates])
            for u in updates:
                u.description = descs.get(u.package, "")

            if repo_packages:
                repos = fetch_repositories([u.package for u in repo_packages])
                for u in repo_packages:
                    info = repos.get(u.package)
                    if info:
                        u.repository, arch = info
                        u.url = f"https://archlinux.org/packages/{u.repository}/{arch}/{u.package}/"

            running_release = subprocess.run(
                ["uname", "-r"], capture_output=True, text=True, **SUBPROCESS_HIDDEN,
            ).stdout.strip()
            running_pkg = kernel_package_for(running_release)
            restart_pkgs = mark_restart_packages(updates, running_pkg)
            result = CheckResult(
                updates=updates,
                needs_restart=len(restart_pkgs) > 0,
                restart_packages=restart_pkgs,
                reboot_info=check_reboot_needed(running_release),
            )
            self.check_complete.emit(result)
        except FileNotFoundError as e:
            self.check_error.emit(f"Command not found: {e.filename}")
        except subprocess.TimeoutExpired:
            self.check_error.emit("Update check timed out after 120 seconds")
        except Exception as e:
            self.check_error.emit(str(e))

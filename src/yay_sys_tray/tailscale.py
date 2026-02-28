import json
import subprocess
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field

from PyQt6.QtCore import QThread, pyqtSignal

from yay_sys_tray.checker import RESTART_PACKAGES, UpdateInfo, parse_update_output

SSH_OPTS = [
    "-o", "ServerAliveInterval=5",
    "-o", "ServerAliveCountMax=2",
    "-o", "BatchMode=yes",
    "-o", "StrictHostKeyChecking=no",
]


@dataclass
class HostResult:
    hostname: str
    updates: list[UpdateInfo] = field(default_factory=list)
    needs_restart: bool = False
    restart_packages: list[str] = field(default_factory=list)
    error: str | None = None


@dataclass
class RemoteCheckResult:
    hosts: list[HostResult]


def discover_all_tags() -> list[str]:
    """Get all unique tag names (without 'tag:' prefix) from Tailscale peers."""
    try:
        result = subprocess.run(
            ["tailscale", "status", "--json"],
            capture_output=True,
            text=True,
            timeout=15,
        )
        if result.returncode != 0:
            return []
        data = json.loads(result.stdout)
        peers = data.get("Peer", {})
        tags: set[str] = set()
        for peer in peers.values():
            for tag in peer.get("Tags") or []:
                if tag.startswith("tag:"):
                    tags.add(tag[4:])
                else:
                    tags.add(tag)
        return sorted(tags)
    except Exception:
        return []


def discover_peers(tags: list[str]) -> list[str]:
    """Get online Tailscale peers whose tags contain ALL specified tags."""
    result = subprocess.run(
        ["tailscale", "status", "--json"],
        capture_output=True,
        text=True,
        timeout=15,
    )
    if result.returncode != 0:
        return []

    data = json.loads(result.stdout)
    peers = data.get("Peer", {})
    hostnames = []

    for peer in peers.values():
        if not peer.get("Online", False):
            continue
        peer_tags = peer.get("Tags") or []
        if all(tag in peer_tags for tag in tags):
            hostname = peer.get("HostName", "")
            if hostname:
                hostnames.append(hostname)

    return sorted(hostnames)


def _ssh_run(hostname: str, command: str, timeout: int) -> subprocess.CompletedProcess[str]:
    """Run a command on a remote host via SSH."""
    ssh_opts = [
        "-o", f"ConnectTimeout={timeout}",
        *SSH_OPTS,
    ]
    return subprocess.run(
        ["ssh", *ssh_opts, hostname, command],
        capture_output=True,
        text=True,
        timeout=timeout + 30,
    )


def _fetch_remote_descriptions(
    hostname: str, packages: list[str], timeout: int
) -> dict[str, str]:
    """Fetch package descriptions from a remote host's pacman database."""
    if not packages:
        return {}
    try:
        pkg_args = " ".join(packages)
        result = _ssh_run(hostname, f"pacman -Qi {pkg_args}", timeout)
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


def _fetch_remote_repositories(
    hostname: str, packages: list[str], timeout: int
) -> dict[str, tuple[str, str]]:
    """Fetch repository and architecture from a remote host's pacman sync database."""
    if not packages:
        return {}
    try:
        pkg_args = " ".join(packages)
        result = _ssh_run(hostname, f"pacman -Si {pkg_args}", timeout)
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


def check_host(hostname: str, timeout: int) -> HostResult:
    """SSH into a host and run checkupdates to check for updates."""
    try:
        result = _ssh_run(hostname, "checkupdates", timeout)
        # checkupdates: exit 0 = updates, exit 2 = no updates, exit 1 = error
        if result.returncode == 0 and result.stdout.strip():
            updates = parse_update_output(result.stdout)

            # Enrich with descriptions and repository info
            pkg_names = [u.package for u in updates]
            descs = _fetch_remote_descriptions(hostname, pkg_names, timeout)
            repos = _fetch_remote_repositories(hostname, pkg_names, timeout)
            for u in updates:
                u.description = descs.get(u.package, "")
                info = repos.get(u.package)
                if info:
                    u.repository, arch = info
                    u.url = f"https://archlinux.org/packages/{u.repository}/{arch}/{u.package}/"

            restart_pkgs = [u.package for u in updates if u.package in RESTART_PACKAGES]
            return HostResult(
                hostname=hostname,
                updates=updates,
                needs_restart=len(restart_pkgs) > 0,
                restart_packages=restart_pkgs,
            )
        return HostResult(hostname=hostname)
    except subprocess.TimeoutExpired:
        return HostResult(hostname=hostname, error="Connection timed out")
    except FileNotFoundError:
        return HostResult(hostname=hostname, error="ssh not found")
    except Exception as e:
        return HostResult(hostname=hostname, error=str(e))


class TailscaleChecker(QThread):
    check_complete = pyqtSignal(object)  # RemoteCheckResult
    check_error = pyqtSignal(str)

    def __init__(self, tags: list[str], timeout: int):
        super().__init__()
        self.tags = tags
        self.timeout = timeout

    def run(self):
        try:
            hostnames = discover_peers(self.tags)
            if not hostnames:
                self.check_complete.emit(RemoteCheckResult(hosts=[]))
                return

            results = []
            max_workers = min(len(hostnames), 8)
            with ThreadPoolExecutor(max_workers=max_workers) as pool:
                futures = {
                    pool.submit(check_host, h, self.timeout): h
                    for h in hostnames
                }
                for future in as_completed(futures):
                    results.append(future.result())

            results.sort(key=lambda r: r.hostname)
            self.check_complete.emit(RemoteCheckResult(hosts=results))
        except FileNotFoundError:
            self.check_error.emit("tailscale command not found")
        except Exception as e:
            self.check_error.emit(f"Tailscale check failed: {e}")

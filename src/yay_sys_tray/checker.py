import subprocess
from dataclasses import dataclass

from PyQt6.QtCore import QThread, pyqtSignal


@dataclass
class UpdateInfo:
    package: str
    old_version: str
    new_version: str


class UpdateChecker(QThread):
    check_complete = pyqtSignal(list)  # list[UpdateInfo]
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
                updates.extend(self._parse_output(repo.stdout))

            # Check AUR packages separately via yay
            aur = subprocess.run(
                ["yay", "-Qua"],
                capture_output=True,
                text=True,
                timeout=120,
            )
            # yay -Qua: exit 0 = updates, exit 1 = no updates
            if aur.returncode == 0 and aur.stdout.strip():
                updates.extend(self._parse_output(aur.stdout))

            self.check_complete.emit(updates)
        except FileNotFoundError as e:
            self.check_error.emit(f"Command not found: {e.filename}")
        except subprocess.TimeoutExpired:
            self.check_error.emit("Update check timed out after 120 seconds")
        except Exception as e:
            self.check_error.emit(str(e))

    @staticmethod
    def _parse_output(output: str) -> list[UpdateInfo]:
        updates = []
        for line in output.strip().splitlines():
            line = line.strip()
            if not line or " -> " not in line:
                continue
            parts = line.split()
            # Format: "package old_version -> new_version"
            if len(parts) >= 4 and parts[-2] == "->":
                updates.append(
                    UpdateInfo(
                        package=parts[0],
                        old_version=parts[1],
                        new_version=parts[-1],
                    )
                )
        return updates

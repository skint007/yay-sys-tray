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
            result = subprocess.run(
                ["yay", "-Qu"],
                capture_output=True,
                text=True,
                timeout=120,
            )
            # yay/pacman: exit 0 = updates available, exit 1 = no updates
            if result.returncode == 1 and not result.stdout.strip():
                self.check_complete.emit([])
                return
            if result.returncode not in (0, 1):
                self.check_error.emit(
                    f"yay exited with code {result.returncode}: {result.stderr.strip()}"
                )
                return
            updates = self._parse_output(result.stdout)
            self.check_complete.emit(updates)
        except FileNotFoundError:
            self.check_error.emit("yay not found. Is it installed?")
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

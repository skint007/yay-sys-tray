import subprocess
from datetime import datetime

from PyQt6.QtCore import QObject, QTimer
from PyQt6.QtGui import QAction
from PyQt6.QtWidgets import QApplication, QMenu, QSystemTrayIcon

from yay_sys_tray.checker import CheckResult, UpdateChecker, UpdateInfo
from yay_sys_tray.config import AppConfig
from yay_sys_tray.icons import (
    create_checking_icon,
    create_error_icon,
    create_ok_icon,
    create_restart_icon,
    create_updates_icon,
)

TERMINAL_CMDS = {
    "kitty": ["kitty", "--hold"],
    "konsole": ["konsole", "--hold", "-e"],
    "alacritty": ["alacritty", "--hold", "-e"],
    "foot": ["foot", "--hold"],
    "xterm": ["xterm", "-hold", "-e"],
}


class TrayApp(QObject):
    def __init__(self, config: AppConfig):
        super().__init__()
        self.config = config
        self.updates: list[UpdateInfo] = []
        self.checker: UpdateChecker | None = None
        self.last_check_time: datetime | None = None

        # Tray icon
        self.tray = QSystemTrayIcon()
        self.tray.setIcon(create_ok_icon())
        self.tray.setToolTip("Yay Update Checker - No checks yet")
        self.tray.activated.connect(self._on_tray_activated)

        # Context menu
        self.menu = QMenu()

        self.action_check = QAction("Check Now")
        self.action_check.triggered.connect(self.start_check)
        self.menu.addAction(self.action_check)

        self.action_show = QAction("Show Updates")
        self.action_show.triggered.connect(self.show_updates_dialog)
        self.action_show.setEnabled(False)
        self.menu.addAction(self.action_show)

        self.action_update = QAction("Update System")
        self.action_update.triggered.connect(self.launch_update)
        self.menu.addAction(self.action_update)

        self.menu.addSeparator()

        self.action_settings = QAction("Settings")
        self.action_settings.triggered.connect(self.show_settings_dialog)
        self.menu.addAction(self.action_settings)

        self.action_quit = QAction("Quit")
        self.action_quit.triggered.connect(QApplication.quit)
        self.menu.addAction(self.action_quit)

        self.tray.setContextMenu(self.menu)

        # Periodic check timer
        self.timer = QTimer()
        self.timer.timeout.connect(self.start_check)
        self._restart_timer()

        # Initial check after a short delay
        QTimer.singleShot(2000, self.start_check)

    def show(self):
        self.tray.show()

    def _restart_timer(self):
        interval_ms = self.config.check_interval_minutes * 60 * 1000
        self.timer.start(interval_ms)

    def start_check(self):
        if self.checker is not None and self.checker.isRunning():
            return
        self.tray.setIcon(create_checking_icon())
        self.tray.setToolTip("Checking for updates...")
        self.action_check.setEnabled(False)

        self.checker = UpdateChecker()
        self.checker.check_complete.connect(self._on_check_complete)
        self.checker.check_error.connect(self._on_check_error)
        self.checker.finished.connect(self._on_thread_finished)
        self.checker.start()

    def _on_check_complete(self, result: CheckResult):
        old_count = len(self.updates)
        self.updates = result.updates
        self.last_check_time = datetime.now()
        count = len(result.updates)

        if count == 0:
            self.tray.setIcon(create_ok_icon())
            self.tray.setToolTip(f"System up to date\nLast check: {self._format_time()}")
            self.action_show.setEnabled(False)
        elif result.needs_restart:
            self.tray.setIcon(create_restart_icon(count))
            restart_list = ", ".join(result.restart_packages)
            self.tray.setToolTip(
                f"{count} update(s) available (restart required)\n"
                f"Restart: {restart_list}\n"
                f"Last check: {self._format_time()}"
            )
            self.action_show.setEnabled(True)
            self._maybe_notify(count, old_count, restart=True)
        else:
            self.tray.setIcon(create_updates_icon(count))
            self.tray.setToolTip(
                f"{count} update(s) available\nLast check: {self._format_time()}"
            )
            self.action_show.setEnabled(True)
            self._maybe_notify(count, old_count)

    def _on_check_error(self, error_msg: str):
        self.tray.setIcon(create_error_icon())
        self.tray.setToolTip(f"Error: {error_msg}")

    def _on_thread_finished(self):
        self.action_check.setEnabled(True)
        self.checker = None

    def _maybe_notify(self, new_count: int, old_count: int, restart: bool = False):
        if self.config.notify == "never":
            return
        if self.config.notify == "new_only" and new_count <= old_count:
            return
        if restart:
            title = "Updates Available (Restart Required)"
            icon = QSystemTrayIcon.MessageIcon.Warning
        else:
            title = "Updates Available"
            icon = QSystemTrayIcon.MessageIcon.Information
        self.tray.showMessage(
            title,
            f"{new_count} package update(s) available",
            icon,
            5000,
        )

    def launch_update(self):
        terminal = self.config.terminal
        yay_cmd = ["yay", "-Syu"]
        if self.config.noconfirm:
            yay_cmd.append("--noconfirm")
        prefix = TERMINAL_CMDS.get(terminal, [terminal, "-e"])
        subprocess.Popen(prefix + yay_cmd)

    def show_updates_dialog(self):
        from yay_sys_tray.dialogs import UpdatesDialog

        dialog = UpdatesDialog(self.updates)
        dialog.exec()

    def show_settings_dialog(self):
        from yay_sys_tray.dialogs import SettingsDialog

        from PyQt6.QtWidgets import QDialog

        dialog = SettingsDialog(self.config)
        if dialog.exec() == QDialog.DialogCode.Accepted:
            self.config = dialog.get_config()
            self.config.save()
            self.config.manage_autostart()
            self._restart_timer()

    def _on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            if self.updates:
                self.show_updates_dialog()
            else:
                self.start_check()

    def _format_time(self) -> str:
        if self.last_check_time:
            return self.last_check_time.strftime("%H:%M")
        return "never"

from datetime import datetime

from PyQt6.QtCore import QObject, QProcess, QTimer
from PyQt6.QtGui import QAction, QIcon
from PyQt6.QtWidgets import QApplication, QMenu, QSystemTrayIcon

from yay_sys_tray.checker import CheckResult, UpdateChecker, UpdateInfo
from yay_sys_tray.config import AppConfig
from yay_sys_tray.icons import (
    create_bounce_icon,
    create_checking_frames,
    create_error_icon,
    create_ok_icon,
    create_restart_icon,
    create_updates_icon,
)
from yay_sys_tray.tailscale import HostResult, RemoteCheckResult, TailscaleChecker

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
        self.local_result: CheckResult | None = None
        self.remote_updates: list[HostResult] = []
        self.checker: UpdateChecker | None = None
        self.tailscale_checker: TailscaleChecker | None = None
        self.update_process: QProcess | None = None
        self.last_check_time: datetime | None = None
        self._old_count = 0

        # Spin animation (checking)
        self._spin_frames: list[QIcon] = create_checking_frames(12)
        self._spin_index = 0
        self._spin_timer = QTimer()
        self._spin_timer.timeout.connect(self._spin_tick)

        # Bounce animation (updates found)
        self._bounce_timer = QTimer()
        self._bounce_timer.timeout.connect(self._bounce_tick)
        self._bounce_count = 0
        self._bounce_icon: QIcon | None = None
        self._bounce_small: QIcon | None = None

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
        if self.tailscale_checker is not None and self.tailscale_checker.isRunning():
            return
        self._stop_bounce()
        self._start_spin()
        self.tray.setToolTip("Checking for updates...")
        self.action_check.setEnabled(False)
        self._old_count = len(self.updates) + sum(
            len(h.updates) for h in self.remote_updates
        )

        self.checker = UpdateChecker()
        self.checker.check_complete.connect(self._on_check_complete)
        self.checker.check_error.connect(self._on_check_error)
        self.checker.finished.connect(self._on_thread_finished)
        self.checker.start()

    # -- Spin animation (checking) --

    def _start_spin(self):
        self._spin_index = 0
        self.tray.setIcon(self._spin_frames[0])
        self._spin_timer.start(80)

    def _stop_spin(self):
        self._spin_timer.stop()

    def _spin_tick(self):
        self._spin_index = (self._spin_index + 1) % len(self._spin_frames)
        self.tray.setIcon(self._spin_frames[self._spin_index])

    # -- Bounce animation (updates found) --

    def _start_bounce(self, icon: QIcon):
        self._bounce_icon = icon
        self._bounce_small = create_bounce_icon(icon, 0.65)
        self._bounce_count = 0
        self._bounce_timer.start(250)

    def _stop_bounce(self):
        self._bounce_timer.stop()
        if self._bounce_icon:
            self.tray.setIcon(self._bounce_icon)

    def _bounce_tick(self):
        self._bounce_count += 1
        if self._bounce_count >= 8:
            self._bounce_timer.stop()
            if self._bounce_icon:
                self.tray.setIcon(self._bounce_icon)
            return
        # Alternate between small and normal
        if self._bounce_count % 2 == 1:
            self.tray.setIcon(self._bounce_small)
        else:
            self.tray.setIcon(self._bounce_icon)

    def _on_check_complete(self, result: CheckResult):
        self.updates = result.updates
        self.local_result = result
        self.last_check_time = datetime.now()

        # Chain remote check if Tailscale is enabled
        if self.config.tailscale_enabled:
            tags = [t.strip() for t in self.config.tailscale_tags.split(",") if t.strip()]
            if tags:
                self.tailscale_checker = TailscaleChecker(tags, self.config.tailscale_timeout)
                self.tailscale_checker.check_complete.connect(self._on_remote_check_complete)
                self.tailscale_checker.check_error.connect(self._on_remote_check_error)
                self.tailscale_checker.finished.connect(self._on_remote_thread_finished)
                self.tailscale_checker.start()
                return

        self.remote_updates = []
        self._update_tray_state()

    def _on_remote_check_complete(self, result: RemoteCheckResult):
        self.remote_updates = result.hosts
        self._update_tray_state()

    def _on_remote_check_error(self, error_msg: str):
        self.remote_updates = []
        self._update_tray_state()

    def _update_tray_state(self):
        self._stop_spin()
        result = self.local_result
        if result is None:
            return

        local_count = len(result.updates)
        remote_update_count = sum(len(h.updates) for h in self.remote_updates)
        remote_needs_restart = any(h.needs_restart for h in self.remote_updates)
        total_count = local_count + remote_update_count
        any_restart = result.needs_restart or remote_needs_restart

        # Build tooltip
        lines = []
        if self.remote_updates:
            local_label = f"Local: {local_count} update(s)"
            if result.needs_restart:
                local_label += " (restart)"
            lines.append(local_label)
            for host in self.remote_updates:
                if host.error:
                    lines.append(f"{host.hostname}: unreachable")
                elif host.updates:
                    label = f"{host.hostname}: {len(host.updates)} update(s)"
                    if host.needs_restart:
                        label += " (restart)"
                    lines.append(label)
                else:
                    lines.append(f"{host.hostname}: up to date")
        else:
            if total_count == 0:
                lines.append("System up to date")
            else:
                lines.append(f"{total_count} update(s) available")
                if result.needs_restart:
                    lines.append(f"Restart: {', '.join(result.restart_packages)}")
        lines.append(f"Last check: {self._format_time()}")

        # Set icon
        if total_count == 0:
            self.tray.setIcon(create_ok_icon())
            self.action_show.setEnabled(False)
        elif any_restart:
            icon = create_restart_icon(total_count)
            self.tray.setIcon(icon)
            self.action_show.setEnabled(True)
            self._start_bounce(icon)
        else:
            icon = create_updates_icon(total_count)
            self.tray.setIcon(icon)
            self.action_show.setEnabled(True)
            self._start_bounce(icon)

        self.tray.setToolTip("\n".join(lines))

        if total_count > 0:
            self._maybe_notify(total_count, self._old_count, restart=any_restart)

    def _on_check_error(self, error_msg: str):
        self._stop_spin()
        self.tray.setIcon(create_error_icon())
        self.tray.setToolTip(f"Error: {error_msg}")

    def _on_thread_finished(self):
        self.checker = None
        if self.tailscale_checker is None:
            self.action_check.setEnabled(True)

    def _on_remote_thread_finished(self):
        self.tailscale_checker = None
        if self.checker is None:
            self.action_check.setEnabled(True)

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
        self.update_process = QProcess(self)
        self.update_process.finished.connect(self._on_update_finished)
        self.update_process.start(prefix[0], prefix[1:] + yay_cmd)

    def _on_update_finished(self):
        self.update_process = None
        self.start_check()

    def show_updates_dialog(self):
        from yay_sys_tray.dialogs import UpdatesDialog

        dialog = UpdatesDialog(
            self.updates,
            remote_hosts=self.remote_updates,
            on_update=self.launch_update,
        )
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

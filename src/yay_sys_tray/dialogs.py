from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QLineEdit,
    QPushButton,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
)

from yay_sys_tray.checker import UpdateInfo
from yay_sys_tray.config import AppConfig


class SettingsDialog(QDialog):
    def __init__(self, config: AppConfig, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Yay Update Checker - Settings")
        self.setMinimumWidth(350)

        layout = QFormLayout(self)

        self.interval_spin = QSpinBox()
        self.interval_spin.setRange(5, 1440)
        self.interval_spin.setSuffix(" minutes")
        self.interval_spin.setValue(config.check_interval_minutes)
        layout.addRow("Check interval:", self.interval_spin)

        self.notify_combo = QComboBox()
        self.notify_combo.addItems(["always", "new_only", "never"])
        self.notify_combo.setCurrentText(config.notify)
        layout.addRow("Notifications:", self.notify_combo)

        self.terminal_edit = QLineEdit(config.terminal)
        layout.addRow("Terminal:", self.terminal_edit)

        self.autostart_check = QCheckBox("Start on login")
        self.autostart_check.setChecked(config.autostart)
        layout.addRow("Autostart:", self.autostart_check)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addRow(buttons)

    def get_config(self) -> AppConfig:
        return AppConfig(
            check_interval_minutes=self.interval_spin.value(),
            notify=self.notify_combo.currentText(),
            terminal=self.terminal_edit.text().strip(),
            autostart=self.autostart_check.isChecked(),
        )


class UpdatesDialog(QDialog):
    def __init__(self, updates: list[UpdateInfo], parent=None):
        super().__init__(parent)
        self.setWindowTitle(f"Available Updates ({len(updates)})")
        self.setMinimumSize(500, 400)

        layout = QVBoxLayout(self)

        self.table = QTableWidget(len(updates), 3)
        self.table.setHorizontalHeaderLabels(["Package", "Current", "Available"])
        self.table.horizontalHeader().setStretchLastSection(True)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)

        for row, update in enumerate(updates):
            self.table.setItem(row, 0, QTableWidgetItem(update.package))
            self.table.setItem(row, 1, QTableWidgetItem(update.old_version))
            self.table.setItem(row, 2, QTableWidgetItem(update.new_version))

        self.table.resizeColumnsToContents()
        layout.addWidget(self.table)

        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.close)
        layout.addWidget(close_btn, alignment=Qt.AlignmentFlag.AlignRight)

from PyQt6.QtCore import QSize, Qt
from PyQt6.QtGui import QColor, QFont, QPainter, QPen
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QSpinBox,
    QStyledItemDelegate,
    QStyleOptionViewItem,
    QVBoxLayout,
)

from yay_sys_tray.checker import RESTART_PACKAGES, UpdateInfo
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

        self.noconfirm_check = QCheckBox("Skip confirmation prompts")
        self.noconfirm_check.setChecked(config.noconfirm)
        layout.addRow("--noconfirm:", self.noconfirm_check)

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
            noconfirm=self.noconfirm_check.isChecked(),
            autostart=self.autostart_check.isChecked(),
        )


class UpdateItemDelegate(QStyledItemDelegate):
    PADDING = 8
    NAME_FONT_SIZE_DELTA = 1
    VERSION_COLOR = QColor(140, 140, 140)
    RESTART_COLOR = QColor(244, 67, 54)
    SEPARATOR_COLOR = QColor(220, 220, 220)

    def paint(self, painter: QPainter, option: QStyleOptionViewItem, index):
        painter.save()
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Draw selection background
        if option.state & option.state.State_Selected:
            painter.fillRect(option.rect, option.palette.highlight())

        update: UpdateInfo = index.data(Qt.ItemDataRole.UserRole)
        if not update:
            painter.restore()
            return

        x = option.rect.x() + self.PADDING
        y = option.rect.y() + self.PADDING
        w = option.rect.width() - 2 * self.PADDING

        # Package name (bold, slightly larger)
        name_font = QFont(option.font)
        name_font.setBold(True)
        name_font.setPointSize(name_font.pointSize() + self.NAME_FONT_SIZE_DELTA)
        painter.setFont(name_font)
        if option.state & option.state.State_Selected:
            painter.setPen(option.palette.highlightedText().color())
        else:
            painter.setPen(option.palette.text().color())
        painter.drawText(x, y, w, 20, Qt.AlignmentFlag.AlignLeft, update.package)

        # Restart badge
        if update.package in RESTART_PACKAGES:
            fm = painter.fontMetrics()
            name_width = fm.horizontalAdvance(update.package)
            badge_font = QFont(option.font)
            badge_font.setPointSize(badge_font.pointSize() - 2)
            badge_font.setBold(True)
            painter.setFont(badge_font)
            painter.setPen(self.RESTART_COLOR)
            painter.drawText(
                x + name_width + 8, y, w, 20,
                Qt.AlignmentFlag.AlignLeft, "restart required",
            )

        # Version line
        version_font = QFont(option.font)
        version_font.setPointSize(version_font.pointSize() - 1)
        painter.setFont(version_font)
        if option.state & option.state.State_Selected:
            painter.setPen(option.palette.highlightedText().color())
        else:
            painter.setPen(self.VERSION_COLOR)
        version_text = f"{update.old_version}  \u2192  {update.new_version}"
        painter.drawText(x, y + 22, w, 16, Qt.AlignmentFlag.AlignLeft, version_text)

        # Bottom separator
        painter.setPen(QPen(self.SEPARATOR_COLOR))
        bottom = option.rect.bottom()
        painter.drawLine(x, bottom, x + w, bottom)

        painter.restore()

    def sizeHint(self, option: QStyleOptionViewItem, index) -> QSize:
        return QSize(option.rect.width(), 50)


class UpdatesDialog(QDialog):
    def __init__(self, updates: list[UpdateInfo], parent=None):
        super().__init__(parent)
        self.setWindowTitle(f"Available Updates ({len(updates)})")
        self.setMinimumSize(400, 350)

        layout = QVBoxLayout(self)

        self.list_widget = QListWidget()
        self.list_widget.setItemDelegate(UpdateItemDelegate(self.list_widget))
        self.list_widget.setSelectionMode(QListWidget.SelectionMode.NoSelection)
        self.list_widget.setFocusPolicy(Qt.FocusPolicy.NoFocus)

        for update in updates:
            item = QListWidgetItem()
            item.setData(Qt.ItemDataRole.UserRole, update)
            item.setSizeHint(QSize(0, 50))
            self.list_widget.addItem(item)

        layout.addWidget(self.list_widget)

        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.close)
        layout.addWidget(close_btn, alignment=Qt.AlignmentFlag.AlignRight)

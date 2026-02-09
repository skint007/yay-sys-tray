from typing import Callable

from PyQt6.QtCore import QRectF, QSize, Qt
from PyQt6.QtGui import QColor, QFont, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QHBoxLayout,
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


def _version_diff_index(old: str, new: str) -> int:
    """Find the index where two version strings start to differ."""
    min_len = min(len(old), len(new))
    for i in range(min_len):
        if old[i] != new[i]:
            return i
    return min_len


class UpdateItemDelegate(QStyledItemDelegate):
    CARD_MARGIN = 4
    CARD_PADDING = 10
    CARD_RADIUS = 8
    NAME_FONT_SIZE_DELTA = 1
    CARD_BG = QColor(245, 245, 245)
    CARD_BORDER = QColor(220, 220, 220)
    VERSION_COLOR = QColor(140, 140, 140)
    OLD_DIFF_COLOR = QColor(220, 50, 47)
    NEW_DIFF_COLOR = QColor(38, 162, 105)
    RESTART_COLOR = QColor(244, 67, 54)
    RESTART_BG = QColor(244, 67, 54, 25)

    def paint(self, painter: QPainter, option: QStyleOptionViewItem, index):
        painter.save()
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        update: UpdateInfo = index.data(Qt.ItemDataRole.UserRole)
        if not update:
            painter.restore()
            return

        # Card rect with margin
        m = self.CARD_MARGIN
        card_rect = QRectF(
            option.rect.x() + m,
            option.rect.y() + m,
            option.rect.width() - 2 * m,
            option.rect.height() - 2 * m,
        )

        # Card background
        path = QPainterPath()
        path.addRoundedRect(card_rect, self.CARD_RADIUS, self.CARD_RADIUS)

        if option.state & option.state.State_Selected:
            painter.fillPath(path, option.palette.highlight())
        else:
            painter.fillPath(path, self.CARD_BG)
            painter.setPen(QPen(self.CARD_BORDER, 1))
            painter.drawPath(path)

        p = self.CARD_PADDING
        x = card_rect.x() + p
        y = card_rect.y() + p
        w = card_rect.width() - 2 * p

        # Package name (bold, slightly larger)
        name_font = QFont(option.font)
        name_font.setBold(True)
        name_font.setPointSize(name_font.pointSize() + self.NAME_FONT_SIZE_DELTA)
        painter.setFont(name_font)
        if option.state & option.state.State_Selected:
            painter.setPen(option.palette.highlightedText().color())
        else:
            painter.setPen(option.palette.text().color())
        painter.drawText(int(x), int(y), int(w), 20, Qt.AlignmentFlag.AlignLeft, update.package)

        # Restart badge
        if update.package in RESTART_PACKAGES:
            fm = painter.fontMetrics()
            name_width = fm.horizontalAdvance(update.package)
            badge_font = QFont(option.font)
            badge_font.setPointSize(badge_font.pointSize() - 2)
            badge_font.setBold(True)
            painter.setFont(badge_font)
            badge_fm = painter.fontMetrics()
            badge_text = "restart"
            badge_w = badge_fm.horizontalAdvance(badge_text) + 8
            badge_h = badge_fm.height() + 2
            badge_x = x + name_width + 8
            badge_y = y + 2
            badge_rect = QRectF(badge_x, badge_y, badge_w, badge_h)
            badge_path = QPainterPath()
            badge_path.addRoundedRect(badge_rect, 3, 3)
            painter.fillPath(badge_path, self.RESTART_BG)
            painter.setPen(self.RESTART_COLOR)
            painter.drawText(
                int(badge_x + 4), int(badge_y), int(badge_w), int(badge_h),
                Qt.AlignmentFlag.AlignCenter, badge_text,
            )

        # Version line with diff highlighting
        version_font = QFont(option.font)
        version_font.setPointSize(version_font.pointSize() - 1)
        painter.setFont(version_font)
        fm = painter.fontMetrics()
        vx = x
        vy = int(y + 24)
        vh = 16

        diff_idx = _version_diff_index(update.old_version, update.new_version)
        old_common = update.old_version[:diff_idx]
        old_diff = update.old_version[diff_idx:]
        new_common = update.new_version[:diff_idx]
        new_diff = update.new_version[diff_idx:]

        selected = bool(option.state & option.state.State_Selected)

        # Old version: common part
        if old_common:
            painter.setPen(
                option.palette.highlightedText().color() if selected else self.VERSION_COLOR
            )
            painter.drawText(int(vx), vy, int(w), vh, Qt.AlignmentFlag.AlignLeft, old_common)
            vx += fm.horizontalAdvance(old_common)

        # Old version: differing part (red)
        if old_diff:
            painter.setPen(
                option.palette.highlightedText().color() if selected else self.OLD_DIFF_COLOR
            )
            painter.drawText(int(vx), vy, int(w), vh, Qt.AlignmentFlag.AlignLeft, old_diff)
            vx += fm.horizontalAdvance(old_diff)

        # Arrow
        arrow = "  \u2192  "
        painter.setPen(
            option.palette.highlightedText().color() if selected else self.VERSION_COLOR
        )
        painter.drawText(int(vx), vy, int(w), vh, Qt.AlignmentFlag.AlignLeft, arrow)
        vx += fm.horizontalAdvance(arrow)

        # New version: common part
        if new_common:
            painter.setPen(
                option.palette.highlightedText().color() if selected else self.VERSION_COLOR
            )
            painter.drawText(int(vx), vy, int(w), vh, Qt.AlignmentFlag.AlignLeft, new_common)
            vx += fm.horizontalAdvance(new_common)

        # New version: differing part (green)
        if new_diff:
            painter.setPen(
                option.palette.highlightedText().color() if selected else self.NEW_DIFF_COLOR
            )
            painter.drawText(int(vx), vy, int(w), vh, Qt.AlignmentFlag.AlignLeft, new_diff)

        painter.restore()

    def sizeHint(self, option: QStyleOptionViewItem, index) -> QSize:
        return QSize(option.rect.width(), 58)


class UpdatesDialog(QDialog):
    def __init__(
        self,
        updates: list[UpdateInfo],
        on_update: Callable[[], None] | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self.on_update = on_update
        self.setWindowTitle(f"Available Updates ({len(updates)})")
        self.setMinimumSize(420, 380)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)

        self.list_widget = QListWidget()
        self.list_widget.setItemDelegate(UpdateItemDelegate(self.list_widget))
        self.list_widget.setSelectionMode(QListWidget.SelectionMode.NoSelection)
        self.list_widget.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self.list_widget.setStyleSheet("QListWidget { background: transparent; border: none; }")
        self.list_widget.setSpacing(2)

        for update in updates:
            item = QListWidgetItem()
            item.setData(Qt.ItemDataRole.UserRole, update)
            item.setSizeHint(QSize(0, 58))
            self.list_widget.addItem(item)

        layout.addWidget(self.list_widget)

        btn_layout = QHBoxLayout()
        btn_layout.addStretch()

        if on_update:
            update_btn = QPushButton("Update Now")
            update_btn.clicked.connect(self._launch_update)
            btn_layout.addWidget(update_btn)

        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.close)
        btn_layout.addWidget(close_btn)

        layout.addLayout(btn_layout)

    def _launch_update(self):
        if self.on_update:
            self.on_update()
            self.close()

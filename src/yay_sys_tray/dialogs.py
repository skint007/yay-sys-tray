from __future__ import annotations

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
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QSpinBox,
    QStyledItemDelegate,
    QStyleOptionViewItem,
    QTabWidget,
    QVBoxLayout,
    QWidget,
)

from yay_sys_tray.checker import RESTART_PACKAGES, UpdateInfo
from yay_sys_tray.config import AppConfig
from yay_sys_tray.icons import create_app_icon


class SettingsDialog(QDialog):
    def __init__(self, config: AppConfig, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Yay Update Checker - Settings")
        self.setWindowIcon(create_app_icon())
        self.setMinimumWidth(400)

        layout = QVBoxLayout(self)

        tabs = QTabWidget()
        layout.addWidget(tabs)

        # --- General Tab ---
        general_widget = QWidget()
        general_layout = QFormLayout(general_widget)

        self.interval_spin = QSpinBox()
        self.interval_spin.setRange(5, 1440)
        self.interval_spin.setSuffix(" minutes")
        self.interval_spin.setValue(config.check_interval_minutes)
        general_layout.addRow("Check interval:", self.interval_spin)

        self.notify_combo = QComboBox()
        self.notify_combo.addItems(["always", "new_only", "never"])
        self.notify_combo.setCurrentText(config.notify)
        general_layout.addRow("Notifications:", self.notify_combo)

        self.terminal_edit = QLineEdit(config.terminal)
        general_layout.addRow("Terminal:", self.terminal_edit)

        self.noconfirm_check = QCheckBox("Skip confirmation prompts")
        self.noconfirm_check.setChecked(config.noconfirm)
        general_layout.addRow("--noconfirm:", self.noconfirm_check)

        self.autostart_check = QCheckBox("Start on login")
        self.autostart_check.setChecked(config.autostart)
        general_layout.addRow("Autostart:", self.autostart_check)

        tabs.addTab(general_widget, "General")

        # --- Tailscale Tab ---
        tailscale_widget = QWidget()
        tailscale_layout = QFormLayout(tailscale_widget)

        self.tailscale_enabled_check = QCheckBox("Check remote servers via Tailscale")
        self.tailscale_enabled_check.setChecked(config.tailscale_enabled)
        tailscale_layout.addRow("Enable:", self.tailscale_enabled_check)

        self.tailscale_tags_edit = QLineEdit(config.tailscale_tags)
        self.tailscale_tags_edit.setPlaceholderText("tag:server,tag:arch")
        tailscale_layout.addRow("Device tags:", self.tailscale_tags_edit)

        self.tailscale_timeout_spin = QSpinBox()
        self.tailscale_timeout_spin.setRange(5, 60)
        self.tailscale_timeout_spin.setSuffix(" seconds")
        self.tailscale_timeout_spin.setValue(config.tailscale_timeout)
        tailscale_layout.addRow("SSH timeout:", self.tailscale_timeout_spin)

        self.tailscale_enabled_check.toggled.connect(self.tailscale_tags_edit.setEnabled)
        self.tailscale_enabled_check.toggled.connect(self.tailscale_timeout_spin.setEnabled)
        self.tailscale_tags_edit.setEnabled(config.tailscale_enabled)
        self.tailscale_timeout_spin.setEnabled(config.tailscale_enabled)

        tabs.addTab(tailscale_widget, "Tailscale")

        # --- Buttons ---
        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

    def get_config(self) -> AppConfig:
        return AppConfig(
            check_interval_minutes=self.interval_spin.value(),
            notify=self.notify_combo.currentText(),
            terminal=self.terminal_edit.text().strip(),
            noconfirm=self.noconfirm_check.isChecked(),
            autostart=self.autostart_check.isChecked(),
            tailscale_enabled=self.tailscale_enabled_check.isChecked(),
            tailscale_tags=self.tailscale_tags_edit.text().strip(),
            tailscale_timeout=self.tailscale_timeout_spin.value(),
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
    OLD_DIFF_COLOR = QColor(220, 50, 47)
    NEW_DIFF_COLOR = QColor(38, 162, 105)
    RESTART_COLOR = QColor(244, 67, 54)
    RESTART_BG = QColor(244, 67, 54, 25)

    def paint(self, painter: QPainter, option: QStyleOptionViewItem, index):
        data = index.data(Qt.ItemDataRole.UserRole)
        if isinstance(data, str):
            self._paint_section_header(painter, option, data)
            return
        if not isinstance(data, UpdateInfo):
            return
        self._paint_update_card(painter, option, data)

    def _paint_section_header(self, painter: QPainter, option: QStyleOptionViewItem, title: str):
        painter.save()
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        font = QFont(option.font)
        font.setBold(True)
        font.setPointSize(font.pointSize() + 1)
        painter.setFont(font)
        painter.setPen(option.palette.text().color())

        x = option.rect.x() + self.CARD_MARGIN + self.CARD_PADDING
        y = option.rect.y()
        w = option.rect.width() - 2 * (self.CARD_MARGIN + self.CARD_PADDING)
        h = option.rect.height()

        painter.drawText(
            int(x), int(y), int(w), int(h),
            Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, title,
        )

        line_y = option.rect.bottom() - 1
        painter.setPen(QPen(option.palette.mid().color(), 1))
        painter.drawLine(int(x), line_y, int(x + w), line_y)

        painter.restore()

    def _paint_update_card(self, painter: QPainter, option: QStyleOptionViewItem, update: UpdateInfo):
        painter.save()
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Card rect with margin
        m = self.CARD_MARGIN
        card_rect = QRectF(
            option.rect.x() + m,
            option.rect.y() + m,
            option.rect.width() - 2 * m,
            option.rect.height() - 2 * m,
        )

        # Card background (derived from system palette)
        path = QPainterPath()
        path.addRoundedRect(card_rect, self.CARD_RADIUS, self.CARD_RADIUS)

        if option.state & option.state.State_Selected:
            painter.fillPath(path, option.palette.highlight())
        else:
            base = option.palette.base().color()
            mid = option.palette.midlight().color()
            card_bg = QColor(
                (base.red() + mid.red()) // 2,
                (base.green() + mid.green()) // 2,
                (base.blue() + mid.blue()) // 2,
            )
            painter.fillPath(path, card_bg)
            painter.setPen(QPen(option.palette.mid().color(), 1))
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
        dim_color = option.palette.placeholderText().color()

        # Old version: common part
        if old_common:
            painter.setPen(
                option.palette.highlightedText().color() if selected else dim_color
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
            option.palette.highlightedText().color() if selected else dim_color
        )
        painter.drawText(int(vx), vy, int(w), vh, Qt.AlignmentFlag.AlignLeft, arrow)
        vx += fm.horizontalAdvance(arrow)

        # New version: common part
        if new_common:
            painter.setPen(
                option.palette.highlightedText().color() if selected else dim_color
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
        data = index.data(Qt.ItemDataRole.UserRole)
        if isinstance(data, str):
            return QSize(option.rect.width(), 32)
        return QSize(option.rect.width(), 58)


def _make_update_list(updates: list[UpdateInfo], parent: QWidget) -> QListWidget:
    """Create a styled QListWidget populated with update cards."""
    lw = QListWidget(parent)
    lw.setItemDelegate(UpdateItemDelegate(lw))
    lw.setSelectionMode(QListWidget.SelectionMode.NoSelection)
    lw.setFocusPolicy(Qt.FocusPolicy.NoFocus)
    lw.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
    lw.setResizeMode(QListWidget.ResizeMode.Adjust)
    lw.setStyleSheet("QListWidget { background: transparent; border: none; }")
    lw.setSpacing(2)
    for update in updates:
        item = QListWidgetItem()
        item.setData(Qt.ItemDataRole.UserRole, update)
        item.setSizeHint(QSize(0, 58))
        lw.addItem(item)
    return lw


def _make_restart_banner() -> QLabel:
    """Create a styled restart-required warning banner."""
    label = QLabel("Restart required")
    label.setStyleSheet(
        "QLabel {"
        "  background-color: rgba(244, 67, 54, 0.15);"
        "  color: #F44336;"
        "  font-weight: bold;"
        "  padding: 6px 10px;"
        "  border-radius: 4px;"
        "}"
    )
    label.setAlignment(Qt.AlignmentFlag.AlignCenter)
    return label


class UpdatesDialog(QDialog):
    def __init__(
        self,
        updates: list[UpdateInfo],
        remote_hosts: list | None = None,
        on_update: Callable[[], None] | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self.on_update = on_update

        remote_hosts = remote_hosts or []
        total = len(updates) + sum(len(h.updates) for h in remote_hosts)
        has_remote = any(h.updates or h.error for h in remote_hosts)

        self.setWindowTitle(f"Available Updates ({total})")
        self.setWindowIcon(create_app_icon())
        self.setMinimumSize(300, 300)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)

        if has_remote:
            # Tabbed view: one tab per host
            local_needs_restart = any(u.package in RESTART_PACKAGES for u in updates)
            tabs = QTabWidget()

            local_tab = self._build_tab(updates, local_needs_restart, on_update)
            local_label = f"Local ({len(updates)})"
            tabs.addTab(local_tab, local_label)
            if local_needs_restart:
                tabs.setTabToolTip(0, "Restart required")

            for host in remote_hosts:
                if host.error:
                    error_widget = QWidget()
                    error_layout = QVBoxLayout(error_widget)
                    error_layout.addWidget(
                        QLabel(f"Could not reach {host.hostname}: {host.error}"),
                    )
                    error_layout.addStretch()
                    tab_label = f"{host.hostname} (error)"
                    tabs.addTab(error_widget, tab_label)
                else:
                    tab = self._build_tab(host.updates, host.needs_restart)
                    tab_label = f"{host.hostname} ({len(host.updates)})"
                    idx = tabs.addTab(tab, tab_label)
                    if host.needs_restart:
                        tabs.setTabToolTip(idx, "Restart required")

            # Style tabs that need restart with red text
            for i in range(tabs.count()):
                tooltip = tabs.tabToolTip(i)
                if tooltip == "Restart required":
                    tabs.tabBar().setTabTextColor(i, QColor(244, 67, 54))

            layout.addWidget(tabs)
        else:
            # Single list, no tabs needed
            needs_restart = any(u.package in RESTART_PACKAGES for u in updates)
            if needs_restart:
                layout.addWidget(_make_restart_banner())
            layout.addWidget(_make_update_list(updates, self))

        btn_layout = QHBoxLayout()
        btn_layout.addStretch()

        if on_update and not has_remote:
            update_btn = QPushButton("Update Now")
            update_btn.clicked.connect(self._launch_update)
            btn_layout.addWidget(update_btn)

        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.close)
        btn_layout.addWidget(close_btn)

        layout.addLayout(btn_layout)

    def _build_tab(
        self,
        updates: list[UpdateInfo],
        needs_restart: bool,
        on_update: Callable[[], None] | None = None,
    ) -> QWidget:
        widget = QWidget()
        tab_layout = QVBoxLayout(widget)
        tab_layout.setContentsMargins(0, 4, 0, 0)

        if needs_restart:
            tab_layout.addWidget(_make_restart_banner())

        tab_layout.addWidget(_make_update_list(updates, widget))

        if on_update:
            btn_row = QHBoxLayout()
            btn_row.addStretch()
            update_btn = QPushButton("Update Now")
            update_btn.clicked.connect(lambda: self._do_update(on_update))
            btn_row.addWidget(update_btn)
            tab_layout.addLayout(btn_row)

        return widget

    def _do_update(self, callback: Callable[[], None]):
        callback()
        self.close()

    def _launch_update(self):
        if self.on_update:
            self.on_update()
            self.close()

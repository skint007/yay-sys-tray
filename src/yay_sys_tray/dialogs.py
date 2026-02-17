from __future__ import annotations

from typing import Callable

from PyQt6.QtCore import QEvent, QPointF, QRectF, QSettings, QSize, Qt, QUrl
from PyQt6.QtGui import QColor, QDesktopServices, QFont, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import QToolTip
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
    QTextEdit,
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
        layout.setSizeConstraint(QVBoxLayout.SizeConstraint.SetFixedSize)

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

        self.animations_check = QCheckBox("Animate tray icon")
        self.animations_check.setChecked(config.animations)
        general_layout.addRow("Animations:", self.animations_check)

        self.recheck_spin = QSpinBox()
        self.recheck_spin.setRange(1, 60)
        self.recheck_spin.setSuffix(" minutes")
        self.recheck_spin.setValue(config.recheck_interval_minutes)
        general_layout.addRow("Re-check cooldown:", self.recheck_spin)

        self.passwordless_check = QCheckBox("No sudo password for pacman")
        self.passwordless_check.setChecked(config.passwordless_updates)
        general_layout.addRow("Passwordless:", self.passwordless_check)

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
            animations=self.animations_check.isChecked(),
            recheck_interval_minutes=self.recheck_spin.value(),
            passwordless_updates=self.passwordless_check.isChecked(),
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
    ICON_SIZE = 18
    ICON_GAP = 6
    OLD_DIFF_COLOR = QColor(220, 50, 47)
    NEW_DIFF_COLOR = QColor(38, 162, 105)
    RESTART_COLOR = QColor(244, 67, 54)
    RESTART_BG = QColor(244, 67, 54, 25)
    REPO_COLORS: dict[str, QColor] = {
        "core": QColor(66, 133, 244),
        "extra": QColor(52, 168, 83),
        "multilib": QColor(251, 188, 4),
        "aur": QColor(171, 71, 188),
    }

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

        # Badges after the package name
        badge_font = QFont(option.font)
        badge_font.setPointSize(badge_font.pointSize() - 2)
        badge_font.setBold(True)
        name_fm = painter.fontMetrics()
        cursor_x = x + name_fm.horizontalAdvance(update.package) + 8
        painter.setFont(badge_font)
        badge_fm = painter.fontMetrics()

        # Repository badge
        if update.repository:
            repo_text = update.repository
            repo_w = badge_fm.horizontalAdvance(repo_text) + 8
            repo_h = badge_fm.height() + 2
            repo_rect = QRectF(cursor_x, y + 2, repo_w, repo_h)
            repo_path = QPainterPath()
            repo_path.addRoundedRect(repo_rect, 3, 3)
            repo_color = self.REPO_COLORS.get(update.repository, QColor(128, 128, 128))
            painter.fillPath(repo_path, QColor(repo_color.red(), repo_color.green(), repo_color.blue(), 25))
            painter.setPen(repo_color)
            painter.drawText(
                int(cursor_x + 4), int(y + 2), int(repo_w), int(repo_h),
                Qt.AlignmentFlag.AlignCenter, repo_text,
            )
            cursor_x += repo_w + 6

        # Restart badge
        if update.package in RESTART_PACKAGES:
            badge_text = "restart"
            badge_w = badge_fm.horizontalAdvance(badge_text) + 8
            badge_h = badge_fm.height() + 2
            badge_rect = QRectF(cursor_x, y + 2, badge_w, badge_h)
            badge_path = QPainterPath()
            badge_path.addRoundedRect(badge_rect, 3, 3)
            painter.fillPath(badge_path, self.RESTART_BG)
            painter.setPen(self.RESTART_COLOR)
            painter.drawText(
                int(cursor_x + 4), int(y + 2), int(badge_w), int(badge_h),
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

        # Right-side icons
        icons = self._icon_positions(card_rect, update)
        icon_color = option.palette.placeholderText().color()
        painter.setBrush(Qt.BrushStyle.NoBrush)

        if "info" in icons:
            painter.setPen(QPen(QColor(66, 133, 244), 1.5))
            painter.drawEllipse(icons["info"])
            info_font = QFont(option.font)
            info_font.setPointSize(info_font.pointSize() - 1)
            info_font.setItalic(True)
            info_font.setBold(True)
            painter.setFont(info_font)
            painter.drawText(icons["info"], Qt.AlignmentFlag.AlignCenter, "i")

        if "link" in icons:
            painter.setPen(QPen(option.palette.link().color(), 1.5))
            painter.drawEllipse(icons["link"])
            link_font = QFont(option.font)
            link_font.setBold(True)
            painter.setFont(link_font)
            painter.drawText(icons["link"], Qt.AlignmentFlag.AlignCenter, "\u2197")

        dep_font = QFont(option.font)
        dep_font.setBold(True)
        painter.setFont(dep_font)
        painter.setPen(QPen(icon_color, 1.5))
        painter.drawEllipse(icons["rdeps"])
        painter.drawText(icons["rdeps"], Qt.AlignmentFlag.AlignCenter, "\u2191")
        painter.drawEllipse(icons["deps"])
        painter.drawText(icons["deps"], Qt.AlignmentFlag.AlignCenter, "\u2193")

        painter.restore()

    def sizeHint(self, option: QStyleOptionViewItem, index) -> QSize:
        data = index.data(Qt.ItemDataRole.UserRole)
        if isinstance(data, str):
            return QSize(option.rect.width(), 32)
        return QSize(option.rect.width(), 58)

    def _icon_positions(self, card_rect: QRectF, data: UpdateInfo) -> dict[str, QRectF]:
        """Compute positions of all right-side icons."""
        sz = self.ICON_SIZE
        p = self.CARD_PADDING
        icon_y = card_rect.y() + (card_rect.height() - sz) / 2
        right_edge = card_rect.right() - p
        positions: dict[str, QRectF] = {}

        if data.description:
            right_edge -= sz
            positions["info"] = QRectF(right_edge, icon_y, sz, sz)
            right_edge -= self.ICON_GAP

        if data.url:
            right_edge -= sz
            positions["link"] = QRectF(right_edge, icon_y, sz, sz)
            right_edge -= self.ICON_GAP

        right_edge -= sz
        positions["rdeps"] = QRectF(right_edge, icon_y, sz, sz)
        right_edge -= self.ICON_GAP

        right_edge -= sz
        positions["deps"] = QRectF(right_edge, icon_y, sz, sz)

        return positions

    ICON_TOOLTIPS = {
        "info": None,  # uses package description
        "link": "Open package page",
        "rdeps": "Show what depends on this",
        "deps": "Show dependencies",
    }

    def helpEvent(self, event, view, option, index):
        if event.type() == QEvent.Type.ToolTip:
            data = index.data(Qt.ItemDataRole.UserRole)
            if isinstance(data, UpdateInfo):
                m = self.CARD_MARGIN
                card_rect = QRectF(
                    option.rect.x() + m, option.rect.y() + m,
                    option.rect.width() - 2 * m, option.rect.height() - 2 * m,
                )
                icons = self._icon_positions(card_rect, data)
                pos = QPointF(event.pos())
                for name, rect in icons.items():
                    if rect.contains(pos):
                        if name == "info":
                            tip = data.description
                        else:
                            tip = self.ICON_TOOLTIPS.get(name, "")
                        if tip:
                            QToolTip.showText(event.globalPos(), tip, view)
                            return True
                QToolTip.hideText()
                return True
        return super().helpEvent(event, view, option, index)

    def icon_hit_test(self, item_rect: QRectF, click_pos: QPointF, data: UpdateInfo) -> str | None:
        """Return the icon name ('link', 'deps', 'rdeps') if clicked, or None."""
        m = self.CARD_MARGIN
        card_rect = QRectF(
            item_rect.x() + m, item_rect.y() + m,
            item_rect.width() - 2 * m, item_rect.height() - 2 * m,
        )
        icons = self._icon_positions(card_rect, data)
        for name in ("link", "deps", "rdeps"):
            if name in icons and icons[name].contains(click_pos):
                return name
        return None


class _ClickableUpdateList(QListWidget):
    """QListWidget that handles icon clicks on update cards."""

    def mouseReleaseEvent(self, event):
        pos = event.position().toPoint()
        index = self.indexAt(pos)
        if index.isValid():
            data = index.data(Qt.ItemDataRole.UserRole)
            if isinstance(data, UpdateInfo):
                delegate = self.itemDelegate()
                if isinstance(delegate, UpdateItemDelegate):
                    rect = QRectF(self.visualRect(index))
                    icon = delegate.icon_hit_test(rect, event.position(), data)
                    if icon == "link" and data.url:
                        QDesktopServices.openUrl(QUrl(data.url))
                        return
                    if icon == "deps":
                        DependencyTreeDialog(data.package, reverse=False, parent=self).exec()
                        return
                    if icon == "rdeps":
                        DependencyTreeDialog(data.package, reverse=True, parent=self).exec()
                        return
        super().mouseReleaseEvent(event)

    def mouseMoveEvent(self, event):
        pos = event.position().toPoint()
        index = self.indexAt(pos)
        if index.isValid():
            data = index.data(Qt.ItemDataRole.UserRole)
            if isinstance(data, UpdateInfo):
                delegate = self.itemDelegate()
                if isinstance(delegate, UpdateItemDelegate):
                    rect = QRectF(self.visualRect(index))
                    if delegate.icon_hit_test(rect, event.position(), data):
                        self.setCursor(Qt.CursorShape.PointingHandCursor)
                        super().mouseMoveEvent(event)
                        return
        self.setCursor(Qt.CursorShape.ArrowCursor)
        super().mouseMoveEvent(event)


def _make_update_list(updates: list[UpdateInfo], parent: QWidget) -> _ClickableUpdateList:
    """Create a styled QListWidget populated with update cards."""
    sorted_updates = sorted(
        updates,
        key=lambda u: (u.package not in RESTART_PACKAGES, u.package.lower()),
    )
    lw = _ClickableUpdateList(parent)
    lw.setItemDelegate(UpdateItemDelegate(lw))
    lw.setSelectionMode(_ClickableUpdateList.SelectionMode.NoSelection)
    lw.setFocusPolicy(Qt.FocusPolicy.NoFocus)
    lw.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
    lw.setResizeMode(_ClickableUpdateList.ResizeMode.Adjust)
    lw.setMouseTracking(True)
    lw.setStyleSheet("QListWidget { background: transparent; border: none; }")
    lw.setSpacing(2)
    for update in sorted_updates:
        item = QListWidgetItem()
        item.setData(Qt.ItemDataRole.UserRole, update)
        item.setSizeHint(QSize(0, 58))
        lw.addItem(item)
    return lw


class DependencyTreeDialog(QDialog):
    """Show the output of pactree for a package."""

    def __init__(self, package: str, reverse: bool = False, parent=None):
        import subprocess

        super().__init__(parent)
        self._settings_key = "rdeps_dialog/size" if reverse else "deps_dialog/size"

        if reverse:
            self.setWindowTitle(f"Required by: {package}")
            cmd = ["pactree", "-r", package]
        else:
            self.setWindowTitle(f"Dependencies: {package}")
            cmd = ["pactree", package]

        self.setWindowIcon(create_app_icon())
        self.setMinimumSize(400, 300)

        settings = QSettings("yay-sys-tray", "yay-sys-tray")
        if settings.contains(self._settings_key):
            self.resize(settings.value(self._settings_key))

        layout = QVBoxLayout(self)

        text = QTextEdit()
        text.setReadOnly(True)
        text.setFont(QFont("monospace", 10))

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            if result.returncode == 0:
                text.setPlainText(result.stdout.rstrip())
            else:
                text.setPlainText(result.stderr.strip() or "No results.")
        except FileNotFoundError:
            text.setPlainText(
                "pactree not found.\n"
                "Install pacman-contrib:\n"
                "  sudo pacman -S pacman-contrib"
            )
        except subprocess.TimeoutExpired:
            text.setPlainText("Command timed out.")

        layout.addWidget(text)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok)
        buttons.accepted.connect(self.accept)
        layout.addWidget(buttons)

    def done(self, result):
        settings = QSettings("yay-sys-tray", "yay-sys-tray")
        settings.setValue(self._settings_key, self.size())
        super().done(result)


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
        on_update: Callable[[bool], None] | None = None,
        on_remote_update: Callable[[str, bool], None] | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self.on_update = on_update
        self._local_needs_restart = False

        remote_hosts = remote_hosts or []
        remote_with_updates = [h for h in remote_hosts if h.updates]
        total = len(updates) + sum(len(h.updates) for h in remote_with_updates)
        use_tabs = len(remote_with_updates) > 0

        self.setWindowTitle(f"Available Updates ({total})")
        self.setWindowIcon(create_app_icon())
        self.setMinimumSize(300, 300)

        settings = QSettings("yay-sys-tray", "yay-sys-tray")
        if settings.contains("updates_dialog/size"):
            self.resize(settings.value("updates_dialog/size"))

        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)

        if use_tabs:
            # Tabbed view: one tab per system with updates
            tabs = QTabWidget()

            if updates:
                local_needs_restart = any(u.package in RESTART_PACKAGES for u in updates)
                local_cb = (lambda r=local_needs_restart: on_update(r)) if on_update else None
                local_tab = self._build_tab(updates, local_needs_restart, local_cb)
                local_label = f"Local ({len(updates)})"
                idx = tabs.addTab(local_tab, local_label)
                if local_needs_restart:
                    tabs.setTabToolTip(idx, "Restart required")

            for host in remote_with_updates:
                remote_cb = None
                if on_remote_update:
                    remote_cb = lambda _h=host.hostname, _r=host.needs_restart: on_remote_update(_h, _r)
                tab = self._build_tab(host.updates, host.needs_restart, remote_cb)
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
            self._local_needs_restart = needs_restart
            if needs_restart:
                layout.addWidget(_make_restart_banner())
            layout.addWidget(_make_update_list(updates, self))

        btn_layout = QHBoxLayout()
        btn_layout.addStretch()

        if on_update and not use_tabs:
            label = "Update Now && Restart" if self._local_needs_restart else "Update Now"
            update_btn = QPushButton(label)
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
            label = "Update Now && Restart" if needs_restart else "Update Now"
            update_btn = QPushButton(label)
            update_btn.clicked.connect(lambda: self._do_update(on_update))
            btn_row.addWidget(update_btn)
            tab_layout.addLayout(btn_row)

        return widget

    def closeEvent(self, event):
        settings = QSettings("yay-sys-tray", "yay-sys-tray")
        settings.setValue("updates_dialog/size", self.size())
        super().closeEvent(event)

    def _do_update(self, callback: Callable[[], None]):
        callback()

    def _launch_update(self):
        if self.on_update:
            self.on_update(self._local_needs_restart)
            self.close()


class AboutDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("About Yay Update Checker")
        self.setWindowIcon(create_app_icon())
        self.setFixedWidth(320)

        import os
        from datetime import datetime

        from yay_sys_tray import __version__

        # Build time from the package file's mtime
        pkg_file = os.path.dirname(__file__)
        try:
            mtime = os.path.getmtime(pkg_file)
            build_time = datetime.fromtimestamp(mtime).strftime("%Y-%m-%d %H:%M")
        except OSError:
            build_time = "unknown"

        layout = QVBoxLayout(self)
        layout.setSizeConstraint(QVBoxLayout.SizeConstraint.SetFixedSize)
        layout.setContentsMargins(20, 20, 20, 20)
        layout.setSpacing(12)

        title = QLabel("Yay Update Checker")
        title_font = QFont()
        title_font.setPointSize(title_font.pointSize() + 4)
        title_font.setBold(True)
        title.setFont(title_font)
        title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(title)

        info = QLabel(
            f"Version: {__version__}\n"
            f"Built: {build_time}\n\n"
            "A lightweight system tray update checker\nfor Arch Linux."
        )
        info.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(info)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok)
        buttons.accepted.connect(self.accept)
        layout.addWidget(buttons)

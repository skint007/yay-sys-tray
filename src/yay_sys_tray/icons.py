from PyQt6.QtCore import Qt, QPointF
from PyQt6.QtGui import QBrush, QColor, QFont, QIcon, QPainter, QPainterPath, QPen, QPixmap

SIZE = 64
INSET = 2
DIAMETER = SIZE - 2 * INSET

GREEN = QColor(76, 175, 80)
ORANGE = QColor(255, 152, 0)
BLUE = QColor(33, 150, 243)
RED = QColor(244, 67, 54)
WHITE = QColor(255, 255, 255)


def _make_pixmap(bg_color: QColor) -> tuple[QPixmap, QPainter]:
    pixmap = QPixmap(SIZE, SIZE)
    pixmap.fill(Qt.GlobalColor.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setBrush(QBrush(bg_color))
    painter.setPen(Qt.PenStyle.NoPen)
    painter.drawEllipse(INSET, INSET, DIAMETER, DIAMETER)
    return pixmap, painter


def _white_pen(width: float = 6) -> QPen:
    return QPen(
        WHITE, width, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap, Qt.PenJoinStyle.RoundJoin
    )


def create_ok_icon() -> QIcon:
    pixmap, painter = _make_pixmap(GREEN)
    painter.setPen(_white_pen(6))
    painter.drawLine(18, 34, 28, 44)
    painter.drawLine(28, 44, 46, 22)
    painter.end()
    return QIcon(pixmap)


def create_updates_icon(count: int) -> QIcon:
    pixmap, painter = _make_pixmap(ORANGE)
    painter.setPen(QPen(WHITE))
    text = str(count) if count <= 99 else "99+"
    if len(text) == 1:
        font_size = 32
    elif len(text) == 2:
        font_size = 26
    else:
        font_size = 20
    font = QFont("sans-serif", font_size, QFont.Weight.Bold)
    painter.setFont(font)
    painter.drawText(INSET, INSET, DIAMETER, DIAMETER, Qt.AlignmentFlag.AlignCenter, text)
    painter.end()
    return QIcon(pixmap)


def create_checking_icon() -> QIcon:
    pixmap, painter = _make_pixmap(BLUE)
    painter.setPen(_white_pen(4))
    painter.setBrush(Qt.BrushStyle.NoBrush)
    # Draw a circular arrow (arc + arrowhead)
    cx, cy, r = 32.0, 32.0, 14.0
    path = QPainterPath()
    # Arc from ~30 degrees to ~330 degrees
    import math

    steps = 20
    start_angle = math.radians(30)
    end_angle = math.radians(300)
    for i in range(steps + 1):
        angle = start_angle + (end_angle - start_angle) * i / steps
        x = cx + r * math.cos(angle)
        y = cy - r * math.sin(angle)
        if i == 0:
            path.moveTo(x, y)
        else:
            path.lineTo(x, y)
    painter.drawPath(path)
    # Arrowhead at the end of the arc
    end_x = cx + r * math.cos(end_angle)
    end_y = cy - r * math.sin(end_angle)
    painter.setPen(_white_pen(3))
    arrow_len = 8
    painter.drawLine(
        QPointF(end_x, end_y),
        QPointF(end_x + arrow_len, end_y - arrow_len),
    )
    painter.drawLine(
        QPointF(end_x, end_y),
        QPointF(end_x + arrow_len, end_y + arrow_len * 0.3),
    )
    painter.end()
    return QIcon(pixmap)


def create_restart_icon(count: int) -> QIcon:
    pixmap, painter = _make_pixmap(RED)
    painter.setPen(QPen(WHITE))
    text = str(count) if count <= 99 else "99+"
    if len(text) == 1:
        font_size = 32
    elif len(text) == 2:
        font_size = 26
    else:
        font_size = 20
    font = QFont("sans-serif", font_size, QFont.Weight.Bold)
    painter.setFont(font)
    painter.drawText(INSET, INSET, DIAMETER, DIAMETER, Qt.AlignmentFlag.AlignCenter, text)
    painter.end()
    return QIcon(pixmap)


def create_error_icon() -> QIcon:
    pixmap, painter = _make_pixmap(RED)
    painter.setPen(_white_pen(6))
    painter.drawLine(22, 22, 42, 42)
    painter.drawLine(42, 22, 22, 42)
    painter.end()
    return QIcon(pixmap)


def create_app_icon() -> QIcon:
    """App window icon: Arch-inspired upward arrow on a blue circle."""
    pixmap, painter = _make_pixmap(QColor(23, 147, 209))
    painter.setPen(Qt.PenStyle.NoPen)
    painter.setBrush(QBrush(WHITE))
    # Upward-pointing arrow/chevron (Arch-style)
    path = QPainterPath()
    cx, cy = 32.0, 30.0
    path.moveTo(cx, cy - 16)       # top point
    path.lineTo(cx + 14, cy + 14)  # bottom right
    path.lineTo(cx + 7, cy + 14)   # inner right
    path.lineTo(cx, cy + 2)        # inner notch
    path.lineTo(cx - 7, cy + 14)   # inner left
    path.lineTo(cx - 14, cy + 14)  # bottom left
    path.closeSubpath()
    painter.drawPath(path)
    painter.end()
    return QIcon(pixmap)

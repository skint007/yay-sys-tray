import sys


def main():
    try:
        from PyQt6.QtWidgets import QApplication, QMessageBox
    except ImportError:
        print(
            "PyQt6 is required. Install it with: sudo pacman -S python-pyqt6",
            file=sys.stderr,
        )
        sys.exit(1)

    from PyQt6.QtWidgets import QSystemTrayIcon

    from yay_sys_tray.app import TrayApp
    from yay_sys_tray.config import AppConfig

    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)
    app.setApplicationName("Yay Update Checker")

    if not QSystemTrayIcon.isSystemTrayAvailable():
        QMessageBox.critical(
            None,
            "Yay Update Checker",
            "System tray is not available on this system.",
        )
        sys.exit(1)

    config = AppConfig.load()
    tray = TrayApp(config)
    tray.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()

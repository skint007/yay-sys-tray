# yay-sys-tray

A lightweight system tray app for Arch Linux that periodically checks for package updates using `yay` and `checkupdates`. It shows the number of available updates in the tray icon with animated notifications, and optionally checks remote Arch servers over Tailscale SSH.

## Features

- Periodic update checking with configurable interval
- Tray icon with update count badge and restart-required indicator
- Animated tray icon (spinning while checking, bounce on new updates)
- Desktop notifications (always, only on new updates, or never)
- One-click "Update Now" to launch `yay -Syu` in your terminal
- Remote server update checking via Tailscale (opt-in)
- Per-server tabs in the updates dialog
- Autostart via systemd user service

## Install

### From the AUR

```sh
yay -S yay-sys-tray-git
```

### Manual

Requires Python 3.12+, PyQt6, `pacman-contrib`, and `yay`.

```sh
git clone https://github.com/skint007/yay-sys-tray.git
cd yay-sys-tray
pip install .
```

## Usage

Run directly:

```sh
yay-sys-tray
```

### Systemd service

Start now and enable on login:

```sh
systemctl --user enable --now yay-sys-tray
```

Restart after an upgrade:

```sh
systemctl --user restart yay-sys-tray
```

Stop and disable:

```sh
systemctl --user disable --now yay-sys-tray
```

## Configuration

Right-click the tray icon and select **Settings**. Options include:

- **Check interval** - how often to check for updates (minutes)
- **Notifications** - always, new only, or never
- **Terminal** - which terminal emulator to use for updates
- **Autostart** - enable/disable the systemd user service
- **Tailscale** - enable remote server checking by tag with configurable SSH timeout

Config is stored in `~/.config/yay-sys-tray/config.json`.

## License

MIT

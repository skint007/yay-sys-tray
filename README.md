# yay-sys-tray

A lightweight system tray application for monitoring package updates on Arch Linux. On Arch-based systems it provides full local update management using `yay` and `checkupdates`. On any OS, it can monitor remote Arch Linux servers over Tailscale SSH.

## Features

### Local (Arch Linux)

- Periodic update checking via `checkupdates` and `yay -Qua`
- Tray icon with update count badge and restart-required indicator
- Per-package info cards with version diff highlighting, repository badges, and restart badges
- One-click "Update Now" launches `yay -Syu` in your terminal
- Dependency tree browsing via `pactree` (dependencies and reverse dependencies)
- Package links to archlinux.org and AUR pages
- Desktop notifications (always, new only, or never)
- Kernel reboot detection (warns when running kernel differs from installed)
- Passwordless sudo updates via configurable sudoers rule for pacman
- Autostart via systemd user service

### Remote (Any OS)

- Monitor remote Arch Linux servers via Tailscale SSH
- Auto-discover peers by Tailscale device tags
- Per-server tabs in the updates dialog with remote update buttons
- Configurable SSH timeout

### UI

- Animated tray icon (spinning during checks, bounce on new updates)
- Configurable animation toggle
- Re-check cooldown to prevent excessive checking

## Install

### Download Binary

Pre-built binaries for Linux, macOS, and Windows are available on the [Releases](https://github.com/skint007/yay-sys-tray/releases) page.

**macOS:** The binary is unsigned. After downloading, remove the quarantine attribute:

```sh
xattr -d com.apple.quarantine yay-sys-tray-macos
chmod +x yay-sys-tray-macos
```

### From the AUR (Arch Linux)

```sh
yay -S yay-sys-tray-git
```

### From Source

Requires Python 3.12+ and PyQt6. On Arch Linux, also install `pacman-contrib` and `yay`.

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

### Systemd Service (Arch Linux)

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

Right-click the tray icon and select **Settings**. Configuration is stored in `~/.config/yay-sys-tray/config.json`.

### General

| Option | Description | Default |
|---|---|---|
| Check interval | How often to check for updates | 60 minutes |
| Notifications | always, new_only, or never | new_only |
| Terminal | Terminal emulator for running updates | auto-detected |
| --noconfirm | Skip yay confirmation prompts (Arch only) | off |
| Autostart | Enable systemd user service (Arch only) | off |
| Animations | Animate tray icon (spin/bounce) | on |
| Re-check cooldown | Minimum minutes between implicit re-checks | 5 minutes |
| Passwordless | No sudo password for pacman (Arch only) | off |

### Tailscale

| Option | Description | Default |
|---|---|---|
| Enable | Check remote servers via Tailscale | off |
| Device tags | Comma-separated Tailscale tags to filter peers | tag:server,tag:arch |
| SSH timeout | Seconds before SSH connection times out | 10 |

## License

MIT

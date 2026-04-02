# yay-sys-tray

A lightweight system tray application for monitoring package updates on Arch Linux. On Arch-based systems it provides full local update management using `yay` and `checkupdates`. On any OS, it can monitor remote Arch Linux servers over Tailscale SSH.

## Features

### Local (Arch Linux)

- Periodic and scheduled update checking via `checkupdates` and `yay -Qua`
- Tray icon with update count badge and restart-required indicator
- Per-package info cards with version diff highlighting, repository badges, and restart badges
- One-click "Update Now" launches `yay -Syu` in your terminal
- Package removal with multiple modes (basic, unneeded deps, full cleanup, force)
- Dependency tree browsing via `pactree` (forward and reverse dependencies)
- Package links to archlinux.org and AUR pages
- Real-time search/filter across all packages
- Desktop notifications (always, new only, or never)
- Kernel reboot detection (warns when running kernel differs from installed)
- Passwordless sudo updates via configurable sudoers rule for pacman
- Autostart via systemd user service
- Automatic re-check after update/remove terminal closes

### Remote (Any OS)

- Monitor remote Arch Linux servers via Tailscale SSH
- Auto-discover peers by Tailscale device tags
- Per-server tabs in the updates dialog with remote update buttons
- Remote package removal and dependency browsing over SSH
- Parallel remote checking (up to 8 concurrent connections)
- "Update All Remote" button for batch updates across multiple servers
- Automatic re-check of individual hosts after their update terminal closes
- Configurable SSH user and timeout

### UI

- Animated tray icon (spinning during checks, bounce on new updates)
- Version diff highlighting (red for old, green for new)
- Color-coded repository badges (core, extra, multilib, aur)
- Multi-line tray tooltip with per-host status and next check time
- Dialog windows remember their size between sessions

## Platform Support

| Platform | Local Updates | Remote Monitoring | Terminal Support |
|----------|:---:|:---:|---|
| Arch Linux | Yes | Yes | kitty, alacritty, konsole, foot, xterm |
| macOS | No | Yes | kitty, alacritty, iTerm2, Terminal.app |
| Windows (experimental) | No | Yes | Windows Terminal, PowerShell |

On non-Arch systems, local update checking is disabled. Remote server monitoring works on any platform with Tailscale and SSH available.

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
| Check interval | How often to check for updates (can be disabled) | 60 minutes |
| Scheduled check | Weekly check on a specific day and time | off |
| Notifications | always, new_only, or never | new_only |
| Terminal | Terminal emulator for running updates (auto-detected) | auto-detected |
| --noconfirm | Skip yay confirmation prompts (Arch only) | off |
| Autostart | Enable systemd user service (Arch only) | off |
| Animations | Animate tray icon (spin/bounce) | on |
| Re-check cooldown | Minimum minutes between implicit re-checks | 5 minutes |
| Passwordless | No sudo password for pacman (Arch only) | off |

### Tailscale

| Option | Description | Default |
|---|---|---|
| Enable | Check remote servers via Tailscale | off |
| Device tags | Comma-separated Tailscale tags to filter peers | server,arch |
| SSH user | Username for SSH connections (blank = current user) | blank |
| SSH timeout | Seconds before SSH connection times out | 10 |

#### Remote SSH Setup

Remote server monitoring connects over Tailscale SSH. On non-Arch systems (or any system where Tailscale SSH is not handling authentication), ensure you can SSH into each remote server without a password using SSH key authentication:

```sh
# Generate a key if you don't have one
ssh-keygen -t ed25519

# Copy your public key to each remote server
ssh-copy-id user@<tailscale-hostname>
```

Verify with `ssh user@<tailscale-hostname>` -- it should connect without prompting for a password.

## License

MIT

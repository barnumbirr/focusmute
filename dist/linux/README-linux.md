# Focusmute — Linux Installation

Hotkey mute control for Focusrite Scarlett 4th Gen interfaces.
Monitors your system's default capture device mute state and reflects it on your Scarlett interface LEDs.

## What's Included

| File | Description |
|------|-------------|
| `focusmute` | System tray app (GTK, hotkey, sound feedback, settings dialog) |
| `focusmute-cli` | CLI tool for monitoring, diagnostics, and device control |
| `99-focusrite.rules` | udev rule granting USB access to logged-in users |
| `focusmute.desktop` | Desktop entry for the tray app |
| `focusmute-cli.desktop` | Desktop entry for the CLI monitor |

## Prerequisites

- **PulseAudio** (`libpulse0`) — most desktop distros include this.
  PipeWire works transparently via `pipewire-pulse` (the PulseAudio compatibility layer). Ensure `pipewire-pulse` or `libpulse0` is installed.
- **GTK 3** (`libgtk-3-0`) — required for the tray app.
- **AppIndicator** (`libappindicator3-1`) — required for the system tray icon.
- **EGL** (`libegl1`) — required for the settings/about dialogs (egui rendering).
- A Focusrite Scarlett 4th Gen USB interface.

## Install from .deb (Debian / Ubuntu / Mint / Pop!_OS)

```bash
sudo dpkg -i focusmute_*.deb
sudo apt-get install -f   # resolve any missing deps
```

This installs both binaries, udev rules, and desktop entries automatically.

## Manual Install (Arch / Fedora / other)

1. Copy the binaries:

   ```bash
   sudo install -m 755 focusmute /usr/local/bin/
   sudo install -m 755 focusmute-cli /usr/local/bin/
   ```

2. Install the udev rule (grants USB access to logged-in users):

   ```bash
   sudo install -m 644 99-focusrite.rules /etc/udev/rules.d/
   sudo udevadm control --reload-rules
   sudo udevadm trigger --subsystem-match=usb
   ```

3. (Optional) Install the desktop entries:

   ```bash
   install -m 644 focusmute.desktop ~/.local/share/applications/
   install -m 644 focusmute-cli.desktop ~/.local/share/applications/
   ```

## Usage

### Tray App

```bash
focusmute
```

Runs as a system tray icon. Right-click for the menu (Status, Toggle Mute, Settings, Reconnect Device, Quit). The global hotkey (default: Ctrl+Shift+M) toggles mute. Works on X11; Wayland may not support global hotkeys (use the tray menu instead). If no Scarlett device is connected at startup, the app starts in "Disconnected" mode and automatically connects when the device is plugged in.

### CLI

```bash
focusmute-cli monitor       # watch mute state, update LEDs in real time
focusmute-cli status        # show device, microphone, and config status
focusmute-cli --help        # see all commands
```

## Uninstall

**.deb:**

```bash
sudo dpkg -r focusmute
```

**Manual:**

```bash
sudo rm /usr/local/bin/focusmute /usr/local/bin/focusmute-cli
sudo rm /etc/udev/rules.d/99-focusrite.rules
rm ~/.local/share/applications/focusmute.desktop
rm ~/.local/share/applications/focusmute-cli.desktop
```

# Build System, Packaging & CI/CD

## Build System

### Cargo Workspace Structure

```
focusmute/
├── Cargo.toml                  Workspace root (members: focusmute-lib, focusmute)
├── .cargo/config.toml          Build configuration (no default target — builds for host)
├── crates/
│   ├── focusmute-lib/          Core library: device protocol, LED, audio, config, models
│   │   └── Cargo.toml          Platform deps: windows (Win), nusb + libpulse-binding (Linux)
│   └── focusmute/              Application crate: tray app (Win + Linux) + CLI (all platforms)
│       ├── Cargo.toml          Platform deps: tray-icon/muda/etc (Win+Linux), gtk (Linux), ctrlc (Linux)
│       ├── build.rs            Embeds app icon into .exe (Windows, via winresource)
│       └── wix/                Windows MSI installer definition (WiX v3)
│           └── main.wxs
├── dist/
│   └── linux/                  Linux distribution assets
│       ├── 99-focusrite.rules      udev rule for Focusrite USB devices
│       ├── focusmute.desktop       XDG desktop entry (tray app)
│       ├── focusmute-cli.desktop   XDG desktop entry (CLI)
│       ├── README-linux.md         Linux install guide (included in tar.gz)
│       └── debian/
│           └── postinst            Post-install script (udev reload)
└── .github/workflows/
    ├── ci.yml                  Lint, test, audit (ubuntu-latest only)
    └── release.yml             Tag-triggered release (MSI + .deb + tar.gz)
```

### Platform-Conditional Compilation

The workspace uses `cfg` attributes extensively — the same source tree produces different binaries on each platform:

| Crate | Windows | Linux |
|-------|---------|-------|
| `focusmute-lib` | `windows` crate (IOCTL via SwRoot, WASAPI mute detection) | `nusb` (raw USB), `libpulse-binding` (PulseAudio mute detection) |
| `focusmute` (tray) | `tray-icon`, `muda`, `global-hotkey`, `rodio`, `image`, `auto-launch`, `notify-rust`, `windows`; build-dep: `winresource` (icon embedding) | `tray-icon`, `muda`, `global-hotkey`, `rodio`, `image`, `auto-launch`, `notify-rust`, `gtk` (GTK 3 for settings/about dialogs) |
| `focusmute` (CLI) | Full functionality via SwRoot driver | Full functionality via raw USB + PulseAudio |

### Cross-Compilation

`.cargo/config.toml` has **no default target** — `cargo build` builds for the host platform. Cross-compilation uses explicit `--target`:

| Scenario | Command |
|----------|---------|
| Linux native (CI or bare metal) | `cargo build --release` |
| Windows native (CI on `windows-latest`) | `cargo build --release` |
| Windows from WSL2 | `cargo build --release --target x86_64-pc-windows-gnu` |

**WSL2 prerequisites** for Windows cross-compilation:
```
rustup target add x86_64-pc-windows-gnu
sudo apt-get install gcc-mingw-w64-x86-64
```

**Linux native prerequisites** (PulseAudio + GTK + tray dependencies):
```
sudo apt-get install libpulse-dev pkg-config libasound2-dev libgtk-3-dev libxdo-dev libappindicator3-dev
```

### Build Outputs

| Platform | Binary | Description |
|----------|--------|-------------|
| Windows | `focusmute.exe` | System tray app with LED control, hotkey, sound feedback |
| Windows | `focusmute-cli.exe` | CLI tool: `status`, `config`, `devices`, `monitor`, `probe`, `map`, `predict`, `descriptor`, `mute`, `unmute` |
| Linux | `focusmute` | System tray app with LED control, hotkey (X11), sound feedback, GTK settings dialog |
| Linux | `focusmute-cli` | CLI tool (same subcommands, raw USB + PulseAudio backend) |

## Packaging

### Windows: MSI Installer (cargo-wix)

- **Tooling**: [cargo-wix](https://github.com/volks73/cargo-wix) wrapping WiX Toolset v3
- **Definition**: `crates/focusmute/wix/main.wxs`
- **Icon embedding**: `build.rs` uses `winresource` to embed `icon-live.ico` into `focusmute.exe` (visible in Windows Search, taskbar, Explorer)
- **What it installs**: `focusmute.exe` + `focusmute-cli.exe` to `Program Files\Focusmute`
- **Built on**: `windows-latest` runner (native; WiX requires Windows)
- **Output**: `target/wix/focusmute-<version>-x86_64.msi`

### Linux: .deb Package (cargo-deb)

- **Tooling**: [cargo-deb](https://github.com/kornelski/cargo-deb)
- **Metadata**: `[package.metadata.deb]` in `crates/focusmute/Cargo.toml`
- **What it installs**:

| File | Destination | Purpose |
|------|-------------|---------|
| `focusmute` | `/usr/bin/focusmute` | Tray app binary |
| `focusmute-cli` | `/usr/bin/focusmute-cli` | CLI binary |
| `99-focusrite.rules` | `/etc/udev/rules.d/99-focusrite.rules` | USB device access for logged-in users |
| `focusmute.desktop` | `/usr/share/applications/focusmute.desktop` | XDG desktop entry (tray app) |
| `focusmute-cli.desktop` | `/usr/share/applications/focusmute-cli.desktop` | XDG desktop entry (CLI) |
| `LICENSE` | `/usr/share/doc/focusmute/LICENSE` | Apache 2.0 license |

- **Dependencies**: `libpulse0`, `libgtk-3-0`, `libappindicator3-1` + auto-detected deps
- **Post-install**: `postinst` reloads udev rules (`udevadm control --reload-rules && udevadm trigger`)
- **Section**: `sound`
- **Build command**: `cargo deb -p focusmute --no-build` (after `cargo build --release`)
- **Output**: `target/debian/focusmute_<version>-1_amd64.deb`
- **Target distros**: Debian, Ubuntu, Linux Mint, Pop!_OS

### Linux: tar.gz Archive (universal fallback)

For non-Debian systems (Arch, Fedora, openSUSE, etc.):

| File | Purpose |
|------|---------|
| `focusmute` | Tray app binary |
| `focusmute-cli` | CLI binary |
| `99-focusrite.rules` | udev rule |
| `focusmute.desktop` | Desktop entry (tray app) |
| `focusmute-cli.desktop` | Desktop entry (CLI) |
| `README.md` | Install instructions |
| `LICENSE` | Apache 2.0 |

Manual install steps documented in `dist/linux/README-linux.md`.

### Why Not AppImage / Flatpak / Snap?

Focusmute requires **raw USB device access** (via nusb/libusb). Sandboxed formats restrict or block direct USB communication:
- **Flatpak**: `org.freedesktop.usb` portal is experimental and limited
- **Snap**: `raw-usb` interface requires manual `snap connect` and has restrictive confinement
- **AppImage**: No USB sandbox issues per se, but no dependency management (libpulse must exist on host)

The .deb + tar.gz approach provides zero-friction USB access with proper udev rules.

### Why Not .rpm?

Can be added via [cargo-generate-rpm](https://github.com/cat-in-136/cargo-generate-rpm) if demand exists. The .deb covers the largest Linux desktop audio user base (Ubuntu Studio, Pop!_OS, Linux Mint).

## udev Rules

`dist/linux/99-focusrite.rules`:
```
SUBSYSTEM=="usb", ATTR{idVendor}=="1235", MODE="0666", TAG+="uaccess"
```

- **VID `1235`**: Focusrite (covers all Scarlett, Clarett, Vocaster devices)
- **`MODE="0666"`**: Fallback for systems without logind (e.g., minimal installs)
- **`TAG+="uaccess"`**: Preferred mechanism on systemd/logind systems — grants access only to the physically logged-in user

Without this rule, `nusb` cannot open the USB device without root privileges.

## CI/CD

### CI Workflow (`.github/workflows/ci.yml`)

Triggers on push to `main` and all pull requests.

| Job | Runner | What it does |
|-----|--------|--------------|
| `check` | `ubuntu-latest` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo-deny-action` (advisory audit), `cargo test` |

Installs `libpulse-dev`, `pkg-config`, `libasound2-dev`, `libgtk-3-dev`, `libxdo-dev`, `libappindicator3-dev` so clippy can analyze all platform code paths. Uses `EmbarkStudios/cargo-deny-action@v2` for dependency audit (no separate install step needed).

### Release Workflow (`.github/workflows/release.yml`)

Triggers on tag push matching `v*` (e.g., `v0.2.0`).

| Job | Runner | What it does |
|-----|--------|--------------|
| `check` | `ubuntu-latest` | Same lint + test + audit gate as CI (must pass before builds start) |
| `release-windows` | `windows-latest` | Build, `cargo-binstall` → `cargo-wix` → MSI installer. Uploads `.exe` (×2) + `.msi` |
| `release-linux` | `ubuntu-latest` | Build, `cargo-binstall` → `cargo-deb` → `.deb` + `.tar.gz`. Uploads both |
| `publish` | `ubuntu-latest` | Downloads all artifacts → creates GitHub Release with auto-generated notes |

Uses `cargo-bins/cargo-binstall@main` to download pre-built `cargo-wix` and `cargo-deb` binaries (faster than compiling from source via `cargo install`).

**Release process**:
1. Tag a commit: `git tag v<version> && git push --tags`
2. `check` runs first (lint, test, audit)
3. Both platform build jobs run in parallel after `check` passes
4. `publish` job waits for both, downloads artifacts, creates the GitHub Release
5. Release page shows: `.msi`, `.exe` (×2), `.deb`, `.tar.gz`

### Why Windows Release Runs on `windows-latest` (Not Cross-Compiled)

The MSI installer uses WiX Toolset, which is Windows-only. While the binaries could be cross-compiled on Linux, `cargo wix` requires a Windows host. The CI check job runs lint, test, and audit on Linux. The release job uses native Windows for the full MSI pipeline.

---
[← Firmware Binary Analysis](14-firmware-binary-analysis.md) | [Index](README.md) | [Multi-Model Mute Design →](16-multi-model-mute-design.md)

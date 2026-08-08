# Locaryn Scripts

This folder contains cross-platform build, dev, and packaging scripts for the Locaryn monorepo.

## Prerequisites

- **Rust**: install via [rustup](https://rustup.rs/) (version specified in `rust-toolchain.toml`).
- **pnpm**: install via [pnpm.io](https://pnpm.io/installation).
- **Tauri CLI**: installed automatically by pnpm in `apps/desktop`.
- **Windows**: PowerShell 5.1+ for `.ps1` scripts; `cmd.exe` for `.bat` scripts.
- **Linux**: `bash`, `dpkg-deb` (for `.deb` packaging), and Tauri system deps on Debian/Ubuntu:
  ```bash
  sudo apt-get update
  sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
  ```
- **macOS**: `bash`, Xcode Command Line Tools.

## Dev scripts

Launch the app in dev. The **desktop app embeds the Locaryn core in-process**, so
it does **not** need the daemon — by default the dev scripts just start the Tauri
desktop dev server (`tauri dev`). The daemon is only needed for the CLI, so it is
opt-in and never blocks the app launch.

| Platform | Launch the app | Also start the daemon | Daemon only |
|----------|----------------|-----------------------|-------------|
| Windows (batch) | `scripts\dev.bat` | `scripts\dev.bat -WithDaemon` | — |
| Windows (PowerShell) | `scripts\dev.ps1` | `scripts\dev.ps1 -WithDaemon` | `scripts\dev.ps1 -DaemonOnly` |
| Linux / macOS | `bash scripts/dev.sh` | `bash scripts/dev.sh --with-daemon` | `bash scripts/dev.sh --daemon-only` |

> **Note**: `tauri dev` compiles the Rust shell on first run, which can take
> several minutes. Subsequent runs are much faster.

## Build scripts

All release artifacts are placed in `release/` at the project root.

### Server binaries

Builds `locaryn` (CLI), `locaryn-daemon`, `locaryn-remote-server`, and `locaryn-supervisor` in release mode.

| Platform | Enterprise (default) | Personal (limited) |
|----------|----------------------|--------------------|
| Windows | `scripts\build-servers.ps1` | `scripts\build-servers.ps1 -Personal` |
| Linux / macOS | `bash scripts/build-servers.sh` | `bash scripts/build-servers.sh --personal` |

- **Enterprise**: remote-server includes enterprise features (default in `services/remote-server/Cargo.toml`).
- **Personal**: remote-server is built with `--no-default-features`, producing a limited version for individual users.

### Desktop app

Builds the Tauri desktop app and copies bundles to `release/desktop/`.

| Platform | Command |
|----------|---------|
| Windows | `scripts\build-desktop.ps1` |
| Linux / macOS | `bash scripts/build-desktop.sh` |

Tauri produces native bundles for the current platform:

- **Windows**: `.msi` installer and `.exe` bundles.
- **Linux**: `.deb` package and `.AppImage`.
- **macOS**: `.app` bundle and `.dmg` installer.

### Full release

Builds both server binaries and the desktop app.

| Platform | Enterprise (default) | Personal (limited) |
|----------|----------------------|--------------------|
| Windows | `scripts\build-all.ps1` | `scripts\build-all.ps1 -Personal` |
| Linux / macOS | `bash scripts/build-all.sh` | `bash scripts/build-all.sh --personal` |

### Debian package for servers (Linux only)

Creates a `.deb` package containing the server binaries.

| Edition | Command |
|---------|---------|
| Enterprise | `bash scripts/build-server-deb.sh` |
| Personal | `bash scripts/build-server-deb.sh --personal` |

## Packaging helpers

If you already built the binaries manually, you can package them with:

| Platform | Command |
|----------|---------|
| Windows | `scripts\package.ps1` |
| Linux / macOS | `bash scripts/package.sh` |

## Clean script

Removes `release/`, `target/`, `node_modules`, and Tauri bundle outputs.

| Platform | Command |
|----------|---------|
| Windows | `scripts\clean.bat` |
| Windows (PowerShell) | `scripts\clean.ps1` |
| Linux / macOS | `bash scripts/clean.sh` |

## npm scripts

You can also run the scripts via pnpm/npm from the project root:

```bash
# Dev
pnpm dev:win
pnpm dev:unix

# Server builds
pnpm build:servers:win
pnpm build:servers:unix
pnpm build:servers:win:personal
pnpm build:servers:unix:personal
pnpm build:servers:unix:deb
pnpm build:servers:unix:deb:personal

# Desktop builds
pnpm build:desktop:win
pnpm build:desktop:unix

# Full release builds
pnpm build:all:win
pnpm build:all:unix
pnpm build:all:win:personal
pnpm build:all:unix:personal

# Clean
pnpm clean:win
pnpm clean:unix
```

> **Note**: the `*:unix` npm scripts assume `bash` is available. On Windows, use the `*:win` scripts or run the `.bat`/`.ps1` files directly.

## Cross-platform builds

The scripts build **natively for the host platform**. This means:

- On Windows, you get `.exe`/`.msi` artifacts.
- On Linux, you get `.deb`/`.AppImage` artifacts.
- On macOS, you get `.app`/`.dmg` artifacts.

To produce artifacts for all three platforms, run the appropriate scripts on each target OS (e.g., in CI with `windows-latest`, `ubuntu-latest`, and `macos-latest` runners). Cross-compilation is not currently provided.

## Output layout

```
release/
├── servers/
│   ├── locaryn
│   ├── locaryn-daemon
│   ├── locaryn-remote-server
│   └── locaryn-supervisor
├── desktop/
│   ├── deb/              # Linux .deb
│   ├── dmg/              # macOS .dmg
│   ├── msi/              # Windows .msi
│   └── ...
└── locaryn-servers-<variant>-<target>.tar.gz  # or .zip on Windows
```

## Known limitations

- Cross-compilation is not supported; build on each target OS for native artifacts (Windows `.exe`/`.msi`, Linux `.deb`/`.AppImage`, macOS `.app`/`.dmg`).
- The desktop `.deb`/`.AppImage`/`.dmg` bundles are produced by `tauri build` on the matching OS; run `build-desktop.*` there (or in a CI matrix).

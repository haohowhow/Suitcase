<div align="center">

# Koofr-GUI

**Windows-first desktop client for Koofr**

Built with Tauri v2 + React + TypeScript + Rust. Aims to feel like a native file manager, not a web wrapper.

[![Source: Forgejo](https://img.shields.io/badge/Source-Forgejo-fb923c)](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6?logo=windows)](#requirements)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri)](https://v2.tauri.app/)

[中文](README.md) · [Download](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases) · [Building](docs/BUILDING.md) · [Architecture](docs/ARCHITECTURE.md) · [Changelog](CHANGELOG.md)

Returning to development or handing off: start with [Project status](docs/PROJECT_STATUS.md) and the [Agent handoff guide](docs/AGENT_HANDOFF.md) (both in Chinese).

</div>

---

## Table of contents

- [Koofr-GUI](#koofr-gui)
  - [Table of contents](#table-of-contents)
  - [Overview](#overview)
  - [Screenshots](#screenshots)
  - [Features](#features)
    - [Working today](#working-today)
    - [Not yet](#not-yet)
  - [Download](#download)
  - [Requirements](#requirements)
    - [Running](#running)
    - [Building from source](#building-from-source)
  - [Building from source](#building-from-source-1)
  - [Common commands](#common-commands)
  - [Architecture \& security boundary](#architecture--security-boundary)
  - [Where data is stored](#where-data-is-stored)
  - [Roadmap](#roadmap)
  - [Contributing](#contributing)
  - [Security](#security)
  - [License](#license)
  - [Acknowledgements](#acknowledgements)

## Overview

[Koofr](https://koofr.eu) is a European cloud storage provider that aggregates Google Drive, OneDrive, Dropbox and other third-party accounts under one login.

Koofr-GUI is an unaffiliated, Windows-first, file-manager-oriented alternative client. While Koofr officially provides desktop apps, this project aims to offer a different experience that aligns closely with native Windows file manager habits. The current version covers the main Koofr file-management flows, resumable transfers, public-link management, local in-app preview for common files, and an end-to-end encrypted workspace interoperable with Koofr Vault and rclone crypt.

## Screenshots

> Screenshots pending. UI targets 1280×720 desktop with an 980×640 minimum, following the design tokens in [DESIGN.md](DESIGN.md).

## Features

### Working today

- **Sign-in & credentials**
  - Sign in with a Koofr app-specific password. Session restore on next launch; explicit sign-out.
  - Optional password persistence through the Windows Credential Manager. Never written to plain config files or WebView storage.

- **File browsing & operations**
  - Browse mounts, folders, recents, shared items and trash.
  - Detects existing Koofr, Google Drive, OneDrive, Dropbox and other connected storage mounts.
  - Create folder, upload, download, rename, move, copy, delete, restore from trash.
  - Public links: list, create, revoke download and file-receive links (revoke requires a second click).

- **Transfers**
  - Live transfer panel with progress, cancel and pause.
  - Split uploads show the current part, total parts, confirmed parts, and final manifest stage in both the list and detail view.
  - Optional always-on-top transfer window aggregates concurrent upload/download progress and collapses to a single-icon square while paused or idle. Auto-collapse, click-to-pause/resume, and double-click-to-open can each be disabled. Left-drag and the native right-click menu remain available; it only receives redacted summary state.
  - **Single-file download**: HTTP Range + on-disk checkpoint for true byte-level resume.
  - **Split upload**: chops a large file into user-sized `part-*.bin` chunks in a user-named remote folder. Resumes from the last confirmed complete chunk. Parts contain no proprietary header; they can be reassembled with `copy /b` on Windows or `cat` on POSIX.
  - **Recursive folder download**: staged into a temporary directory, cleaned up on failure or cancel.

- **Local folder sync (Windows 10/11)**
  - Classic two-way sync remains available. The optional on-demand preview uses Windows Cloud Files API placeholders and native Explorer status, hydrates online files on open, and supports the system “Always keep on this device” and “Free up space” actions.
  - On-demand sync supports one unified `%USERPROFILE%\Koofr` root for `/` or non-overlapping custom mappings. Vaults, shared spaces, and third-party mounts are excluded from the preview.
  - Removing a classic pair only removes its index. Unregistering an on-demand root keeps hydrated files as normal files, removes online-only placeholders locally, and never changes cloud data.

- **Local in-app preview**
  - Preview PDFs, plain text and code, common and RAW images, video, and audio from My files, Recent, Shared, and Vault. Trash has no preview. Double-click opens supported files in the preview; unsupported formats keep the existing download action.
  - PDFs are paged over Range requests; text is limited to the first 5 MiB and supports UTF-8, UTF-16, GB18030, and Big5. SVG is sanitized, while TIFF, HEIC, and RAW use offline bundled decoders. Office, OpenDocument, PSD, AI, and ebook formats still require download.
  - The Rust-handled `koofr-stream` scheme keeps Koofr tokens out of the WebView. Full decoding is capped at 100 MiB for normal files. Vault plaintext stays in zeroized Rust memory, is capped at 50 MiB, and is removed on close, lock, account change, or session expiry.

- **Koofr Vault (end-to-end encryption)**
  - Create, import, export, unlock, lock, and unregister Vaults; browse decrypted metadata and create, rename, move, copy, or delete encrypted entries.
  - The Safe Key is entered only in a native Windows credential prompt and remains in Rust memory. The WebView receives decrypted display metadata plus short-lived opaque handles.
  - AES-EME names, rclone's `RCLONE\0\0` + chunked XSalsa20-Poly1305 content format, and rclone-compatible default scrypt parameters.
  - Encrypted uploads support safe whole-file restart. Encrypted downloads resume ciphertext with HTTP Range and only publish the plaintext after complete authentication and decryption.
  - Import/export of rclone crypt configs. Exported configs contain an obscured but recoverable Safe Key and must be handled as credentials.

- **Settings**
  - All settings load at startup. The UI can switch immediately between Simplified Chinese, English, and the Windows UI language; compact density and reduced motion apply live.
  - Launch at login, close/minimize to tray, active-transfer quit confirmation, native completion notifications, and taskbar activity overlays are supported. The app runs as a single instance; launching it again brings the existing window forward, and a launch at login stays in the tray when close-to-tray is on.
  - Quitting, signing out (including the idle sign-out) or switching accounts pauses running transfers instead of cancelling them; resumable ones can be continued after the next sign-in.
  - The first version with the transfer floating window asks once before enabling it. It can be toggled later under Notifications, and its dragged position is restored within the available displays.
  - Upload/download concurrency and global/per-task KiB/s limits apply live. “Full speed while idle” means five continuous minutes without Windows keyboard or mouse input.
  - Download conflicts can ask, skip, or add a stable numeric suffix and never overwrite. Optional Koofr MD5 verification, failed-part retention, and open-folder-on-complete are supported.
  - Public-link defaults include type, expiration, and a server-generated random password. The password appears only in the creation response and is never persisted.
  - Per-account download history applies retention days and item counts continuously; `0` means unlimited.
  - Configurable default download folder, optional "ask each time" per-download prompt.
  - Configurable application work directory, with an optional complete migration of settings, transfer state, download history, cache, and logs before the next start.
  - Metadata cache: memory / disk / off. Diagnostic log level, retention, rotation size, usage, and cleanup remain configurable.
  - Optional automatic retry for transfers that fail with `network_error`, with a configurable fixed interval and either a finite or unlimited retry count. A stalled connection (no download data for 60 seconds, no upload progress for 5 minutes) counts as a network error too.
  - Five themes (koofr / ocean / iris / coral / berry), covering accent tokens only.

### Not yet

- OAuth sign-in and add/remove/re-authorize third-party mounts. This project currently uses app-specific passwords and does not implement OAuth or third-party mount management. Adding, removing, or re-authorizing third-party mounts must be done through the official Koofr web interface.
- macOS / Linux support.

> **Why can't regular uploads resume?** Koofr's public upload API only exposes whole-file `FilesPut`, with no chunk sessions or server-side confirmed offsets; a broken regular upload must restart from zero. Split upload is the deliberate interop escape hatch. See [ARCHITECTURE.md](docs/ARCHITECTURE.md#分卷上传设计) for details.

## Download

Published installers, when available, are listed on the Releases page. Grab the latest NSIS installer from the [Releases page](https://forgejo.haohaohow.com/sora-admin/Koofr-GUI/releases) (e.g., `Koofr GUI_<version>_x64-setup.exe`).

> ⚠️ **Unsigned binaries**: releases are not code-signed. Windows SmartScreen may warn about an "unknown publisher". Only download from this repository's Releases page, and never from third-party mirrors. See [RELEASING.md](docs/RELEASING.md) for verification.

First launch needs Microsoft Edge WebView2 Runtime, which is usually pre-installed on supported Windows 10 / 11.

## Requirements

### Running

- Windows 10 or 11 (x64)
- Microsoft Edge WebView2 Runtime (usually pre-installed)
- A Koofr account with an [app-specific password](https://app.koofr.net/app-password)

### Building from source

- Node.js **24 LTS** (recommended; 22.12+ 22.x LTS also works)
- npm **10+**
- Rust **1.88+** with the `x86_64-pc-windows-msvc` toolchain
- Visual Studio 2022 Build Tools with the "Desktop development with C++" workload + Windows SDK

Full install instructions in [BUILDING.md](docs/BUILDING.md#1-环境要求).

## Building from source

```powershell
git clone https://forgejo.haohaohow.com/sora-admin/Koofr-GUI.git
Set-Location Koofr-GUI
npm ci
npm run dev:desktop          # dev mode: full desktop app
```

Frontend-only dev server (no Tauri commands):

```powershell
npm run dev
```

Release builds:

```powershell
npm run check                # full check: lint + tests + Rust fmt/clippy
npm run build:desktop        # produces src-tauri/target/release/koofr-gui.exe
npm run build:installer      # explicitly build the NSIS installer
```

See [BUILDING.md](docs/BUILDING.md) for the full walk-through and troubleshooting.

## Common commands

| Command | What it does |
| --- | --- |
| `npm ci` | Install pinned deps from `package-lock.json` |
| `npm run dev` | Vite frontend dev server only (browser) |
| `npm run dev:desktop` | Full Tauri desktop dev environment |
| `npm run build` | Type-check + emit `dist/` frontend bundle |
| `npm run build:desktop` | Build the release exe, no installer |
| `npm run build:installer` | Explicitly build the NSIS installer |
| `npm run verify:quick` | Fast gate: lint + split-upload tests + frontend build |
| `npm run check` | Full gate: lint + tests + build + Rust fmt + clippy + Rust tests |
| `npm run clean` | Clean build artifacts, keep `node_modules/` |
| `npm run clean:all` | Also wipe `node_modules/` (requires running `npm ci` afterwards) |

## Architecture & security boundary

```
┌──────────────────────────────────────────────┐
│ React + TypeScript UI (src/)                 │
│ - only calls narrowly-typed Tauri commands   │
│ - never touches passwords, tokens, Safe Keys │
└──────────────┬───────────────────────────────┘
               │ typed Tauri commands + events
               ▼
┌──────────────────────────────────────────────┐
│ Rust + Tauri core (src-tauri/src/)           │
│ ├─ file_ops/           path / op validation  │
│ ├─ transfer/           up/down + resume      │
│ ├─ koofr_api/          Koofr REST client     │
│ ├─ credential_manager  Windows Credentials   │
│ ├─ metadata_cache      memory / disk cache   │
│ ├─ crypto/             rclone crypt boundary │
│ └─ vault_core/         sessions + handles    │
└──────────────────────────────────────────────┘
```

Rules that matter:

- **Credentials never cross the Rust boundary.** Session tokens live in memory only; the optional app-password persistence uses the Windows Credential Manager.
- **Safe Keys never cross the Rust boundary.** A native Windows credential prompt passes them directly to Rust, rather than through Tauri IPC, React state, logs, or checkpoints.
- **Paths are validated.** Remote paths reject `.`, `..`, NUL, and overlong names. Download parents must be absolute, existing, non-symlink directories.
- **Never overwrite.** Downloads land in a `.koofr-part-*` staging file. Name selection skips existing files and the targets of other unfinished downloads, and publishing never replaces an existing file; a taken name gets a numbered suffix.
- **Error messages leak nothing**; they return a stable error code and a safe message, with no paths, tokens, or response bodies.

Full design and data flow in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Where data is stored

App data defaults to the current Windows user's local data directory (`identifier = net.koofr.desktop.gui`) and can be changed to an existing empty directory:

```
<application work directory>\
├─ settings.json                # app settings
├─ transfer-checkpoints.json    # resumable transfer checkpoints
├─ download-history.json        # account-scoped download history, locations, and bounded speed samples
├─ sync-state.sqlite3           # account-scoped sync pairs, placeholders, baselines, issues, and operations (WAL)
├─ cache/metadata-cache.json    # disk metadata cache
└─ logs/koofr-gui*.jsonl       # redacted diagnostic logs
```

The default work directory is `%LOCALAPPDATA%\net.koofr.desktop.gui`. A small adjacent `%LOCALAPPDATA%\net.koofr.desktop.gui.work-directory.json` locator lets the application find a custom directory after restart; this locator is not moved with the work data. A complete migration runs before any work file is opened on the next start, supports cross-volume copies, and removes the old data only after activation succeeds. The destination must be empty; a failed migration keeps a usable copy and is retried on the next start.

**Credentials are NOT in the work directory**; the Koofr app-specific password is stored in the Windows Credential Manager, survives reinstalls and rebuilds, and is not part of work-directory migration.

## Roadmap

- [x] Koofr Vault unlock + encrypt/decrypt + Vault transfers (rclone crypt compatible)
- [ ] OAuth sign-in & third-party mount management (blocked on public Koofr desktop API)
- [ ] Code signing / SmartScreen reputation
- [ ] macOS support
- [ ] Linux support

## Contributing

Issues and pull requests welcome. Before opening a PR:

1. Read [CONTRIBUTING.md](CONTRIBUTING.md).
2. Run `npm run check`, lint, tests, Rust fmt and clippy must be green.
3. Any new Tauri command must validate paths, identifiers and operation scope on the Rust side.

## Security

**Don't** file public issues for vulnerabilities. Use the private channel in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © 2026 Koofr-GUI contributors

This project is **not affiliated** with Koofr d.o.o. "Koofr" is a trademark of [Koofr d.o.o.](https://koofr.eu).

## Acknowledgements

- [Koofr](https://koofr.eu) for the cloud storage service and their reference [Go client](https://github.com/koofr/go-koofrclient) / [Java SDK](https://github.com/koofr/java-koofr).
- The [Tauri](https://tauri.app) team for the desktop framework.
- The `vault-crypto` library from [Koofr Vault](https://github.com/koofr/vault), with interoperability with [rclone](https://rclone.org)'s crypt format.
- [Lucide](https://lucide.dev) for the icon set.

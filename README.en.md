<div align="center">

# Suitcase

**Unofficial Windows-first desktop client for Koofr**

Built with Tauri v2, React, TypeScript, and Rust. The goal is a native file-manager experience rather than a thin wrapper around the web interface.

[![Source: GitHub](https://img.shields.io/badge/Source-GitHub-181717?logo=github)](https://github.com/haohowhow/Suitcase)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6?logo=windows)](#requirements)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri)](https://v2.tauri.app/)

[中文](README.md) · [Download](https://github.com/haohowhow/Suitcase/releases) · [Building](docs/BUILDING.md) · [Architecture](docs/ARCHITECTURE.md) · [Changelog](CHANGELOG.md)

</div>

---

## Table of contents

- [Suitcase](#suitcase)
  - [Table of contents](#table-of-contents)
  - [Overview](#overview)
  - [Screenshots](#screenshots)
  - [Features](#features)
    - [Implemented](#implemented)
    - [Not yet implemented](#not-yet-implemented)
  - [Download](#download)
  - [Requirements](#requirements)
    - [Running](#running)
    - [Building from source](#building-from-source)
  - [Building from source](#building-from-source-1)
  - [Common commands](#common-commands)
  - [Architecture \& security boundary](#architecture--security-boundary)
  - [Where data is stored](#where-data-is-stored)
  - [Roadmap](#roadmap)
  - [Security](#security)
  - [License](#license)
  - [Acknowledgements](#acknowledgements)

## Overview

[Koofr](https://koofr.eu) is a European cloud storage provider that aggregates Google Drive, OneDrive, Dropbox, and other third-party accounts under a single login.

Suitcase is an unaffiliated, Windows-first alternative client built around a native file-manager experience. Koofr offers its own desktop applications; this project provides a different option that follows Windows file-manager conventions more closely. The current version covers Koofr file management, resumable transfers, public-link management, local preview for common file types, and an end-to-end encrypted workspace interoperable with Koofr Vault and rclone crypt.

## Screenshots

> Screenshots to be added. The UI targets a 1280×720 desktop with a 980×640 minimum window, following the design tokens in [DESIGN.md](DESIGN.md).

## Features

### Implemented

- **Sign-in and credentials**
  - Sign in with a Koofr app-specific password; the session is restored on the next launch and can be ended explicitly.
  - The app-specific password can optionally be stored in the Windows Credential Manager. It is never written to plain configuration files or WebView storage.

- **File browsing and operations**
  - Browse mounts, folders, recents, shared items, and trash.
  - Detects connected Koofr, Google Drive, OneDrive, Dropbox, and other storage mounts.
  - Create folders, upload, download, rename, move, copy, delete, and restore from trash.
  - Public links: list, create, and revoke download and file-receive links; revocation requires a second confirmation.

- **File tools**
  - Report space usage per storage location and find empty folders and duplicate files across Koofr primary storage, accessible shared folders, and connected network drives.
  - Scan concurrency is configurable in Settings → Storage & logs (1–1024, default 128) and applies from the next scan. A normal scan reuses the time-limited metadata cache; a forced scan re-reads from the cloud.
  - When no comparable checksum is available, the estimated read volume is shown first and content verification only starts after confirmation. Cleanup accepts only items from the current scan and uses conditional deletion on Koofr primary storage; shared and connected locations are view-and-verify only.

- **Transfers**
  - Transfer panel with live progress, cancellation, and pausing.
  - The upload speed chart updates after a retry. Whole-file retries start a fresh set of speed samples.
  - Split uploads show part progress, full-file checksum progress and estimated time remaining, and support-file progress in both the list and detail views.
  - After an interruption during finalization, resume from the transfer panel to reuse the saved full-file checksum and upload any missing checksums, manifest, and restore scripts.
  - Optional always-on-top transfer window aggregates concurrent upload/download progress and collapses to a single-icon square while paused or idle. Auto-collapse, click-to-pause/resume, and double-click-to-open can each be disabled. The surface remains draggable with the left button, and the native context menu can open the app, close the floating window, or quit. It receives redacted summary state only.
  - **Single-file download**: HTTP Range plus an on-disk checkpoint for byte-level resume.
  - **Split upload**: splits a large file into user-sized `part-*.bin` chunks inside a user-named remote folder and resumes from the last confirmed complete chunk. The folder appears as an ordinary folder and can be reassembled with `copy /b` on Windows or `cat` on POSIX.
  - **Recursive folder download**: staged in a temporary directory and cleaned up on failure or cancellation.

- **Local folder sync (Windows 10/11)**
  - Multiple two-way pairs between a local folder and an ordinary folder in Koofr primary storage. Sync continues while the app runs or is minimized to the tray and stops when the app exits.
  - Optional on-demand sync (preview) uses Windows Cloud Files API placeholders and native Explorer status, hydrates online files when opened, and supports the system "Always keep on this device" and "Free up space" actions.
  - On-demand sync supports a unified `%USERPROFILE%\Koofr` root mapped to `/` as well as custom mappings. Local and remote scopes of the same account must not overlap; Vault, shared spaces, and third-party mounts are excluded.
  - Local watching plus a 60-second remote scan: the first round only merges and never propagates deletions; later rounds propagate one-sided changes and deletions from the three-way baseline and keep conflict copies for two-sided edits.
  - Each pair can sync immediately, pause/resume, show file-level issues, or be removed. Removing a classic pair only removes its index; unregistering an on-demand root keeps hydrated files as ordinary files, removes online-only placeholders, and never changes cloud data. A root `.syncignore` file supports comments, globs, and `!` negation.
  - In a sync pair’s File status tab, search by file name or relative path and filter pending or failed items. On-demand sync shows cloud metadata, local availability, and the last successful content download. The Issues tab explains how to recheck the pair or retry in File Explorer.
  - Sync state and indexes live in `sync-state.sqlite3` in the work directory, isolated per account, and never appear in the manual transfer resume list or the floating transfer window.

- **Local in-app preview**
  - Preview PDFs, plain text and code, common and RAW images, video, and audio from My files, Recent, Shared, and Vault. Trash has no preview. Double-click opens supported formats in the preview; unsupported formats keep the existing download action.
  - PDFs are paged over Range requests; text is limited to the first 5 MiB and supports UTF-8, UTF-16, GB18030, and Big5. SVG is sanitized, while TIFF, HEIC, and RAW are handled by offline bundled decoders. Office, OpenDocument, PSD, AI, and ebook formats still require a download.
  - The Rust-handled `koofr-stream` scheme proxies Range requests so Koofr tokens never reach the WebView. Full decoding is capped at 100 MiB for ordinary files. Vault plaintext stays in zeroized Rust memory, is capped at 50 MiB, and is cleared on close, lock, account change, or session expiry.

- **Koofr Vault (end-to-end encryption)**
  - Create, import, export, unlock, lock, and remove Vault registrations; browse decrypted metadata and create, rename, move, copy, or delete encrypted entries.
  - The Safe Key is entered only in a native Windows credential prompt and remains in Rust memory. The WebView receives decrypted display metadata and short-lived opaque handles only.
  - AES-EME file names, rclone's `RCLONE\0\0` plus chunked XSalsa20-Poly1305 content format, and scrypt parameters compatible with rclone defaults.
  - Encrypted uploads support safe whole-file retries. Encrypted downloads resume ciphertext over HTTP Range and publish the plaintext only after complete authentication and decryption.
  - Import and export of rclone crypt configurations. Exported configurations contain an obscured but recoverable Safe Key and must be treated as credentials.

- **Settings**
  - All settings load at startup. The UI switches immediately among Simplified Chinese, English, and the system language; compact density and reduced motion apply instantly.
  - Launch at login, close/minimize to tray, active-transfer quit confirmation, native completion notifications, and taskbar activity overlays. The app runs as a single instance; launching it again brings the existing window forward, and an autostart launch stays in the tray when close-to-tray is enabled.
  - Quitting, signing out (including idle sign-out), or switching accounts pauses running transfers instead of cancelling them; resumable transfers continue after the next sign-in.
  - The first run asks whether to enable the floating transfer window; it can be toggled later under Notifications, and its dragged position is restored within the available displays.
  - Upload/download concurrency and global/per-task KiB/s limits apply live. "Full speed while idle" means five continuous minutes without Windows keyboard or mouse input.
  - Download conflicts can ask, skip, or add a stable numeric suffix and never overwrite. Optional Koofr MD5 verification, failed-part retention, and open-folder-on-complete are supported.
  - Public-link defaults cover type, expiration, and a server-generated random password. The password appears only in the creation result and is never persisted.
  - Per-account download history applies retention days and item counts; `0` means unlimited.
  - Configurable default download folder and an optional "ask each time" prompt per download.
  - Optional automatic retry for transfers that fail with `network_error`, with a configurable fixed interval and either a finite or unlimited retry count. A stalled connection (no download data for 60 seconds, no upload progress for 5 minutes) counts as a network error as well.
  - A configurable work directory, with an optional complete migration of settings, transfer state, download history, cache, and logs before the next start.
  - Metadata cache: memory, disk, or off. Diagnostic log level, retention, and rotation size are configurable, with usage reporting and one-click cleanup.
  - Enable Debug logging in Settings to trace request timings, split-upload hashing and finalization, sync cycles, and preview preparation.
  - Five themes (koofr / ocean / iris / coral / berry) that override accent tokens only.

### Not yet implemented

- OAuth sign-in and adding, removing, or re-authorizing third-party mounts. Sign-in currently uses app-specific passwords only; third-party mount management must be done through the official Koofr web interface.
- macOS and Linux support.

> **Why ordinary uploads cannot resume** Koofr's public upload API exposes only whole-file `FilesPut`, with no chunk sessions or server-confirmed offsets, so an interrupted ordinary upload must be resent in full. Split upload is the deliberate interoperability approach for this limitation; see [ARCHITECTURE.md](docs/ARCHITECTURE.md#分卷上传设计) for details.

## Download

Published portable archives, where available, are listed on the Releases page. Download the latest portable ZIP from the [Releases page](https://github.com/haohowhow/Suitcase/releases) (for example, `Suitcase_<version>_x64-portable.zip`), extract it to any folder, and run `suitcase.exe` directly — no installation required.

> ⚠️ **Unsigned binaries**: release builds are not code-signed, and Windows SmartScreen may report an unknown publisher. Download only from this repository's Releases page, never from third-party mirrors. See [RELEASING.md](docs/RELEASING.md) for verification steps.

The first launch requires Microsoft Edge WebView2 Runtime, which is normally pre-installed on supported Windows 10 / 11. If you enabled Windows on-demand sync (CFAPI), remove the sync root in the app (or run `suitcase.exe --unregister-sync-roots`) before deleting the extracted folder.

## Requirements

### Running

- Windows 10 or 11 (x64)
- Microsoft Edge WebView2 Runtime (usually pre-installed)
- A Koofr account with an [app-specific password](https://app.koofr.net/app-password)

### Building from source

- Node.js **24 LTS** (recommended; 22.12 and later 22.x LTS releases also work)
- npm **10+**
- Rust **1.88+** with the `x86_64-pc-windows-msvc` toolchain
- Visual Studio 2022 Build Tools with the "Desktop development with C++" workload and the Windows SDK

See [BUILDING.md](docs/BUILDING.md#1-环境要求) for full setup instructions.

## Building from source

```powershell
git clone https://github.com/haohowhow/Suitcase.git
Set-Location Suitcase
npm ci
npm run dev:desktop          # development mode: full desktop app
```

Frontend-only development server (browser only, no Tauri commands):

```powershell
npm run dev
```

Release builds:

```powershell
npm run check                # full quality gate: lint + tests + Rust fmt/clippy
npm run build:desktop        # produces src-tauri/target/release/suitcase.exe
npm run build:installer      # builds the NSIS installer explicitly
```

See [BUILDING.md](docs/BUILDING.md) for the complete walkthrough and troubleshooting.

## Common commands

| Command | Purpose |
| --- | --- |
| `npm ci` | Install dependencies from `package-lock.json` reproducibly |
| `npm run dev` | Start the Vite frontend development server (browser only) |
| `npm run dev:desktop` | Start the full Tauri desktop development environment |
| `npm run build` | Type-check and emit the `dist/` frontend bundle |
| `npm run build:desktop` | Build the Windows release executable without an installer |
| `npm run build:installer` | Build the NSIS installer explicitly (release workflow) |
| `npm run verify:quick` | Quick gate: lint, split-upload tests, and frontend build |
| `npm run check` | Full gate: lint, tests, frontend build, Rust fmt, clippy, and Rust tests |
| `npm run clean` | Remove build artifacts and keep `node_modules/` |
| `npm run clean:all` | Remove build artifacts and `node_modules/` (run `npm ci` afterwards) |

## Architecture & security boundary

```text
┌──────────────────────────────────────────────┐
│ React + TypeScript UI (src/)                 │
│ - calls only narrowly scoped Tauri commands  │
│ - never handles passwords, tokens, Safe Keys │
└──────────────┬───────────────────────────────┘
               │ typed Tauri commands + events
               ▼
┌──────────────────────────────────────────────┐
│ Rust + Tauri core (src-tauri/src/)           │
│ ├─ file_ops/           path and op validation│
│ ├─ transfer/           up/down + resume      │
│ ├─ koofr_api/          Koofr REST client     │
│ ├─ credential_manager  Windows Credentials   │
│ ├─ metadata_cache      memory / disk cache   │
│ ├─ crypto/             rclone crypt boundary │
│ └─ vault_core/         sessions + handles    │
└──────────────────────────────────────────────┘
```

Key rules:

- **Credentials never cross the Rust boundary.** Session tokens live in memory only; the optional app-specific password is stored in the Windows Credential Manager.
- **Safe Keys never cross the Rust boundary.** A native Windows credential prompt passes them directly to Rust, never through Tauri IPC, React state, logs, or checkpoints.
- **Paths are validated.** Remote paths reject `.`, `..`, NUL, and overlong segments. Download parents must be absolute, existing, non-symlink directories.
- **Existing files are never overwritten.** Downloads are staged in `.koofr-part-*` files. Name selection skips existing files and the targets of other unfinished downloads, and publishing never replaces an existing file; a taken name receives a numbered suffix.
- **Error messages expose nothing** beyond a stable error code and a safe message; they contain no paths, tokens, or response bodies.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design and data flow.

## Where data is stored

Application data defaults to the current Windows user's local data directory (`identifier = app.suitcase.desktop`) and can be changed to an existing empty directory in Settings:

```text
<application work directory>\
├─ settings.json                # application settings
├─ transfer-checkpoints.json    # resumable transfer checkpoints
├─ download-history.json        # account-scoped download history, locations, and bounded speed samples
├─ sync-state.sqlite3           # account-scoped sync pairs, baselines, issues, and operations (WAL)
├─ cache/metadata-cache.json    # disk metadata cache
└─ logs/suitcase*.jsonl         # redacted diagnostic logs
```

The default work directory is `%LOCALAPPDATA%\app.suitcase.desktop`. A small adjacent `%LOCALAPPDATA%\app.suitcase.desktop.work-directory.json` locator allows the application to find a custom directory after a restart; this locator is not migrated with the work data. A full migration runs on the next start before any work file is opened, supports cross-volume copies, and removes the old directory only after the switch succeeds. The destination must be empty; if migration fails, a usable copy is retained and the migration is retried on the next start.

**Credentials are not stored in the work directory.** The Koofr app-specific password is held by the Windows Credential Manager, survives reinstalls and rebuilds, and is not part of work-directory migration.

## Roadmap

- [x] Koofr Vault unlock, encryption/decryption, and Vault transfers (compatible with rclone crypt)
- [ ] OAuth sign-in and third-party mount management (pending a public Koofr desktop API)
- [ ] Code signing / SmartScreen reputation
- [ ] macOS support
- [ ] Linux support

## Security

**Do not** report vulnerabilities in public issues. Use the private channel described in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © 2026 Suitcase contributors

Third-party component notices and bundled license texts are documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [`licenses/`](licenses/).

This project is **not affiliated** with Koofr d.o.o. "Koofr" is a trademark of [Koofr d.o.o.](https://koofr.eu).

## Acknowledgements

- [Koofr](https://koofr.eu) for the cloud storage service and the reference [Go client](https://github.com/koofr/go-koofrclient) and [Java SDK](https://github.com/koofr/java-koofr).
- The [Tauri](https://tauri.app) team for the desktop framework.
- The `vault-crypto` library from [Koofr Vault](https://github.com/koofr/vault), which interoperates with [rclone](https://rclone.org)'s crypt format.
- [Lucide](https://lucide.dev) for the icon set.

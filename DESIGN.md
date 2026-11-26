# Suitcase Design System

This document defines the design system and visual standards for Suitcase (Unofficial Windows Client for Koofr). It serves as the canonical engineering contract for the user interface. Developers must follow these specifications to maintain a consistent, accessible, and high-performance desktop experience. Any updates to the visual styles or layout parameters must be recorded here and synchronized with the CSS variables in `src/styles.css`.

## 1. Atmosphere & Identity

Suitcase should feel like a calm Windows file manager: compact, dependable, and immediately legible during long file operations. Its signature is Koofr green used sparingly as an operational signal against cool neutral surfaces; downloads and destructive actions favor clarity over decoration.

## 2. Color

### Palette

| Role | Token | Light | Usage |
| --- | --- | --- | --- |
| Surface/chrome | `--chrome-bg` | `#f3f4f6` | Outer window frame, title bar, sidebar, and sign-in canvas |
| Surface/primary | `--surface` | `#ffffff` | Inset workspace sheet and modal surface |
| Surface/canvas | `--canvas` | `#eceef2` | Base window background |
| Surface/subtle | `--surface-subtle` | `#f6f7f9` | Dialog footers, summary wells, and quiet surfaces |
| Surface/recessed | `--surface-recessed` | `#f3f5f8` | Address bar and search inputs at rest |
| Surface/raised | `--surface-raised` | `#fafbfc` | Subtle raised cards |
| Text/primary | `--text` | `#181b20` | Primary copy and headings |
| Text/secondary | `--muted` | `#5c6470` | Supporting copy |
| Text/tertiary | `--subtle` | `#868e9b` | Metadata, placeholders, quiet glyphs |
| Border/subtle | `--border-subtle` | `#f2f4f7` | Soft file row hairlines |
| Border/default | `--border` | `#e5e8ee` | Dividers and recessed outlines |
| Border/strong | `--border-strong` | `#d3d8e1` | Control outlines |
| Accent/primary | `--accent` | `#2fa84f` | Primary actions, progress, selected state |
| Accent/hover | `--accent-dark` | `#238a3e` | Hover and active accent |
| Accent/contrast | `--accent-contrast` | `#ffffff` | Text/icons on accent surfaces |
| Accent/subtle | `--accent-soft` | `#eef9f1` | Selected and informational backgrounds |
| Accent/subtle-strong | `--accent-soft-strong` | `#dcf3e2` | Stronger accent tint |
| Status/error | `--status-error` | `#c23934` | Error messages and destructive feedback |
| Status/error-soft | `--status-error-soft` | `#fef2f1` | Error background |
| Status/warning | `--status-warning` | `#94601c` | Cautionary notes |
| Status/warning-soft | `--status-warning-soft` | `#fef8ee` | Warning background |
| Provider/Google | `--provider-google` | `#e6ad27` | Google Drive connection identifier |
| Provider/OneDrive | `--provider-onedrive` | `#3979c7` | OneDrive connection identifier |
| Provider/Dropbox | `--provider-dropbox` | `#236dc9` | Dropbox connection identifier |
| Overlay | `--overlay` | `rgba(20, 23, 28, 0.28)` | Modal backdrop |
| File/folder | `--file-folder`, `--file-folder-deep` | `#f4be42`, `#c9820f` | Layered folder glyph |
| File/spreadsheet | `--file-sheet`, `--file-sheet-deep` | `#278b5b`, `#17633f` | Spreadsheet glyph |
| File/PDF | `--file-pdf`, `--file-pdf-deep` | `#df5149`, `#ac322d` | PDF glyph |
| File/document | `--file-doc`, `--file-doc-deep` | `#3979c7`, `#285891` | Text-document glyph |
| File/image | `--file-image`, `--file-image-deep` | `#765ac2`, `#563d9c` | Image glyph |
| File/archive | `--file-archive`, `--file-archive-deep` | `#b0712b`, `#7d4b18` | Compressed archive glyph |
| File/executable | `--file-executable`, `--file-executable-deep` | `#397b91`, `#245264` | Windows executable and script glyph |
| File/generic | `--file-generic`, `--file-generic-deep` | `#74808d`, `#4d5864` | Unknown-file glyph |

The complete list of file-type tokens is defined in `src/styles.css`. This includes additional families for slides, video, audio, code, text, font, ebook, disk, and database.

### Theme accents

The `koofr`, `ocean`, `iris`, `coral`, and `berry` themes may override only the five accent tokens. Neutral surfaces, statuses, and text remain stable so file-management semantics do not shift with appearance preferences.

### Rules

- Accent colors communicate selection, progress, or an available action; they are not decorative gradients.
- Errors use the status tokens and never depend on accent theme.
- New colors must be added here before use.

## 3. Typography

### Scale

| Level | Size | Weight | Line height | Usage |
| --- | --- | --- | --- | --- |
| Display | `2.25rem` | 700 | 1.12 | Authentication heading |
| H1 | `1.625rem` | 700 | 1.2 | Workspace title |
| H2 | `1.25rem` | 650 | 1.3 | Modal title |
| H3 | `0.875rem` | 650 | 1.4 | Settings section heading |
| Body | `0.8125rem` | 400 | 1.6 | Default desktop copy |
| Body/small | `0.75rem` | 400 | 1.5 | Controls and file rows |
| Caption | `0.6875rem` | 500 | 1.45 | Metadata and helper text |

### Font stack

- Primary: `"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", system-ui, sans-serif`
- Mono: `Consolas, "Cascadia Mono", monospace`

### Rules

- Use the Windows-native stack for a desktop-client feel and reliable Chinese glyph metrics.
- Apply tabular figures to byte counts, percentages, and transfer rates.
- Long Chinese helper copy uses natural wrapping and at least 1.5 line height.

## 4. Spacing & Layout

### Base unit

All spacing derives from a 4px base.

| Token | Value | Usage |
| --- | --- | --- |
| `--space-1` | `4px` | Icon micro-gap |
| `--space-2` | `8px` | Compact inline groups |
| `--space-3` | `12px` | Field padding and row gaps |
| `--space-4` | `16px` | Standard group spacing |
| `--space-5` | `20px` | Modal and panel breathing room |
| `--space-6` | `24px` | Large component padding |
| `--space-8` | `32px` | Major group separation |

### Grid

- Desktop shell target: 1280×720 with a 980×640 minimum native window; `36px` caption bar and `220px` navigation pane frame an inset `--surface` workspace sheet (`12px` top-left radius).
- Sign-in view: centered `400px` tactile card (`14px` radius) on a subtly top-lit `--chrome-bg` canvas rather than a multi-column web marketing split.
- Transfer panel: `392px` desktop inspector rail, overlaying content at narrower desktop widths (`<= 1280px`); toggled from a dedicated sidebar `传输队列` utility entry with an active-count badge so controls never float over list rows.
- Primary features (`我的文件`, `最近的文件`, `共享` with `共享项目` / `分享链接` tabs, `回收站`, `同步`, `私人保险箱`, `存储位置`) render directly on the inset `--surface` workspace sheet; modals are reserved for focused creation/confirmation flows and Settings.
- Modal widths: `440px` standard, `656px` wide, and `996px` extra-wide (`12px` radius), constrained to the viewport with a fixed header, scrollable body, and bordered `--surface-subtle` footer bar. Settings use a fixed `900×640` dialog (clamped to the viewport) so switching tabs never resizes it; only the page area scrolls.
- Controls: toolbar buttons, selects, recessed address bar, and search inputs are `34px` high (`36px` in standalone forms) with `7px–8px` radii; file rows are `42px` (`34px` in compact density) with `8px` hover/selection pills beneath a sticky `32px` header; workspace headings use `18px` semibold typography.
- Responsive checkpoints: 1280px, 920px, and 650px; forms and setting choices collapse to one column at the compact breakpoint.

### Rules

- Dense file lists remain compact; settings and confirmation flows receive larger spacing.
- Input/action groups remain one row while usable, then stack without horizontal scrolling.
- Values outside the spacing tokens are reserved for icon geometry and existing Windows optical alignment.

## 5. Components

### Lists and navigation

- Selected sidebar, settings-nav and theme items use a neutral tint plus a 3px accent marker inside the item; accent color is not used as the row fill.
- File-list columns have fixed tracks for date, size and row actions, so rows with different action buttons stay aligned.
- Selection actions appear in the heading row only while something is selected; there is no empty placeholder bar.

### Modal

- **Structure**: backdrop → semantic dialog section → close button, heading, content, footer with right-aligned buttons (secondary, then primary). Buttons keep their natural width (min 96px); never stretch them across the dialog.
- **Variants**: standard, wide, settings (fixed size, bordered header and footer, two-pane body), download destination, and split-upload setup.
- **Surface**: opaque `--surface` with a 1px border; no backdrop blur on the dialog or the overlay.
- **Spacing**: `--space-3` through `--space-6`.
- **States**: opening, visible, busy/disabled action, validation error, closing.
- **Accessibility**: labelled modal dialog, Escape dismissal, focus entry and restoration, backdrop click dismissal, clicks inside never dismiss.
- **Motion**: backdrop opacity and dialog transform/opacity using standard timing.

### Path field

- **Structure**: visible label → text input + adjacent folder-picker icon button → helper or inline error.
- **Variants**: settings and per-download.
- **States**: default, hover, focus-within, busy, invalid.
- **Accessibility**: persistent label, described helper/error, explicit folder-picker button name.

### Choice switch

- **Structure**: copy block plus native button with `role="switch"` and visible track/thumb.
- **States**: off, on, hover, focus, disabled.
- **Accessibility**: `aria-checked` reflects the persisted setting.

### Share and connection management

- **Structure**: file selection → Share action → compact creation dialog; sidebar `共享` workspace → segmented `共享项目` / `分享链接` tabs; sidebar `存储位置` (or clickable storage quota card) → connected storage workspace with direct navigation to any online mount.
- **Link states**: loading, empty, ready, validation error, creating, copied, revoke confirmation, and revoked.
- **Connected storage states**: online, offline, known provider, and generic external provider.
- **Safety**: destructive link revocation requires an explicit second click. Third-party authorization stays on Koofr's official account surface until a public desktop API exists.
- **Accessibility**: persistent form labels, selectable read-only URLs, named refresh/revoke controls, and status/error announcements.

### Transfer panel

- **Structure**: upload-mode menu, header, status tabs, transfer list, per-item controls, footer.
- **Upload modes**: ordinary upload remains a normal Koofr file; resumable large-file upload creates a user-named folder with a configurable part size, ordered raw binary parts, a bilingual command reference, built-in Windows/POSIX restore scripts, and an open checksum manifest. The menu must state this interoperability tradeoff before selection.
- **States**: open, hidden, empty, running, completed, failed, cancelled.
- **Accessibility**: live transfer values remain textual; action buttons have explicit labels.

### Floating transfer window

- **Structure**: a `280×56` Windows always-on-top capsule pill hosted inside a `308×82` transparent shadow canvas (`14px` horizontal, `10px` top, `16px` bottom gutter) that morphs smoothly into a `52×48` sculpted organic pebble (`80×74` canvas) while paused or idle. The visible pill/pebble is the left-button drag target; there is no separate drag strip.
- **Dial & Brand Mark**: a concentric `38×38` circular status dial (`40×40` in compact pebble) pairs an inline SVG progress ring with a soft-tinted center glyph. While idle, the asymmetric pebble uses a subtle theme-tinted porcelain sheen without a harsh circular stroke and displays Suitcase's miniature jewel brand mark.
- **Telemetry**: expanded active and paused states show a crisp header row (status, task count, tabular percentage) over an inline `4px` rounded progress bar and smart single-line live speed readout (showing only active upload/download directions without zero-rate clutter).
- **States**: weighted upload/download/mixed progress, indeterminate total, paused (preserving circular progress ring even when collapsed), short completion acknowledgement, sync attention/offline, and idle.
- **Context menu**: right-click opens the native Windows menu for opening Suitcase, closing the floating window, or safely quitting the application.
- **Privacy**: receives only direction, task count, aggregate percentage, aggregate rates, theme/language, and a completion counter; never a file name or path.
- **Motion**: expand/collapse morphs entirely inside the WebView compositor at native display refresh rate (with exact `px` 8-corner `border-radius` interpolation and fixed-width copy fade so text never reflows), paired with a single atomic Win32 `SetWindowPos` handoff before expand and after collapse so the nearest horizontal edge and visible vertical center stay pixel-locked while empty space remains click-through. Reduced motion skips the animation and snaps immediately.
- **Surface**: porcelain `--surface` with a soft multi-layer drop shadow rendered inside the transparent WebView gutter and subtle top inset highlight so the floating surface reads cleanly over both light and dark wallpapers.

### File type icon

- **Structure**: a reusable 36x38 inline SVG with a layered folder or folded document silhouette and a type-specific interior mark.
- **Variants**: folder, spreadsheet, PDF, text document, image, archive, executable, and generic file.
- **Color**: semantic file colors remain stable across accent themes; tinted paper surfaces are derived from the declared file tokens and `--surface`.
- **States**: the icon itself is non-interactive and inherits row selection/hover without decorative animation.
- **Accessibility**: decorative only (`aria-hidden`); the adjacent filename supplies the accessible name.

## 6. Motion & Interaction

| Type | Duration | Easing | Usage |
| --- | --- | --- | --- |
| Micro | `120ms` | ease-out | Hover, press, switch feedback |
| Standard | `240ms` | `cubic-bezier(0.16, 1, 0.3, 1)` | Modal, menu and panel open/close |
| Emphasis | `400ms` | cubic-bezier(0.16, 1, 0.3, 1) | Rare onboarding emphasis |

- Animate only transform and opacity for new motion.
- Pointer-down immediately compresses ordinary controls to `0.97 to 0.985`; window controls use a color response so their hit geometry stays fixed.
- Menus materialize from the invoking corner, while modal dialogs scale from their own center and pair with a dimmed, gently blurred backdrop.
- Motion remains interruptible: opening panels never block close or navigation input while transitioning.
- Every clickable control has hover, active, focus-visible, and disabled treatment.
- All transient overlays close on outside click and Escape unless an operation is in an unsafe commit phase.
- `prefers-reduced-motion` removes spatial transforms and collapses non-essential durations while keeping state changes visible.
- `prefers-reduced-transparency` replaces structural material layers with solid design-system surfaces; `prefers-contrast: more` strengthens borders, text, and focus rings.

## 7. Depth & Surface

Strategy: mixed, restrained to native desktop hierarchy.

| Level | Treatment | Usage |
| --- | --- | --- |
| Inline | 1px neutral border, no shadow | Inputs, settings choices, rows |
| Raised | Tonal shift with 1px border | Settings sections and contextual groups |
| Overlay | `0 24px 70px rgba(20, 24, 30, 0.24)` | Modal only |
| Side rail | `-12px 0 34px rgba(28, 32, 38, 0.14)` | Transfer panel overlay |

- Shadows indicate overlays or floating controls, never ordinary content cards.
- The title bar, sidebar, sticky workspace toolbar, menus, modal, and narrow-window transfer rail may use restrained translucency because they are structural or floating chrome. Operational rows and settings cards remain solid.
- Do not stack multiple light translucent surfaces; a modal sits over a dimming backdrop and menus sit directly over solid workspace content.
- Nested controls use tighter radii than the containing modal or section.
- Avoid decorative glass, gradients, or texture in operational file-management surfaces.

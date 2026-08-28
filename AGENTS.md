# PastePaw Development Guide

**Windows-only** clipboard history manager built with Rust + Tauri 2.x + React + TypeScript. Do not add macOS/Linux code or `#[cfg(target_os = ...)]` branches.

## Project Structure

```
PastePaw/
├── src-tauri/src/
│   ├── lib.rs               # App bootstrap, window animation, tray, hotkey, blur handler
│   ├── commands.rs          # Tauri IPC commands (clips, paste, folders, AI, shortcuts)
│   ├── settings_commands.rs # Settings IPC (get_settings, save_settings, ignored apps)
│   ├── clipboard.rs         # Clipboard polling loop, content capture
│   ├── database.rs          # SQLite via sqlx (clips, folders, settings tables)
│   ├── models.rs            # Shared types + global tokio runtime (get_runtime())
│   ├── settings_manager.rs  # In-memory settings cache with DB persistence
│   ├── constants.rs         # WINDOW_HEIGHT (330.0), WINDOW_MARGIN (0.0)
│   ├── ai.rs                # AI clip processing
│   └── main.rs              # Entry point
├── frontend/src/
│   ├── App.tsx              # Root component, keyboard shortcuts, IPC calls
│   ├── components/          # SearchBar, ClipItem, SettingsPanel, FolderPanel, ...
│   ├── hooks/               # useClips, useSearch, useKeyboard, ...
│   ├── types/index.ts       # Shared TS types
│   └── constants.ts         # WINDOW_HEIGHT, LAYOUT constants
└── .github/workflows/release.yml  # CI: builds x64 + arm64 NSIS installers
```

## Architecture & Key Systems

### Window Show/Hide State Machine
All show/hide goes through `lib.rs`. Two global atomics guard it:
- `IS_ANIMATING: AtomicBool` — prevents concurrent animations. Both `animate_window_show` and `animate_window_hide` use `compare_exchange(false, true)` at entry and set back to `false` on exit.
- `LAST_SHOW_TIME: AtomicI64` — timestamp set on show; blur events within 500ms are ignored to prevent immediate re-hide.

`position_window_at_bottom()` is the public entry point — it calls `animate_window_show()`.

**Hotkey toggle logic** (both in `setup` and in `register_global_shortcut`):
```rust
if win.is_visible().unwrap_or(false) && win.is_focused().unwrap_or(false) {
    animate_window_hide(&win, None);
} else {
    position_window_at_bottom(&win);
}
```
Both places must have identical toggle logic. Issue #6 was caused by `register_global_shortcut` missing the toggle.

### Window Hide Animation (Z-order trick)
During `animate_window_hide`, the window starts as `HWND_TOPMOST`, then when it reaches the taskbar's top Y coordinate it drops behind the taskbar (`SetWindowPos` with `taskbar_hwnd` as insert-after), then `window.hide()` at the end. This makes it slide behind the taskbar naturally.

### Blur → Auto-hide
`on_window_event` → `Focused(false)` → skips if: settings window is open, `LAST_SHOW_TIME` debounce < 500ms, `IS_ANIMATING` is true, or window is already hidden. If cursor moved to a different monitor, repositions there instead of hiding.

### Settings
`SettingsManager` is managed state (`app.manage(Arc::new(settings_manager))`). Access via `window.state::<Arc<SettingsManager>>().get()`. Persisted to DB. Settings changes that affect hotkey require calling `commands::register_global_shortcut` which unregisters the old shortcut and re-registers with the toggle logic.

### IPC (Frontend → Backend)
All commands are registered in `lib.rs` `invoke_handler!`. Frontend calls via `invoke("command_name", args)`. Commands return `Result<T, String>`.

### Feature Flags
- `app-store` feature: disables `tauri-plugin-autostart` and `tauri-plugin-updater` (not applicable to Windows builds, but keep the `#[cfg(not(feature = "app-store"))]` guards).

### Window Effects
`apply_window_effect(window, effect, theme)` wraps `window_vibrancy` crate:
- `"mica"` / `"dark"` → `apply_mica`
- `"mica_alt"` / `"auto"` / default → `apply_tabbed`
- `"clear"` → `clear_mica`
Re-applied on system theme change if user setting is `"system"`.

### CI / Release
- Builds both `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`
- winget installer regex: `.*-setup\.exe$` (NSIS only — WiX MSIs have x64 bootstrap stubs that fool komac's arch detection)
- Triggered by `v*` tags; `workflow_dispatch` creates a draft prerelease

## Build Commands

```bash
# Full dev (hot reload)
pnpm tauri dev

# Production build
pnpm tauri build

# Rust only
cargo check          # fast error check
cargo clippy         # lint
cargo fmt            # format
cargo test           # tests

# Frontend only (in frontend/)
pnpm install && pnpm dev
```

## Code Style

### Rust
- Errors: `Result<T, String>` for IPC commands; use `.map_err(|e| e.to_string())`
- Async: use `get_runtime().unwrap().block_on(...)` in sync contexts
- Shared state: `Arc<T>` + `app.manage()`; retrieve with `state::<Arc<T>>()`
- `OnceLock` for global singletons (e.g. tokio runtime)

### TypeScript / React
- Strict mode; `noUnusedLocals`/`noUnusedParameters` — clean up imports when removing features
- `useCallback` for props, `useMemo` for expensive computations
- `@/*` alias maps to `frontend/src/`
- Tailwind + `clsx`/`tailwind-merge` for conditional styles

## Known Gotchas

- **Removing a variable used in JSX** — TypeScript won't always catch `ReferenceError` at compile time if the variable is used inside a JSX expression that the TS compiler doesn't fully evaluate. Always `grep` for the variable name across all `.tsx` files before deleting it.
- **Two hotkey registration sites** — initial setup in `lib.rs::setup()` and re-registration in `commands::register_global_shortcut`. Both must have identical show/hide toggle logic.
- **WiX MSI arm64** — both x64 and arm64 WiX MSIs are detected as x64 by komac. Use NSIS `*-setup.exe` for winget.
- **`IS_ANIMATING` deadlock** — if a thread panics between setting `IS_ANIMATING = true` and resetting it to `false`, the window becomes permanently stuck. Always ensure the reset happens in all code paths.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+F` | Focus search |
| `Escape` | Close window / clear search |
| `Enter` | Paste selected item |
| `Delete` | Delete selected item |
| `P` | Pin/Unpin selected item |
| `↑` / `↓` | Navigate items |

## Tech Stack

| Layer | Tech |
|-------|------|
| Backend | Rust, Tauri 2.x |
| Database | SQLite via sqlx |
| Frontend | React 18, TypeScript, Vite |
| Styling | Tailwind CSS |
| Icons | Lucide React |
| Package manager | pnpm |
| Window effects | window_vibrancy crate |
| Clipboard | tauri-plugin-clipboard-x |

## Version Bumping

Update version in **all three** locations:
1. `src-tauri/Cargo.toml` → `version = "x.y.z"`
2. `src-tauri/tauri.conf.json` → `"version": "x.y.z"`
3. `package.json` → `"version": "x.y.z"`

Before writing the CHANGELOG entry, **always** review all commits since the previous tag:
```bash
git log --oneline v{prev_version}..HEAD
```
Summarize from the full list — never from just the most recent commit. The last commit often has a `fix:` prefix that misrepresents the primary feature.

**Release Checklist (Must Include):**
1. **Release Notes (Bilingual):** Always write the update notes in both English and Chinese in `CHANGELOG.md` under the new `## vx.y.z` section.
2. **Rules & Guidelines Source:** Ensure the release conforms strictly to the current rules defined in `AGENTS.md` (e.g., verifying CI workflows, referencing the source of the release guidelines).

After updating the changelog and versions, commit the changes, tag `vx.y.z`, and push (`git push origin refs/heads/main:refs/heads/main` and `git push origin --tags`) to trigger the release workflow.

## Working Conventions

### Language
Always respond in English, regardless of the language the user writes in. All content in this file must also be in English.

### Honesty
Always be truthful and face problems directly. Never fabricate, obscure, or work around a real issue to make things appear to work.
- If a check fails, investigate and fix the root cause — do not delete the check or skip it.
- If something is uncertain or unknown, say so explicitly rather than guessing with false confidence.
- If a fix is incomplete or only partially verified, state that clearly.
- Never claim something is working unless it has been confirmed to work.

### Changelog
CHANGELOG.md includes both English and Chinese entries for every version. Always add both when writing a new version section.

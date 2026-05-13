# Agent Tool Use Runbook

This runbook makes the project easier for LLM agents to inspect, modify, and
verify without guessing the workflow.

## What This Project Is

`droidscope` is a terminal UI for Android developers. It combines ratatui panels,
ADB subprocesses, persistent local workspace state, an embedded `adb shell`, and
a Kotlin Gradle Tooling API sidecar.

The core shape is synchronous Rust with background threads and `mpsc` events.
There is no async runtime.

## Common Commands

Routine Rust verification:

```sh
scripts/agent-check.sh
```

Equivalent manual commands:

```sh
cargo test --all-targets
```

Optional rustfmt gate:

```sh
DROIDSCOPE_CHECK_FMT=1 scripts/agent-check.sh
```

Optional lint gate:

```sh
DROIDSCOPE_CHECK_CLIPPY=1 scripts/agent-check.sh
```

Optional sidecar build:

```sh
DROIDSCOPE_CHECK_SIDECAR=1 scripts/agent-check.sh
```

Run the TUI locally:

```sh
cargo run
```

Read one section without launching the TUI:

```sh
cargo run -- sections
cargo run -- daemons
cargo run -- daemons --json
cargo run -- section daemons
cargo run -- devices --json
cargo run -- projects --limit 20
cargo run -- packages --root ~/Documents/git
cargo run -- config
```

`deamons` is accepted as an alias for typo-tolerant tool use. `cargo run -- tui`
starts the interactive TUI explicitly; no command still starts the TUI too.

Read-only section commands include `devices`, `processes`, `monitor`, `logcat`,
`network`, `issues`, `projects`, `packages`, `emulators`, `files`, `config`,
`workspaces`, `panels`, `gradle`, `manifest`, `fps`, and `perf`. Sections that
normally mutate state or require a live interactive surface, such as `actions`,
`app`, `data`, `intents`, and `shell`, return metadata/capabilities only.

Build release binary:

```sh
cargo build --release
```

Build only the Gradle sidecar:

```sh
gradle -p sidecar/gradle-agent jar
```

## Repository Map

Use this map to start in the likely owner file instead of scanning every module.

| Area | Primary files |
| --- | --- |
| Terminal setup and event loop | `src/main.rs` |
| Shared state and workspace application | `src/app.rs`, `src/config.rs` |
| Panel definitions and feature gating | `src/panel.rs` |
| Overall rendering, header, footer, overlays | `src/ui.rs`, `src/theme.rs` |
| Grid layout editor | `src/layout.rs` |
| Logcat and issue capture | `src/logcat.rs`, `src/logcat_ui.rs`, `src/issues.rs`, `src/issues_ui.rs` |
| Gradle task stream and host process poller | `src/gradle.rs`, `src/gradle_ui.rs` |
| Non-interactive section commands | `src/cli.rs` |
| Device list and ADB helpers | `src/adb/`, `src/devices_ui.rs` |
| App control actions | `src/app_control.rs`, `src/app_control_ui.rs` |
| App-private files, DBs, prefs | `src/app_data.rs`, `src/app_data_ui.rs` |
| Manifest inspection | `src/manifest.rs`, `src/manifest_ui.rs` |
| Deep link runner | `src/intents.rs`, `src/intents_ui.rs` |
| Device actions and tools dialog | `src/device_actions.rs`, `src/device_actions_ui.rs`, `src/device_tools.rs`, `src/device_tools_ui.rs` |
| Embedded shell | `src/shell.rs`, `src/shell_ui.rs` |
| Project and emulator pickers | `src/project_picker.rs`, `src/emulator_picker.rs` |
| Gradle sidecar | `sidecar/gradle-agent/` |

## Change Routing

- For new panel behavior, update the panel's state/action module first, then the
  matching `*_ui.rs` renderer, then `src/main.rs` only if new global keys or
  event routing are needed.
- For new global panels, update `src/panel.rs`, `src/app.rs`, `src/ui.rs`, and
  the README key tables.
- For new persisted settings, update `src/config.rs` and check whether workspace
  profiles in `src/app.rs` also need to carry the value.
- For new Android commands, isolate subprocess construction in the owning
  non-UI module and keep the UI module focused on selection, prompts, and
  rendering.
- For parsing work, add unit tests near the parser module. Existing examples are
  in `src/manifest.rs`, `src/device_tools.rs`, `src/perf.rs`, `src/fps.rs`, and
  `src/app_data.rs`.

## Safe Tool Boundaries

The repository is connected to real Android devices and user-local config. LLM
agents should separate read-only discovery from mutating actions.

Read-only or low-risk discovery:

```sh
git status --short
rg --files
rg -n "pattern" src sidecar README.md
adb devices -l
```

Mutating commands require an explicit user request or task necessity:

```sh
adb install
adb uninstall
adb shell pm clear
adb shell am force-stop
adb shell settings put
adb tcpip
scrcpy
```

Do not edit files under `~/.config/droidscope/` as part of normal development.
If a reproduction needs config, create a temporary config snippet in the repo or
describe the manual setup.

## Verification Matrix

| Change type | Minimum verification |
| --- | --- |
| Docs only | `git diff --check` |
| Rust parser/state/UI | `scripts/agent-check.sh` |
| Rust formatting cleanup | `DROIDSCOPE_CHECK_FMT=1 scripts/agent-check.sh` |
| ADB command wiring | `scripts/agent-check.sh`; run live device command only if requested |
| Gradle sidecar Kotlin | `DROIDSCOPE_CHECK_SIDECAR=1 scripts/agent-check.sh` |
| README keymap docs | `git diff --check`; targeted Rust checks if behavior changed |

If verification is blocked by missing `adb`, `gradle`, `java`, network access,
or no attached device, keep the code change narrow and report the blocker.

## Handoff Checklist

Before final handoff, report:

- files changed
- verification commands run and their result
- skipped live-device or sidecar checks, if any
- any user-local state intentionally touched

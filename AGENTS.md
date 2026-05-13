# Agent Instructions

This repository is `droidscope`, a Rust terminal UI for Android development.
Treat this file as the default starting point for LLM agents and automated
tool-use flows.

## Fast Orientation

- Main app: `src/main.rs` owns terminal setup, the event loop, and top-level
  hotkey dispatch.
- App state: `src/app.rs` keeps shared TUI state and persisted workspace/screen
  state.
- Panels: `src/*_ui.rs` files render panel UI; paired non-UI files usually own
  parsing, subprocess calls, and state transitions.
- Panel registry: `src/panel.rs`.
- Layout/rendering: `src/layout.rs`, `src/ui.rs`, `src/theme.rs`.
- Android subprocess helpers: `src/adb/`, `src/device_actions.rs`,
  `src/device_tools.rs`, `src/app_control.rs`, `src/app_data.rs`,
  `src/manifest.rs`, `src/intents.rs`.
- Gradle sidecar: `sidecar/gradle-agent/` is a Kotlin JVM fat jar launched by
  the Rust app.
- Non-interactive CLI sections live in `src/cli.rs`; use them when an agent
  needs a single panel-like snapshot without launching the TUI.

For a longer command and ownership map, read `docs/agent-tool-use.md`.

## Tooling Defaults

- Prefer `rg` and `rg --files` for search.
- Keep changes scoped to the panel, parser, or subprocess wrapper that owns the
  behavior.
- Use `cargo fmt` for Rust formatting.
- Use the shared check script for routine Rust-only verification:

```sh
scripts/agent-check.sh
```

- Use optional checks only when relevant to the change:

```sh
DROIDSCOPE_CHECK_FMT=1 scripts/agent-check.sh
DROIDSCOPE_CHECK_CLIPPY=1 scripts/agent-check.sh
DROIDSCOPE_CHECK_SIDECAR=1 scripts/agent-check.sh
```

Formatting is opt-in in the check script because the current Rust source is not
globally rustfmt-normalized yet. Do not include a broad formatting-only diff
unless that cleanup is the task.

## One-Shot Section Commands

Use these for agentic reads that should not enter the interactive TUI:

```sh
droidscope sections
droidscope daemons
droidscope daemons --json
droidscope section daemons
```

`deamons` is accepted as an alias for `daemons`. Most read-only TUI sections
also have one-shot commands: `devices`, `processes`, `monitor`, `logcat`,
`network`, `issues`, `projects`, `packages`, `emulators`, `files`, `config`,
`workspaces`, `panels`, `gradle`, `manifest`, `fps`, and `perf`. Mutating or
interactive sections such as `actions`, `app`, `data`, `intents`, and `shell`
print metadata/capabilities instead of executing actions.

## Live Device Safety

This project can run mutating Android commands. Do not run these unless the user
explicitly asks for that device action or the current task clearly requires it:

- `adb install`, `adb uninstall`
- app data clearing, force-stop, launch, settings mutations
- screen recording or screenshot capture
- `scrcpy`
- Wi-Fi ADB switching or `adb tcpip`

Safe discovery commands such as `adb devices -l` are fine when device state is
needed. Report when a check was skipped because no device was connected.

## Local State Boundaries

The app reads and writes user-local state under `~/.config/droidscope/`.
Avoid changing real user config/state/workspace files unless the user asks for
it. For tests or experiments, prefer temp directories, narrow unit tests, or
documented manual steps.

## Completion Gate

For documentation-only changes, run a syntax/whitespace check such as:

```sh
git diff --check
```

For Rust behavior changes, run at least:

```sh
scripts/agent-check.sh
```

If the change edits Rust code and formatting cleanup is in scope, also run:

```sh
DROIDSCOPE_CHECK_FMT=1 scripts/agent-check.sh
```

For Gradle sidecar changes, also run:

```sh
DROIDSCOPE_CHECK_SIDECAR=1 scripts/agent-check.sh
```

If a command cannot run because of missing local tools, network restrictions, or
no Android device, state that explicitly in the handoff.

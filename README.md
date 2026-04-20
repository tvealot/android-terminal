# droidscope

A customizable terminal UI for Android developers with live Gradle task monitoring.

Inspired by [measure-sh/holo](https://github.com/measure-sh/holo), with two extras:

- **Toggle panels** with `Alt+<digit>` — show/hide any panel, state persists across runs.
- **Gradle panel** that streams active/completed tasks in real time via the official
  Gradle Tooling API (through a small JVM sidecar).

## Stack

- **Rust + ratatui** — TUI rendering and event loop (sync, `std::thread::spawn` +
  `mpsc` channels for background I/O — no async runtime).
- **Kotlin + Gradle Tooling API** — sidecar binary that emits task events as
  newline-delimited JSON on stdout.
- `adb` (external) — subprocess for logcat.

## Layout

```
src/
  main.rs            event loop, hotkey dispatch
  app.rs             App state (visible panels, focus, Gradle state, logcat buffer)
  config.rs          ~/.config/droidscope/config.toml + state.json
  dispatch.rs        mpsc Event channel
  panel.rs           PanelId + static PANELS registry + feature gates
  theme.rs           Dark/Light themes
  ui.rs              dynamic layout from visible panels + header/footer/help
  logcat.rs / logcat_ui.rs
  gradle.rs  / gradle_ui.rs
  adb/               subprocess wrappers
sidecar/gradle-agent/ Kotlin fat-jar using GradleConnector + ProgressListener
```

## Build

```sh
cargo build --release
# → target/release/droidscope

cd sidecar/gradle-agent
gradle jar
# → build/libs/gradle-agent-0.1.0.jar
```

Install the jar where the Rust app expects it, or point to it from `config.toml`:

```sh
mkdir -p ~/.local/share/droidscope
cp sidecar/gradle-agent/build/libs/gradle-agent-0.1.0.jar \
   ~/.local/share/droidscope/gradle-agent.jar
```

## Configure

`~/.config/droidscope/config.toml`:

```toml
[ui]
theme = "dark"   # or "light"

[gradle]
project_dir   = "/path/to/your/android/project"
default_task  = "assembleDebug"
jar_path      = "~/.local/share/droidscope/gradle-agent.jar"
```

## Use

```sh
./target/release/droidscope
```

Key bindings:

| Key         | Action                              |
| ----------- | ----------------------------------- |
| `Alt+1..5`  | toggle panel visibility             |
| `l/m/g/f/n` | focus logcat / monitor / gradle / files / network |
| `r`         | run the configured Gradle task      |
| `?`         | help overlay                        |
| `q` / Esc   | quit                                |

Panel visibility and focus persist in `~/.config/droidscope/state.json`.

## How the Gradle panel works

The Rust process launches `java -jar gradle-agent.jar --project <dir> --task <task>`
as a child process. The sidecar uses `GradleConnector` to connect to the project
and registers a `ProgressListener` for `OperationType.TASK`. Each `TaskStartEvent`
and `TaskFinishEvent` is emitted to stdout as a single JSON line:

```json
{"kind":"task_start","ts":"2026-04-20T12:34:56Z","path":":app:compileDebugKotlin"}
{"kind":"task_finish","ts":"2026-04-20T12:34:59Z","path":":app:compileDebugKotlin","outcome":"SUCCESS","duration_ms":2843}
{"kind":"build_finish","ts":"2026-04-20T12:35:10Z","outcome":"SUCCESS"}
```

A Rust reader thread parses each line and pushes events through the `mpsc` channel
into the main loop, where `GradleState::apply` updates the live view.

If `java` is not on PATH the Gradle panel is hidden automatically and a status
flash shows `install JDK 17+ to enable Gradle panel`.

## MVP scope

Panels currently implemented:

- `logcat` — real `adb logcat -v threadtime` subprocess, 2000-line ring buffer
- `gradle` — live Tooling-API events

Panels stubbed (`Coming soon`): `monitor`, `files`, `network`. They already
participate in the toggle/focus/layout system; only the content renderers
need to land.

## License

MIT

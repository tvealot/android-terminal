# droidscope

Customizable terminal UI for Android developers with live Gradle task monitoring,
logcat, issues triage, and an embedded `adb shell`.

Inspired by [measure-sh/holo](https://github.com/measure-sh/holo), extended with:

- **Toggleable panels** (`1..9`, `A`, `B`, `M`, `U`, `D`, `F`, `H`) with a grid **layout editor**
  (`0`) — state persists across runs.
- **Gradle panel** — live Tooling-API task stream (JVM sidecar) plus a host
  process list (daemon / wrapper / kotlin / android / agent) with `SIGTERM` kill.
- **App panel** — launch, force-stop, clear data, open app settings, and inspect
  package metadata for a shared target package.
- **Data panel** — browse app-private files through `run-as`, with text preview.
- **Manifest panel** — inspect installed APK path, version, permissions,
  components, and deep links using `aapt`/`apkanalyzer` when available.
- **Intents panel** — run deep links via `am start -a VIEW -d`, optionally scoped
  to the shared target package.
- **Device Actions panel** — screenshot, screenrecord, rotation, dark mode,
  locale, font scale, battery simulation, airplane/Wi-Fi/data toggles, text
  input, and tap.
- **Perf panel** — periodic `dumpsys meminfo` + `dumpsys gfxinfo` + `/proc/<pid>/stat`
  sampler for the shared target package: PSS/RSS, App Summary breakdown, CPU%,
  jank %, frame p50/p90/p95/p99, GC marker count, with sparkline history.
- **FPS panel** — focused-app frame pacing sample.
- **Issues panel** — detects Java/Kotlin stacktraces in logcat and captures the
  full trace for a detail view.
- **Project picker** (`w`) — scans `~/Documents` for Android projects (directories
  containing `gradlew`), sorted by mtime. Selection updates `files` root and
  writes `gradle.project_dir` back to `config.toml`.
- **Saved workspaces** (`S` save / `W` switch) — bind project dir, default Gradle
  task, target package, preferred device, logcat filters, and per-screen layouts
  to a named profile in `workspaces.json`.
- **Emulator picker** (`e`) — list and launch installed AVDs.
- **Embedded `adb shell`** (`s`/`9`) — real PTY with `vt100` rendering; keys route
  to the shell while focused, `Ctrl+\` to defocus.
- **Zoom** (`z`) — fullscreen the focused panel; `Esc` to restore.
- **Cyrillic keymap** — Russian layout chars normalized to QWERTY equivalents,
  so hotkeys work without switching layout.

## Stack

- **Rust + ratatui + crossterm** — sync TUI, `std::thread::spawn` + `mpsc`
  channels for background I/O (no async runtime).
- **portable-pty + vt100** — embedded `adb shell` PTY.
- **Kotlin + Gradle Tooling API** — sidecar fat-jar emitting task events as
  NDJSON on stdout.
- **`adb`** (external) — subprocess for logcat, monitor, processes, devices.

## Panels

| Id  | Toggle | Focus | Notes                                                       |
| --- | ------ | ----- | ----------------------------------------------------------- |
| logcat    | `1` | `l` | `adb logcat -v threadtime`, 2000-line ring buffer, filter / regex / level / package filter / pause |
| monitor   | `2` | `m` | device runtime sample (`dumpsys`), focus / layout summary   |
| gradle    | `3` | `g` | Tooling-API task stream + host gradle/kotlin daemons, `K` to `SIGTERM` selected |
| processes | `4` | `p` | `adb shell ps`-style list                                   |
| issues    | `5` | `i` | Java/Kotlin stacktraces auto-captured from logcat           |
| files     | `6` | `f` | Local project tree, text preview pane                       |
| network   | `7` | `n` | Logcat view filtered to `okhttp`/`http`/`socket`/`dns`/...  |
| devices   | `8` | `v` | `adb devices` list, `Enter` to switch                       |
| actions   | `D` | `o` | Device actions: screenshot, screenrecord, rotate, dark mode, locale, font, battery, network, input |
| shell     | `9` | `s` | Embedded `adb shell` PTY                                    |
| app       | `A` | `a` | Launch / force-stop / clear data / settings / package info  |
| data      | `B` | `b` | `run-as` app-private file browser with preview              |
| manifest  | `M` | `x` | Installed APK / manifest inspector via `aapt` or `apkanalyzer` |
| intents   | `U` | `u` | Deep link runner using `am start -a VIEW -d`                 |
| fps       | `F` | `F` | Focused app frame pacing sample                             |
| perf      | `H` | `H` | meminfo + CPU + gfxinfo jank + GC markers, sparkline history |

## Layout

The body is stacked vertically across visible panels by default. Press `0` to
open the **grid layout editor** — pick cells with `h/j/k/l` + `v`, assign a
panel with its toggle key, `Enter` to save. Grid persists in `state.json`.

```
src/
  main.rs             event loop, hotkey dispatch
  app.rs              App state (visible panels, focus, sub-panel states)
  config.rs           ~/.config/droidscope/config.toml + state.json
  dispatch.rs         mpsc Event channel
  panel.rs            PanelId + static PANELS registry + feature gates
  layout.rs           grid layout + interactive editor
  theme.rs            Dark/Light themes
  ui.rs               dynamic layout renderer, header / footer / help / overlays
  logcat.rs           logcat ring buffer, filter/regex/level/package state
  logcat_ui.rs
  gradle.rs           sidecar spawn, host ps poller, GradleState
  gradle_ui.rs
  app_control.rs / app_control_ui.rs
  device_actions.rs / device_actions_ui.rs
  app_data.rs / app_data_ui.rs
  manifest.rs / manifest_ui.rs
  intents.rs / intents_ui.rs
  monitor.rs / monitor_ui.rs
  processes.rs / processes_ui.rs
  issues.rs / issues_ui.rs     stacktrace detector + expanded detail
  files.rs / files_ui.rs       tree + preview
  network_ui.rs
  devices_ui.rs
  fps.rs / fps_ui.rs
  shell.rs / shell_ui.rs       portable-pty + vt100
  project_picker.rs            ~/Documents scan for gradlew projects
  adb/                         subprocess wrappers (logcat, devices, ...)
sidecar/gradle-agent/          Kotlin fat-jar using GradleConnector + ProgressListener
```

## Build

Requires Rust 1.88 or newer.

Homebrew:

```sh
brew upgrade rust
```

`rustup`:

```sh
rustup toolchain install 1.88.0
rustup override set 1.88.0
```

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
project_dir  = "/path/to/your/android/project"
default_task = "assembleDebug"
jar_path     = "~/.local/share/droidscope/gradle-agent.jar"

[android]
package = "com.example.app"
```

Use `w` in the TUI to pick a project interactively — it scans `~/Documents`
for directories containing `gradlew`, lists them sorted by mtime, and writes
the selection back to `gradle.project_dir`.

Saved workspaces live in `~/.config/droidscope/workspaces.json`. Press `S` to
save the current project profile, or `W` to switch between saved workspaces.
A workspace binds the project directory, default Gradle task, target package,
preferred device, logcat filters, and screen/layout state.

## Use

```sh
./target/release/droidscope
```

Text selection: mouse capture is disabled, so terminal-native
selection + copy works normally (drag to select, ⌘C / right-click
copy). The embedded shell PTY is the exception — its viewport repaints
on resize.

### Global

| Key          | Action                                            |
| ------------ | ------------------------------------------------- |
| `1..9`, `A`, `B`, `M`, `U`, `D`, `F`, `H` | toggle panel visibility |
| `[` / `]`    | switch previous / next layout screen; each screen keeps its own panels, focus, and grid layout |
| `Alt+1..4`   | switch directly to layout screen 1..4, if your keyboard has Alt |
| `0`          | open grid layout editor                           |
| `l/m/g/p/i/f/n/v/o/s/a/b/x/u/F/H` | focus panel              |
| `Tab` / `Shift+Tab` | cycle focus across visible panels          |
| `z`          | zoom focused panel (`Esc` restores)               |
| `d`          | open device selector overlay                      |
| `w`          | open project picker overlay                       |
| `W`          | open saved workspace overlay                      |
| `S`          | save current project as a workspace               |
| `e`          | open emulator picker overlay                      |
| `r`          | run configured Gradle task                        |
| `?`          | help overlay                                      |
| `q` / `Esc`  | quit                                              |

### Logcat

| Key       | Action                                        |
| --------- | --------------------------------------------- |
| `/`       | enter filter mode (tag/message substring)     |
| `R`       | toggle regex filter                           |
| `L`       | cycle minimum level (V→D→I→W→E→V)             |
| `P`       | filter by package (`pidof`)                   |
| `X`       | clear package filter                          |
| `Space`   | pause/resume                                  |
| `C`       | clear buffer                                  |
| `j`/`k` `↑`/`↓` | scroll 1 line; `PgUp`/`PgDn` 20         |
| `gg` / `G` | jump top / bottom (follow tail)              |

### Files / Gradle / Issues / Shell

| Key | Action |
| --- | ------ |
| `j`/`k` or `↓`/`↑` | navigate (files tree / issues list / host gradle procs) |
| `Enter` / `→` | expand dir or open file preview; toggle issue stacktrace |
| `←` / `Backspace` | collapse dir / close preview |
| `Tab` | switch tree ↔ detail in files (when preview open) |
| `r` | refresh files tree |
| `K` (gradle) | send `SIGTERM` to selected host process |
| `y` (issues) | copy full stacktrace of selected issue to clipboard |
| `C` (issues) | clear issues list |
| `Ctrl+\` (shell) | defocus PTY (cycle to next panel) |

### App / Data / Manifest / Intents

| Key | Action |
| --- | ------ |
| `P` | set shared target package (`[android].package`) |
| `A` / `a` | toggle / focus App Control |
| `B` / `b` | toggle / focus App Data |
| `M` / `x` | toggle / focus Manifest inspector |
| `U` / `u` | toggle / focus Intents |
| `j`/`k` or `↓`/`↑` | navigate app actions / data entries |
| `Enter` (app) | run selected app action |
| `!` (app) | confirm destructive app action (`clear data`) |
| `r` (data) | refresh current `run-as` directory |
| `Enter` / `→` (data) | open directory or file preview |
| `←` / `Backspace` (data) | close preview or go to parent directory |
| `Tab` (data) | switch to preview pane |
| `r` (manifest) | refresh installed APK / manifest report |
| `j`/`k`, `PgUp`/`PgDn` (manifest) | scroll report |
| `/` (intents) | edit deep link URL |
| `T` (intents) | toggle resolver vs explicit target package |
| `C` (intents) | clear deep link URL |
| `Enter` (intents) | launch deep link |

### Device Actions

| Key | Action |
| --- | ------ |
| `D` / `o` | toggle / focus Device Actions |
| `j`/`k` or `↓`/`↑` | choose action |
| `Enter` | run selected action |
| input actions | prompt in footer; `Enter` runs, `Esc` cancels |

Actions include screenshot (`droidscope-screenshot-*.png`), 10-second
screenrecord (`droidscope-screenrecord-*.mp4`), rotate right, dark mode
on/off, locale, font scale, battery unplug/reset, airplane mode, Wi-Fi,
mobile data, `input text`, and `input tap x y`.

### Perf / FPS

| Key | Action |
| --- | ------ |
| `H` | toggle / focus Perf panel |
| `F` | toggle / focus FPS panel |
| `P` (perf/fps) | set shared target package |
| `X` (perf/fps) | clear target package, reset history |

Perf samples every 2 s via `dumpsys meminfo`, `dumpsys gfxinfo`, and
`/proc/<pid>/stat`. GC markers fire on Dalvik heap drops ≥ 512 KB. The
ring buffer keeps 60 samples (~2 minutes).

### Layout editor (after `0`)

Use `[` / `]` to switch between independent layout screens. `Alt+1..4` also
switches directly to a screen when available. Editing with `0` updates the
currently active screen only.

| Key | Action |
| --- | ------ |
| `h/j/k/l` | move cursor |
| `v` / `Space` | toggle selection |
| panel toggle key (`1..9`, `A`, `B`, `M`, `U`, `D`, `F`, `H`) | assign panel to selected cell |
| `x` / `d` | delete cell at cursor |
| `c` | clear all cells |
| `[` / `]` | cols -/+ |
| `-` / `=` | rows -/+ |
| `Enter` | save layout |
| `Esc` | cancel |

Panel visibility, focus, active screen, and per-screen grid layouts persist in
`~/.config/droidscope/state.json`. Saved project profiles persist in
`~/.config/droidscope/workspaces.json`.

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

A Rust reader thread parses each line and pushes events through the `mpsc`
channel into the main loop, where `GradleState::apply` updates the live view.

In parallel, a poller runs `ps -axo pid,pcpu,rss,command` every 2 s and
classifies matching host processes (`GradleDaemon`, `gradle-wrapper.jar`,
`KotlinCompileDaemon`, `com.android.build`, `aapt2`, `gradle-agent.jar`) so the
panel shows external builds too — select one and press `K` to `SIGTERM` it.

If `java` is not on PATH the Gradle panel is hidden automatically and a status
flash shows `install JDK 17+ to enable Gradle panel`.

## License

MIT — see [LICENSE](LICENSE).

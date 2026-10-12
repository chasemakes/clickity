<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://shieldcn.dev/header/grid.svg?title=Clickity&amp;subtitle=Clicks%2C+right+on+cue.&amp;logo=rust&amp;align=center&amp;theme=violet&amp;mode=dark" />
    <img src="https://shieldcn.dev/header/grid.svg?title=Clickity&amp;subtitle=Clicks%2C+right+on+cue.&amp;logo=rust&amp;align=center&amp;theme=violet&amp;mode=light" alt="Clickity — Clicks, right on cue." />
  </picture>
</p>

<p align="center">
  A lightweight desktop autoclicker with precise timing, flexible targeting, and a global start/stop shortcut.
</p>

<p align="center">
  <a href="Cargo.toml"><img src="https://shieldcn.dev/badge/version-0.1.0-7c3aed.svg?variant=secondary&amp;logo=false" alt="Version 0.1.0" /></a>
  <a href="https://gpui-kit.com/"><img src="https://shieldcn.dev/badge/GPUI_Kit-0.7-24c8db.svg?variant=secondary&amp;logo=rust" alt="Built with GPUI Kit 0.7" /></a>
  <a href="https://github.com/chasebailly/clickity/stargazers"><img src="https://shieldcn.dev/github/stars/chasebailly/clickity.svg?variant=secondary" alt="GitHub stars" /></a>
  <a href="https://github.com/chasebailly/clickity/commits/main"><img src="https://shieldcn.dev/github/last-commit/chasebailly/clickity.svg?variant=secondary" alt="Last commit" /></a>
  <a href="LICENSE"><img src="https://shieldcn.dev/badge/license-MIT-22c55e.svg?variant=secondary&amp;logo=false" alt="MIT license" /></a>
</p>

Clickity is a small, cross-platform desktop app written entirely in Rust. Its interface is drawn on the GPU with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://gpui-kit.com/), with no webview or browser runtime, and the same process handles timing, global shortcuts, and native mouse input.

> [!IMPORTANT]
> Clickity generates real mouse input. Test new configurations somewhere harmless, keep the stop shortcut in mind, and only use automation where you have permission.

## Features

- **Precise intervals** — combine hours, minutes, seconds, and milliseconds.
- **Any mouse button** — automate left, middle, or right clicks.
- **Flexible repeat modes** — run for an exact count or until you stop it.
- **Two targeting modes** — follow the live cursor or return to fixed X/Y coordinates for every click.
- **Position capture** — move the pointer anywhere and press `F7` (rebindable) at any time to save its coordinates.
- **Global control** — start or stop Clickity from any application with a configurable shortcut (`F6` by default).
- **Rebindable shortcuts** — change the start/stop and position-capture shortcuts from the settings dialog in the title bar.
- **Persistent preferences** — restore your last configuration on launch while always starting safely in an idle state.
- **Responsive cancellation** — stop immediately, even while waiting through a long interval.
- **Clear feedback** — see inline validation, live click progress, and native errors.

## Getting started

Clickity does not have packaged downloads yet. Run it from source with the steps below.

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.92 or later
- **macOS:** macOS 15 or later and the Xcode Command Line Tools (`xcode-select --install`)
- **Windows:** Windows 10 or later, Visual Studio 2022 Build Tools with the *Desktop development with C++* workload, the MSVC Rust toolchain, and CMake on `PATH`
- **Linux:** a Wayland or X11 session with a working Vulkan driver, plus the development packages below

On Debian or Ubuntu, install the native dependencies with:

```bash
sudo apt update
sudo apt install -y gcc g++ clang pkg-config libfontconfig-dev libwayland-dev \
  libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev libvulkan1
```

Other distributions need the equivalent packages. These requirements come from the [GPUI Kit installation guide](https://gpui-kit.com/docs/installation).

Clone the repository and launch the desktop app:

```bash
git clone https://github.com/chasebailly/clickity.git
cd clickity
cargo run
```

## Using Clickity

1. Set how long Clickity should wait between clicks.
2. Choose the mouse button and either a click count or **Until stopped**.
3. Select **Follow cursor** or **Fixed position**. For a fixed target, move the pointer to the target and press `F7` to record its coordinates.
4. Select **Start clicking** or press the global shortcut. Press the shortcut again—or select **Stop clicking**—to stop.

The first click occurs after one complete interval. Starting **Follow cursor** from the window adds a three-second countdown so you can move away from the Start button; starting with the global shortcut begins immediately. The capture shortcut stays registered for the whole session. Both shortcuts are rebindable from the settings dialog, and neither can be assigned the other's combination. Capture is ignored while a click run is active, since the settings are locked then.

Preferences are saved to `settings.json` in a `clickity` folder inside your configuration directory: `~/.config` on Linux, `~/Library/Application Support` on macOS, and `%APPDATA%` on Windows.

## Platform support

| Platform | Status | Notes |
| --- | --- | --- |
| Windows | Supported | A normally launched app cannot click inside applications running as administrator. Mixed-DPI, multi-monitor setups should be verified manually. |
| macOS | Supported | Grant Clickity **Accessibility** permission when macOS asks so it can synthesize mouse input. The window keeps the native macOS window buttons. |
| Linux (X11) | Supported | Clickity currently uses the X11 input backend. |
| Linux (Wayland) | Supported | Compositors control global shortcuts and synthetic input; permission prompts and capability errors vary by desktop. |

Very short intervals are best-effort on every platform. Operating-system scheduling means Clickity cannot provide hard real-time guarantees.

## Development

The main project commands are:

| Command | Purpose |
| --- | --- |
| `cargo run` | Run the desktop app |
| `cargo test` | Run the test suite with a fake mouse driver |
| `cargo clippy --all-targets -- -D warnings` | Lint the code |
| `cargo packager --release` | Produce installers for the current platform |

Packaging uses [cargo-packager](https://github.com/crabnebula-dev/cargo-packager) (`cargo install cargo-packager --locked`), configured under `[package.metadata.packager]` in `Cargo.toml`. Installers are written to `target/packages/`. Build on each target operating system; production Windows and macOS releases should also be signed, with macOS builds notarized.

## How it works

The window is a single GPUI view (`src/app.rs`) that owns the form, saved settings, validation, the start countdown, shortcut recording, and status display. Its controls are built from GPUI Kit's unstyled primitives and styled with the palette in `DESIGN.md`; the settings dialog and text inputs come from GPUI Kit's component library. Global shortcuts are registered directly with the [`global-hotkey`](https://crates.io/crates/global-hotkey) crate, and mouse input goes through [`enigo`](https://crates.io/crates/enigo).

The click engine runs on its own thread and reports each state change to the view over a channel. The click loop waits on an interruptible channel instead of sleeping, which keeps **Stop** responsive even when the configured interval is several hours. Tests use a fake mouse driver and never generate real input.

## Project structure

```text
clickity/
├── src/
│   ├── main.rs        # App startup and window
│   ├── app.rs         # Main window, settings dialog, and run control
│   ├── ui.rs          # Shared controls: segmented rails, buttons, keycaps
│   ├── engine.rs      # Click loop and native mouse driver
│   ├── model.rs       # Click configuration and runtime state
│   ├── settings.rs    # Saved preferences and validation
│   ├── shortcuts.rs   # Global shortcuts and shortcut recording
│   ├── theme.rs       # Palette and fonts
│   └── assets.rs      # Embedded icons and fonts
├── assets/            # Fonts, UI icons, and app icons
└── Cargo.toml         # Dependencies and packaging configuration
```

## Contributing

Bug reports and focused pull requests are welcome. Before opening a pull request, run:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Changes involving native input should be tested on every affected operating system. Keep automated tests behind the fake mouse driver so the test suite never clicks the user's desktop.

## License

Clickity is available under the [MIT License](LICENSE).

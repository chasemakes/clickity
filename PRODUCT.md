# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

Clickity is delivered as a cross-platform desktop application written in Rust. Its interface is rendered natively with GPUI and GPUI Kit, in the same process that handles timing, global shortcuts, cursor access, and native mouse input.

## Users

People who use, or would otherwise use, OP Auto Clicker and want a modern autoclicker that is not tied to a single desktop platform. Gaming is the primary usage context. Their core job is to automate repetitive mouse clicking with predictable timing and targeting while retaining immediate control over when automation starts and stops.

## Product Purpose

Clickity provides a lightweight, dependable way to automate repeated mouse clicks across desktop operating systems. Success means a user can configure a click routine quickly, start it from Clickity or a global shortcut, understand its current state, and stop it immediately.

## Positioning

Clickity is a modern, cross-platform replacement for OP Auto Clicker. Unlike tools whose practical support is limited to Windows or X11, Clickity is intended to work across Windows, macOS, and Linux, including Wayland sessions.

## Operating Context

- Users configure an interval, mouse button, repeat limit, and pointer-targeting mode before starting a run.
- Clickity is a session-based utility: users open it when needed for a game, configure and run it quickly, then close it when finished.
- Runs can follow the live cursor or return to fixed screen coordinates for every click.
- A configurable global shortcut starts and stops clicking while another application has focus.
- Fixed-position capture is initiated in Clickity, then confirmed at the target with a dedicated `F7` shortcut; delayed starts for cursor-following runs give users time to move away from the Clickity window.
- The product generates real operating-system mouse input and must be used only where the user has permission to automate.

## Capabilities and Constraints

- Supports intervals composed from hours, minutes, seconds, and milliseconds; very short intervals remain subject to operating-system scheduling rather than hard real-time guarantees.
- Supports left, middle, and right mouse buttons.
- Supports an exact click count or continuous clicking until stopped.
- Supports live-cursor and fixed-coordinate targeting. The approved capture interaction minimizes Clickity, lets the user position the pointer without a countdown, and records the coordinates when they press the dedicated `F7` shortcut.
- Persists the most recent preferences locally but always launches in an idle state.
- Provides responsive cancellation even during long intervals.
- Targets Windows, macOS, and Linux, including Linux Wayland sessions.
- Renders its interface on the GPU with GPUI rather than through a webview or bundled browser runtime.
- A normally launched Windows build cannot send clicks into applications running as administrator.
- macOS requires Accessibility permission to synthesize mouse input.
- Release builds are produced separately for each target operating system; production Windows and macOS releases require their respective signing workflows.
- Packaged downloads are not currently available.

## Brand Commitments

- Product name: Clickity.
- Tagline: “Clicks, right on cue.”
- The product voice is concise, approachable, and safety-conscious.
- The visual identity must feel distinctly modern and must not closely imitate OP Auto Clicker.
- The existing product mark is stored at `assets/icons/clickity-mark.svg`; platform icons are stored under `assets/app-icons/`.
- The project is distributed under the MIT License.

## Evidence on Hand

- `README.md` documents the current product proposition, feature set, operating instructions, platform notes, development workflow, and safety guidance.
- `src/app.rs`, `src/ui.rs`, and `src/settings.rs` contain the working interface, validation, persistence, runtime feedback, and user-facing terminology.
- `src/engine.rs`, `src/model.rs`, and `src/shortcuts.rs` implement native input, timing, cancellation, cursor capture, and global shortcuts.
- `src/app.rs` records the desktop window configuration; `Cargo.toml` records the packaging configuration.
- Fixed-position capture is implemented as an explicit `F7` confirmation flow, independent of the configurable start/stop shortcut.
- Linux input builds enable both Wayland and X11 backends. Exact permission prompts and available input capabilities remain compositor-controlled and are reported honestly in the interface and README.
- No testimonials, customer logos, usage benchmarks, packaged-download claims, or other market proof are currently present and must not be fabricated.

## Product Principles

1. Make repetitive clicking quick to configure and obvious to control.
2. Treat cross-platform behavior, including Linux Wayland support, as a core product requirement.
3. Keep automation safe and reversible through clear state, deliberate starts, and immediate stopping.
4. Favor a lightweight native footprint without sacrificing timing precision or reliability.
5. Preserve honest platform limitations and avoid guarantees the operating system cannot support.

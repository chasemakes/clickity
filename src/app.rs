//! The Clickity window: the timing console, three behavior rails, the run dock, and the
//! settings dialog for the two global shortcuts.

use crate::engine::{ClickEngine, MouseDriver, SystemMouse};
use crate::model::{ClickButton, ClickConfig, RunPhase, RuntimeSnapshot};
use crate::settings::{
    format_duration, parse_field, Draft, PositionKind, RepeatKind, Settings, ValidationErrors,
};
use crate::shortcuts::{self, Recorded, ShortcutKind, Shortcuts};
use crate::theme::*;
use crate::ui::*;
use futures::StreamExt as _;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use gpui_kit::base::{Button, NumberInput as BaseNumberInput};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::WindowExt as _;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::{cell::Cell, rc::Rc, time::Duration};

const TITLEBAR_HEIGHT: f32 = 38.;
const COUNTDOWN_SECONDS: u8 = 3;
/// Roughly the settings dialog's height, used to centre it vertically.
const SETTINGS_DIALOG_HEIGHT: f32 = 210.;
/// GPUI Kit frames Linux windows itself: a 20px shadow margin plus a 1px border per side.
const FRAME_INSET: f32 = if cfg!(target_os = "linux") { 21. } else { 0. };
/// The frame's border; its shadow margin comes from `window_paddings`.
const FRAME_BORDER: Pixels = px(1.);

pub fn window_options(cx: &App) -> WindowOptions {
    let framed =
        |width: f32, height: f32| size(px(width + 2. * FRAME_INSET), px(height + 2. * FRAME_INSET));
    WindowOptions {
        // Opens at the default form's height; the window then tracks its content.
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            framed(600., 462.),
            cx,
        ))),
        // Windows and Linux draw Clickity's own title bar; macOS keeps its traffic lights.
        titlebar: Some(TitlebarOptions {
            title: Some("Clickity".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.), px(12.))),
        }),
        app_owns_titlebar_drag: true,
        window_min_size: Some(framed(520., 320.)),
        // The Linux frame's shadow margin needs a transparent surface behind it.
        window_background: if cfg!(target_os = "linux") {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        },
        window_decorations: Some(WindowDecorations::Client),
        // Matches the packaged clickity.desktop entry so Linux docks show the right icon.
        app_id: Some("clickity".into()),
        ..Default::default()
    }
}

struct Fields {
    hours: Entity<InputState>,
    minutes: Entity<InputState>,
    seconds: Entity<InputState>,
    milliseconds: Entity<InputState>,
    repeat_count: Entity<InputState>,
    x: Entity<InputState>,
    y: Entity<InputState>,
}

impl Fields {
    fn all(&self) -> [&Entity<InputState>; 7] {
        [
            &self.hours,
            &self.minutes,
            &self.seconds,
            &self.milliseconds,
            &self.repeat_count,
            &self.x,
            &self.y,
        ]
    }
}

struct Countdown {
    remaining: u8,
    config: ClickConfig,
    _task: Task<()>,
}

struct Recording {
    kind: ShortcutKind,
    copy: SharedString,
    _interceptor: Subscription,
}

pub struct ClickityApp {
    engine: ClickEngine,
    shortcuts: Shortcuts,
    /// The last valid form, which is what gets saved.
    settings: Settings,
    runtime: RuntimeSnapshot,
    fields: Fields,
    button: ClickButton,
    repeat_mode: RepeatKind,
    position_mode: PositionKind,
    show_validation: bool,
    capture_error: Option<String>,
    countdown: Option<Countdown>,
    recording: Option<Recording>,
    recorder_focus: FocusHandle,
    /// Debounces held hotkeys: index 0 is the run shortcut, 1 the capture shortcut.
    hotkey_down: [bool; 2],
    titlebar_drag: bool,
    measured_height: Rc<Cell<Option<Pixels>>>,
    _tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ClickityApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut settings = Settings::load();

        let (runtime_tx, mut runtime_rx) = futures::channel::mpsc::unbounded();
        let engine = ClickEngine::new(move |snapshot| {
            let _ = runtime_tx.unbounded_send(snapshot);
        });
        let (shortcuts, mut hotkey_rx) = Shortcuts::new(&settings.hotkey, &settings.capture_hotkey);

        let saved_shortcuts = (settings.hotkey.clone(), settings.capture_hotkey.clone());
        settings.hotkey = shortcuts.accelerator(ShortcutKind::Run).into();
        settings.capture_hotkey = shortcuts.accelerator(ShortcutKind::Capture).into();
        if saved_shortcuts != (settings.hotkey.clone(), settings.capture_hotkey.clone()) {
            persist(&settings);
        }

        if let Some(config) = Draft::from(&settings).click_config() {
            let _ = engine.update_config(config);
        }

        let fields = Fields {
            hours: interval_field(settings.hours, 9_999., window, cx),
            minutes: interval_field(settings.minutes, 59., window, cx),
            seconds: interval_field(settings.seconds, 59., window, cx),
            milliseconds: interval_field(settings.milliseconds, 999., window, cx),
            repeat_count: text_field(settings.repeat_count, false, window, cx),
            x: text_field(settings.x, true, window, cx),
            y: text_field(settings.y, true, window, cx),
        };
        let subscriptions = fields
            .all()
            .into_iter()
            .map(|field| {
                cx.subscribe_in(
                    field,
                    window,
                    |this, _, event: &InputEvent, _, cx| match event {
                        InputEvent::Change => this.form_changed(cx),
                        InputEvent::Blur => {
                            this.show_validation = true;
                            this.form_changed(cx);
                        }
                        _ => {}
                    },
                )
            })
            .collect();

        let tasks = vec![
            cx.spawn(async move |this, cx| {
                while let Some(snapshot) = runtime_rx.next().await {
                    if this
                        .update(cx, |this, cx| this.apply_runtime(snapshot, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }),
            cx.spawn_in(window, async move |this, cx| {
                while let Some(event) = hotkey_rx.next().await {
                    let handled = this
                        .update_in(cx, |this, window, cx| this.handle_hotkey(event, window, cx));
                    if handled.is_err() {
                        break;
                    }
                }
            }),
        ];

        Self {
            runtime: engine.snapshot(),
            engine,
            shortcuts,
            button: settings.button,
            repeat_mode: settings.repeat_mode,
            position_mode: settings.position_mode,
            settings,
            fields,
            show_validation: false,
            capture_error: None,
            countdown: None,
            recording: None,
            recorder_focus: cx.focus_handle(),
            hotkey_down: [false; 2],
            titlebar_drag: false,
            measured_height: Rc::default(),
            _tasks: tasks,
            _subscriptions: subscriptions,
        }
    }

    fn draft(&self, cx: &App) -> Draft {
        let value = |field: &Entity<InputState>| parse_field(&field.read(cx).value());
        Draft {
            hours: value(&self.fields.hours),
            minutes: value(&self.fields.minutes),
            seconds: value(&self.fields.seconds),
            milliseconds: value(&self.fields.milliseconds),
            button: self.button,
            repeat_mode: self.repeat_mode,
            repeat_count: value(&self.fields.repeat_count),
            position_mode: self.position_mode,
            x: value(&self.fields.x),
            y: value(&self.fields.y),
        }
    }

    fn controls_locked(&self) -> bool {
        self.runtime.phase == RunPhase::Running || self.countdown.is_some()
    }

    /// Saves a valid form and hands it to the engine; invalid forms only re-render.
    fn form_changed(&mut self, cx: &mut Context<Self>) {
        let draft = self.draft(cx);
        if let Some(config) = draft.click_config() {
            draft.apply_to(&mut self.settings);
            persist(&self.settings);
            if self.runtime.phase == RunPhase::Error {
                self.runtime.phase = RunPhase::Idle;
                self.runtime.message = None;
            }
            if !self.controls_locked() {
                if let Err(error) = self.engine.update_config(config) {
                    self.set_local_error(error);
                }
            }
        }
        cx.notify();
    }

    fn select<T>(
        &self,
        cx: &Context<Self>,
        apply: impl Fn(&mut Self, T) + 'static,
    ) -> impl Fn(T, &mut Window, &mut App) + 'static {
        let this = cx.weak_entity();
        move |value, _, cx| {
            let _ = this.update(cx, |this, cx| {
                apply(this, value);
                this.form_changed(cx);
            });
        }
    }

    fn apply_runtime(&mut self, snapshot: RuntimeSnapshot, cx: &mut Context<Self>) {
        // A run started from the hotkey supersedes a pending countdown.
        if snapshot.phase != RunPhase::Idle {
            self.countdown = None;
        }
        self.runtime = snapshot;
        cx.notify();
    }

    fn set_local_error(&mut self, message: String) {
        self.runtime.phase = RunPhase::Error;
        self.runtime.message = Some(message);
    }

    fn handle_action(&mut self, cx: &mut Context<Self>) {
        if self.runtime.phase == RunPhase::Running {
            let snapshot = self.engine.stop();
            self.apply_runtime(snapshot, cx);
            return;
        }

        if self.countdown.take().is_some() {
            cx.notify();
            return;
        }

        let draft = self.draft(cx);
        self.show_validation = true;
        let Some(config) = draft.click_config() else {
            cx.notify();
            return;
        };
        draft.apply_to(&mut self.settings);
        persist(&self.settings);

        // Following the cursor needs a moment to move the pointer off the start button.
        if self.position_mode == PositionKind::Current {
            self.begin_countdown(config, cx);
        } else {
            self.start_clicking(config, cx);
        }
    }

    fn start_clicking(&mut self, config: ClickConfig, cx: &mut Context<Self>) {
        match self
            .engine
            .update_config(config)
            .and_then(|()| self.engine.start())
        {
            Ok(snapshot) => self.apply_runtime(snapshot, cx),
            Err(error) => {
                self.set_local_error(error);
                cx.notify();
            }
        }
    }

    fn begin_countdown(&mut self, config: ClickConfig, cx: &mut Context<Self>) {
        self.runtime.phase = RunPhase::Idle;
        self.runtime.message = None;
        let task = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            match this.update(cx, |this, cx| this.tick_countdown(cx)) {
                Ok(false) => {}
                _ => break,
            }
        });
        self.countdown = Some(Countdown {
            remaining: COUNTDOWN_SECONDS,
            config,
            _task: task,
        });
        cx.notify();
    }

    /// Returns true once the countdown is over, whether it started a run or was cancelled.
    fn tick_countdown(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(countdown) = self.countdown.as_mut() else {
            return true;
        };
        countdown.remaining -= 1;
        if countdown.remaining > 0 {
            cx.notify();
            return false;
        }
        let config = countdown.config.clone();
        self.countdown = None;
        self.start_clicking(config, cx);
        true
    }

    fn handle_hotkey(
        &mut self,
        event: GlobalHotKeyEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self.shortcuts.kind_for(event.id()) else {
            return;
        };
        let down = &mut self.hotkey_down[kind as usize];
        match event.state() {
            HotKeyState::Released => {
                *down = false;
                return;
            }
            HotKeyState::Pressed if std::mem::replace(down, true) => return,
            HotKeyState::Pressed => {}
        }

        match kind {
            // The engine reports the new state through the runtime channel.
            ShortcutKind::Run => self.engine.toggle(),
            // Settings are locked mid-run, so captures are ignored then.
            ShortcutKind::Capture if !self.controls_locked() => self.capture_position(window, cx),
            ShortcutKind::Capture => {}
        }
    }

    fn capture_position(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match SystemMouse::new().and_then(|mouse| mouse.location()) {
            Ok((x, y)) => {
                for (field, value) in [(&self.fields.x, x), (&self.fields.y, y)] {
                    field.update(cx, |field, cx| {
                        field.set_value(value.to_string(), window, cx)
                    });
                }
                self.capture_error = None;
                self.form_changed(cx);
            }
            Err(error) => {
                self.capture_error = Some(error);
                cx.notify();
            }
        }
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            this.read(cx).settings_dialog(dialog, &this, window, cx)
        });
    }

    fn toggle_recording(
        &mut self,
        kind: ShortcutKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .recording
            .as_ref()
            .is_some_and(|recording| recording.kind == kind)
        {
            self.end_recording(cx);
            return;
        }
        if self.controls_locked() {
            return;
        }

        self.shortcuts.set_error(kind, None);
        // Park focus where Enter and Space cannot press a button while recording.
        self.recorder_focus.focus(window, cx);
        let this = cx.weak_entity();
        let interceptor = cx.intercept_keystrokes(move |event, _, cx| {
            let handled = this
                .update(cx, |this, cx| this.record_keystroke(&event.keystroke, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        });
        self.recording = Some(Recording {
            kind,
            copy: "Press a shortcut · Esc to cancel".into(),
            _interceptor: interceptor,
        });
        cx.notify();
    }

    /// Handles one keystroke while recording, returning whether it was consumed.
    fn record_keystroke(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(kind) = self.recording.as_ref().map(|recording| recording.kind) else {
            return false;
        };

        match shortcuts::record(keystroke) {
            Recorded::Ignored => return false,
            Recorded::Cancel => self.end_recording(cx),
            Recorded::NeedsModifier => {
                self.shortcuts.set_error(
                    kind,
                    Some("Use a modifier combination or a function key.".into()),
                );
            }
            Recorded::Accelerator(accelerator) => match self.shortcuts.bind(kind, &accelerator) {
                Ok(()) => {
                    match kind {
                        ShortcutKind::Run => self.settings.hotkey = accelerator,
                        ShortcutKind::Capture => self.settings.capture_hotkey = accelerator,
                    }
                    persist(&self.settings);
                    self.end_recording(cx);
                }
                Err(error) => {
                    self.shortcuts.set_error(kind, Some(error));
                    if let Some(recording) = self.recording.as_mut() {
                        recording.copy = "Press another shortcut · Esc to cancel".into();
                    }
                }
            },
        }
        cx.notify();
        true
    }

    /// Stops recording. Errors on screen came from this recording, since starting one
    /// clears the binding's error, so they go with it.
    fn end_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(recording) = self.recording.take() {
            self.shortcuts.set_error(recording.kind, None);
            cx.notify();
        }
    }

    fn display_phase(&self) -> DisplayPhase {
        match (&self.countdown, self.runtime.phase) {
            (Some(_), _) => DisplayPhase::Starting,
            (None, RunPhase::Running) => DisplayPhase::Running,
            (None, RunPhase::Error) => DisplayPhase::Error,
            (None, RunPhase::Idle) => DisplayPhase::Idle,
        }
    }
}

impl Drop for ClickityApp {
    fn drop(&mut self) {
        self.engine.stop();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayPhase {
    Idle,
    Starting,
    Running,
    Error,
}

fn persist(settings: &Settings) {
    if let Err(error) = settings.save() {
        eprintln!("{error}");
    }
}

fn interval_field(
    value: i64,
    max: f64,
    window: &mut Window,
    cx: &mut Context<ClickityApp>,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(value.to_string())
            .min(0.)
            .max(max)
            .step(1.)
            .validate(|text, _| text.chars().all(|c| c.is_ascii_digit()))
    })
}

fn text_field(
    value: i64,
    signed: bool,
    window: &mut Window,
    cx: &mut Context<ClickityApp>,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(value.to_string())
            .validate(move |text, _| {
                let digits = if signed {
                    text.strip_prefix('-').unwrap_or(text)
                } else {
                    text
                };
                digits.chars().all(|c| c.is_ascii_digit())
            })
    })
}

impl Render for ClickityApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.draft(cx);
        let errors = draft.validate();

        div()
            .size_full()
            .bg(rgb(CARBON))
            .font_family(UI_FONT)
            .text_size(px(16.))
            .line_height(relative(1.35))
            .text_color(rgb(WARM_WHITE))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .child(self.render_titlebar(cx))
                    .child(self.render_shell(&draft, errors, window, cx))
                    .child(fit_window_to_content(self.measured_height.clone())),
            )
    }
}

impl ClickityApp {
    fn render_titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let macos = cfg!(target_os = "macos");
        let windows = cfg!(target_os = "windows");

        let control =
            |id: &'static str, path: &'static str, area: WindowControlArea, close: bool| {
                window_control(id, path, 12., close)
                    .when(windows, |this| this.window_control_area(area))
                    .when(!windows, |this| {
                        this.on_click(move |_, window, _| match area {
                            WindowControlArea::Min => window.minimize_window(),
                            WindowControlArea::Max => window.zoom_window(),
                            _ => window.remove_window(),
                        })
                    })
            };

        div()
            .id("titlebar")
            .flex()
            .flex_none()
            .items_center()
            .h(px(TITLEBAR_HEIGHT))
            .border_b_1()
            .border_color(rgb(BORDER_SOFT))
            .bg(rgb(CARBON))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.titlebar_drag = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.titlebar_drag = false),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.titlebar_drag = false))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, _| {
                if std::mem::take(&mut this.titlebar_drag)
                    && event.pressed_button == Some(MouseButton::Left)
                {
                    window.start_window_move();
                }
            }))
            .on_click(move |event, window, _| {
                if event.click_count() == 2 {
                    if macos {
                        window.titlebar_double_click();
                    } else {
                        window.zoom_window();
                    }
                }
            })
            .when(cfg!(target_os = "linux"), |this| {
                this.on_mouse_down(MouseButton::Right, |event, window, _| {
                    window.show_window_menu(event.position)
                })
            })
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .pl(px(if macos { 78. } else { 14. }))
                    .when(windows, |this| {
                        this.window_control_area(WindowControlArea::Drag)
                    })
                    .child(img("clickity/clickity-mark.svg").flex_none().size(px(20.)))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(weight(780.))
                            .child("Clickity"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .h_full()
                    .child(
                        window_control("open-settings", "clickity/gear.svg", 15., false)
                            .accessibility_label("Settings")
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                            ),
                    )
                    .when(!macos, |this| {
                        this.child(
                            control(
                                "minimize",
                                "clickity/minimize.svg",
                                WindowControlArea::Min,
                                false,
                            )
                            .accessibility_label("Minimize"),
                        )
                        .child(
                            control(
                                "maximize",
                                "clickity/maximize.svg",
                                WindowControlArea::Max,
                                false,
                            )
                            .accessibility_label("Maximize"),
                        )
                        .child(
                            control(
                                "close",
                                "clickity/close.svg",
                                WindowControlArea::Close,
                                true,
                            )
                            .accessibility_label("Close"),
                        )
                    }),
            )
    }

    fn render_shell(
        &self,
        draft: &Draft,
        errors: ValidationErrors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let narrow = window.viewport_size().width < px(540.);
        div()
            .flex()
            .justify_center()
            .p(px(if narrow { 14. } else { 18. }))
            .child(
                div()
                    .w_full()
                    .max_w(px(644.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(self.render_interval(draft, errors, window, cx))
                    .child(self.render_mouse_row(cx))
                    .child(self.render_repeat_row(errors, window, cx))
                    .child(self.render_position_row(errors, window, cx))
                    .child(self.render_dock(&errors, narrow, cx)),
            )
    }

    fn render_interval(
        &self,
        draft: &Draft,
        errors: ValidationErrors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locked = self.controls_locked();
        // The CSS clamp(34px, 7vw, 47px) this replaces.
        let numeral_size = (f32::from(window.viewport_size().width) * 0.07).clamp(34., 47.);
        let fields = [
            ("Hours", &self.fields.hours),
            ("Minutes", &self.fields.minutes),
            ("Seconds", &self.fields.seconds),
            ("Milliseconds", &self.fields.milliseconds),
        ];
        let summary = match (errors.interval, draft.interval_ms()) {
            (None, Some(interval)) => format!("Every {}", format_duration(interval)),
            _ => "Check interval".into(),
        };

        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("interval")
                    .role(Role::Group)
                    .aria_label("Click interval")
                    .overflow_hidden()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(GRAPHITE))
                    .shadow(vec![lift(14., 32., 0.16)])
                    .child(div().flex().children(fields.into_iter().enumerate().map(
                        |(index, (label, state))| {
                            let focused = state.focus_handle(cx).is_focused(window);
                            let last = index == 3;
                            div()
                                .relative()
                                .flex_1()
                                .min_w_0()
                                .pt(px(14.))
                                .px(px(11.))
                                .pb(px(12.))
                                .when(!last, |this| this.border_r_1().border_color(rgb(BORDER)))
                                .when(index == 0, |this| this.rounded_tl(px(11.)))
                                .when(last, |this| this.rounded_tr(px(11.)))
                                .when(focused, |this| this.bg(alpha(SIGNAL, 0.05)))
                                .child(
                                    div()
                                        .mb(px(7.))
                                        .text_size(px(10.))
                                        .font_weight(weight(650.))
                                        .text_color(rgb(MUTED_STEEL))
                                        .truncate()
                                        .child(label),
                                )
                                .child(
                                    BaseNumberInput::new(state)
                                        .controls_right()
                                        .disabled(locked)
                                        .w_full()
                                        .h(px(55.))
                                        .when(locked, |this| this.opacity(0.45))
                                        .input(
                                            Input::new(state)
                                                .appearance(false)
                                                .disabled(locked)
                                                .aria_label(label)
                                                .h_full()
                                                .px(px(4.))
                                                .text_center()
                                                .font_family(NUMERAL_FONT)
                                                .font_weight(weight(650.))
                                                .text_size(px(numeral_size))
                                                .line_height(px(numeral_size * 1.15)),
                                        )
                                        .increment_button(move |button| {
                                            stepper(button, true, label)
                                        })
                                        .decrement_button(move |button| {
                                            stepper(button, false, label)
                                        }),
                                )
                                .when(focused, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .right_0()
                                            .bottom_0()
                                            .h(px(2.))
                                            .bg(rgb(SIGNAL)),
                                    )
                                })
                        },
                    )))
                    .child(
                        div()
                            .pt(px(9.))
                            .pb(px(10.))
                            .px(px(14.))
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .text_center()
                            .text_size(px(13.))
                            .font_weight(weight(620.))
                            .text_color(rgb(BRIGHT_STEEL))
                            .child(StyledText::new(summary).with_highlights([(
                                0..1,
                                HighlightStyle {
                                    color: Some(rgb(WARM_WHITE).into()),
                                    ..Default::default()
                                },
                            )])),
                    ),
            )
            .children(field_message(
                self.show_validation.then_some(errors.interval).flatten(),
                DANGER,
            ))
    }

    fn render_mouse_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let segment = |button, label, fill| {
            Segment::new(button, label).icon(SegmentIcon::TwoTone("clickity/mouse.svg", fill))
        };
        segmented(
            "mouse-button",
            "Mouse button",
            vec![
                segment(ClickButton::Left, "Left", "clickity/mouse-left-fill.svg"),
                segment(
                    ClickButton::Middle,
                    "Middle",
                    "clickity/mouse-middle-fill.svg",
                ),
                segment(ClickButton::Right, "Right", "clickity/mouse-right-fill.svg"),
            ],
            self.button,
            self.controls_locked(),
            self.select(cx, |this, button| this.button = button),
        )
    }

    fn render_repeat_row(
        &self,
        errors: ValidationErrors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locked = self.controls_locked();
        let counting = self.repeat_mode == RepeatKind::Count;
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        segmented(
                            "repeat-mode",
                            "Repeat",
                            vec![
                                Segment::new(RepeatKind::UntilStopped, "Until stopped")
                                    .icon(SegmentIcon::Plain("clickity/infinity.svg")),
                                Segment::new(RepeatKind::Count, "Count"),
                            ],
                            self.repeat_mode,
                            locked,
                            self.select(cx, |this, mode| this.repeat_mode = mode),
                        )
                        .flex_1(),
                    )
                    .child(
                        compact_field(&self.fields.repeat_count, locked || !counting, window, cx)
                            .w(px(98.))
                            .h(px(45.))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(5.))
                                    .left(px(11.))
                                    .text_size(px(8.))
                                    .line_height(relative(1.))
                                    .font_weight(weight(700.))
                                    .text_color(rgb(MUTED_STEEL))
                                    .child("CLICKS"),
                            )
                            // Input centres its text in its own box, so the offset below the
                            // label has to come from a wrapper.
                            .child(
                                div().size_full().pt(px(12.)).pb(px(2.)).px(px(10.)).child(
                                    Input::new(&self.fields.repeat_count)
                                        .appearance(false)
                                        .disabled(locked || !counting)
                                        .aria_label("Clicks")
                                        .h_full()
                                        .px_0()
                                        .text_center()
                                        .font_family(NUMERAL_FONT)
                                        .font_weight(weight(620.))
                                        .text_size(px(18.)),
                                ),
                            ),
                    ),
            )
            .children(field_message(
                self.show_validation.then_some(errors.repeat).flatten(),
                DANGER,
            ))
    }

    fn render_position_row(
        &self,
        errors: ValidationErrors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locked = self.controls_locked();
        let fixed = self.position_mode == PositionKind::Fixed;
        let capture_label = self.shortcuts.label(ShortcutKind::Capture);
        let help_prefix = "Move the pointer anywhere, then press ";
        let help = format!("{help_prefix}{capture_label} to capture it.");
        let key_range = help_prefix.len()..help_prefix.len() + capture_label.len();
        let error = self.capture_error.clone().or_else(|| {
            self.show_validation
                .then_some(errors.position)
                .flatten()
                .map(Into::into)
        });

        div()
            .flex()
            .flex_col()
            .child(segmented(
                "position-mode",
                "Cursor position",
                vec![
                    Segment::new(PositionKind::Current, "Follow cursor"),
                    Segment::new(PositionKind::Fixed, "Fixed position"),
                ],
                self.position_mode,
                locked,
                self.select(cx, |this, mode| this.position_mode = mode),
            ))
            .when(fixed, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .pt(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(9.))
                                .line_height(relative(1.45))
                                .text_color(rgb(MUTED_STEEL))
                                .child(StyledText::new(help).with_highlights([(
                                    key_range,
                                    HighlightStyle {
                                        color: Some(rgb(BRIGHT_STEEL).into()),
                                        font_weight: Some(weight(700.)),
                                        ..Default::default()
                                    },
                                )])),
                        )
                        .child(coordinate_field("X", &self.fields.x, locked, window, cx))
                        .child(coordinate_field("Y", &self.fields.y, locked, window, cx)),
                )
            })
            .children(field_message(error, DANGER))
    }

    fn render_dock(
        &self,
        errors: &ValidationErrors,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let phase = self.display_phase();
        let (title, detail): (SharedString, Option<SharedString>) = match phase {
            DisplayPhase::Starting => (
                format!(
                    "Starting in {}…",
                    self.countdown.as_ref().map_or(0, |c| c.remaining)
                )
                .into(),
                None,
            ),
            DisplayPhase::Running => {
                let clicks = self.runtime.clicks_completed;
                let title = match self.runtime.target_clicks {
                    Some(target) => format!("{clicks} of {target} clicks"),
                    None => format!("{clicks} click{}", if clicks == 1 { "" } else { "s" }),
                };
                (title.into(), None)
            }
            DisplayPhase::Error => (
                "Clickity stopped".into(),
                Some(
                    self.runtime
                        .message
                        .clone()
                        .unwrap_or_else(|| "Check the settings and try again.".into())
                        .into(),
                ),
            ),
            DisplayPhase::Idle => (
                self.runtime
                    .message
                    .clone()
                    .unwrap_or_else(|| "Ready when you are".into())
                    .into(),
                None,
            ),
        };
        let (action, action_icon) = match phase {
            DisplayPhase::Starting => ("Cancel start", "clickity/play.svg"),
            DisplayPhase::Running => ("Stop clicking", "clickity/stop.svg"),
            DisplayPhase::Error => ("Try again", "clickity/play.svg"),
            DisplayPhase::Idle => ("Start clicking", "clickity/play.svg"),
        };
        let action_disabled =
            !matches!(phase, DisplayPhase::Running | DisplayPhase::Starting) && !errors.is_empty();
        let pulsing =
            matches!(phase, DisplayPhase::Running | DisplayPhase::Starting) && !cx.reduce_motion();

        div()
            .flex()
            .min_h(px(56.))
            .overflow_hidden()
            .rounded(px(12.))
            .bg(rgb(SIGNAL))
            .shadow(vec![lift(14., 36., 0.22)])
            .text_color(rgb(SIGNAL_INK))
            .child(
                div()
                    .id("run-state")
                    .role(Role::Status)
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(if narrow { 8. } else { 10. }))
                    .py(px(14.))
                    .px(px(if narrow { 10. } else { 16. }))
                    .child(status_dot(pulsing))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .font_weight(weight(800.))
                                    .child(title),
                            )
                            .children(detail.map(|detail| {
                                div()
                                    .mt(px(4.))
                                    .truncate()
                                    .text_size(px(8.))
                                    .font_weight(weight(650.))
                                    .opacity(0.72)
                                    .child(detail)
                            })),
                    ),
            )
            .child(
                Button::new("action")
                    .disabled(action_disabled)
                    .flex_1()
                    .min_w_0()
                    .gap(px(if narrow { 8. } else { 12. }))
                    .px(px(if narrow { 10. } else { 16. }))
                    .border_l_1()
                    .border_color(alpha(SIGNAL_INK, 0.28))
                    .when(action_disabled, |this| this.opacity(0.44))
                    .when(!action_disabled, |this| {
                        this.cursor_pointer()
                            .hover(|style| style.bg(alpha(0xffffff, 0.12)))
                    })
                    .focus_visible(|style| style.border_2().border_color(rgb(SIGNAL_INK)))
                    .child(icon(
                        action_icon,
                        if narrow { 22. } else { 28. },
                        rgb(SIGNAL_INK),
                    ))
                    .child(
                        div()
                            .truncate()
                            // Button's line height of 1 would clip descenders in a truncated label.
                            .line_height(relative(1.3))
                            .text_size(px(if narrow { 18. } else { 20. }))
                            .font_weight(weight(800.))
                            .child(action),
                    )
                    .child(
                        kbd(self.shortcuts.label(ShortcutKind::Run))
                            .flex_none()
                            .py(px(3.))
                            .px(px(6.))
                            .rounded(px(5.))
                            .border_1()
                            .border_color(alpha(SIGNAL_INK, 0.28))
                            .text_size(px(10.))
                            .line_height(relative(1.2)),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.handle_action(cx))),
            )
    }

    fn settings_dialog(
        &self,
        dialog: Dialog,
        this: &Entity<Self>,
        window: &mut Window,
        _: &App,
    ) -> Dialog {
        let view = this.clone();
        let viewport = window.viewport_size();

        dialog
            .w(px(430.))
            .margin_top(((viewport.height - px(SETTINGS_DIALOG_HEIGHT)) / 2.).max(px(16.)))
            .close_button(false)
            .bg(rgb(GRAPHITE))
            .border_color(rgb(BORDER))
            .rounded(px(14.))
            .pt(px(16.))
            .px(px(18.))
            .pb(px(18.))
            .on_close(move |_, _, cx| view.update(cx, |this, cx| this.end_recording(cx)))
            .child(
                div()
                    .track_focus(&self.recorder_focus)
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .mb(px(14.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(weight(780.))
                                    .child("Settings"),
                            )
                            .child(
                                Button::new("close-settings")
                                    .accessibility_label("Close settings")
                                    .size(px(26.))
                                    .rounded(px(7.))
                                    .text_color(rgb(MUTED_STEEL))
                                    .cursor_pointer()
                                    .group("close-settings")
                                    .hover(|style| style.bg(rgb(GRAPHITE_RAISED)))
                                    .child(
                                        icon("clickity/close.svg", 12., rgb(MUTED_STEEL))
                                            .group_hover("close-settings", |style| {
                                                style.text_color(rgb(WARM_WHITE))
                                            }),
                                    )
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            ),
                    )
                    .child(self.shortcut_row(
                        ShortcutKind::Run,
                        "Works while Clickity is in the background.",
                        this,
                    ))
                    .child(div().h(px(10.)))
                    .child(self.shortcut_row(
                        ShortcutKind::Capture,
                        "Saves the pointer coordinates from anywhere.",
                        this,
                    )),
            )
    }

    fn shortcut_row(
        &self,
        kind: ShortcutKind,
        description: &'static str,
        this: &Entity<Self>,
    ) -> impl IntoElement {
        let recording = self
            .recording
            .as_ref()
            .filter(|recording| recording.kind == kind);
        let locked = self.controls_locked() && recording.is_none();
        let view = this.clone();

        div()
            .flex()
            .flex_col()
            .min_h(px(56.))
            .py(px(9.))
            .px(px(11.))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(weight(740.))
                                    .line_height(relative(1.2))
                                    .child(kind.name()),
                            )
                            .child(
                                div()
                                    .mt(px(3.))
                                    .text_size(px(9.))
                                    .text_color(rgb(MUTED_STEEL))
                                    .child(description),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                kbd(if recording.is_some() {
                                    "…".to_string()
                                } else {
                                    self.shortcuts.label(kind)
                                })
                                .min_w(px(45.))
                                .text_center()
                                .text_size(px(18.)),
                            )
                            .child(
                                secondary_button(
                                    ("change-shortcut", kind as usize),
                                    if recording.is_some() {
                                        "Cancel"
                                    } else {
                                        "Change"
                                    },
                                )
                                .disabled(locked)
                                .when(locked, |this| this.opacity(0.45))
                                .on_click(move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.toggle_recording(kind, window, cx)
                                    })
                                }),
                            ),
                    ),
            )
            .children(field_message(
                recording.map(|recording| recording.copy.clone()),
                MUTED_STEEL,
            ))
            .children(field_message(
                self.shortcuts.error(kind).map(str::to_owned),
                DANGER,
            ))
    }
}

fn window_control(id: &'static str, path: &'static str, icon_size: f32, close: bool) -> Button {
    let (hover_bg, hover_fg) = if close {
        (DANGER, SIGNAL_INK)
    } else {
        (GRAPHITE_RAISED, WARM_WHITE)
    };
    Button::new(id)
        .w(px(44.))
        .h_full()
        .group(id)
        .hover(move |style| style.bg(rgb(hover_bg)))
        .focus_visible(|style| style.border_2().border_color(rgb(SIGNAL)))
        // Keep presses on a control from arming the title bar's window drag.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            icon(path, icon_size, rgb(MUTED_STEEL))
                .group_hover(id, move |style| style.text_color(rgb(hover_fg))),
        )
}

fn stepper(button: Button, up: bool, label: &'static str) -> Button {
    let (group, path, verb) = if up {
        ("step-up", "clickity/chevron-up.svg", "Increase")
    } else {
        ("step-down", "clickity/chevron-down.svg", "Decrease")
    };
    button
        .accessibility_label(format!("{verb} {}", label.to_lowercase()))
        .w(px(30.))
        .bg(rgb(GRAPHITE_RAISED))
        .border_l_1()
        .border_r_1()
        .border_b_1()
        .border_color(rgb(BORDER_SOFT))
        .when(up, |this| {
            this.border_t_1().rounded_tl(px(7.)).rounded_tr(px(7.))
        })
        .when(!up, |this| this.rounded_bl(px(7.)).rounded_br(px(7.)))
        .cursor_pointer()
        .group(group)
        .hover(|style| style.bg(rgb(GRAPHITE_ACTIVE)))
        .child(
            icon(path, 13., rgb(WARM_WHITE))
                .group_hover(group, |style| style.text_color(rgb(SIGNAL))),
        )
}

/// The raised frame shared by the clicks and coordinate inputs.
fn compact_field(state: &Entity<InputState>, disabled: bool, window: &Window, cx: &App) -> Div {
    let focused = state.focus_handle(cx).is_focused(window);
    div()
        .relative()
        .flex_none()
        .rounded(px(9.))
        .border_1()
        .border_color(rgb(if focused { SIGNAL } else { BORDER_SOFT }))
        .bg(rgb(GRAPHITE_RAISED))
        .when(focused, |this| {
            this.shadow(vec![ring_shadow(SIGNAL, 0.22, 2.)])
        })
        .when(disabled, |this| this.opacity(0.45))
}

fn coordinate_field(
    axis: &'static str,
    state: &Entity<InputState>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    compact_field(state, disabled, window, cx)
        .w(px(74.))
        .h(px(34.))
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(9.))
                .flex()
                .items_center()
                .text_size(px(9.))
                .font_weight(weight(760.))
                .text_color(rgb(MUTED_STEEL))
                .child(axis),
        )
        .child(
            div().size_full().pl(px(22.)).pr(px(7.)).child(
                Input::new(state)
                    .appearance(false)
                    .disabled(disabled)
                    .aria_label(axis)
                    .h_full()
                    .px_0()
                    .font_family(NUMERAL_FONT)
                    .font_weight(weight(620.))
                    .text_size(px(14.)),
            ),
        )
}

/// A 16px dot that pulses while Clickity is starting or running.
fn status_dot(pulsing: bool) -> impl IntoElement {
    let dot = div().rounded_full().bg(rgb(SIGNAL_INK));
    div()
        .flex_none()
        .size(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .map(|this| {
            if pulsing {
                this.child(dot.with_animation(
                    "status-pulse",
                    Animation::new(Duration::from_millis(1400)).repeat(),
                    |dot, delta| {
                        // Eases from full size down to 72% and back, like the CSS keyframes.
                        let scale = 1. - 0.14 * (1. - (delta * std::f32::consts::TAU).cos());
                        dot.size(px(16. * scale)).shadow(vec![ring_shadow(
                            SIGNAL_INK,
                            0.13,
                            5. * scale,
                        )])
                    },
                ))
            } else {
                this.child(dot.size(px(16.)))
            }
        })
}

/// Shrinks or grows the window to its content whenever the content's height changes, so no
/// dead space sits below the run dock. A manual resize sticks until the content changes.
fn fit_window_to_content(measured: Rc<Cell<Option<Pixels>>>) -> impl IntoElement {
    canvas(
        move |bounds, window, cx| {
            let content = bounds.size.height.ceil();
            if measured.replace(Some(content)) == Some(content) {
                return;
            }
            let height = content + frame_height(window);
            let viewport = window.viewport_size();
            if (viewport.height - height).abs() >= px(2.) {
                window.defer(cx, move |window, _| {
                    window.resize(size(viewport.width, height))
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The vertical space GPUI Kit's Linux frame takes around the content.
fn frame_height(window: &Window) -> Pixels {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return px(0.);
    };
    let paddings = component::window_paddings(window);
    let border = |tiled: bool| if tiled { px(0.) } else { FRAME_BORDER };
    paddings.top + paddings.bottom + border(tiling.top) + border(tiling.bottom)
}

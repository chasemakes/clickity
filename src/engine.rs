use crate::model::{ClickButton, ClickConfig, PositionMode, RunPhase, RuntimeSnapshot};
use enigo::{Button, Coordinate, Direction::Click, Enigo, Mouse, Settings};
use std::{
    sync::{
        mpsc::{self, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

/// Receives every runtime snapshot the engine publishes, from whichever thread produced it.
type SnapshotListener = Arc<dyn Fn(RuntimeSnapshot) + Send + Sync>;

pub trait MouseDriver {
    fn move_to(&mut self, x: i32, y: i32) -> Result<(), String>;
    fn click(&mut self, button: ClickButton) -> Result<(), String>;
    fn location(&self) -> Result<(i32, i32), String>;
}

pub struct SystemMouse {
    enigo: Enigo,
}

impl SystemMouse {
    pub fn new() -> Result<Self, String> {
        let settings = Settings {
            linux_delay: 0,
            ..Settings::default()
        };
        Enigo::new(&settings)
            .map(|enigo| Self { enigo })
            .map_err(|error| format!("Could not connect to the system input service: {error}"))
    }
}

impl MouseDriver for SystemMouse {
    fn move_to(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|error| format!("Could not move the pointer: {error}"))
    }

    fn click(&mut self, button: ClickButton) -> Result<(), String> {
        let button = match button {
            ClickButton::Left => Button::Left,
            ClickButton::Middle => Button::Middle,
            ClickButton::Right => Button::Right,
        };

        self.enigo
            .button(button, Click)
            .map_err(|error| format!("Could not send a mouse click: {error}"))
    }

    fn location(&self) -> Result<(i32, i32), String> {
        self.enigo
            .location()
            .map_err(|error| format!("Could not read the pointer position: {error}"))
    }
}

fn perform_click(mouse: &mut impl MouseDriver, config: &ClickConfig) -> Result<(), String> {
    if let PositionMode::Fixed { x, y } = config.position {
        mouse.move_to(x, y)?;
    }
    mouse.click(config.button)
}

#[derive(Debug)]
struct EngineInner {
    config: ClickConfig,
    snapshot: RuntimeSnapshot,
    run_id: u64,
    cancel: Option<Sender<()>>,
}

#[derive(Clone)]
pub struct ClickEngine {
    inner: Arc<Mutex<EngineInner>>,
    on_change: SnapshotListener,
}

impl ClickEngine {
    pub fn new(on_change: impl Fn(RuntimeSnapshot) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(Mutex::new(EngineInner {
                snapshot: RuntimeSnapshot::default(),
                config: ClickConfig::default(),
                run_id: 0,
                cancel: None,
            })),
            on_change: Arc::new(on_change),
        }
    }

    pub fn update_config(&self, config: ClickConfig) -> Result<(), String> {
        config.validate()?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Click engine state is unavailable")?;
        if inner.snapshot.phase == RunPhase::Running {
            return Err("Stop Clickity before changing its configuration.".into());
        }
        inner.snapshot.target_clicks = config.target_clicks();
        inner.snapshot.interval_ms = config.interval_ms;
        inner.config = config;
        Ok(())
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.inner
            .lock()
            .map(|inner| inner.snapshot.clone())
            .unwrap_or_default()
    }

    pub fn is_running(&self) -> bool {
        self.snapshot().phase == RunPhase::Running
    }

    pub fn start(&self) -> Result<RuntimeSnapshot, String> {
        let (config, run_id, cancel_rx, snapshot) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| "Click engine state is unavailable")?;

            if inner.snapshot.phase == RunPhase::Running {
                return Ok(inner.snapshot.clone());
            }

            inner.config.validate()?;
            inner.run_id = inner.run_id.wrapping_add(1);
            let run_id = inner.run_id;
            let (cancel_tx, cancel_rx) = mpsc::channel();
            inner.cancel = Some(cancel_tx);
            inner.snapshot = RuntimeSnapshot {
                phase: RunPhase::Running,
                clicks_completed: 0,
                target_clicks: inner.config.target_clicks(),
                interval_ms: inner.config.interval_ms,
                message: Some("Clicking started".into()),
            };

            (
                inner.config.clone(),
                run_id,
                cancel_rx,
                inner.snapshot.clone(),
            )
        };

        self.emit(&snapshot);

        let engine = self.clone();
        thread::Builder::new()
            .name("clickity-click-loop".into())
            .spawn(move || {
                let mut mouse = match SystemMouse::new() {
                    Ok(mouse) => mouse,
                    Err(error) => {
                        engine.fail_run(run_id, error);
                        return;
                    }
                };

                let interval = Duration::from_millis(config.interval_ms);
                loop {
                    match cancel_rx.recv_timeout(interval) {
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => {}
                    }

                    if !engine.is_active(run_id) {
                        return;
                    }

                    if let Err(error) = perform_click(&mut mouse, &config) {
                        engine.fail_run(run_id, error);
                        return;
                    }

                    if engine.record_click(run_id) {
                        return;
                    }
                }
            })
            .map_err(|error| {
                self.fail_run(run_id, format!("Could not start the click loop: {error}"));
                format!("Could not start the click loop: {error}")
            })?;

        Ok(snapshot)
    }

    pub fn stop(&self) -> RuntimeSnapshot {
        let (cancel, snapshot) = {
            let mut inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return RuntimeSnapshot::default(),
            };

            if inner.snapshot.phase != RunPhase::Running {
                return inner.snapshot.clone();
            }

            let clicks = inner.snapshot.clicks_completed;
            inner.run_id = inner.run_id.wrapping_add(1);
            let cancel = inner.cancel.take();
            inner.snapshot.phase = RunPhase::Idle;
            inner.snapshot.message = Some(if clicks == 0 {
                "Clicking stopped".into()
            } else {
                format!(
                    "Stopped after {clicks} click{}",
                    if clicks == 1 { "" } else { "s" }
                )
            });
            (cancel, inner.snapshot.clone())
        };

        if let Some(cancel) = cancel {
            let _ = cancel.send(());
        }
        self.emit(&snapshot);
        snapshot
    }

    pub fn toggle(&self) {
        if self.is_running() {
            self.stop();
        } else if let Err(error) = self.start() {
            self.fail_current(error);
        }
    }

    fn is_active(&self, run_id: u64) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.run_id == run_id && inner.snapshot.phase == RunPhase::Running)
            .unwrap_or(false)
    }

    /// Returns true when the configured click count has completed.
    fn record_click(&self, run_id: u64) -> bool {
        let (snapshot, completed) = {
            let mut inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return true,
            };

            if inner.run_id != run_id || inner.snapshot.phase != RunPhase::Running {
                return true;
            }

            inner.snapshot.clicks_completed += 1;
            let completed = inner
                .snapshot
                .target_clicks
                .is_some_and(|target| inner.snapshot.clicks_completed >= target);

            if completed {
                let clicks = inner.snapshot.clicks_completed;
                inner.snapshot.phase = RunPhase::Idle;
                inner.snapshot.message = Some(format!(
                    "Completed {clicks} click{}",
                    if clicks == 1 { "" } else { "s" }
                ));
                inner.cancel = None;
            } else {
                inner.snapshot.message = None;
            }

            (inner.snapshot.clone(), completed)
        };

        self.emit(&snapshot);
        completed
    }

    fn fail_run(&self, run_id: u64, error: String) {
        let snapshot = {
            let mut inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return,
            };
            if inner.run_id != run_id {
                return;
            }
            inner.snapshot.phase = RunPhase::Error;
            inner.snapshot.message = Some(error);
            inner.cancel = None;
            inner.snapshot.clone()
        };
        self.emit(&snapshot);
    }

    fn fail_current(&self, error: String) {
        let snapshot = {
            let mut inner = match self.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return,
            };
            inner.snapshot.phase = RunPhase::Error;
            inner.snapshot.message = Some(error);
            inner.cancel = None;
            inner.snapshot.clone()
        };
        self.emit(&snapshot);
    }

    fn emit(&self, snapshot: &RuntimeSnapshot) {
        (self.on_change)(snapshot.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeMouse {
        moves: Vec<(i32, i32)>,
        clicks: Vec<ClickButton>,
    }

    impl MouseDriver for FakeMouse {
        fn move_to(&mut self, x: i32, y: i32) -> Result<(), String> {
            self.moves.push((x, y));
            Ok(())
        }

        fn click(&mut self, button: ClickButton) -> Result<(), String> {
            self.clicks.push(button);
            Ok(())
        }

        fn location(&self) -> Result<(i32, i32), String> {
            Ok((40, 80))
        }
    }

    #[test]
    fn current_position_click_does_not_move_pointer() {
        let mut mouse = FakeMouse::default();
        let config = ClickConfig::default();
        perform_click(&mut mouse, &config).unwrap();
        assert!(mouse.moves.is_empty());
        assert_eq!(mouse.clicks, vec![ClickButton::Left]);
    }

    #[test]
    fn fixed_position_moves_before_clicking() {
        let mut mouse = FakeMouse::default();
        let config = ClickConfig {
            button: ClickButton::Right,
            position: PositionMode::Fixed { x: -120, y: 450 },
            ..ClickConfig::default()
        };
        perform_click(&mut mouse, &config).unwrap();
        assert_eq!(mouse.moves, vec![(-120, 450)]);
        assert_eq!(mouse.clicks, vec![ClickButton::Right]);
    }
}

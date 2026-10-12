//! The two system-wide shortcuts, and the helpers that turn a recorded keystroke into an
//! accelerator and an accelerator into a keycap label.

use crate::settings::{DEFAULT_CAPTURE_SHORTCUT, DEFAULT_RUN_SHORTCUT};
use futures::channel::mpsc::{self, UnboundedReceiver};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager};
use gpui_kit::Keystroke;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKind {
    Run,
    Capture,
}

impl ShortcutKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Run => "Start / stop shortcut",
            Self::Capture => "Capture position shortcut",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Run => Self::Capture,
            Self::Capture => Self::Run,
        }
    }

    fn default_accelerator(self) -> &'static str {
        match self {
            Self::Run => DEFAULT_RUN_SHORTCUT,
            Self::Capture => DEFAULT_CAPTURE_SHORTCUT,
        }
    }
}

struct Binding {
    accelerator: String,
    registered: Option<HotKey>,
    error: Option<String>,
}

impl Binding {
    fn new(accelerator: &str) -> Self {
        Self {
            accelerator: accelerator.into(),
            registered: None,
            error: None,
        }
    }
}

pub struct Shortcuts {
    manager: Result<GlobalHotKeyManager, String>,
    run: Binding,
    capture: Binding,
}

impl Shortcuts {
    /// Connects to the platform's hotkey service and registers the saved accelerators,
    /// falling back to the defaults when a saved one is unavailable. Hotkey events arrive
    /// on the returned channel from whichever thread the platform reports them on.
    pub fn new(run: &str, capture: &str) -> (Self, UnboundedReceiver<GlobalHotKeyEvent>) {
        let (events, receiver) = mpsc::unbounded();
        GlobalHotKeyEvent::set_event_handler(Some(move |event| {
            let _ = events.unbounded_send(event);
        }));

        // Each binding starts from its saved accelerator rather than its default, so a
        // pair that uses the other's default (such as swapped F6 and F7) restores intact.
        let mut shortcuts = Self {
            manager: GlobalHotKeyManager::new()
                .map_err(|error| format!("Global shortcuts are unavailable: {error}")),
            run: Binding::new(run),
            capture: Binding::new(capture),
        };
        shortcuts.restore(ShortcutKind::Run, run);
        shortcuts.restore(ShortcutKind::Capture, capture);
        (shortcuts, receiver)
    }

    fn restore(&mut self, kind: ShortcutKind, saved: &str) {
        let Err(error) = self.bind(kind, saved) else {
            return;
        };

        let fallback = kind.default_accelerator();
        let error = if saved != fallback && self.bind(kind, fallback).is_ok() {
            error
        } else {
            format!("{fallback} could not be registered. Choose another shortcut: {error}")
        };
        self.binding_mut(kind).error = Some(error);
    }

    /// Registers `accelerator` for one binding, releasing whatever it held before.
    pub fn bind(&mut self, kind: ShortcutKind, accelerator: &str) -> Result<(), String> {
        let accelerator = accelerator.trim();
        if accelerator.is_empty() {
            return Err("Choose a keyboard shortcut.".into());
        }

        let other = &self.binding(kind.other()).accelerator;
        if shortcuts_conflict(accelerator, other) {
            let label = display_label(other);
            return Err(match kind {
                ShortcutKind::Run => format!("{label} is already used for position capture."),
                ShortcutKind::Capture => {
                    format!("{label} is already used to start and stop clicking.")
                }
            });
        }

        let hotkey: HotKey = accelerator
            .parse()
            .map_err(|error| format!("That shortcut could not be registered: {error}"))?;
        let manager = self.manager.as_ref().map_err(Clone::clone)?;
        let binding = match kind {
            ShortcutKind::Run => &mut self.run,
            ShortcutKind::Capture => &mut self.capture,
        };

        if binding.registered != Some(hotkey) {
            manager
                .register(hotkey)
                .map_err(|error| format!("That shortcut could not be registered: {error}"))?;
            if let Some(previous) = binding.registered {
                if let Err(error) = manager.unregister(previous) {
                    let _ = manager.unregister(hotkey);
                    return Err(format!("Could not replace the existing shortcut: {error}"));
                }
            }
        }

        binding.accelerator = accelerator.into();
        binding.registered = Some(hotkey);
        binding.error = None;
        Ok(())
    }

    pub fn accelerator(&self, kind: ShortcutKind) -> &str {
        &self.binding(kind).accelerator
    }

    pub fn label(&self, kind: ShortcutKind) -> String {
        display_label(self.accelerator(kind))
    }

    pub fn error(&self, kind: ShortcutKind) -> Option<&str> {
        self.binding(kind).error.as_deref()
    }

    pub fn set_error(&mut self, kind: ShortcutKind, error: Option<String>) {
        self.binding_mut(kind).error = error;
    }

    /// Which binding a platform hotkey event belongs to.
    pub fn kind_for(&self, id: u32) -> Option<ShortcutKind> {
        [ShortcutKind::Run, ShortcutKind::Capture]
            .into_iter()
            .find(|kind| {
                self.binding(*kind)
                    .registered
                    .is_some_and(|hotkey| hotkey.id() == id)
            })
    }

    fn binding(&self, kind: ShortcutKind) -> &Binding {
        match kind {
            ShortcutKind::Run => &self.run,
            ShortcutKind::Capture => &self.capture,
        }
    }

    fn binding_mut(&mut self, kind: ShortcutKind) -> &mut Binding {
        match kind {
            ShortcutKind::Run => &mut self.run,
            ShortcutKind::Capture => &mut self.capture,
        }
    }
}

/// Two accelerators collide when they parse to the same key combination.
fn shortcuts_conflict(left: &str, right: &str) -> bool {
    match (left.parse::<HotKey>(), right.parse::<HotKey>()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left.eq_ignore_ascii_case(right),
    }
}

/// The keycap text for an accelerator, using the platform's modifier names.
pub fn display_label(accelerator: &str) -> String {
    let mac = cfg!(target_os = "macos");
    accelerator
        .split('+')
        .map(|part| match part {
            "CmdOrCtrl" if mac => "⌘",
            "CmdOrCtrl" => "Ctrl",
            "Command" => "⌘",
            "Control" if mac => "⌃",
            "Control" => "Ctrl",
            "Alt" if mac => "⌥",
            "Shift" if mac => "⇧",
            part => part,
        })
        .collect::<Vec<_>>()
        .join(if mac { "" } else { " + " })
}

/// What a keystroke means while a shortcut is being recorded.
#[derive(Debug, PartialEq, Eq)]
pub enum Recorded {
    Accelerator(String),
    /// Plain keys would fire while typing anywhere, so only function keys may stand alone.
    NeedsModifier,
    Cancel,
    Ignored,
}

pub fn record(keystroke: &Keystroke) -> Recorded {
    if keystroke.key == "escape" {
        return Recorded::Cancel;
    }
    let Some(token) = key_token(&keystroke.key) else {
        return Recorded::Ignored;
    };

    let modifiers = &keystroke.modifiers;
    let mut parts = Vec::new();
    if modifiers.platform || modifiers.control {
        parts.push("CmdOrCtrl".to_string());
    }
    if modifiers.alt {
        parts.push("Alt".into());
    }
    if modifiers.shift {
        parts.push("Shift".into());
    }

    let function_key =
        token.len() > 1 && token.starts_with('F') && token[1..].parse::<u8>().is_ok();
    if parts.is_empty() && !function_key {
        return Recorded::NeedsModifier;
    }
    parts.push(token);
    Recorded::Accelerator(parts.join("+"))
}

/// Maps GPUI's key names onto the accelerator tokens `global-hotkey` parses.
fn key_token(key: &str) -> Option<String> {
    if let Some(number) = key
        .strip_prefix('f')
        .and_then(|rest| rest.parse::<u8>().ok())
    {
        return (1..=24).contains(&number).then(|| format!("F{number}"));
    }

    let mut chars = key.chars();
    if let (Some(single), None) = (chars.next(), chars.next()) {
        if single.is_ascii_alphanumeric() {
            return Some(single.to_ascii_uppercase().to_string());
        }
    }

    let token = match key {
        "space" => "Space",
        "enter" => "Enter",
        "tab" => "Tab",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "insert" => "Insert",
        _ => return None,
    };
    Some(token.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keystroke(source: &str) -> Keystroke {
        Keystroke::parse(source).unwrap()
    }

    #[test]
    fn function_keys_stand_alone() {
        assert_eq!(record(&keystroke("f8")), Recorded::Accelerator("F8".into()));
    }

    #[test]
    fn other_keys_need_a_modifier() {
        assert_eq!(record(&keystroke("k")), Recorded::NeedsModifier);
        assert_eq!(
            record(&keystroke("ctrl-shift-k")),
            Recorded::Accelerator("CmdOrCtrl+Shift+K".into())
        );
        assert_eq!(
            record(&keystroke("alt-space")),
            Recorded::Accelerator("Alt+Space".into())
        );
    }

    #[test]
    fn escape_cancels_and_unknown_keys_are_ignored() {
        assert_eq!(record(&keystroke("escape")), Recorded::Cancel);
        assert_eq!(record(&keystroke("ctrl-backspace")), Recorded::Ignored);
    }

    #[test]
    fn recorded_accelerators_parse_as_hotkeys() {
        for accelerator in [
            "F6",
            "CmdOrCtrl+Shift+K",
            "Alt+ArrowUp",
            "CmdOrCtrl+7",
            "Shift+PageDown",
        ] {
            assert!(accelerator.parse::<HotKey>().is_ok(), "{accelerator}");
        }
    }

    #[test]
    fn conflicts_compare_key_combinations() {
        assert!(shortcuts_conflict("f6", "F6"));
        assert!(shortcuts_conflict("Shift+CmdOrCtrl+K", "CmdOrCtrl+Shift+K"));
        assert!(!shortcuts_conflict("F6", "F7"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn labels_spell_out_modifiers() {
        assert_eq!(display_label("CmdOrCtrl+Shift+K"), "Ctrl + Shift + K");
        assert_eq!(display_label("F6"), "F6");
    }
}

//! The Timing Console palette from DESIGN.md, applied to GPUI Kit's theme so its dialog,
//! inputs, and focus rings match the hand-built controls.

use crate::assets::FONTS;
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::{px, rgb, rgba, App, FontWeight, Hsla};
use std::borrow::Cow;

pub const CARBON: u32 = 0x101214;
pub const GRAPHITE: u32 = 0x17191c;
pub const GRAPHITE_RAISED: u32 = 0x212429;
pub const GRAPHITE_ACTIVE: u32 = 0x2a2e34;
pub const BORDER: u32 = 0x3b3f45;
pub const BORDER_SOFT: u32 = 0x2d3035;
pub const BORDER_HOVER: u32 = 0x565b62;
/// The dark seam between segments of a segmented control.
pub const SEGMENT_DIVIDER: u32 = 0x0f1113;
pub const WARM_WHITE: u32 = 0xf5f5f1;
pub const MUTED_STEEL: u32 = 0x969aa1;
pub const BRIGHT_STEEL: u32 = 0xc4c7cd;
pub const SIGNAL: u32 = 0xb9e643;
pub const SIGNAL_INK: u32 = 0x11130e;
pub const DANGER: u32 = 0xff827d;

pub const UI_FONT: &str = "Manrope";
pub const NUMERAL_FONT: &str = "Roboto Condensed";

pub fn weight(value: f32) -> FontWeight {
    FontWeight(value)
}

/// `color` at `alpha` opacity.
pub fn alpha(color: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(color).into();
    color.a = alpha;
    color
}

pub fn install(cx: &mut App) {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
        .expect("bundled fonts are valid");

    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.font_family = UI_FONT.into();
        theme.font_size = px(13.);
        theme.radius = px(9.);
        theme.radius_lg = px(14.);
        theme.focus_ring = false;

        theme.background = rgb(CARBON).into();
        theme.foreground = rgb(WARM_WHITE).into();
        theme.border = rgb(BORDER).into();
        theme.input = rgb(BORDER_SOFT).into();
        theme.ring = rgb(SIGNAL).into();
        theme.caret = rgb(WARM_WHITE).into();
        theme.selection = alpha(SIGNAL, 0.35);
        theme.muted = rgb(GRAPHITE_RAISED).into();
        theme.muted_foreground = rgb(MUTED_STEEL).into();
        theme.popover = rgb(GRAPHITE).into();
        theme.popover_foreground = rgb(WARM_WHITE).into();
        theme.overlay = rgba(0x08090a9e).into();
        theme.primary = rgb(SIGNAL).into();
        theme.primary_foreground = rgb(SIGNAL_INK).into();
        theme.secondary = rgb(GRAPHITE_RAISED).into();
        theme.secondary_hover = rgb(GRAPHITE_ACTIVE).into();
        theme.secondary_active = rgb(GRAPHITE_ACTIVE).into();
        theme.secondary_foreground = rgb(BRIGHT_STEEL).into();
        theme.accent = rgb(GRAPHITE_ACTIVE).into();
        theme.accent_foreground = rgb(WARM_WHITE).into();
        theme.danger = rgb(DANGER).into();
        theme.danger_foreground = rgb(SIGNAL_INK).into();
        theme.title_bar = rgb(CARBON).into();
        theme.title_bar_border = rgb(BORDER_SOFT).into();
    });
}

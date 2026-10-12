//! Small building blocks shared by the main window and the settings dialog.

use crate::theme::*;
use gpui_kit::base::{Button, RoleOverride};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::rc::Rc;

pub fn icon(path: &'static str, size: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .path(path)
        .flex_none()
        .size(px(size))
        .text_color(color)
}

/// An outline icon with a translucent fill layer, for the two-tone mouse glyphs.
pub fn two_tone_icon(outline: &'static str, fill: &'static str, size: f32, color: Hsla) -> Div {
    let mut fill_color = color;
    fill_color.a *= 0.28;
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .child(icon(outline, size, color))
        .child(icon(fill, size, fill_color).absolute().top_0().left_0())
}

/// A keycap label set in the condensed numeral face.
pub fn kbd(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(NUMERAL_FONT)
        .font_weight(weight(650.))
        .child(text.into())
}

pub fn ring_shadow(color: u32, opacity: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        color: alpha(color, opacity),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(spread),
        inset: false,
    }
}

pub fn lift(offset: f32, blur: f32, opacity: f32) -> BoxShadow {
    BoxShadow {
        color: alpha(0x000000, opacity),
        offset: point(px(0.), px(offset)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }
}

/// Error or helper copy under a control; renders nothing while empty.
pub fn field_message(message: Option<impl Into<SharedString>>, color: u32) -> Option<Div> {
    message.map(|message| {
        div()
            .mt(px(5.))
            .text_size(px(9.))
            .line_height(relative(1.35))
            .text_color(rgb(color))
            .child(message.into())
    })
}

pub fn secondary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .min_h(px(34.))
        .px(px(12.))
        .rounded(px(9.))
        .border_1()
        .border_color(rgb(BORDER_SOFT))
        .bg(rgb(GRAPHITE_RAISED))
        .shadow(vec![lift(7., 16., 0.12)])
        .text_color(rgb(BRIGHT_STEEL))
        .text_size(px(10.))
        .font_weight(weight(700.))
        .cursor_pointer()
        .hover(|style| {
            style
                .border_color(rgb(BORDER_HOVER))
                .bg(rgb(GRAPHITE_ACTIVE))
                .text_color(rgb(WARM_WHITE))
        })
        .focus_visible(|style| style.border_color(rgb(SIGNAL)))
        .child(label.into())
}

pub enum SegmentIcon {
    Plain(&'static str),
    TwoTone(&'static str, &'static str),
}

pub struct Segment<T> {
    pub value: T,
    pub label: &'static str,
    pub icon: Option<SegmentIcon>,
}

impl<T> Segment<T> {
    pub fn new(value: T, label: &'static str) -> Self {
        Self {
            value,
            label,
            icon: None,
        }
    }

    pub fn icon(mut self, icon: SegmentIcon) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// One raised frame of adjoining radio options; the selected option fills with the signal.
pub fn segmented<T: Copy + PartialEq + 'static>(
    id: &'static str,
    label: &'static str,
    segments: Vec<Segment<T>>,
    selected: T,
    disabled: bool,
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let on_select = Rc::new(on_select);
    let count = segments.len();

    div()
        .id(id)
        .role(Role::RadioGroup)
        .aria_label(label)
        .flex()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(9.))
        .border_1()
        .border_color(rgb(BORDER_SOFT))
        .bg(rgb(GRAPHITE_RAISED))
        .shadow(vec![lift(7., 16., 0.12)])
        .children(segments.into_iter().enumerate().map(|(index, segment)| {
            let checked = segment.value == selected;
            let color: Hsla = if checked {
                rgb(SIGNAL_INK).into()
            } else {
                rgb(BRIGHT_STEEL).into()
            };
            let on_select = on_select.clone();
            let value = segment.value;

            Button::new((id, index))
                .role(RoleOverride::from(Role::RadioButton))
                .aria_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .disabled(disabled)
                .flex_1()
                .min_w_0()
                .min_h(px(43.))
                .gap(px(8.))
                .px(px(10.))
                .text_size(px(12.))
                .font_weight(weight(if checked { 740. } else { 620. }))
                .text_color(color)
                .whitespace_nowrap()
                .when(index + 1 < count, |this| {
                    this.border_r_1().border_color(rgb(SEGMENT_DIVIDER))
                })
                // GPUI clips to rectangles, so the end fills follow the frame's corners themselves.
                .when(index == 0, |this| {
                    this.rounded_tl(px(8.)).rounded_bl(px(8.))
                })
                .when(index + 1 == count, |this| {
                    this.rounded_tr(px(8.)).rounded_br(px(8.))
                })
                .when(checked, |this| this.bg(rgb(SIGNAL)))
                .when(disabled, |this| this.opacity(0.46))
                .when(!disabled, |this| this.cursor_pointer())
                .focus_visible(|style| style.border_2().border_color(rgb(WARM_WHITE)))
                .when_some(segment.icon, |this, segment_icon| {
                    this.child(match segment_icon {
                        SegmentIcon::Plain(path) => div().child(icon(path, 19., color)),
                        SegmentIcon::TwoTone(outline, fill) => {
                            two_tone_icon(outline, fill, 19., color)
                        }
                    })
                })
                .child(segment.label)
                .on_click(move |_, window, cx| {
                    if !checked {
                        on_select(value, window, cx);
                    }
                })
        }))
}

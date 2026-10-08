//! Small Orca-styled building blocks shared by the new pages. No state, no
//! app context; the callers compose them with their own data.

use gpui::{AnyElement, ClickEvent, Div, SharedString, Window, div, px, App};
use ui::prelude::*;

use crate::palette::{self as p, c, state_mark};

/// An Orca card: 1px border, 10px corners, CARD background, 12px inset.
pub fn card() -> Div {
    div()
        .bg(c(p::CARD))
        .border_1()
        .border_color(c(p::BORDER))
        .rounded(px(p::RADIUS_CARD))
        .p(px(12.))
}

/// A state pill: the state word, uppercase at PILL size, in the state colour,
/// on that colour at 15% alpha. Orca's "needs you" / "done" / "working" tags.
pub fn state_pill(state: &str) -> AnyElement {
    let (_, colour) = state_mark(state);
    let tint = c(colour).opacity(0.15);
    let label: SharedString = state.to_ascii_uppercase().into();
    h_flex()
        .flex_none()
        .h(px(18.))
        .px(px(8.))
        .items_center()
        .rounded_full()
        .bg(tint)
        .text_size(px(p::PILL))
        .text_color(c(colour))
        .child(label)
        .into_any_element()
}

/// Orca's status glyph for a run state. Working nodes are a ring spinner
/// (delegated to `crate::sidebar::status_mark` so the animation id stays
/// unique); everything else is the static `state_mark` glyph.
pub fn status_mark(state: &str, size: f32, key: &str) -> AnyElement {
    if matches!(state, "working" | "running") {
        return crate::sidebar::status_mark(state, key);
    }
    let (glyph, colour) = state_mark(state);
    div()
        .flex_none()
        .text_size(px(size))
        .text_color(c(colour))
        .child(SharedString::from(glyph))
        .into_any_element()
}

/// A section heading: the title in 11px semibold MUTED, with a count pill
/// (rendered by `state_pill` so it matches the rest of the design).
pub fn section_head(title: &str, count: Option<usize>) -> Div {
    h_flex()
        .flex_none()
        .gap(px(6.))
        .items_center()
        .text_size(px(p::META))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(c(p::MUTED))
        .child(SharedString::from(title.to_string()))
        .when_some(count, |this, n| this.child(state_pill(&n.to_string())))
}

/// A primary action button next to a `▾` caret. Today the secondary actions
/// render as ghost buttons next to the caret; a future iteration will group
/// them under a real popover/dropdown menu (TODO) so the row stays a single
/// chip.
pub fn split_button(
    id: impl Into<SharedString>,
    primary: (impl Into<SharedString>, &str),
    on_primary: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    more: Vec<(SharedString, Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>)>,
) -> AnyElement {
    let id = id.into();
    let (label, kind) = primary;
    let primary_button = crate::palette::button(
        SharedString::from(format!("{id}-primary")),
        label,
        kind,
        24.,
        12.,
        on_primary,
    );
    if more.is_empty() {
        return primary_button;
    }
    let caret = crate::palette::button(
        SharedString::from(format!("{id}-caret")),
        SharedString::from("▾"),
        "ghost",
        24.,
        12.,
        None,
    );
    // TODO: replace these ghost buttons with a real `▾` menu (`popover` or
    // `dropdown_menu`) so the row stays a single chip. Until that lands the
    // secondary actions appear inline next to the caret.
    let extras = more.into_iter().enumerate().map(|(i, (label, handler))| {
        crate::palette::button(
            SharedString::from(format!("{id}-more-{i}")),
            label,
            "ghost",
            24.,
            12.,
            Some(handler),
        )
    });
    h_flex()
        .flex_none()
        .gap(px(4.))
        .items_center()
        .child(primary_button)
        .child(caret)
        .children(extras)
        .into_any_element()
}

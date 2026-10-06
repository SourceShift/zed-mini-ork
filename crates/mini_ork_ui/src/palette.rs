//! The mini-ork IDE design's palette, sizes and small building blocks.
//!
//! Colours are fixed (the design is drawn in Zed's One Dark), and pages name
//! colours rather than sending hex — see `mini_ork/ide_pages/spec.py`.

use gpui::{AnyElement, App, ClickEvent, Div, Font, Hsla, Window, px, rgb, rgba};
use ui::prelude::*;

pub const BG: u32 = 0x282c33;
pub const CHROME: u32 = 0x2f343e;
pub const BORDER: u32 = 0x363c46;
pub const BORDER2: u32 = 0x464b57;
pub const HOVER: u32 = 0x363c46;
pub const SELECTED: u32 = 0x454a56;
pub const ROW_SELECTED: u32 = 0x3b414d;
pub const ROW_RULE: u32 = 0x30353f;
pub const NODE_BG: u32 = 0x353b45;

pub const TEXT: u32 = 0xdce0e5;
pub const BODY: u32 = 0xc8ccd4;
pub const MUTED: u32 = 0xa9afbc;
pub const SUB: u32 = 0x878a98;
pub const DIM: u32 = 0x5f6672;
pub const BLUE: u32 = 0x74ade8;
pub const GREEN: u32 = 0xa1c181;
pub const RED: u32 = 0xd07277;
pub const YELLOW: u32 = 0xdec184;
pub const PURPLE: u32 = 0xb477cf;
pub const CYAN: u32 = 0x6eb4bf;
pub const ORANGE: u32 = 0xc0966b;

pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn ca(hex_with_alpha: u32) -> Hsla {
    rgba(hex_with_alpha).into()
}

/// A colour name from a page (`"green"`, `"fam:sonnet"`, …).
pub fn named(name: &str) -> Hsla {
    if let Some(lane) = name.strip_prefix("fam:") {
        return c(family(lane));
    }
    c(match name {
        "text" => TEXT,
        "body" => BODY,
        "muted" => MUTED,
        "sub" => SUB,
        "dim" => DIM,
        "blue" => BLUE,
        "green" => GREEN,
        "red" => RED,
        "yellow" => YELLOW,
        "purple" => PURPLE,
        "cyan" => CYAN,
        "orange" => ORANGE,
        _ => BODY,
    })
}

/// The colour of a model family, matched loosely on the lane name.
pub fn family(lane: &str) -> u32 {
    let lane = lane.to_ascii_lowercase();
    let pick = [
        ("opus", PURPLE),
        ("sonnet", ORANGE),
        ("haiku", ORANGE),
        ("claude", ORANGE),
        ("glm", CYAN),
        ("zhipu", CYAN),
        ("kimi", BLUE),
        ("moonshot", BLUE),
        ("codex", YELLOW),
        ("gpt", YELLOW),
        ("openai", YELLOW),
        ("deepseek", 0x8f9cf0),
        ("minimax", 0xe08fb0),
        ("gemini", GREEN),
        ("qwen", 0x9fd4a3),
    ];
    pick.iter()
        .find(|(needle, _)| lane.contains(needle))
        .map(|(_, colour)| *colour)
        .unwrap_or(SUB)
}

/// The mark and colour of a run state.
pub fn state_mark(state: &str) -> (&'static str, u32) {
    match state {
        "working" | "running" => ("●", BLUE),
        "needs_you" | "needs" => ("✋", YELLOW),
        "failed" | "error" => ("✗", RED),
        "done" | "published" => ("✓", GREEN),
        _ => ("○", SUB),
    }
}

pub fn mono_font(cx: &App) -> Font {
    theme::theme_settings(cx).buffer_font(cx).clone()
}

/// Text at a design size (px) and colour.
pub fn text(content: impl Into<SharedString>, size: f32, colour: u32) -> Div {
    div().text_size(px(size)).text_color(c(colour)).child(content.into())
}

/// The design's button kinds: background, text, border.
pub fn button_colours(kind: &str) -> (Hsla, Hsla, Hsla) {
    match kind {
        "primary" => (ca(0x74ade829), c(BLUE), ca(0x74ade873)),
        "danger" => (ca(0x00000000), c(RED), ca(0xd0727773)),
        "warn" => (ca(0x00000000), c(YELLOW), ca(0xdec18473)),
        "ghost" => (ca(0x00000000), c(MUTED), ca(0x00000000)),
        _ => (c(HOVER), c(TEXT), c(BORDER2)),
    }
}

/// A design button. `on_click: None` draws it dimmed and inert.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: &str,
    height: f32,
    font_size: f32,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
) -> AnyElement {
    let (bg, fg, border) = button_colours(kind);
    let enabled = on_click.is_some();
    let base = h_flex()
        .id(id.into())
        .flex_none()
        .h(px(height))
        .px(px(if height <= 22. { 8. } else if height <= 24. { 9. } else { 10. }))
        .items_center()
        .rounded(px(5.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_size(px(font_size))
        .text_color(fg)
        .whitespace_nowrap()
        .child(label.into());
    match on_click {
        Some(handler) if enabled => base
            .cursor_pointer()
            .hover(|style| style.opacity(0.85))
            .on_click(handler)
            .into_any_element(),
        _ => base.opacity(0.55).into_any_element(),
    }
}

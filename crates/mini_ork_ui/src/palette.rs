//! The mini-ork IDE design's palette, sizes and small building blocks.
//!
//! The design is drawn in Zed's One Dark, so views name its colours (`BG`,
//! `TEXT`, `GREEN`, …, and a few literal greys). `c` / `ca` map each of them
//! onto the ACTIVE Zed theme (editor/panel backgrounds, borders, text levels,
//! status and syntax colours), so the IDE follows any theme, light or dark,
//! and re-colours when the theme changes. Pages name colours rather than
//! sending hex — see `mini_ork/ide_pages/spec.py`.

use std::{collections::HashMap, sync::RwLock};

use gpui::{AnyElement, App, ClickEvent, Div, Font, Hsla, Window, px, rgb};
use theme::ActiveTheme as _;
use ui::prelude::*;

pub const BG: u32 = 0x282c33;
pub const CHROME: u32 = 0x2f343e;
pub const BORDER: u32 = 0x363c46;
pub const BORDER2: u32 = 0x464b57;
// One step off BORDER so the theme map can tell hover from a border.
pub const HOVER: u32 = 0x363c47;
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

/// Literal design greys that views use directly.
const STREAM_BOX: u32 = 0x262a31;
const ELEMENT: u32 = 0x353a44;
const CANVAS: u32 = 0x1f2227;
const SKELETON: u32 = 0x434a56;

/// Design colour → active-theme colour; `None` until `sync_theme` runs.
static THEMED: RwLock<Option<HashMap<u32, Hsla>>> = RwLock::new(None);

/// Rebuild the design → theme map from the active theme. Called at init and
/// whenever the theme changes.
pub fn sync_theme(cx: &App) {
    let theme = cx.theme();
    let colors = theme.colors();
    let status = theme.status();
    let syntax = |name: &str, fallback: u32| -> Hsla {
        theme
            .syntax()
            .style_for_name(name)
            .and_then(|style| style.color)
            .unwrap_or_else(|| rgb(fallback).into())
    };
    // A surface a little darker than `base`, in light and dark themes alike.
    let shade = |base: Hsla, amount: f32| Hsla { l: (base.l - amount).clamp(0., 1.), ..base };
    let map = HashMap::from([
        (BG, colors.editor_background),
        (CHROME, colors.panel_background),
        (BORDER, colors.border_variant),
        (BORDER2, colors.border),
        (HOVER, colors.element_hover),
        (SELECTED, colors.element_selected),
        (ROW_SELECTED, colors.background),
        (ROW_RULE, colors.border_variant.opacity(0.6)),
        (NODE_BG, colors.element_background.blend(colors.element_hover.opacity(0.6))),
        (TEXT, colors.text),
        (BODY, colors.text.blend(colors.text_muted.opacity(0.45))),
        (MUTED, colors.text_muted),
        (SUB, colors.text_placeholder),
        (DIM, colors.text_disabled.opacity(0.75)),
        (BLUE, colors.text_accent),
        (GREEN, status.success),
        (RED, status.error),
        (YELLOW, status.warning),
        (PURPLE, syntax("keyword", PURPLE)),
        (CYAN, syntax("type", CYAN)),
        (ORANGE, syntax("number", ORANGE)),
        (STREAM_BOX, shade(colors.editor_background, 0.02)),
        (ELEMENT, colors.element_background),
        (CANVAS, shade(colors.editor_background, 0.04)),
        (SKELETON, colors.border),
    ]);
    if let Ok(mut themed) = THEMED.write() {
        *themed = Some(map);
    }
}

/// A design colour, as the active theme draws it (the literal hex when the
/// colour is not part of the design palette).
pub fn c(hex: u32) -> Hsla {
    THEMED
        .read()
        .ok()
        .and_then(|themed| themed.as_ref().and_then(|map| map.get(&hex).copied()))
        .unwrap_or_else(|| rgb(hex).into())
}

/// A design colour with an alpha byte (`0xRRGGBBAA`): the themed colour of
/// `RRGGBB` at that alpha, so tints follow the theme too.
pub fn ca(hex_with_alpha: u32) -> Hsla {
    let alpha = (hex_with_alpha & 0xff) as f32 / 255.;
    let base = c(hex_with_alpha >> 8);
    Hsla { a: base.a * alpha, ..base }
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

/// The logo of a model family, matched on the lane name the same loose way
/// `family` matches its colour — so a lane and its mark are decided together.
/// `None` means "no mark ships for this family yet"; the caller draws the
/// coloured dot instead, so an unmapped lane degrades rather than vanishes.
///
/// Only the icons the tree already has. glm/zhipu is the one family with a
/// `family` colour but no mark yet — it falls back to the dot until an SVG
/// lands.
pub fn model_icon(lane: &str) -> Option<IconName> {
    let lane = lane.to_ascii_lowercase();
    let pick = [
        ("opus", IconName::AiClaude),
        ("sonnet", IconName::AiClaude),
        ("haiku", IconName::AiClaude),
        ("claude", IconName::AiClaude),
        ("anthropic", IconName::AiClaude),
        ("codex", IconName::AiOpenAi),
        ("gpt", IconName::AiOpenAi),
        ("openai", IconName::AiOpenAi),
        ("deepseek", IconName::AiDeepSeek),
        ("minimax", IconName::AiMiniMax),
        ("kimi", IconName::AiKimi),
        ("moonshot", IconName::AiKimi),
        ("qwen", IconName::AiQwen),
        ("gemini", IconName::AiGemini),
        ("google", IconName::AiGoogle),
        ("mistral", IconName::AiMistral),
        ("llama", IconName::AiLlamaCpp),
        ("ollama", IconName::AiOllama),
        ("xai", IconName::AiXAi),
        ("grok", IconName::AiXAi),
        ("router", IconName::AiOpenRouter),
    ];
    pick.iter().find(|(needle, _)| lane.contains(needle)).map(|(_, icon)| *icon)
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

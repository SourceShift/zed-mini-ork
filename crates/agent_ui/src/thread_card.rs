//! Inline thread cards for the agent panel.
//!
//! When a tool call carries a `_meta["mini_ork"]` envelope, this module
//! renders the card instead of the stock tool-call body. The envelope is a
//! small, versioned JSON object (see `CARD_KEY` / `CARD_VERSION`); unknown
//! versions, unknown kinds, unknown block types and malformed JSON all
//! degrade to the stock body without panicking.
//!
//! The renderer is server-agnostic: any ACP server that emits the envelope
//! gets the same cards. A later override can register a custom
//! [`ThreadCardRenderer`] against a different `_meta` key without touching
//! the call sites in `thread_view.rs`.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use acp_thread::PermissionRequestId;
use agent_client_protocol::schema::{v1 as acp_v1, v2 as acp_v2};
use gpui::{AnyElement, App, Entity, IntoElement, SharedString, Window, actions, prelude::*};
use serde::Deserialize;
use settings::Settings;
use theme_settings::ThemeSettings;
use ui::{Button, ButtonStyle, Label, LabelSize, TintColor, Tooltip, prelude::*};
use workspace::Workspace;

pub const CARD_KEY: &str = "mini_ork";
pub const CARD_VERSION: u32 = 1;

actions!(
    agent,
    [
        /// Focus the message editor in the active thread. Dispatched by
        /// `prompt` card actions; handled by `ThreadView`.
        FocusMessageEditor,
    ]
);

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ThreadCard {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub kind: String,
    #[serde(default, rename = "id")]
    pub card_id: String,
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub header: CardHeader,
    #[serde(default)]
    pub blocks: Vec<CardBlock>,
    #[serde(default)]
    pub actions: Vec<CardAction>,
    #[serde(default)]
    pub suppress_stock: bool,
    #[serde(default)]
    pub fallback_text: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CardHeader {
    #[serde(default)]
    pub mark: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub meta: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub tag: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub enum CardBlock {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
        #[serde(default)]
        tone: Option<String>,
    },
    #[serde(rename = "kv")]
    Kv {
        #[serde(default)]
        rows: Vec<KvRow>,
    },
    #[serde(rename = "items")]
    Items {
        #[serde(default)]
        items: Vec<ItemRow>,
    },
    #[serde(rename = "files")]
    Files {
        #[serde(default)]
        files: Vec<FileRow>,
    },
    #[serde(rename = "steps")]
    Steps {
        #[serde(default)]
        steps: Vec<StepRow>,
    },
    #[serde(rename = "code")]
    Code {
        #[serde(default)]
        lang: Option<String>,
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "section")]
    Section {
        #[serde(default)]
        title: String,
        #[serde(default)]
        text: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct KvRow {
    #[serde(default)]
    pub k: String,
    #[serde(default)]
    pub v: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ItemRow {
    #[serde(default)]
    pub mark: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FileRow {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct StepRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub lane: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub cost: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum CardAction {
    #[serde(rename = "permission")]
    Permission {
        #[serde(default)]
        id: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        option_id: String,
        #[serde(default)]
        option_kind: String,
        #[serde(default)]
        style: Option<String>,
    },
    #[serde(rename = "prompt")]
    Prompt {
        #[serde(default)]
        id: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        placeholder: Option<String>,
    },
    #[serde(rename = "link")]
    Link {
        #[serde(default)]
        id: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        target: String,
    },
    #[serde(rename = "command")]
    Command {
        #[serde(default)]
        id: String,
        #[serde(default)]
        label: String,
        #[serde(default)]
        command: String,
    },
}

impl ThreadCard {
    /// Parse a card envelope from `_meta`. Returns `None` when the envelope
    /// is absent, mistyped, the wrong version, or has an empty `kind`.
    pub fn from_meta(meta: Option<&acp_v2::Meta>) -> Option<ThreadCard> {
        let value = meta?.get(CARD_KEY)?;
        if !value.is_object() {
            return None;
        }
        let card: ThreadCard = serde_json::from_value(value.clone()).ok()?;
        if card.v != CARD_VERSION {
            return None;
        }
        if card.kind.is_empty() {
            return None;
        }
        Some(card)
    }

    pub fn fallback(&self) -> &str {
        &self.fallback_text
    }
}

/// Resolved permission data for a card's `permission` actions.
#[derive(Clone, Debug)]
pub struct ResolvedPermission {
    pub request_id: PermissionRequestId,
    /// (option_id, option_kind) pairs for every option the server proposed.
    pub options: Vec<(String, String)>,
}

/// Host-facing context handed to a renderer.
pub struct ThreadCardView<'a> {
    pub session_id: &'a acp_v1::SessionId,
    pub tool_call_id: String,
    pub entry_ix: usize,
    pub permission: Option<ResolvedPermission>,
    pub workspace: Option<Entity<Workspace>>,
}

/// Trait for an alternative renderer registered against a particular `_meta`
/// key.
pub trait ThreadCardRenderer: Send + Sync {
    fn namespace(&self) -> &'static str;
    fn render(
        &self,
        card: &ThreadCard,
        view: &ThreadCardView<'_>,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement>;
    fn suppress_entry(&self, _card: &ThreadCard) -> bool {
        false
    }
}

struct EnvelopeRenderer;

impl ThreadCardRenderer for EnvelopeRenderer {
    fn namespace(&self) -> &'static str {
        CARD_KEY
    }

    fn render(
        &self,
        card: &ThreadCard,
        view: &ThreadCardView<'_>,
        _window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        Some(render_envelope(card, view, cx).into_any_element())
    }

    fn suppress_entry(&self, card: &ThreadCard) -> bool {
        card.kind == "node"
    }
}

fn registry() -> &'static Mutex<Vec<Box<dyn ThreadCardRenderer>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Box<dyn ThreadCardRenderer>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

fn ensure_built_in_registered() {
    let mut guard = registry()
        .lock()
        .expect("thread_card registry lock poisoned");
    if !guard.iter().any(|r| r.namespace() == CARD_KEY) {
        guard.push(Box::new(EnvelopeRenderer));
    }
}

pub fn register_renderer(renderer: Box<dyn ThreadCardRenderer>) {
    let mut guard = registry()
        .lock()
        .expect("thread_card registry lock poisoned");
    guard.push(renderer);
}

pub fn render(
    meta: Option<&acp_v2::Meta>,
    view: &ThreadCardView<'_>,
    window: &Window,
    cx: &App,
) -> Option<AnyElement> {
    let card = ThreadCard::from_meta(meta)?;
    ensure_built_in_registered();
    let guard = registry()
        .lock()
        .expect("thread_card registry lock poisoned");
    for renderer in guard.iter().rev() {
        if meta.and_then(|m| m.get(renderer.namespace())).is_some() {
            if let Some(element) = renderer.render(&card, view, window, cx) {
                return Some(element);
            }
        }
    }
    None
}

pub fn suppresses_entry(meta: Option<&acp_v2::Meta>) -> bool {
    let Some(card) = ThreadCard::from_meta(meta) else {
        return false;
    };
    ensure_built_in_registered();
    let guard = registry()
        .lock()
        .expect("thread_card registry lock poisoned");
    for renderer in guard.iter().rev() {
        if meta.and_then(|m| m.get(renderer.namespace())).is_some() {
            return renderer.suppress_entry(&card);
        }
    }
    false
}

/// The monospace family used for paths, code and stderr excerpts — the same
/// buffer font the rest of the agent panel uses for code content.
fn mono_font(cx: &App) -> SharedString {
    ThemeSettings::get_global(cx)
        .agent_buffer_font_family()
        .clone()
}

fn render_envelope(card: &ThreadCard, view: &ThreadCardView<'_>, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let border = colors.border;

    let mut container = v_flex()
        .w_full()
        .p_2()
        .gap_1p5()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(colors.panel_background);

    container = container.child(render_header(card, cx));

    for block in &card.blocks {
        container = container.child(render_block(block, cx));
    }

    if let Some(actions) = render_actions(card, view) {
        container = container.child(actions);
    }

    container
}

fn render_header(card: &ThreadCard, cx: &App) -> Div {
    let mut row = h_flex().gap_2().items_center().w_full();

    let mark_glyph = match card.header.mark.as_str() {
        "check" => Some("✓"),
        "cross" => Some("✗"),
        "hand" => Some("✋"),
        "dot" => Some("·"),
        _ => None,
    };
    if let Some(glyph) = mark_glyph {
        row = row.child(
            Label::new(glyph)
                .size(LabelSize::Small)
                .color(Color::Default),
        );
    }

    if !card.header.title.is_empty() {
        row = row.child(Label::new(card.header.title.clone()).size(LabelSize::Small));
    }

    if !card.header.meta.is_empty() || !card.header.detail.is_empty() {
        let mut middle = String::new();
        if !card.header.meta.is_empty() {
            middle.push_str(&card.header.meta);
        }
        if !card.header.detail.is_empty() {
            if !middle.is_empty() {
                middle.push_str(" · ");
            }
            middle.push_str(&card.header.detail);
        }
        row = row.child(
            Label::new(middle)
                .size(LabelSize::Small)
                .color(Color::Muted),
        );
    }

    row = row.child(div().flex_grow(1.0));

    if !card.header.tag.is_empty() {
        let colors = cx.theme().colors();
        row = row.child(
            div()
                .px_1p5()
                .py_0p5()
                .rounded_sm()
                .bg(colors.surface_background)
                .child(
                    Label::new(card.header.tag.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                ),
        );
    }

    row
}

fn render_block(block: &CardBlock, cx: &App) -> AnyElement {
    match block {
        CardBlock::Text { text, tone } => render_text(text, tone.as_deref(), cx),
        CardBlock::Kv { rows } => render_kv(rows, cx),
        CardBlock::Items { items } => render_items(items, cx),
        CardBlock::Files { files } => render_files(files, cx),
        CardBlock::Steps { steps } => render_steps(steps, cx),
        CardBlock::Code { lang, text } => render_code(lang.as_deref(), text, cx),
        CardBlock::Section { title, text } => render_section(title, text, cx),
        CardBlock::Unknown => div().into_any_element(),
    }
}

fn render_text(text: &str, tone: Option<&str>, _cx: &App) -> AnyElement {
    let color = match tone {
        Some("muted") => Color::Muted,
        _ => Color::Default,
    };
    Label::new(text.to_string())
        .size(LabelSize::Small)
        .color(color)
        .into_any_element()
}

fn render_kv(rows: &[KvRow], _cx: &App) -> AnyElement {
    let mut col = v_flex().gap_0p5().w_full();
    for row in rows {
        col = col.child(
            h_flex()
                .gap_1()
                .w_full()
                .child(
                    Label::new(row.k.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(row.v.clone())
                        .size(LabelSize::Small)
                        .color(Color::Default),
                ),
        );
    }
    col.into_any_element()
}

fn item_mark_color(status: Option<&str>) -> Color {
    match status.unwrap_or("") {
        "ok" => Color::Success,
        "warn" => Color::Warning,
        "bad" => Color::Error,
        "new" => Color::Modified,
        "muted" => Color::Muted,
        _ => Color::Default,
    }
}

fn render_items(items: &[ItemRow], _cx: &App) -> AnyElement {
    let mut col = v_flex().gap_0p5().w_full();
    for item in items {
        let mark_glyph = match item.mark.as_str() {
            "check" => "✓",
            "cross" => "✗",
            "hand" => "✋",
            "dot" => "·",
            _ => "·",
        };
        let color = item_mark_color(item.status.as_deref());
        col = col.child(
            h_flex()
                .gap_1()
                .w_full()
                .child(
                    Label::new(mark_glyph.to_string())
                        .size(LabelSize::Small)
                        .color(color),
                )
                .child(
                    Label::new(item.text.clone())
                        .size(LabelSize::Small)
                        .color(Color::Default),
                ),
        );
    }
    col.into_any_element()
}

fn render_files(files: &[FileRow], cx: &App) -> AnyElement {
    let mono = mono_font(cx);
    let mut col = v_flex().gap_0p5().w_full();
    for file in files {
        let label_color = match file.status.as_deref() {
            Some("new") => Color::Success,
            _ => Color::Default,
        };
        col = col.child(
            h_flex()
                .gap_1()
                .w_full()
                .font_family(mono.clone())
                .child(
                    Label::new(file.path.clone())
                        .size(LabelSize::Small)
                        .color(label_color),
                )
                .when_some(file.label.clone(), |this, label| {
                    this.child(Label::new(label).size(LabelSize::Small).color(Color::Muted))
                }),
        );
    }
    col.into_any_element()
}

fn render_steps(steps: &[StepRow], cx: &App) -> AnyElement {
    let mono = mono_font(cx);
    let error_background = cx.theme().status().error_background;
    let mut col = v_flex().gap_1().w_full();
    for step in steps {
        let status_glyph = match step.status.as_str() {
            "ok" | "completed" | "done" => "✓",
            "failed" | "bad" => "✗",
            "running" | "in_progress" => "▶",
            _ => "·",
        };
        let status_color = match step.status.as_str() {
            "ok" | "completed" | "done" => Color::Success,
            "failed" | "bad" => Color::Error,
            _ => Color::Default,
        };

        let mut row = h_flex()
            .gap_1()
            .w_full()
            .items_center()
            .child(
                Label::new(status_glyph.to_string())
                    .size(LabelSize::Small)
                    .color(status_color),
            )
            .child(
                Label::new(step.label.clone())
                    .size(LabelSize::Small)
                    .color(Color::Default),
            );

        if let Some(lane) = step.lane.as_ref().filter(|s| !s.is_empty()) {
            row = row.child(
                Label::new("·".to_string())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
            row = row.child(
                Label::new(lane.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }

        if let Some(model) = step.model.as_ref().filter(|s| !s.is_empty()) {
            row = row.child(
                Label::new(model.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }

        if let Some(detail) = step.detail.as_ref().filter(|s| !s.is_empty()) {
            row = row.child(
                Label::new(detail.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }

        row = row.child(div().flex_grow(1.0));

        if let Some(cost) = step.cost.as_ref().filter(|s| !s.is_empty()) {
            row = row.child(
                Label::new(cost.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }

        col = col.child(row);

        if let Some(stderr) = step.stderr.as_ref().filter(|s| !s.is_empty()) {
            col = col.child(
                div()
                    .ml_4()
                    .p_1()
                    .rounded_sm()
                    .bg(error_background)
                    .font_family(mono.clone())
                    .child(
                        Label::new(stderr.clone())
                            .size(LabelSize::XSmall)
                            .color(Color::Error),
                    ),
            );
        }
    }
    col.into_any_element()
}

fn render_code(lang: Option<&str>, text: &str, cx: &App) -> AnyElement {
    let mut col = v_flex().w_full();
    if let Some(lang) = lang.filter(|s| !s.is_empty()) {
        col = col.child(
            Label::new(lang.to_string())
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        );
    }
    let colors = cx.theme().colors();
    col = col.child(
        div()
            .w_full()
            .p_2()
            .rounded_sm()
            .bg(colors.surface_background)
            .font_family(mono_font(cx))
            .child(Label::new(text.to_string()).size(LabelSize::XSmall)),
    );
    col.into_any_element()
}

fn render_section(title: &str, text: &str, _cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .gap_0p5()
        .when(!title.is_empty(), |this| {
            this.child(
                Label::new(title.to_string())
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::BOLD),
            )
        })
        .child(
            Label::new(text.to_string())
                .size(LabelSize::Small)
                .color(Color::Default),
        )
        .into_any_element()
}

fn render_actions(card: &ThreadCard, view: &ThreadCardView<'_>) -> Option<AnyElement> {
    let mut row = h_flex().gap_1().w_full().flex_wrap();
    let mut drew_any = false;
    for action in &card.actions {
        if let Some(element) = render_action(action, view) {
            row = row.child(element);
            drew_any = true;
        }
    }
    drew_any.then(|| row.into_any_element())
}

fn render_action(action: &CardAction, view: &ThreadCardView<'_>) -> Option<AnyElement> {
    match action {
        CardAction::Permission {
            id,
            label,
            option_id,
            style,
            ..
        } => render_permission_action(view.entry_ix, id, label, option_id, style.as_deref(), view),
        CardAction::Prompt { id, label, .. } => {
            Some(render_prompt_action(view.entry_ix, id, label))
        }
        CardAction::Link {
            id, label, target, ..
        } => Some(render_link_action(view.entry_ix, id, label, target, view)),
        CardAction::Command {
            id, label, command, ..
        } => Some(render_command_action(view.entry_ix, id, label, command)),
    }
}

fn permission_button_style(style: Option<&str>) -> ButtonStyle {
    match style.unwrap_or("") {
        "primary" => ButtonStyle::Tinted(TintColor::Accent),
        "danger" => ButtonStyle::Tinted(TintColor::Error),
        _ => ButtonStyle::Subtle,
    }
}

/// Pure selection helper: returns the (option_id, option_kind) pair that
/// matches `option_id` in the offered permission options, when present.
pub fn resolve_permission_option<'a>(
    options: &'a [(String, String)],
    option_id: &str,
) -> Option<&'a (String, String)> {
    options.iter().find(|(id, _)| id == option_id)
}

fn render_permission_action(
    entry_ix: usize,
    action_id: &str,
    label: &str,
    option_id: &str,
    style: Option<&str>,
    view: &ThreadCardView<'_>,
) -> Option<AnyElement> {
    let button = Button::new(
        format!("thread-card-perm-{entry_ix}-{action_id}"),
        label.to_string(),
    )
    .label_size(LabelSize::Small)
    .style(permission_button_style(style));

    let Some(permission) = view.permission.as_ref() else {
        // No pending request for this tool call: draw the button disabled
        // (muted, no handler) rather than hiding the action.
        return Some(button.disabled(true).into_any_element());
    };
    // When the option is not among those the server offered, the action
    // cannot be answered: skip the button entirely.
    let (resolved_id, resolved_kind) = resolve_permission_option(&permission.options, option_id)?;

    let request_id = permission.request_id.clone();
    let session_id = view.session_id.clone();
    let tool_call_id = view.tool_call_id.clone();
    let option_id = resolved_id.clone();
    let option_kind = resolved_kind.clone();
    Some(
        button
            .on_click(move |_event, window, cx| {
                window.dispatch_action(
                    Box::new(crate::AuthorizeToolCall {
                        tool_call_id: tool_call_id.clone(),
                        request_id: Some(request_id.clone()),
                        session_id: Some(session_id.0.to_string()),
                        option_id: option_id.clone(),
                        option_kind: option_kind.clone(),
                    }),
                    cx,
                );
            })
            .into_any_element(),
    )
}

fn render_prompt_action(entry_ix: usize, action_id: &str, label: &str) -> AnyElement {
    Button::new(
        format!("thread-card-prompt-{entry_ix}-{action_id}"),
        label.to_string(),
    )
    .label_size(LabelSize::Small)
    .style(ButtonStyle::Subtle)
    .on_click(|_event, window, cx| {
        window.dispatch_action(Box::new(FocusMessageEditor), cx);
    })
    .into_any_element()
}

fn render_link_action(
    entry_ix: usize,
    action_id: &str,
    label: &str,
    target: &str,
    view: &ThreadCardView<'_>,
) -> AnyElement {
    let path = PathBuf::from(target);
    let path_exists = path.exists();
    let is_dir = path.is_dir();
    let mut button = Button::new(
        format!("thread-card-link-{entry_ix}-{action_id}"),
        label.to_string(),
    )
    .label_size(LabelSize::Small)
    .style(ButtonStyle::Subtle)
    .tooltip(Tooltip::text(format!("Open {target}")));

    // Fail closed: the target must exist and a workspace must be available
    // before the link can do anything.
    if !path_exists {
        button = button.disabled(true);
    } else if let Some(workspace) = view.workspace.as_ref() {
        let workspace = workspace.clone();
        let path = path.clone();
        button = button.on_click(move |_event, window, cx| {
            if is_dir {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .open_paths(vec![path.clone()], Default::default(), None, window, cx)
                        .detach();
                });
            } else {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .open_abs_path(path.clone(), Default::default(), window, cx)
                        .detach_and_log_err(cx);
                });
            }
        });
    } else {
        button = button.disabled(true);
    }

    button.into_any_element()
}

fn render_command_action(
    entry_ix: usize,
    action_id: &str,
    label: &str,
    command: &str,
) -> AnyElement {
    Button::new(
        format!("thread-card-cmd-{entry_ix}-{action_id}"),
        label.to_string(),
    )
    .label_size(LabelSize::Small)
    .style(ButtonStyle::Subtle)
    .tooltip(Tooltip::text(format!("Send /{command}")))
    .disabled(true)
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_meta_returns_none_when_absent() {
        let meta: acp_v2::Meta = acp_v2::Meta::new();
        assert!(ThreadCard::from_meta(Some(&meta)).is_none());
        assert!(ThreadCard::from_meta(None).is_none());
    }

    #[test]
    fn from_meta_returns_none_for_mistyped_payload() {
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(CARD_KEY.to_string(), json!("not an object"));
        assert!(ThreadCard::from_meta(Some(&meta)).is_none());
    }

    #[test]
    fn from_meta_returns_none_for_wrong_version() {
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(
            CARD_KEY.to_string(),
            json!({ "v": 2, "kind": "run", "header": {} }),
        );
        assert!(ThreadCard::from_meta(Some(&meta)).is_none());
    }

    #[test]
    fn from_meta_returns_none_for_empty_kind() {
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(
            CARD_KEY.to_string(),
            json!({ "v": 1, "kind": "", "header": {} }),
        );
        assert!(ThreadCard::from_meta(Some(&meta)).is_none());
    }

    #[test]
    fn from_meta_parses_valid_payload() {
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(
            CARD_KEY.to_string(),
            json!({
                "v": 1,
                "kind": "run",
                "id": "run-1:parent",
                "revision": 4,
                "parent": null,
                "phase": "execute",
                "status": "in_progress",
                "header": { "mark": "check", "title": "Add dark-mode line" },
                "blocks": [],
                "actions": [],
                "suppress_stock": true,
                "fallback_text": "run-1",
            }),
        );
        let card = ThreadCard::from_meta(Some(&meta)).expect("card");
        assert_eq!(card.v, 1);
        assert_eq!(card.kind, "run");
        assert_eq!(card.card_id, "run-1:parent");
        assert_eq!(card.revision, 4);
        assert_eq!(card.fallback(), "run-1");
        assert!(card.header.title.contains("dark-mode"));
    }

    #[test]
    fn unknown_block_type_is_skipped_while_siblings_parse() {
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(
            CARD_KEY.to_string(),
            json!({
                "v": 1,
                "kind": "run",
                "header": {},
                "blocks": [
                    { "type": "text", "text": "before" },
                    { "type": "future-block", "anything": true },
                    { "type": "section", "title": "After", "text": "still here" },
                ],
                "actions": [],
            }),
        );
        let card = ThreadCard::from_meta(Some(&meta)).expect("card");
        assert_eq!(card.blocks.len(), 3);
        assert!(matches!(card.blocks[0], CardBlock::Text { .. }));
        assert!(matches!(card.blocks[1], CardBlock::Unknown));
        assert!(matches!(card.blocks[2], CardBlock::Section { .. }));
    }

    #[test]
    fn suppress_entry_true_for_node_kind() {
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(
            CARD_KEY.to_string(),
            json!({ "v": 1, "kind": "node", "header": {} }),
        );
        assert!(suppresses_entry(Some(&meta)));
    }

    #[test]
    fn suppress_entry_false_for_run_kind() {
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(
            CARD_KEY.to_string(),
            json!({ "v": 1, "kind": "run", "header": {} }),
        );
        assert!(!suppresses_entry(Some(&meta)));
    }

    #[test]
    fn comp_a_kickoff_fixture_parses() {
        let raw = r#"{
            "v": 1,
            "kind": "kickoff",
            "id": "draft-1:parent",
            "revision": 1,
            "parent": null,
            "phase": "classify",
            "status": "pending",
            "header": {
                "mark": "hand",
                "title": "Rate limit POST /api/upload",
                "meta": "kickoff ready to check",
                "detail": "",
                "tag": "code-fix"
            },
            "blocks": [
                { "type": "section", "title": "TASK", "text": "Cap POST /api/upload at 5 r/s per token." },
                { "type": "files", "files": [
                    { "path": "src/api/upload.rs", "label": "route", "status": "new" }
                ] },
                { "type": "items", "items": [
                    { "mark": "check", "text": "request stays under 6 r/s", "status": "ok" }
                ] }
            ],
            "actions": [
                { "kind": "permission", "id": "a1", "label": "Start run", "option_id": "start", "option_kind": "AllowOnce", "style": "primary" },
                { "kind": "permission", "id": "a2", "label": "Save only", "option_id": "save", "option_kind": "AllowOnce" },
                { "kind": "permission", "id": "a3", "label": "Change something", "option_id": "change", "option_kind": "RejectOnce" },
                { "kind": "permission", "id": "a4", "label": "Discard", "option_id": "discard", "option_kind": "RejectAlways", "style": "danger" }
            ],
            "suppress_stock": true,
            "fallback_text": "Rate limit POST /api/upload"
        }"#;
        let card: ThreadCard = serde_json::from_str(raw).expect("parse");
        assert_eq!(card.kind, "kickoff");
        assert_eq!(card.blocks.len(), 3);
        assert_eq!(card.actions.len(), 4);
        let mut meta: acp_v2::Meta = acp_v2::Meta::new();
        meta.insert(
            CARD_KEY.to_string(),
            serde_json::from_str(raw).expect("json"),
        );
        let parsed = ThreadCard::from_meta(Some(&meta)).expect("from meta");
        assert_eq!(parsed.kind, "kickoff");
        assert_eq!(parsed.actions.len(), 4);
    }

    #[test]
    fn comp_b_run_success_fixture_parses() {
        let raw = r#"{
            "v": 1,
            "kind": "run",
            "id": "run-1791:parent",
            "revision": 4,
            "parent": null,
            "phase": "publish",
            "status": "completed",
            "header": {
                "mark": "check",
                "title": "Add dark-mode line to CHANGELOG",
                "meta": "+4 -0",
                "detail": "published",
                "tag": "docs"
            },
            "blocks": [
                { "type": "steps", "steps": [
                    { "id": "node-1", "label": "draft", "status": "ok", "lane": "docs", "cost": "0.01" },
                    { "id": "node-2", "label": "publish", "status": "ok", "lane": "docs", "cost": "0.02" }
                ] },
                { "type": "kv", "rows": [
                    { "k": "Where it landed", "v": "CHANGELOG.md" },
                    { "k": "Not committed", "v": "no" },
                    { "k": "Cost", "v": "$0.03" }
                ] }
            ],
            "actions": [],
            "suppress_stock": true,
            "fallback_text": "Add dark-mode line to CHANGELOG · published"
        }"#;
        let card: ThreadCard = serde_json::from_str(raw).expect("parse");
        assert_eq!(card.kind, "run");
        assert_eq!(card.header.mark, "check");
        assert_eq!(card.blocks.len(), 2);
        assert_eq!(
            card.fallback(),
            "Add dark-mode line to CHANGELOG · published"
        );
    }

    #[test]
    fn comp_c_failure_fixture_parses() {
        let raw = r#"{
            "v": 1,
            "kind": "failure",
            "id": "run-1792:failure",
            "revision": 7,
            "parent": "run-1792:parent",
            "phase": "recovery",
            "status": "failed",
            "header": {
                "mark": "cross",
                "title": "Migrate users.email to citext",
                "meta": "rollback-proof.sh",
                "detail": "rc=1",
                "tag": "db-migration"
            },
            "blocks": [
                { "type": "steps", "steps": [
                    { "id": "node-3", "label": "apply", "status": "failed", "lane": "db",
                      "detail": "citext extension missing",
                      "stderr": "ERROR: extension citext not available" }
                ] },
                { "type": "code", "lang": "STDERR", "text": "ERROR: extension citext not available\nHINT: CREATE EXTENSION citext" },
                { "type": "kv", "rows": [
                    { "k": "node", "v": "node-3" },
                    { "k": "attempt", "v": "2" },
                    { "k": "checkpoint", "v": "pre-apply" },
                    { "k": "healer", "v": "none" }
                ] }
            ],
            "actions": [
                { "kind": "permission", "id": "a1", "label": "Recover", "option_id": "recover", "option_kind": "AllowOnce", "style": "primary" },
                { "kind": "permission", "id": "a2", "label": "Resume from node-3", "option_id": "resume:node-3", "option_kind": "AllowOnce" },
                { "kind": "link", "id": "a3", "label": "Open forensics", "target": ".mini-ork/runs/run-1792/diagnosis.json" }
            ],
            "suppress_stock": true,
            "fallback_text": "Migrate users.email to citext · rc=1"
        }"#;
        let card: ThreadCard = serde_json::from_str(raw).expect("parse");
        assert_eq!(card.kind, "failure");
        assert_eq!(card.parent.as_deref(), Some("run-1792:parent"));
        assert_eq!(card.blocks.len(), 3);
        assert_eq!(card.actions.len(), 3);
    }

    #[test]
    fn permission_option_resolution_skips_unknown_options() {
        let options = vec![
            ("start".to_string(), "AllowOnce".to_string()),
            ("save".to_string(), "AllowOnce".to_string()),
        ];
        assert_eq!(
            resolve_permission_option(&options, "start"),
            Some(&("start".to_string(), "AllowOnce".to_string()))
        );
        assert_eq!(resolve_permission_option(&options, "missing"), None);
    }
}

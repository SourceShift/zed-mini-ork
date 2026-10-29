//! Inline thread cards for the agent panel.
//!
//! When a tool call carries a `_meta["mini_ork"]` envelope, this module
//! renders the card instead of the stock tool-call body. The envelope is a
//! small, versioned JSON object (see `CARD_KEY` / `CARD_VERSION`); unknown
//! versions, unknown kinds, unknown block types and malformed JSON all
//! degrade to the stock body without panicking.
//!
//! A run is one `run` card plus flat `node` sibling entries (the engine never
//! nests tool calls). The node entries are suppressed from the list and folded
//! into the run card's rows here, so the run reads as one card: header, node
//! rows with their lane, and the actions the engine offered. Navigation into
//! the mini-ork pages goes through actions dispatched *by name*
//! (`mini_ork::OpenRun`, `mini_ork::OpenNode`): `mini_ork_ui` depends on this
//! crate, so the dependency cannot point the other way. When those actions are
//! not registered the buttons are simply not drawn.
//!
//! The renderer is server-agnostic: any ACP server that emits the envelope
//! gets the same cards. A later override can register a custom
//! [`ThreadCardRenderer`] against a different `_meta` key without touching
//! the call sites in `thread_view.rs`.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use std::collections::HashMap;

use acp_thread::{AgentThreadEntry, PermissionRequestId, ToolCall, ToolCallContent};
use agent_client_protocol::schema::{v1 as acp_v1, v2 as acp_v2};
use gpui::{
    Action, AnyElement, App, ElementId, Entity, Hsla, IntoElement, SharedString, Window, actions,
    prelude::*, rgb,
};
use schemars::JsonSchema;
use serde::Deserialize;
use ui::{
    Button, ButtonStyle, Chip, IconButton, Label, LabelSize, TintColor, Tooltip, prelude::*,
};
use workspace::Workspace;

pub const CARD_KEY: &str = "mini_ork";
pub const CARD_VERSION: u32 = 1;

/// Opens a run's page in the editor area. Owned by `mini_ork_ui`; built by
/// name so this crate needs no dependency on it.
pub const OPEN_RUN_ACTION: &str = "mini_ork::OpenRun";
/// Opens one node of a run as its own tab. Owned by `mini_ork_ui`.
pub const OPEN_NODE_ACTION: &str = "mini_ork::OpenNode";

actions!(
    agent,
    [
        /// Focus the message editor in the active thread. Dispatched by
        /// `prompt` card actions; handled by `ThreadView`.
        FocusMessageEditor,
    ]
);

/// Put `text` in the message editor and focus it, so the user sends it with
/// one more key. Dispatched by `command` card actions; handled by
/// `ThreadView`.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = agent)]
pub struct PrefillMessageEditor {
    pub text: String,
}

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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
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

impl StepRow {
    /// The identity a node keeps across its start and end cards.
    fn key(&self) -> &str {
        if self.id.is_empty() {
            &self.label
        } else {
            &self.id
        }
    }

    /// Take every field the newer row states; keep what it leaves out. A
    /// `node_end` card names the verdict but not always the lane, and the
    /// `node_start` card is the only one that named it.
    fn absorb(&mut self, newer: &StepRow) {
        if !newer.label.is_empty() {
            self.label = newer.label.clone();
        }
        if !newer.status.is_empty() {
            self.status = newer.status.clone();
        }
        for (mine, theirs) in [
            (&mut self.lane, &newer.lane),
            (&mut self.model, &newer.model),
            (&mut self.cost, &newer.cost),
            (&mut self.detail, &newer.detail),
            (&mut self.stderr, &newer.stderr),
        ] {
            if theirs.as_deref().is_some_and(|value| !value.is_empty()) {
                *mine = theirs.clone();
            }
        }
    }
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

    fn steps(&self) -> impl Iterator<Item = &StepRow> {
        self.blocks.iter().flat_map(|block| match block {
            CardBlock::Steps { steps } => steps.as_slice(),
            _ => &[],
        })
    }
}

/// The run id a run marker's card id names (`<run_id>:parent`).
pub fn run_id_of(card_id: &str) -> Option<&str> {
    card_id.strip_suffix(":parent").filter(|id| !id.is_empty())
}

/// One run sibling as the host hands it to the card: its card, and the tail
/// of its live text while the node is still working.
#[derive(Clone, Debug, Default)]
pub struct SiblingCard {
    pub card: ThreadCard,
    pub live_tail: Vec<String>,
}

/// How many lines of a working node's live text the run card shows.
const LIVE_TAIL_LINES: usize = 4;
/// Where a live line is cut; the node tab has the whole stream.
const LIVE_LINE_CHARS: usize = 160;

/// The last few non-empty lines of a node's live text, each clipped to one
/// row. The engine folds a node's stream into the content chunks of its
/// tool call (design §2.7), so the text is already in the entry.
pub fn tail_lines<'a>(texts: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for text in texts {
        for line in text.lines() {
            let line = line.trim_end();
            if line.trim().is_empty() {
                continue;
            }
            let clipped: String = line.chars().take(LIVE_LINE_CHARS).collect();
            if clipped.len() < line.len() {
                lines.push(format!("{clipped}…"));
            } else {
                lines.push(clipped);
            }
        }
    }
    let keep_from = lines.len().saturating_sub(LIVE_TAIL_LINES);
    lines.drain(..keep_from);
    lines
}

fn live_tail(tool_call: &ToolCall, cx: &App) -> Vec<String> {
    tail_lines(tool_call.content().iter().filter_map(|content| match content {
        ToolCallContent::ContentBlock { block, .. } => Some(block.to_markdown(cx)),
        _ => None,
    }))
}

/// Every sibling card that belongs to the run marker `run_card_id`: an entry
/// whose id starts with `<run_id>:` (the router's prefix), or whose card
/// names the marker as its `parent` (the cross-check the design keeps). A
/// node that is still working also brings the tail of its live text.
pub fn collect_siblings(
    run_card_id: &str,
    entries: &[AgentThreadEntry],
    cx: &App,
) -> Vec<SiblingCard> {
    let Some(run_id) = run_id_of(run_card_id) else {
        return Vec::new();
    };
    let prefix = format!("{run_id}:");
    entries
        .iter()
        .filter_map(|entry| match entry {
            AgentThreadEntry::ToolCall(tool_call) => {
                let id = tool_call.id.0.to_string();
                if id == run_card_id {
                    return None;
                }
                let card = ThreadCard::from_meta(tool_call.meta.as_ref())?;
                let belongs =
                    id.starts_with(&prefix) || card.parent.as_deref() == Some(run_card_id);
                if !belongs {
                    return None;
                }
                let working =
                    card.kind == "node" && step_state(&card.status) == StepState::Working;
                let live_tail = if working {
                    live_tail(tool_call, cx)
                } else {
                    Vec::new()
                };
                Some(SiblingCard { card, live_tail })
            }
            _ => None,
        })
        .collect()
}

/// One row per node: the run card's own rows first, then every `node`
/// sibling folded in, in thread order. A node seen again updates its row in
/// place (a later card, or an equal-or-higher revision, wins) so the list
/// keeps the order nodes started in.
pub fn fold_steps(card: &ThreadCard, siblings: &[SiblingCard]) -> Vec<StepRow> {
    let mut rows: Vec<(StepRow, i64)> = Vec::new();
    let mut upsert = |step: &StepRow, revision: i64| {
        if step.key().is_empty() {
            return;
        }
        match rows.iter_mut().find(|(row, _)| row.key() == step.key()) {
            Some((row, seen)) if revision >= *seen => {
                row.absorb(step);
                *seen = revision;
            }
            Some(_) => {}
            None => rows.push((step.clone(), revision)),
        }
    };
    for step in card.steps() {
        upsert(step, card.revision);
    }
    for sibling in siblings.iter().filter(|sibling| sibling.card.kind == "node") {
        for step in sibling.card.steps() {
            upsert(step, sibling.card.revision);
        }
    }
    rows.into_iter().map(|(row, _)| row).collect()
}

/// The live tail of every working node, keyed by the node's row key.
fn live_tails(siblings: &[SiblingCard]) -> HashMap<String, Vec<String>> {
    let mut tails = HashMap::new();
    for sibling in siblings.iter().filter(|sibling| !sibling.live_tail.is_empty()) {
        for step in sibling.card.steps() {
            tails.insert(step.key().to_string(), sibling.live_tail.clone());
        }
    }
    tails
}

/// How a status string reads, whatever vocabulary produced it: a node
/// verdict, a run status, or an ACP tool-call status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    Failed,
    Working,
    Pending,
    Other,
}

pub fn step_state(status: &str) -> StepState {
    let status = status.trim().to_ascii_lowercase();
    if status.is_empty() {
        return StepState::Other;
    }
    const FAILED: &[&str] = &["fail", "refut", "error", "reject", "cancel", "refus", "kill", "bad"];
    const WORKING: &[&str] = &["progress", "running", "executing", "working", "reviewing"];
    const PENDING: &[&str] = &["pending", "queued", "waiting", "not run"];
    const DONE: &[&str] = &[
        "pass", "ok", "approv", "done", "complet", "publish", "success", "verified", "kept",
        "merged",
    ];
    let any = |needles: &[&str]| needles.iter().any(|needle| status.contains(needle));
    if any(FAILED) {
        StepState::Failed
    } else if any(WORKING) {
        StepState::Working
    } else if any(PENDING) {
        StepState::Pending
    } else if any(DONE) {
        StepState::Done
    } else {
        StepState::Other
    }
}

/// The colour of a model family, matched loosely on the lane name. The hues
/// are the design's (`mini-ork IDE.dc.html` `FAM`), the same ones the
/// mini-ork pages use, so a lane keeps its colour across the IDE. `None` is a
/// lane with no family (a `shell` node): the caller draws it muted.
pub fn lane_color(lane: &str) -> Option<Hsla> {
    const FAMILIES: &[(&str, u32)] = &[
        ("opus", 0xb477cf),
        ("sonnet", 0xc0966b),
        ("haiku", 0xc0966b),
        ("claude", 0xc0966b),
        ("anthropic", 0xc0966b),
        ("glm", 0x6eb4bf),
        ("zhipu", 0x6eb4bf),
        ("kimi", 0x74ade8),
        ("moonshot", 0x74ade8),
        ("codex", 0xdec184),
        ("gpt", 0xdec184),
        ("openai", 0xdec184),
        ("deepseek", 0x8f9cf0),
        ("minimax", 0xe08fb0),
        ("gemini", 0xa1c181),
        ("qwen", 0x9fd4a3),
    ];
    let lane = lane.to_ascii_lowercase();
    FAMILIES
        .iter()
        .find(|(needle, _)| lane.contains(needle))
        .map(|(_, hex)| Hsla::from(rgb(*hex)))
}

/// `392123 ms` (what the engine writes) or a bare millisecond count, read as
/// `6m 32s`. Anything else is shown as it came.
pub fn humanize_duration(raw: &str) -> String {
    let digits: String = raw.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    let rest = raw.trim()[digits.len()..].trim();
    let Ok(value) = digits.parse::<u64>() else {
        return raw.trim().to_string();
    };
    let millis = match rest {
        "" | "ms" => value,
        "s" => value * 1000,
        _ => return raw.trim().to_string(),
    };
    let seconds = millis / 1000;
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
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
    /// This entry's run siblings (see [`collect_siblings`]); empty for
    /// anything but a run marker.
    pub siblings: Vec<SiblingCard>,
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
        // The engine's review and lane-repair offers land on the run marker
        // itself (`<run_id>:parent`) and replace its sticky `_meta`, so the
        // marker is drawn as the run card whatever kind it carries now: the
        // folded node rows stay, and the offer's buttons join them.
        if card.kind == "run" || run_id_of(&card.card_id).is_some() {
            return Some(render_run_card(card, view, cx).into_any_element());
        }
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

/// Whether `mini_ork_ui` (or anything else) registered the named action in
/// this app. The card never draws a button it cannot answer.
pub fn action_registered(cx: &App, name: &str) -> bool {
    cx.all_action_names().contains(&name)
}

fn dispatch_named(name: &str, data: serde_json::Value, window: &mut Window, cx: &mut App) {
    match cx.build_action(name, Some(data)) {
        Ok(action) => window.dispatch_action(action, cx),
        Err(error) => log::warn!("thread card: cannot build action {name}: {error}"),
    }
}

// ── marks and states ────────────────────────────────────────────────────────

/// The glyph and colour a status reads as, in the design's vocabulary:
/// `✓` done, `✗` failed, `●` working, `○` pending.
fn state_mark(state: StepState) -> (&'static str, Color) {
    match state {
        StepState::Done => ("✓", Color::Success),
        StepState::Failed => ("✗", Color::Error),
        StepState::Working => ("●", Color::Accent),
        StepState::Pending => ("○", Color::Muted),
        StepState::Other => ("·", Color::Muted),
    }
}

fn state_text(state: StepState, raw: &str) -> String {
    match state {
        StepState::Done => "done".to_string(),
        StepState::Failed => "failed".to_string(),
        StepState::Working => "working".to_string(),
        StepState::Pending => "pending".to_string(),
        StepState::Other => raw.trim().to_string(),
    }
}

/// The mark a card's header shows. A `hand` mark means the engine is waiting
/// on the user and wins over the status; otherwise the status decides, and
/// the engine's own mark is the fallback for a status we do not classify.
fn card_mark(card: &ThreadCard) -> Option<(&'static str, Color)> {
    if card.header.mark == "hand" {
        return Some(("✋", Color::Warning));
    }
    match step_state(&card.status) {
        StepState::Other => match card.header.mark.as_str() {
            "check" => Some(("✓", Color::Success)),
            "cross" => Some(("✗", Color::Error)),
            "dot" => Some(("·", Color::Muted)),
            _ => None,
        },
        state => Some(state_mark(state)),
    }
}

fn mark_label(glyph: &'static str, color: Color) -> Label {
    Label::new(glyph).size(LabelSize::Small).color(color)
}

fn tag_chip(tag: &str) -> Chip {
    Chip::new(tag.to_string())
        .label_size(LabelSize::XSmall)
        .label_color(Color::Muted)
}

fn mono_label(text: impl Into<SharedString>, size: LabelSize, color: Color, cx: &App) -> Label {
    Label::new(text).size(size).color(color).buffer_font(cx)
}

// ── run card ────────────────────────────────────────────────────────────────

/// What the run card lifts out of the engine's `kv` rows into its header.
#[derive(Default, Debug, PartialEq, Eq)]
struct RunFacts {
    cost: Option<String>,
    duration: Option<String>,
    workspace: Option<String>,
    branch: Option<String>,
}

impl RunFacts {
    /// Pull the header facts out of every `kv` block; the rows left over
    /// are returned so they still render as a `kv` block.
    fn extract(card: &ThreadCard) -> (RunFacts, Vec<KvRow>) {
        let mut facts = RunFacts::default();
        let mut leftover = Vec::new();
        for block in &card.blocks {
            let CardBlock::Kv { rows } = block else {
                continue;
            };
            for row in rows {
                let slot = match row.k.trim().to_ascii_lowercase().as_str() {
                    "cost" => &mut facts.cost,
                    "duration" => &mut facts.duration,
                    "workspace" => &mut facts.workspace,
                    "branch" => &mut facts.branch,
                    _ => {
                        leftover.push(row.clone());
                        continue;
                    }
                };
                if !row.v.trim().is_empty() {
                    *slot = Some(row.v.trim().to_string());
                }
            }
        }
        (facts, leftover)
    }

    /// `worktree mo/run-a41c`, `this worktree main`, `in place`, or nothing.
    fn workspace_text(&self) -> Option<String> {
        match (&self.workspace, &self.branch) {
            (Some(mode), Some(branch)) => Some(format!("{mode} {branch}")),
            (Some(mode), None) => Some(mode.clone()),
            (None, Some(branch)) => Some(branch.clone()),
            (None, None) => None,
        }
    }

    /// `$0.42 · 6m 32s`, either half alone, or nothing.
    fn cost_text(&self) -> Option<String> {
        let parts: Vec<String> = [
            self.cost.clone(),
            self.duration.as_deref().map(humanize_duration),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

fn render_run_card(card: &ThreadCard, view: &ThreadCardView<'_>, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let run_id = run_id_of(&card.card_id)
        .map(str::to_string)
        .unwrap_or_else(|| card.card_id.clone());
    let run_label = format!("run {run_id}");
    let (facts, leftover_kv) = RunFacts::extract(card);
    let steps = fold_steps(card, &view.siblings);
    let tails = live_tails(&view.siblings);
    let can_open_run = action_registered(cx, OPEN_RUN_ACTION);
    let can_open_node = action_registered(cx, OPEN_NODE_ACTION);
    let entry_ix = view.entry_ix;

    // ── header: mark · run id · recipe · worktree · cost · ⤢ ──
    let mut header = h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_2()
        .items_center()
        .flex_wrap()
        .border_b_1()
        .border_color(colors.border);
    if let Some((glyph, color)) = card_mark(card) {
        header = header.child(mark_label(glyph, color));
    }
    header = header.child(mono_label(run_label.clone(), LabelSize::Small, Color::Default, cx));
    if !card.header.tag.is_empty() {
        header = header.child(tag_chip(&card.header.tag));
    }
    if let Some(workspace) = facts.workspace_text() {
        header = header.child(
            Label::new(workspace)
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        );
    }
    header = header.child(div().flex_1());
    if let Some(cost) = facts.cost_text() {
        header = header.child(mono_label(cost, LabelSize::XSmall, Color::Muted, cx));
    }
    if can_open_run {
        let run_id = run_id.clone();
        let title = card.header.title.clone();
        header = header.child(
            IconButton::new(("thread-card-open-run", entry_ix), IconName::ArrowUpRight)
                .icon_size(IconSize::XSmall)
                .icon_color(Color::Muted)
                .tooltip(Tooltip::text("Open the run graph"))
                .on_click(move |_event, window, cx| {
                    dispatch_named(
                        OPEN_RUN_ACTION,
                        serde_json::json!({ "run": run_id, "title": title }),
                        window,
                        cx,
                    );
                }),
        );
    }

    let mut container = v_flex()
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.panel_background)
        .overflow_hidden()
        .child(header);

    // The task title, when the engine knows it, sits under the run line.
    let title = card.header.title.trim();
    if !title.is_empty() && title != run_label && title != card.fallback_text {
        let mut line = h_flex().w_full().px_3().pt_2().gap_2().items_center();
        line = line.child(Label::new(title.to_string()).size(LabelSize::Small));
        for extra in [&card.header.meta, &card.header.detail] {
            if !extra.trim().is_empty() {
                line = line.child(
                    Label::new(extra.trim().to_string())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                );
            }
        }
        container = container.child(line);
    }

    // The node rows come first, folded from every `steps` block and the node
    // siblings; then the card's other blocks in the engine's order, minus the
    // `kv` rows the header consumed.
    let mut body = v_flex().w_full().gap_2().px_3().py_2();
    let mut body_has_children = false;
    if !steps.is_empty() {
        body = body.child(render_node_rows(
            &steps,
            &tails,
            &run_id,
            can_open_node,
            entry_ix,
            cx,
        ));
        body_has_children = true;
    }
    let mut drew_kv = false;
    for block in &card.blocks {
        match block {
            CardBlock::Steps { .. } => {}
            CardBlock::Kv { .. } => {
                if !drew_kv && !leftover_kv.is_empty() {
                    body = body.child(render_kv(&leftover_kv, cx));
                    body_has_children = true;
                }
                drew_kv = true;
            }
            other => {
                body = body.child(render_block(other, cx));
                body_has_children = true;
            }
        }
    }
    if body_has_children {
        container = container.child(body);
    }

    // ── actions: the engine's buttons, then "Open run" when the IDE has it ──
    let mut actions = h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_1()
        .flex_wrap()
        .border_t_1()
        .border_color(colors.border);
    let mut drew_action = false;
    for action in &card.actions {
        if let Some(element) = render_action(action, view) {
            actions = actions.child(element);
            drew_action = true;
        }
    }
    if can_open_run {
        let title = card.header.title.clone();
        actions = actions.child(
            Button::new(("thread-card-open-run-button", entry_ix), "Open run")
                .label_size(LabelSize::Small)
                .style(ButtonStyle::Outlined)
                .on_click(move |_event, window, cx| {
                    dispatch_named(
                        OPEN_RUN_ACTION,
                        serde_json::json!({ "run": run_id, "title": title }),
                        window,
                        cx,
                    );
                }),
        );
        drew_action = true;
    }
    if drew_action {
        container = container.child(actions);
    }

    container
}

/// The node rows: `mark · label · (lane dot) lane … status · cost`. A working
/// node shows the tail of its live text under its row; each row opens the
/// node's tab when the IDE registered that action.
fn render_node_rows(
    steps: &[StepRow],
    tails: &HashMap<String, Vec<String>>,
    run_id: &str,
    can_open_node: bool,
    entry_ix: usize,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let error_background = cx.theme().status().error_background;
    let show_cost = steps
        .iter()
        .any(|step| step.cost.as_deref().is_some_and(|cost| !cost.is_empty()));
    let mut list = v_flex().w_full();
    for (index, step) in steps.iter().enumerate() {
        let state = step_state(&step.status);
        let (glyph, mark_color) = state_mark(state);
        let label_color = if state == StepState::Pending {
            Color::Muted
        } else {
            Color::Default
        };
        let lane = step.lane.as_deref().unwrap_or("").trim().to_string();

        let mut name = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(Label::new(step.label.clone()).size(LabelSize::Small).color(label_color).truncate());
        if !lane.is_empty() {
            let dot_color = lane_color(&lane).unwrap_or(colors.text_muted);
            name = name.child(
                h_flex()
                    .flex_none()
                    .gap_1()
                    .items_center()
                    .child(div().size(px(6.)).rounded_full().bg(dot_color))
                    .child(Label::new(lane).size(LabelSize::XSmall).color(Color::Muted)),
            );
        }
        if let Some(model) = step.model.as_deref().filter(|m| !m.is_empty()) {
            name = name.child(
                Label::new(model.to_string())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            );
        }

        let mut row = h_flex()
            .w_full()
            .px_1()
            .py_0p5()
            .gap_2()
            .items_center()
            .rounded_sm()
            .child(div().w_4().flex_none().child(mark_label(glyph, mark_color)))
            .child(name)
            .child(
                Label::new(state_text(state, &step.status))
                    .size(LabelSize::XSmall)
                    .color(mark_color),
            );
        if show_cost {
            row = row.child(
                h_flex().w(px(52.)).flex_none().justify_end().child(mono_label(
                    step.cost.clone().unwrap_or_default(),
                    LabelSize::XSmall,
                    Color::Muted,
                    cx,
                )),
            );
        }
        let tooltip_text = step
            .detail
            .as_deref()
            .filter(|detail| !detail.is_empty())
            .map(str::to_string);

        let row_id = ElementId::Name(format!("thread-card-node-{entry_ix}-{index}").into());
        let mut row = row.id(row_id);
        if let Some(tooltip_text) = tooltip_text {
            row = row.tooltip(Tooltip::text(tooltip_text));
        }
        if can_open_node {
            let run_id = run_id.to_string();
            let node_id = step.key().to_string();
            row = row
                .cursor_pointer()
                .hover(|style| style.bg(colors.element_hover))
                .on_click(move |_event, window, cx| {
                    dispatch_named(
                        OPEN_NODE_ACTION,
                        serde_json::json!({ "run": run_id, "node": node_id }),
                        window,
                        cx,
                    );
                });
        }
        list = list.child(row);

        if let Some(tail) = tails.get(step.key()).filter(|tail| !tail.is_empty()) {
            let mut lines = v_flex()
                .ml_6()
                .my_1()
                .px_2()
                .py_1()
                .rounded_sm()
                .border_1()
                .border_color(colors.border_variant)
                .bg(colors.editor_background);
            for line in tail {
                lines = lines.child(
                    mono_label(line.clone(), LabelSize::XSmall, Color::Muted, cx).truncate(),
                );
            }
            list = list.child(lines);
        }

        if let Some(stderr) = step.stderr.as_deref().filter(|s| !s.is_empty()) {
            list = list.child(
                div()
                    .ml_6()
                    .my_1()
                    .p_1p5()
                    .rounded_sm()
                    .bg(error_background)
                    .child(mono_label(stderr.to_string(), LabelSize::XSmall, Color::Error, cx)),
            );
        }
    }
    list.into_any_element()
}

// ── generic envelope ────────────────────────────────────────────────────────

fn render_envelope(card: &ThreadCard, view: &ThreadCardView<'_>, cx: &App) -> Div {
    let colors = cx.theme().colors();

    let mut container = v_flex()
        .w_full()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.panel_background)
        .overflow_hidden()
        .child(render_header(card, cx));

    if !card.blocks.is_empty() {
        let mut body = v_flex().w_full().gap_2().px_3().py_2();
        for block in &card.blocks {
            body = body.child(render_block(block, cx));
        }
        container = container.child(body);
    }

    if let Some(actions) = render_actions(card, view) {
        container = container.child(
            div()
                .w_full()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(colors.border)
                .child(actions),
        );
    }

    container
}

fn render_header(card: &ThreadCard, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let mut row = h_flex()
        .w_full()
        .px_3()
        .py_2()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(colors.border);

    if let Some((glyph, color)) = card_mark(card) {
        row = row.child(mark_label(glyph, color));
    }

    let mut titles = v_flex().flex_1().min_w_0();
    if !card.header.title.is_empty() {
        titles = titles.child(
            Label::new(card.header.title.clone())
                .size(LabelSize::Small)
                .weight(gpui::FontWeight::SEMIBOLD)
                .truncate(),
        );
    }
    let subtitle: Vec<&str> = [card.header.meta.as_str(), card.header.detail.as_str()]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect();
    if !subtitle.is_empty() {
        titles = titles.child(
            Label::new(subtitle.join(" · "))
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
        );
    }
    row = row.child(titles);

    if !card.header.tag.is_empty() {
        row = row.child(tag_chip(&card.header.tag));
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

/// Key over value, wrapped into as many columns as fit — the design's
/// `kv` grid.
fn render_kv(rows: &[KvRow], _cx: &App) -> AnyElement {
    let mut grid = h_flex().w_full().flex_wrap().gap_x_4().gap_y_2();
    for row in rows {
        grid = grid.child(
            v_flex()
                .min_w(px(120.))
                .child(
                    Label::new(row.k.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .child(Label::new(row.v.clone()).size(LabelSize::Small)),
        );
    }
    grid.into_any_element()
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
            "warn" => "!",
            _ => "·",
        };
        let color = item_mark_color(item.status.as_deref());
        col = col.child(
            h_flex()
                .gap_2()
                .w_full()
                .items_start()
                .child(div().w_3().flex_none().child(mark_label(mark_glyph, color)))
                .child(Label::new(item.text.clone()).size(LabelSize::Small)),
        );
    }
    col.into_any_element()
}

fn render_files(files: &[FileRow], cx: &App) -> AnyElement {
    let mut col = v_flex().gap_0p5().w_full();
    for file in files {
        let label_color = match file.status.as_deref() {
            Some("new") => Color::Success,
            Some("conflict") => Color::Warning,
            _ => Color::Default,
        };
        col = col.child(
            h_flex()
                .gap_2()
                .w_full()
                .items_center()
                .child(mono_label(file.path.clone(), LabelSize::Small, label_color, cx).truncate())
                .when_some(file.label.clone(), |this, label| {
                    this.child(Label::new(label).size(LabelSize::XSmall).color(Color::Muted))
                })
                .when_some(
                    file.status.clone().filter(|status| status != "new"),
                    |this, status| {
                        this.child(div().flex_1()).child(
                            Label::new(status)
                                .size(LabelSize::XSmall)
                                .color(label_color),
                        )
                    },
                ),
        );
    }
    col.into_any_element()
}

/// Steps outside a run card (a failure card quoting the failed node) use the
/// same rows, without navigation.
fn render_steps(steps: &[StepRow], cx: &App) -> AnyElement {
    render_node_rows(steps, &HashMap::new(), "", false, usize::MAX, cx)
}

fn render_code(lang: Option<&str>, text: &str, cx: &App) -> AnyElement {
    let mut col = v_flex().w_full().gap_0p5();
    if let Some(lang) = lang.filter(|s| !s.is_empty()) {
        col = col.child(
            Label::new(lang.to_uppercase())
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
            .border_1()
            .border_color(colors.border_variant)
            .bg(colors.editor_background)
            .child(mono_label(text.to_string(), LabelSize::XSmall, Color::Default, cx)),
    );
    col.into_any_element()
}

fn render_section(title: &str, text: &str, _cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .gap_0p5()
        .when(!title.is_empty(), |this| {
            this.child(
                Label::new(title.to_uppercase())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
        })
        .child(Label::new(text.to_string()).size(LabelSize::Small))
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
        "ghost" => ButtonStyle::Transparent,
        _ => ButtonStyle::Outlined,
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

    let request_id = permission.request_id;
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
                        request_id: Some(request_id),
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
    .style(ButtonStyle::Outlined)
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
    .style(ButtonStyle::Outlined)
    .tooltip(Tooltip::text(format!("Open {target}")));

    // Fail closed: the target must exist and a workspace must be available
    // before the link can do anything.
    if !path_exists {
        button = button.disabled(true);
    } else if let Some(workspace) = view.workspace.as_ref() {
        let workspace = workspace.clone();
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

/// The slash command a `command` action puts in the composer: the engine
/// may name it with or without the leading slash.
pub fn command_text(command: &str) -> String {
    let command = command.trim();
    if command.starts_with('/') {
        command.to_string()
    } else {
        format!("/{command}")
    }
}

fn render_command_action(
    entry_ix: usize,
    action_id: &str,
    label: &str,
    command: &str,
) -> AnyElement {
    let text = command_text(command);
    let tooltip = format!("Put {text} in the composer");
    Button::new(
        format!("thread-card-cmd-{entry_ix}-{action_id}"),
        label.to_string(),
    )
    .label_size(LabelSize::Small)
    .style(ButtonStyle::Outlined)
    .tooltip(Tooltip::text(tooltip))
    .on_click(move |_event, window, cx| {
        window.dispatch_action(Box::new(PrefillMessageEditor { text: text.clone() }), cx);
    })
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card_from(value: serde_json::Value) -> ThreadCard {
        serde_json::from_value(value).expect("card")
    }

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
    fn run_id_of_strips_the_parent_suffix_only() {
        assert_eq!(run_id_of("run-1791302645-a41c:parent"), Some("run-1791302645-a41c"));
        assert_eq!(run_id_of("run-1791302645-a41c:failure"), None);
        assert_eq!(run_id_of(":parent"), None);
        assert_eq!(run_id_of("implementer"), None);
    }

    /// The engine's run marker is built before any node runs: no `steps`.
    /// Every row has to come from the node siblings, in the order they
    /// started, and a node's end card updates its start row in place.
    #[test]
    fn fold_steps_builds_rows_from_node_siblings_in_thread_order() {
        let run = card_from(json!({
            "v": 1, "kind": "run", "id": "run-1:parent", "revision": 0,
            "status": "in_progress", "header": {}, "blocks": [], "actions": []
        }));
        let node = |id: &str, status: &str, lane: Option<&str>, detail: Option<&str>| {
            let mut row = json!({ "id": id, "label": id, "status": status });
            if let Some(lane) = lane {
                row["lane"] = json!(lane);
            }
            if let Some(detail) = detail {
                row["detail"] = json!(detail);
            }
            card_from(json!({
                "v": 1, "kind": "node", "id": id, "revision": 0,
                "parent": "run-1:parent", "status": status, "header": {},
                "blocks": [{ "type": "steps", "steps": [row] }], "actions": []
            }))
        };
        let sibling = |_id: &str, card: ThreadCard| SiblingCard {
            card,
            live_tail: Vec::new(),
        };
        let siblings = vec![
            sibling("run-1:classify", node("classify", "in_progress", Some("glm"), None)),
            sibling("run-1:plan", node("plan", "in_progress", Some("sonnet"), None)),
            // classify's end card arrives after plan started: it must update
            // row 0, not append, and must keep the lane only the start named.
            sibling("run-1:classify", node("classify", "pass", None, Some("done · pass"))),
        ];
        let rows = fold_steps(&run, &siblings);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "classify");
        assert_eq!(rows[0].status, "pass");
        assert_eq!(rows[0].lane.as_deref(), Some("glm"));
        assert_eq!(rows[0].detail.as_deref(), Some("done · pass"));
        assert_eq!(rows[1].id, "plan");
        assert_eq!(rows[1].status, "in_progress");
    }

    #[test]
    fn fold_steps_keeps_the_run_cards_own_rows_first_and_ignores_older_revisions() {
        let run = card_from(json!({
            "v": 1, "kind": "run", "id": "run-1:parent", "revision": 3,
            "status": "completed", "header": {},
            "blocks": [{ "type": "steps", "steps": [
                { "id": "implementer", "label": "implementer", "status": "completed", "lane": "sonnet" }
            ] }],
            "actions": []
        }));
        let stale = card_from(json!({
            "v": 1, "kind": "node", "id": "implementer", "revision": 1,
            "parent": "run-1:parent", "status": "in_progress", "header": {},
            "blocks": [{ "type": "steps", "steps": [
                { "id": "implementer", "label": "implementer", "status": "in_progress" }
            ] }], "actions": []
        }));
        let other_kind = card_from(json!({
            "v": 1, "kind": "failure", "id": "run-1:failure", "revision": 9,
            "parent": "run-1:parent", "status": "failed", "header": {},
            "blocks": [{ "type": "steps", "steps": [
                { "id": "verifier", "label": "verifier", "status": "failed" }
            ] }], "actions": []
        }));
        let sibling = |_id: &str, card: ThreadCard| SiblingCard {
            card,
            live_tail: Vec::new(),
        };
        let rows = fold_steps(
            &run,
            &[
                sibling("run-1:implementer", stale),
                sibling("run-1:failure", other_kind),
            ],
        );
        // The stale revision does not downgrade the completed row, and a
        // failure card's quoted row is not a node of the run.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "completed");
        assert_eq!(rows[0].lane.as_deref(), Some("sonnet"));
    }

    #[test]
    fn tail_lines_keeps_the_last_four_non_empty_lines_clipped() {
        let chunks = ["Read src/a.ts\n\n", "Edit src/b.ts  +9 −4\n$ npm test\n", "  ✓ 14 passed\n\n"];
        assert_eq!(
            tail_lines(chunks.iter().copied()),
            vec!["Read src/a.ts", "Edit src/b.ts  +9 −4", "$ npm test", "  ✓ 14 passed"]
        );
        let five = "1\n2\n3\n4\n5\n";
        assert_eq!(tail_lines([five].into_iter()), vec!["2", "3", "4", "5"]);
        let long = "x".repeat(200);
        let clipped = tail_lines([long.as_str()].into_iter());
        assert_eq!(clipped.len(), 1);
        assert_eq!(clipped[0].chars().count(), 161);
        assert!(clipped[0].ends_with('…'));
        assert!(tail_lines(["\n  \n"].into_iter()).is_empty());
    }

    #[test]
    fn live_tails_follow_the_working_nodes_row_key() {
        let node = card_from(json!({
            "v": 1, "kind": "node", "id": "implementer", "revision": 0,
            "parent": "run-1:parent", "status": "in_progress", "header": {},
            "blocks": [{ "type": "steps", "steps": [
                { "id": "implementer", "label": "implementer", "status": "in_progress" }
            ] }], "actions": []
        }));
        let siblings = vec![SiblingCard {
            card: node,
            live_tail: vec!["Edit src/b.ts".to_string()],
        }];
        let tails = live_tails(&siblings);
        assert_eq!(tails.get("implementer").map(Vec::len), Some(1));
        assert!(tails.get("plan").is_none());
    }

    #[test]
    fn command_text_adds_the_slash_once() {
        assert_eq!(command_text("recover run-1"), "/recover run-1");
        assert_eq!(command_text("/status run-1"), "/status run-1");
        assert_eq!(command_text("  certify  "), "/certify");
    }

    #[test]
    fn step_state_reads_every_vocabulary_the_engine_uses() {
        for done in ["completed", "done", "pass", "PASS", "approve", "published", "ok"] {
            assert_eq!(step_state(done), StepState::Done, "{done}");
        }
        for failed in ["failed", "REFUTED", "error", "cancelled", "refused", "killed"] {
            assert_eq!(step_state(failed), StepState::Failed, "{failed}");
        }
        for working in ["in_progress", "running", "executing", "reviewing"] {
            assert_eq!(step_state(working), StepState::Working, "{working}");
        }
        for pending in ["pending", "queued"] {
            assert_eq!(step_state(pending), StepState::Pending, "{pending}");
        }
        assert_eq!(step_state("needs_change"), StepState::Other);
        assert_eq!(step_state(""), StepState::Other);
    }

    #[test]
    fn lane_color_matches_families_loosely_and_leaves_shell_unmapped() {
        assert_eq!(lane_color("opus"), lane_color("claude-opus-4"));
        assert_eq!(lane_color("GLM"), lane_color("glm-5.3"));
        assert_ne!(lane_color("kimi"), lane_color("codex"));
        assert!(lane_color("shell").is_none());
        assert!(lane_color("").is_none());
    }

    #[test]
    fn humanize_duration_reads_the_engines_millisecond_rows() {
        assert_eq!(humanize_duration("392123 ms"), "6m 32s");
        assert_eq!(humanize_duration("392123"), "6m 32s");
        assert_eq!(humanize_duration("4500 ms"), "4s");
        assert_eq!(humanize_duration("3725 s"), "1h 02m");
        assert_eq!(humanize_duration("6m 32s"), "6m 32s");
        assert_eq!(humanize_duration(""), "");
    }

    #[test]
    fn run_facts_lift_header_rows_and_leave_the_rest() {
        let card = card_from(json!({
            "v": 1, "kind": "run", "id": "run-1:parent", "revision": 0,
            "status": "completed", "header": {},
            "blocks": [{ "type": "kv", "rows": [
                { "k": "workspace", "v": "worktree" },
                { "k": "branch", "v": "mo/run-a41c" },
                { "k": "cost", "v": "$0.42" },
                { "k": "duration", "v": "392123 ms" },
                { "k": "Where it landed", "v": "src/theme/tokens.ts" }
            ] }],
            "actions": []
        }));
        let (facts, leftover) = RunFacts::extract(&card);
        assert_eq!(facts.workspace_text().as_deref(), Some("worktree mo/run-a41c"));
        assert_eq!(facts.cost_text().as_deref(), Some("$0.42 · 6m 32s"));
        assert_eq!(leftover.len(), 1);
        assert_eq!(leftover[0].k, "Where it landed");
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
        assert_eq!(card_mark(&card), Some(("✋", Color::Warning)));
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
        assert_eq!(card_mark(&card), Some(("✓", Color::Success)));
        assert_eq!(fold_steps(&card, &[]).len(), 2);
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
        assert_eq!(card_mark(&card), Some(("✗", Color::Error)));
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

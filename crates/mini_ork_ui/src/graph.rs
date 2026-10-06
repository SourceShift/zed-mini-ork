//! A run's graph, full screen (the design's "Run graph · full screen"): every
//! node on a dotted canvas with its edges, a Graph/List switch, a filter, zoom
//! and fit — and, for the selected node, an inspector whose Stream tab shows
//! the agent's session the way Claude Code's terminal does: replayable when the
//! node is done, followed live while it works, with steering.
//!
//! Node data comes from `mini-ork board node <run> <node> --view …`.

use std::{collections::HashMap, sync::Arc, time::Duration};

use gpui::{
    AnimationExt as _, AnyElement, App, Bounds, ClickEvent, ClipboardItem, Context, DismissEvent, EventEmitter,
    FocusHandle, Focusable, KeyDownEvent, PathBuilder, Pixels, Point, Render, ScrollHandle,
    SharedString, Subscription, Task, WeakEntity, Window, canvas, fill, point, px, size,
};
use serde_json::Value;
use ui::{CommonAnimationExt as _, prelude::*};
use ui_input::{ERASED_EDITOR_FACTORY, ErasedEditor, ErasedEditorEvent};
use workspace::Workspace;

use crate::{
    board,
    page::{MiniOrkPage, arr, str_of},
    palette::{self as p, c},
    store::{self, BoardStore, Origin},
};

const NW: f32 = 176.;
const NH: f32 = 76.;
const GX: f32 = 74.;
const GY: f32 = 18.;
const STREAM_POLL: Duration = Duration::from_millis(1500);
const TABS: [(&str, &str); 5] = [
    ("stream", "Stream"),
    ("output", "Output"),
    ("prompt", "Prompt"),
    ("telemetry", "Telemetry"),
    ("learning", "Learning"),
];

#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub lane: String,
    pub state: String,
    pub role: String,
    pub cost: String,
    pub dur: String,
    col: usize,
    row: usize,
    len: usize,
}

/// The graph as the run page's DAG section describes it.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub run_id: String,
    pub recipe: String,
    pub title: String,
    pub state: String,
    pub heads: Vec<String>,
    pub cols: Vec<Vec<Node>>,
}

impl Model {
    pub fn from_page(page: &Value, run_id: &str) -> Option<Self> {
        let dag = arr(page, "sections").into_iter().find(|s| str_of(s, "type") == "dag")?;
        let mut cols: Vec<Vec<Node>> = Vec::new();
        for (ci, column) in arr(&dag, "cols").iter().enumerate() {
            let nodes = column.as_array().cloned().unwrap_or_default();
            let len = nodes.len();
            cols.push(
                nodes
                    .iter()
                    .enumerate()
                    .map(|(row, n)| Node {
                        id: str_of(n, "id"),
                        label: str_of(n, "label"),
                        lane: str_of(n, "lane"),
                        state: str_of(n, "state"),
                        role: str_of(n, "role"),
                        cost: str_of(n, "cost"),
                        dur: str_of(n, "dur"),
                        col: ci,
                        row,
                        len,
                    })
                    .collect(),
            );
        }
        let heads = {
            let given: Vec<String> = arr(&dag, "heads").iter().filter_map(|h| h.as_str().map(String::from)).collect();
            if given.len() == cols.len() {
                given
            } else {
                (0..cols.len())
                    .map(|i| {
                        cols[i]
                            .first()
                            .map(|n: &Node| if n.role.is_empty() { format!("stage {}", i + 1) } else { n.role.clone() })
                            .unwrap_or_default()
                    })
                    .collect()
            }
        };
        let sub = str_of(page, "sub");
        let mut parts = sub.split(" · ");
        let recipe = str_of(&dag, "recipe");
        let recipe = if recipe.is_empty() { parts.next().unwrap_or_default().to_string() } else { recipe };
        let title = str_of(&dag, "run_title");
        let title = if title.is_empty() { parts.next().unwrap_or_default().to_string() } else { title };
        let state = arr(page, "chips").first().map(|c| str_of(c, "t")).unwrap_or_default();
        Some(Self { run_id: run_id.to_string(), recipe, title, state, heads, cols })
    }

    fn flat(&self) -> Vec<&Node> {
        self.cols.iter().flatten().collect()
    }

    fn max_rows(&self) -> usize {
        self.cols.iter().map(Vec::len).max().unwrap_or(1).max(1)
    }

    fn width(&self) -> f32 {
        (self.cols.len() as f32 * (NW + GX) - GX).max(NW)
    }

    fn height(&self) -> f32 {
        26. + self.max_rows() as f32 * (NH + GY) - GY
    }

    fn pos(&self, n: &Node) -> (f32, f32) {
        let x = n.col as f32 * (NW + GX);
        let y = 26. + (self.max_rows() - n.len) as f32 * (NH + GY) / 2. + n.row as f32 * (NH + GY);
        (x, y)
    }
}

fn mark(state: &str) -> (&'static str, u32) {
    match state {
        "done" => ("✓", p::GREEN),
        "running" => ("●", p::BLUE),
        "failed" => ("✗", p::RED),
        "skipped" => ("–", p::DIM),
        _ => ("○", p::DIM),
    }
}

fn state_text(state: &str) -> &str {
    match state {
        "running" => "working",
        "" => "pending",
        other => other,
    }
}

pub struct MiniOrkGraph {
    workspace: WeakEntity<Workspace>,
    page: WeakEntity<MiniOrkPage>,
    focus_handle: FocusHandle,
    model: Model,
    list_view: bool,
    zoom: f32,
    filter: Arc<dyn ErasedEditor>,
    query: String,
    canvas_scroll: ScrollHandle,
    selected: Option<String>,
    tab: String,
    /// Detail per (node, tab), so switching back is instant.
    detail: HashMap<(String, String), Value>,
    detail_error: Option<String>,
    loading: bool,
    stream: Vec<Value>,
    stream_offset: u64,
    stream_scroll: ScrollHandle,
    /// Replay: how many stream entries are shown (None = all).
    replay: Option<usize>,
    steer: Arc<dyn ErasedEditor>,
    severity: &'static str,
    _load: Task<()>,
    _replay: Task<()>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<DismissEvent> for MiniOrkGraph {}

impl Focusable for MiniOrkGraph {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl MiniOrkGraph {
    pub fn new(
        model: Model,
        selected: Option<String>,
        workspace: WeakEntity<Workspace>,
        page: WeakEntity<MiniOrkPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let factory = ERASED_EDITOR_FACTORY.get().expect("editor factory is set at start-up");
        let filter = factory(window, cx);
        filter.set_placeholder_text("Filter nodes, lanes, status…", window, cx);
        let steer = factory(window, cx);
        steer.set_placeholder_text("Steer this agent… (Enter to send)", window, cx);
        let filter_sub = filter.subscribe(
            Box::new({
                let this = cx.entity().downgrade();
                let editor = filter.clone();
                move |event, _window, cx| {
                    if event == ErasedEditorEvent::BufferEdited {
                        let text = editor.text(cx);
                        this.update(cx, |this, cx| {
                            this.query = text.trim().to_lowercase();
                            cx.notify();
                        })
                        .ok();
                    }
                }
            }),
            window,
            cx,
        );
        let mut this = Self {
            workspace,
            page,
            focus_handle: cx.focus_handle(),
            model,
            list_view: false,
            zoom: 1.0,
            filter,
            query: String::new(),
            canvas_scroll: ScrollHandle::new(),
            selected: None,
            tab: "stream".into(),
            detail: HashMap::new(),
            detail_error: None,
            loading: false,
            stream: Vec::new(),
            stream_offset: 0,
            stream_scroll: ScrollHandle::new(),
            replay: None,
            steer,
            severity: "info",
            _load: Task::ready(()),
            _replay: Task::ready(()),
            _subs: vec![filter_sub],
        };
        if let Some(node) = selected {
            this.select(node, cx);
        }
        this
    }

    /// The page re-read the run: node states, costs and durations move on.
    pub fn set_model(&mut self, model: Model, cx: &mut Context<Self>) {
        self.model = model;
        cx.notify();
    }

    fn node(&self, id: &str) -> Option<&Node> {
        self.model.flat().into_iter().find(|n| n.id == id)
    }

    fn matches(&self, n: &Node) -> bool {
        self.query.is_empty()
            || [n.label.as_str(), n.lane.as_str(), n.state.as_str(), n.role.as_str()]
                .join(" ")
                .to_lowercase()
                .contains(&self.query)
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn select(&mut self, node: String, cx: &mut Context<Self>) {
        if self.selected.as_deref() != Some(node.as_str()) {
            self.stream.clear();
            self.stream_offset = 0;
            self.replay = None;
        }
        self.selected = Some(node);
        self.load_detail(cx);
        cx.notify();
    }

    fn set_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        self.tab = tab.to_string();
        self.replay = None;
        self.load_detail(cx);
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let flat: Vec<String> = self.model.flat().iter().map(|n| n.id.clone()).collect();
        if flat.is_empty() {
            return;
        }
        let ix = self
            .selected
            .as_ref()
            .and_then(|s| flat.iter().position(|id| id == s))
            .unwrap_or(0) as isize;
        let next = (ix + delta).rem_euclid(flat.len() as isize) as usize;
        self.select(flat[next].clone(), cx);
    }

    fn target(&self, cx: &App) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        let workspace = self.workspace.upgrade()?;
        let store = BoardStore::for_workspace(&workspace, cx)?;
        let store = store.read(cx);
        Some((store.bin.clone(), store.home.clone()?))
    }

    /// Fetch the selected node's tab; a live stream keeps following by offset.
    fn load_detail(&mut self, cx: &mut Context<Self>) {
        let Some(node) = self.selected.clone() else {
            return;
        };
        let Some((bin, home)) = self.target(cx) else {
            return;
        };
        let (run, tab) = (self.model.run_id.clone(), self.tab.clone());
        self.loading = true;
        self.detail_error = None;
        self._load = cx.spawn(async move |this, cx| {
            loop {
                let offset = this
                    .update(cx, |this, _| (tab == "stream" && this.stream_offset > 0).then_some(this.stream_offset))
                    .ok()
                    .flatten();
                let result = cx
                    .background_spawn(board::fetch_node(
                        bin.clone(),
                        home.clone(),
                        run.clone(),
                        node.clone(),
                        tab.clone(),
                        offset,
                    ))
                    .await;
                let live = this
                    .update(cx, |this, cx| {
                        if this.selected.as_deref() != Some(node.as_str()) || this.tab != tab {
                            return false;
                        }
                        this.loading = false;
                        let value = match result {
                            Ok(value) => value,
                            Err(error) => {
                                this.detail_error = Some(error.to_string());
                                cx.notify();
                                return false;
                            }
                        };
                        let live = value.get("live").and_then(Value::as_bool).unwrap_or(false);
                        if tab == "stream" {
                            let entries = arr(&value, "entries");
                            let grew = !entries.is_empty();
                            if offset.is_some() {
                                this.stream.extend(entries);
                            } else {
                                this.stream = entries;
                            }
                            this.stream_offset = value.get("offset").and_then(Value::as_u64).unwrap_or(this.stream_offset);
                            if grew && live {
                                this.stream_scroll.scroll_to_bottom();
                            }
                        }
                        this.detail.insert((node.clone(), tab.clone()), value);
                        cx.notify();
                        live && tab == "stream"
                    })
                    .unwrap_or(false);
                if !live {
                    break;
                }
                cx.background_executor().timer(STREAM_POLL).await;
            }
        });
    }

    fn start_replay(&mut self, cx: &mut Context<Self>) {
        self.replay = Some(1);
        let total = self.stream.len();
        self._replay = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(110)).await;
                let more = this
                    .update(cx, |this, cx| match this.replay {
                        Some(n) if n < total => {
                            this.replay = Some(n + 1);
                            this.stream_scroll.scroll_to_bottom();
                            cx.notify();
                            true
                        }
                        _ => {
                            this.replay = None;
                            cx.notify();
                            false
                        }
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        });
        cx.notify();
    }

    fn send_steer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.steer.text(cx).trim().to_string();
        let Some(node) = self.selected.as_ref().and_then(|id| self.node(id)).cloned() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.steer.clear(window, cx);
        let role = if node.role.is_empty() { "any".to_string() } else { node.role.clone() };
        let args = vec![
            "board".to_string(),
            "steer".into(),
            self.model.run_id.clone(),
            "--role".into(),
            role,
            "--severity".into(),
            self.severity.into(),
            "--text".into(),
            text,
        ];
        if let Some(workspace) = self.workspace.upgrade() {
            let page = self.page.clone();
            window.defer(cx, move |_, cx| {
                store::run_cli(args, true, &workspace, Origin::Page(page), cx);
            });
        }
    }

    fn act(&self, action: Value) -> Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static> {
        let (workspace, page) = (self.workspace.clone(), self.page.clone());
        Box::new(move |_, window, cx| {
            let (action, workspace, page) = (action.clone(), workspace.clone(), page.clone());
            window.defer(cx, move |window, cx| {
                if let Some(workspace) = workspace.upgrade() {
                    store::perform(&action, &workspace, Origin::Page(page), window, cx);
                }
            });
        })
    }

    // ── header ────────────────────────────────────────────────────────────

    fn render_header(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let flat = self.model.flat();
        let count = |state: &str| flat.iter().filter(|n| n.state == state).count();
        let counts = [
            (count("done"), "done", p::GREEN),
            (count("running"), "working", p::BLUE),
            (flat.iter().filter(|n| n.state.is_empty() || n.state == "pending").count(), "pending", p::SUB),
            (count("failed"), "failed", p::RED),
        ];
        let (m, mc) = mark(if self.model.state.contains("running") { "running" } else if self.model.state.contains("fail") { "failed" } else { "done" });
        let mono = p::mono_font(cx);
        let small = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .border_1()
                .border_color(c(p::BORDER2))
                .text_color(c(p::TEXT))
                .cursor_pointer()
                .hover(|s| s.bg(c(p::HOVER)))
                .child(label)
        };
        h_flex()
            .flex_none()
            .h(px(46.))
            .gap(px(10.))
            .pl(px(16.))
            .pr(px(12.))
            .bg(c(p::CHROME))
            .border_b_1()
            .border_color(c(p::BORDER))
            .whitespace_nowrap()
            .child(p::text(m, 13., mc))
            .child(div().text_size(px(13.)).font(mono.clone()).text_color(c(p::TEXT)).child(self.model.run_id.clone()))
            .when(!self.model.recipe.is_empty(), |this| {
                this.child(
                    div()
                        .px(px(6.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(c(p::BORDER2))
                        .text_size(px(11.))
                        .font(mono.clone())
                        .text_color(c(p::MUTED))
                        .child(self.model.recipe.clone()),
                )
            })
            .child(p::text(self.model.title.clone(), 12.5, p::MUTED).max_w(px(260.)).truncate())
            .children(
                counts
                    .into_iter()
                    .filter(|(n, _, _)| *n > 0)
                    .map(|(n, label, colour)| p::text(format!("{n} {label}"), 11.5, colour)),
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(c(p::BORDER2))
                    .overflow_hidden()
                    .children([(false, "Graph"), (true, "List")].into_iter().map(|(list, label)| {
                        let on = self.list_view == list;
                        div()
                            .id(SharedString::from(format!("graph-view-{label}")))
                            .h(px(26.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .when(on, |this| this.bg(c(p::SELECTED)))
                            .text_size(px(12.))
                            .text_color(c(if on { p::TEXT } else { p::MUTED }))
                            .cursor_pointer()
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.list_view = list;
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .w(px(190.))
                    .h(px(28.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .border_1()
                    .border_color(c(p::BORDER2))
                    .bg(c(p::BG))
                    .text_size(px(12.5))
                    .child(self.filter.render(window, cx)),
            )
            .when(!self.list_view, |this| {
                this.child(
                    h_flex()
                        .gap(px(2.))
                        .child(small("graph-zoom-out", "−").on_click(cx.listener(|this, _, _, cx| {
                            this.zoom = (this.zoom - 0.1).max(0.4);
                            cx.notify();
                        })))
                        .child(
                            div()
                                .w(px(46.))
                                .text_center()
                                .text_size(px(11.5))
                                .font(mono.clone())
                                .text_color(c(p::MUTED))
                                .child(format!("{}%", (self.zoom * 100.).round() as i32)),
                        )
                        .child(small("graph-zoom-in", "+").on_click(cx.listener(|this, _, _, cx| {
                            this.zoom = (this.zoom + 0.1).min(1.8);
                            cx.notify();
                        })))
                        .child(
                            div()
                                .id("graph-fit")
                                .ml(px(4.))
                                .h(px(26.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .border_1()
                                .border_color(c(p::BORDER2))
                                .text_size(px(12.))
                                .text_color(c(p::TEXT))
                                .cursor_pointer()
                                .hover(|s| s.bg(c(p::HOVER)))
                                .child("Fit")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let view = this.canvas_scroll.bounds().size;
                                    let (w, h) = (this.model.width(), this.model.height());
                                    if f32::from(view.width) > 0. {
                                        let z = ((f32::from(view.width) - 56.) / w).min((f32::from(view.height) - 56.) / h);
                                        this.zoom = (z.clamp(0.4, 1.4) * 100.).round() / 100.;
                                        cx.notify();
                                    }
                                })),
                        ),
                )
            })
            .child(
                div()
                    .id("graph-close")
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .border_1()
                    .border_color(c(p::BORDER2))
                    .bg(c(p::HOVER))
                    .text_size(px(12.5))
                    .text_color(c(p::TEXT))
                    .cursor_pointer()
                    .child("✕ Close")
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
            .into_any_element()
    }
}


impl MiniOrkGraph {
    // ── canvas ────────────────────────────────────────────────────────────

    fn render_graph(&self, cx: &mut Context<Self>) -> AnyElement {
        let z = self.zoom;
        let model = self.model.clone();
        let (w, h) = (model.width() * z, model.height() * z);
        let selected = self.selected.clone();
        let mono = p::mono_font(cx);

        // Edges: every node of one column to every node of the next, as in the
        // design; the selected node's edges are blue, edges into pending nodes dim.
        let edge_model = model.clone();
        let edge_query = self.query.clone();
        let edges = canvas(
            |_, _, _| (),
            move |bounds: Bounds<Pixels>, _, window, _| {
                let origin = bounds.origin;
                let at = |x: f32, y: f32| -> Point<Pixels> { point(origin.x + px(x * z), origin.y + px(y * z)) };
                let matches = |n: &Node| {
                    edge_query.is_empty()
                        || [n.label.as_str(), n.lane.as_str(), n.state.as_str(), n.role.as_str()]
                            .join(" ")
                            .to_lowercase()
                            .contains(&edge_query)
                };
                for ci in 0..edge_model.cols.len().saturating_sub(1) {
                    for a in &edge_model.cols[ci] {
                        for b in &edge_model.cols[ci + 1] {
                            let (ax, ay) = edge_model.pos(a);
                            let (bx, by) = edge_model.pos(b);
                            let (x1, y1, x2, y2) = (ax + NW, ay + NH / 2., bx, by + NH / 2.);
                            let hot = selected.as_deref().is_some_and(|s| s == a.id || s == b.id);
                            let mut colour = if hot {
                                c(p::BLUE)
                            } else if b.state.is_empty() || b.state == "pending" {
                                c(p::BORDER2)
                            } else {
                                c(p::DIM)
                            };
                            if !(matches(a) && matches(b)) {
                                colour = colour.opacity(0.25);
                            }
                            let mut path = PathBuilder::stroke(px(if hot { 2. } else { 1.3 } * z.max(0.6)));
                            path.move_to(at(x1, y1));
                            path.cubic_bezier_to(at(x2, y2), at(x1 + 36., y1), at(x2 - 36., y2));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, colour);
                            }
                        }
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        let heads = model.heads.iter().enumerate().map(|(i, head)| {
            div()
                .absolute()
                .left(px(i as f32 * (NW + GX) * z))
                .top_0()
                .w(px(NW * z))
                .text_size(px(10.5 * z))
                .text_color(c(p::DIM))
                .child(head.to_uppercase())
                .into_any_element()
        });

        let nodes = model.flat().into_iter().cloned().collect::<Vec<_>>().into_iter().map(|n| {
            let (x, y) = model.pos(&n);
            let on = self.selected.as_deref() == Some(n.id.as_str());
            let (m, mc) = mark(&n.state);
            let border = if on {
                c(p::BLUE)
            } else if n.state == "running" {
                p::ca(0x74ade88c)
            } else if n.state == "failed" {
                p::ca(0xd0727799)
            } else {
                c(p::BORDER2)
            };
            let pending = n.state.is_empty() || n.state == "pending";
            let id = n.id.clone();
            div()
                .id(SharedString::from(format!("graph-node-{}", n.id)))
                .absolute()
                .left(px(x * z))
                .top(px(y * z))
                .w(px(NW * z))
                .h(px(NH * z))
                .px(px(10. * z))
                .py(px(8. * z))
                .flex()
                .flex_col()
                .gap(px(3. * z))
                .rounded(px(8.))
                .border(px(if on { 2. } else { 1. }))
                .border_color(border)
                .bg(c(if pending { 0x2a2e36 } else { 0x323843 }))
                .when(!self.matches(&n), |this| this.opacity(0.28))
                .cursor_pointer()
                .hover(|s| s.border_color(c(p::BLUE)))
                .child(
                    h_flex()
                        .gap(px(6. * z))
                        .min_w_0()
                        .child(p::text(m, 10. * z, mc))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.5 * z))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(c(p::TEXT))
                                .child(n.label.clone()),
                        ),
                )
                .child(
                    h_flex()
                        .gap(px(5. * z))
                        .min_w_0()
                        .child(div().size(px(6. * z)).rounded_full().bg(c(p::family(&n.lane))))
                        .child(
                            p::text(
                                if n.role.is_empty() { n.lane.clone() } else { format!("{} · {}", n.lane, n.role) },
                                11. * z,
                                p::MUTED,
                            )
                            .truncate(),
                        ),
                )
                .child(
                    h_flex()
                        .gap(px(8. * z))
                        .font(mono.clone())
                        .text_size(px(11. * z))
                        .child(div().text_color(c(mc)).child(state_text(&n.state).to_string()))
                        .child(div().text_color(c(p::SUB)).child(n.cost.clone()))
                        .child(div().text_color(c(p::SUB)).child(n.dur.clone())),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
                .into_any_element()
        });

        let dots = canvas(
            |_, _, _| (),
            |bounds: Bounds<Pixels>, _, window, _| {
                let colour = c(0x353b45);
                let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let mut y = 9.;
                while y < bh {
                    let mut x = 9.;
                    while x < bw {
                        window.paint_quad(
                            fill(
                                Bounds::new(point(bounds.origin.x + px(x), bounds.origin.y + px(y)), size(px(2.), px(2.))),
                                colour,
                            )
                            .corner_radii(px(1.)),
                        );
                        x += 18.;
                    }
                    y += 18.;
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(c(0x262a31))
            .child(dots)
            .child(
                div()
                    .id("graph-canvas")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.canvas_scroll)
                    .child(
                        div().p(px(28.)).child(
                            div()
                                .relative()
                                .w(px(w))
                                .h(px(h))
                                .child(edges)
                                .children(heads)
                                .children(nodes),
                        ),
                    ),
            )
            .when(self.selected.is_none(), |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(16.))
                        .bottom(px(16.))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(c(p::BORDER))
                        .bg(c(p::CHROME))
                        .child(p::text("Click a node to inspect it", 12., p::SUB)),
                )
            })
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let mono = p::mono_font(cx);
        let stages = self.model.cols.iter().enumerate().map(|(ci, col)| {
            let head = self.model.heads.get(ci).cloned().unwrap_or_default();
            v_flex()
                .gap(px(4.))
                .child(p::text(format!("{} · {}", ci + 1, head).to_uppercase(), 10.5, p::DIM))
                .children(col.iter().map(|n| {
                    let on = self.selected.as_deref() == Some(n.id.as_str());
                    let (m, mc) = mark(&n.state);
                    let pending = n.state.is_empty() || n.state == "pending";
                    let id = n.id.clone();
                    h_flex()
                        .id(SharedString::from(format!("graph-row-{}", n.id)))
                        .gap(px(10.))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(if on { c(p::BLUE) } else { c(p::BORDER2) })
                        .bg(c(if pending { 0x2a2e36 } else { 0x323843 }))
                        .when(!self.matches(n), |this| this.opacity(0.28))
                        .cursor_pointer()
                        .hover(|s| s.border_color(c(p::BLUE)))
                        .text_size(px(12.5))
                        .child(p::text(m, 11., mc).w(px(16.)).flex_none())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(c(p::TEXT))
                                .child(n.label.clone()),
                        )
                        .child(
                            h_flex()
                                .w(px(120.))
                                .flex_none()
                                .gap(px(5.))
                                .child(div().size(px(6.)).rounded_full().bg(c(p::family(&n.lane))))
                                .child(p::text(n.lane.clone(), 11.5, p::MUTED).truncate()),
                        )
                        .child(p::text(state_text(&n.state).to_string(), 11.5, mc).w(px(80.)).flex_none())
                        .child(p::text(n.cost.clone(), 11.5, p::SUB).font(mono.clone()).w(px(60.)).flex_none())
                        .child(p::text(n.dur.clone(), 11.5, p::SUB).font(mono.clone()).w(px(60.)).flex_none())
                        .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
                }))
        });
        div()
            .id("graph-list")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .bg(c(0x262a31))
            .child(
                v_flex()
                    .max_w(px(880.))
                    .mx_auto()
                    .px(px(24.))
                    .pt(px(20.))
                    .pb(px(60.))
                    .gap(px(14.))
                    .children(stages),
            )
            .into_any_element()
    }

    // ── inspector ─────────────────────────────────────────────────────────

    fn render_inspector(&self, node: &Node, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let detail = self.detail.get(&(node.id.clone(), self.tab.clone())).cloned();
        let live = detail
            .as_ref()
            .and_then(|d| d.get("live").and_then(Value::as_bool))
            .unwrap_or(false);
        let (m, mc) = mark(&node.state);
        let stage_count = self.model.cols.len();
        let head = self.model.heads.get(node.col).cloned().unwrap_or_default();
        let is_stream = self.tab == "stream";
        let icon_button = |id: &'static str, glyph: &'static str| {
            div()
                .id(id)
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .text_color(c(p::MUTED))
                .cursor_pointer()
                .hover(|s| s.bg(c(p::HOVER)))
                .child(glyph)
        };
        let tabs = h_flex().gap(px(2.)).border_b_1().border_color(c(p::BORDER)).children(TABS.iter().map(|(key, label)| {
            let on = self.tab == *key;
            let label = if *key == "stream" && live && node.state == "running" { "Stream ●".to_string() } else { label.to_string() };
            let key = key.to_string();
            div()
                .id(SharedString::from(format!("node-tab-{key}")))
                .px(px(9.))
                .py(px(6.))
                .text_size(px(12.5))
                .text_color(c(if on { p::TEXT } else { p::SUB }))
                .border_b_2()
                .border_color(if on { c(p::BLUE) } else { p::ca(0x00000000) })
                .cursor_pointer()
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.set_tab(&key, cx)))
        }));
        let header = v_flex()
            .flex_none()
            .px(px(14.))
            .pt(px(12.))
            .gap(px(8.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(p::text(m, 13., mc))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(c(p::TEXT))
                            .child(node.label.clone()),
                    )
                    .child(p::text(state_text(&node.state).to_string(), 12., mc))
                    .child(div().flex_1())
                    .child(icon_button("node-prev", "↑").on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))))
                    .child(icon_button("node-next", "↓").on_click(cx.listener(|this, _, _, cx| this.step(1, cx))))
                    .child(icon_button("node-close", "✕").on_click(cx.listener(|this, _, _, cx| {
                        this.selected = None;
                        cx.notify();
                    }))),
            )
            .child(p::text(
                format!("{} · stage {} of {} · {}", if node.role.is_empty() { "node" } else { &node.role }, node.col + 1, stage_count, head),
                12.,
                p::SUB,
            ))
            .child(tabs);

        let body = match (&detail, &self.detail_error) {
            (_, Some(error)) => div().p(px(14.)).child(p::text(error.clone(), 12.5, p::RED)).into_any_element(),
            (None, None) => div()
                .p(px(14.))
                .child(crate::loading::loading_line(format!("board node {} {} --view {}", self.model.run_id, node.id, self.tab), cx))
                .into_any_element(),
            (Some(detail), None) if is_stream => self.render_stream(node, detail, live, window, cx),
            (Some(detail), None) => self.render_facts(detail, cx),
        };

        let mut acts: Vec<AnyElement> = arr(detail.as_ref().unwrap_or(&Value::Null), "acts")
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let kind = str_of(b, "kind");
                let handler = b.get("do").filter(|d| !d.is_null()).map(|d| self.act(d.clone()));
                p::button(SharedString::from(format!("node-act-{i}")), str_of(b, "label"), if kind.is_empty() { "default" } else { &kind }, 26., 12.5, handler)
            })
            .collect();
        if is_stream && !live && !self.stream.is_empty() && node.state != "pending" {
            acts.insert(
                0,
                p::button(
                    "node-replay",
                    "▶ Replay stream",
                    "primary",
                    26.,
                    12.5,
                    Some(Box::new({
                        let this = cx.entity().downgrade();
                        move |_, _, cx| {
                            this.update(cx, |this, cx| this.start_replay(cx)).ok();
                        }
                    })),
                ),
            );
        }
        let copy_id = format!("{}/{}", self.model.run_id, node.id);
        acts.push(p::button(
            "node-copy",
            "Copy node id",
            "ghost",
            26.,
            12.5,
            Some(Box::new(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy_id.clone())))),
        ));

        v_flex()
            .flex_none()
            .w(px(if is_stream { 600. } else { 420. }))
            .h_full()
            .border_l_1()
            .border_color(c(p::BORDER))
            .bg(c(p::CHROME))
            .child(header)
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
            .child(
                h_flex()
                    .flex_none()
                    .flex_wrap()
                    .gap(px(6.))
                    .px(px(14.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(c(p::BORDER))
                    .children(acts),
            )
            .into_any_element()
    }

    fn render_facts(&self, detail: &Value, cx: &mut Context<Self>) -> AnyElement {
        let mono = p::mono_font(cx);
        let kv = arr(detail, "kv");
        let block = arr(detail, "block");
        let list = arr(detail, "list");
        div()
            .id("node-facts")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(14.))
            .py(px(12.))
            .child(
                v_flex()
                    .gap(px(12.))
                    .child(h_flex().flex_wrap().gap(px(8.)).children(kv.iter().map(|k| {
                        v_flex()
                            .w(px(186.))
                            .px(px(9.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .bg(c(p::BG))
                            .border_1()
                            .border_color(c(p::BORDER))
                            .child(p::text(str_of(k, "k"), 10.5, p::SUB))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font(mono.clone())
                                    .text_color(p::named(&str_of(k, "c")))
                                    .child(str_of(k, "v")),
                            )
                    })))
                    .when(!block.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .gap(px(4.))
                                .child(p::text(str_of(detail, "block_title").to_uppercase(), 11., p::SUB))
                                .child(crate::page::code_box(block.clone(), cx)),
                        )
                    })
                    .when(!list.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .gap(px(4.))
                                .child(p::text(str_of(detail, "list_title").to_uppercase(), 11., p::SUB))
                                .children(list.iter().map(|it| {
                                    let sub = str_of(it, "sub");
                                    h_flex()
                                        .items_start()
                                        .gap(px(8.))
                                        .py(px(4.))
                                        .border_b_1()
                                        .border_color(c(0x353a44))
                                        .child(p::text(str_of(it, "m"), 12.5, p::SUB).w(px(12.)).flex_none())
                                        .child(
                                            v_flex()
                                                .min_w_0()
                                                .child(p::text(str_of(it, "t"), 12.5, p::TEXT))
                                                .when(!sub.is_empty(), |this| this.child(p::text(sub, 11.5, p::SUB))),
                                        )
                                })),
                        )
                    }),
            )
            .into_any_element()
    }

    /// The agent's session, rendered the way Claude Code's terminal shows it.
    fn render_stream(&self, node: &Node, detail: &Value, live: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mono = p::mono_font(cx);
        let shown = match self.replay {
            Some(n) => &self.stream[..n.min(self.stream.len())],
            None => &self.stream[..],
        };
        let status = if let Some(n) = self.replay {
            format!("replaying {} / {}", n, self.stream.len())
        } else {
            str_of(detail, "status")
        };
        let status_colour = if self.replay.is_some() {
            p::BLUE
        } else {
            match str_of(detail, "status_c").as_str() {
                "green" => p::GREEN,
                "yellow" => p::YELLOW,
                _ => p::SUB,
            }
        };
        let entries = shown.iter().map(|e| stream_entry(e, &mono));
        let running = live && node.state == "running";
        let source = str_of(detail, "source");
        let header = h_flex()
            .flex_none()
            .gap(px(8.))
            .px(px(14.))
            .py(px(7.))
            .border_b_1()
            .border_color(c(p::ROW_RULE))
            .text_size(px(11.5))
            .font(mono.clone())
            .child(p::text("●", 11.5, status_colour))
            .child(p::text(status, 11.5, status_colour))
            .child(p::text(str_of(detail, "meta"), 11.5, p::DIM).truncate().min_w_0())
            .child(div().flex_1())
            .when(self.replay.is_some(), |this| {
                this.child(p::button(
                    "stream-skip",
                    "Skip to end",
                    "ghost",
                    22.,
                    11.5,
                    Some(Box::new({
                        let this = cx.entity().downgrade();
                        move |_, _, cx| {
                            this.update(cx, |this, cx| {
                                this.replay = None;
                                cx.notify();
                            })
                            .ok();
                        }
                    })),
                ))
            });
        let transcript = div()
            .id("node-stream")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.stream_scroll)
            .px(px(16.))
            .pt(px(14.))
            .pb(px(18.))
            .child(
                v_flex()
                    .gap(px(11.))
                    .font(mono.clone())
                    .text_size(px(12.5))
                    .line_height(px(20.))
                    .text_color(c(p::TEXT))
                    .children(entries)
                    .when(shown.is_empty(), |this| {
                        this.child(
                            div()
                                .py(px(30.))
                                .w_full()
                                .text_center()
                                .child(p::text(
                                    if node.state == "pending" { "Not dispatched yet." } else { "No stream yet." },
                                    12.5,
                                    p::DIM,
                                )),
                        )
                    })
                    .when(running && self.replay.is_none(), |this| {
                        this.child(
                            h_flex()
                                .gap(px(9.))
                                .text_color(c(p::ORANGE))
                                .child(
                                    Icon::new(IconName::LoadCircle)
                                        .size(IconSize::Small)
                                        .color(Color::Custom(c(p::ORANGE)))
                                        .with_rotate_animation(1),
                                )
                                .child("Working…")
                                .child(p::text(format!("· streaming from {source}"), 12.5, p::DIM)),
                        )
                    }),
            );
        let footer = if running {
            let sevs = ["info", "warn", "critical"].map(|sev| {
                let on = self.severity == sev;
                let colour = match (on, sev) {
                    (false, _) => p::SUB,
                    (true, "critical") => p::RED,
                    (true, "warn") => p::YELLOW,
                    _ => p::TEXT,
                };
                div()
                    .id(SharedString::from(format!("sev-{sev}")))
                    .h(px(22.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(c(if on { p::DIM } else { p::BORDER2 }))
                    .when(on, |this| this.bg(c(p::SELECTED)))
                    .text_size(px(11.))
                    .text_color(c(colour))
                    .cursor_pointer()
                    .child(sev)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.severity = sev;
                        cx.notify();
                    }))
            });
            v_flex()
                .flex_none()
                .px(px(14.))
                .pt(px(10.))
                .pb(px(12.))
                .gap(px(6.))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(c(p::DIM))
                        .bg(c(0x262a31))
                        .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "enter" && !event.keystroke.modifiers.shift {
                                this.send_steer(window, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(p::text(">", 12.5, p::MUTED).font(mono.clone()))
                        .child(div().flex_1().min_w_0().text_size(px(12.5)).child(self.steer.render(window, cx)))
                        .children(sevs),
                )
                .child(
                    h_flex()
                        .gap(px(12.))
                        .text_size(px(11.))
                        .font(mono.clone())
                        .text_color(c(p::DIM))
                        .child("⏵⏵ attached")
                        .child("lands as operator steering · consumed on its next turn")
                        .child("esc to detach"),
                )
                .into_any_element()
        } else {
            let note = str_of(detail, "done_note");
            div()
                .flex_none()
                .px(px(14.))
                .py(px(9.))
                .border_t_1()
                .border_color(c(p::ROW_RULE))
                .when(!note.is_empty(), |this| this.child(p::text(note, 12., p::SUB)))
                .into_any_element()
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .bg(c(0x1f2227))
            .border_t_1()
            .border_color(c(p::BORDER))
            .child(header)
            .child(transcript)
            .child(footer)
            .into_any_element()
    }
}

/// One transcript entry: `user`/`steer` boxes, `note` asides, and rows —
/// `⏺ Tool(args)` with `⎿` result lines, `✻ Thinking…`, text, todos.
fn stream_entry(e: &Value, mono: &gpui::Font) -> AnyElement {
    let kind = str_of(e, "k");
    let head = str_of(e, "head");
    let arg = str_of(e, "arg");
    let lines = arr(e, "lines");
    let line_el = |l: &Value| {
        let bg = match str_of(l, "bg").as_str() {
            "green-bg" => Some(p::ca(0xa1c1811f)),
            "red-bg" => Some(p::ca(0xd072771f)),
            _ => None,
        };
        let colour = str_of(l, "c");
        div()
            .px(px(6.))
            .rounded(px(2.))
            .text_size(px(12.))
            .whitespace_nowrap()
            .text_color(if colour.is_empty() { c(p::MUTED) } else { p::named(&colour) })
            .when_some(bg, |this, bg| this.bg(bg))
            .child(str_of(l, "t"))
    };
    match kind.as_str() {
        "user" | "steer" => {
            let steer = kind == "steer";
            div()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(6.))
                .border_1()
                .border_color(if steer { p::ca(0xdec18480) } else { c(p::BORDER2) })
                .bg(if steer { p::ca(0xdec18410) } else { c(0x262a31) })
                .child(
                    h_flex()
                        .items_start()
                        .gap(px(6.))
                        .child(p::text(">", 12.5, if steer { p::YELLOW } else { p::SUB }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_color(c(if steer { p::YELLOW } else { p::TEXT })).child(format!("{head}{arg}"))),
                        ),
                )
                .into_any_element()
        }
        "note" => div()
            .pl(px(20.))
            .text_size(px(11.5))
            .text_color(c(p::DIM))
            .child(format!("⎿  {arg}"))
            .into_any_element(),
        _ => {
            let (dot, dot_colour, italic) = match kind.as_str() {
                "think" => ("✻", p::PURPLE, true),
                "tool" => {
                    let failed = lines.iter().any(|l| str_of(l, "c") == "red");
                    ("⏺", if failed { p::RED } else { p::GREEN }, false)
                }
                "out" => ("", p::DIM, false),
                _ => ("⏺", p::TEXT, false),
            };
            let arg_text = if kind == "tool" && !arg.is_empty() { format!("({arg})") } else { arg };
            v_flex()
                .when(kind != "out", |this| {
                    this.child(
                        h_flex()
                            .items_start()
                            .gap(px(9.))
                            .child(div().flex_none().w(px(12.)).text_size(px(11.)).text_color(c(dot_colour)).child(dot))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .when(italic, |this| this.italic())
                                    .child(
                                        h_flex()
                                            .flex_wrap()
                                            .child(
                                                div()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(c(p::TEXT))
                                                    .child(head),
                                            )
                                            .child(div().text_color(c(p::MUTED)).child(arg_text)),
                                    ),
                            ),
                    )
                })
                .when(!lines.is_empty(), |this| {
                    this.child(
                        h_flex()
                            .items_start()
                            .gap(px(8.))
                            .mt(px(2.))
                            .pl(px(if kind == "out" { 22. } else { 0. }))
                            .child(div().flex_none().w(px(12.)).text_color(c(p::DIM)).child("⎿"))
                            .child(
                                div()
                                    .id(SharedString::from(format!("lines-{}", lines.len())))
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_x_scroll()
                                    .child(v_flex().font(mono.clone()).children(lines.iter().map(line_el))),
                            ),
                    )
                })
                .into_any_element()
        }
    }
}

impl Render for MiniOrkGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let selected = self.selected.as_ref().and_then(|id| self.node(id)).cloned();
        let header = self.render_header(window, cx);
        let canvas = if self.list_view { self.render_list(cx) } else { self.render_graph(cx) };
        let inspector = selected.map(|node| self.render_inspector(&node, window, cx));
        v_flex()
            .id("mini-ork-run-graph")
            .track_focus(&self.focus_handle)
            .key_context("MiniOrkRunGraph")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    if this.selected.is_some() {
                        this.selected = None;
                        cx.notify();
                    } else {
                        this.close(cx);
                    }
                }
            }))
            .w(viewport.width)
            .h(viewport.height)
            .bg(c(p::BG))
            .text_color(c(p::TEXT))
            .occlude()
            .child(header)
            .child(h_flex().flex_1().min_h_0().items_stretch().child(canvas).children(inspector))
            .with_animation(
                "graph-fade-in",
                gpui::Animation::new(Duration::from_millis(180)),
                |el, delta| el.opacity(delta),
            )
    }
}

impl MiniOrkGraph {
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    /// The overlay is already open: jump to another node's stream.
    pub fn open_node(&mut self, node: String, cx: &mut Context<Self>) {
        self.tab = "stream".into();
        self.select(node, cx);
    }
}

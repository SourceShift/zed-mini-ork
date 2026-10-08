//! A mini-ork page as an editor tab: one generic renderer for every
//! `mini-ork board page <key>` document (see `mini_ork/ide_pages/spec.py`).
//! A run's tab is the `run` page with `args.run`.

use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, Render,
    SharedString, Task, WeakEntity, Window, px,
};
use serde_json::Value;
use gpui::{
    Animation, AnimationExt as _, AnchoredPositionMode, DismissEvent, ScrollHandle, Subscription, anchored,
    deferred, point,
};
use ui::{ScrollAxes, Scrollbars, WithScrollbar as _, prelude::*};
use workspace::{
    Workspace,
    item::{Item, TabContentParams},
};

use crate::{
    board,
    loading::{self, Shape},
    select::{Look, Texts},
    palette::{self as p, c},
    store::{self, BoardStore, Origin},
};

/// The glyph each page shows in its tab and in the activity bar.
pub fn glyph(key: &str) -> &'static str {
    match key {
        "run" => "◇",
        "orch" => "✦",
        "runs" => "≡",
        "changes" => "⑂",
        "verify" => "✓",
        "recipes" => "▥",
        "autos" => "◷",
        "lanes" => "$",
        "learn" => "❖",
        "context" => "◫",
        "nodes" => "⬡",
        "setup" => "⚙",
        _ => "◇",
    }
}

/// The tab label before the page has loaded.
pub fn default_label(key: &str) -> &'static str {
    match key {
        "orch" => "Orchestrator",
        "runs" => "Runs",
        "changes" => "Changes",
        "verify" => "Verify & safety",
        "recipes" => "Recipes & epics",
        "autos" => "Automations",
        "lanes" => "Lanes & cost",
        "learn" => "Learning & memory",
        "context" => "Context",
        "nodes" => "Nodes",
        "setup" => "Setup & health",
        _ => "mini-ork",
    }
}

/// `run-1791302645-a41c` → `run-a41c`; `run-le-1791314676-64879-1` →
/// `run-64879-1`: whatever follows the epoch seconds, else the last part.
fn run_label(run_id: &str) -> String {
    let parts: Vec<&str> = run_id.split('-').collect();
    let epoch = parts
        .iter()
        .position(|part| part.len() >= 9 && part.chars().all(|c| c.is_ascii_digit()));
    match epoch {
        Some(i) if i + 1 < parts.len() => format!("run-{}", parts[i + 1..].join("-")),
        _ => match run_id.rsplit_once('-') {
            Some((_, tail)) if !tail.is_empty() => format!("run-{tail}"),
            _ => run_id.to_string(),
        },
    }
}

pub struct MiniOrkPage {
    pub(crate) key: String,
    pub(crate) args: BTreeMap<String, String>,
    tab: Option<String>,
    title_hint: String,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    data: Option<Result<Value, String>>,
    loading: bool,
    /// A user switched tab or filter and the new content is on its way: the
    /// content area shows a loader instead of the previous content.
    switching: bool,
    /// Every (tab, args) result seen this session: a revisited tab shows at
    /// once and refreshes under the sweep bar.
    cache: HashMap<String, Value>,
    /// The section shapes each tab last had, for its skeleton.
    shapes: HashMap<String, Vec<Shape>>,
    /// Bumped whenever fresh content replaces a skeleton (its fade-in key).
    fade: u64,
    /// The DAG card scrolls sideways, as in the design.
    dag_scroll: ScrollHandle,
    /// Selectable text for everything this page shows.
    pub(crate) texts: Texts,
    /// The run graph, full screen, when open.
    graph: Option<Entity<crate::graph::MiniOrkGraph>>,
    /// Open the graph (on this node, if any) once the page has loaded.
    pub(crate) pending_graph: Option<Option<String>>,
    _graph_sub: Option<Subscription>,
    /// Board reads this page has followed (it re-reads when the board moves).
    seen_board: u64,
    last_reload: std::time::Instant,
    _store_sub: Option<Subscription>,
    generation: u64,
    _load: Task<()>,
}

impl MiniOrkPage {
    fn new(
        key: String,
        tab: Option<String>,
        args: BTreeMap<String, String>,
        title_hint: String,
        workspace: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            key,
            args,
            tab,
            title_hint,
            workspace,
            focus_handle: cx.focus_handle(),
            data: None,
            loading: false,
            switching: false,
            cache: HashMap::new(),
            shapes: HashMap::new(),
            fade: 0,
            dag_scroll: ScrollHandle::new(),
            texts: Texts::default(),
            graph: None,
            pending_graph: None,
            _graph_sub: None,
            seen_board: 0,
            last_reload: std::time::Instant::now(),
            _store_sub: None,
            generation: 0,
            _load: Task::ready(()),
        };
        this.reload(cx);
        this
    }

    fn label(&self) -> String {
        if self.key == "run" {
            return run_label(self.args.get("run").map(String::as_str).unwrap_or("run"));
        }
        self.data
            .as_ref()
            .and_then(|d| d.as_ref().ok())
            .and_then(|d| d.get("label").and_then(Value::as_str))
            .map(String::from)
            .unwrap_or_else(|| default_label(&self.key).to_string())
    }

    pub fn set_args(&mut self, args: BTreeMap<String, String>, cx: &mut Context<Self>) {
        for (k, v) in args {
            if k == "tab" {
                self.tab = Some(v);
            } else if v.is_empty() {
                self.args.remove(&k);
            } else {
                self.args.insert(k, v);
            }
        }
        self.begin_switch();
        self.reload(cx);
    }

    fn cache_key(&self) -> String {
        format!("{}|{:?}", self.tab.clone().unwrap_or_default(), self.args)
    }

    /// A user asked for other content: show it from the cache when it has been
    /// seen, else a skeleton until it arrives.
    pub(crate) fn begin_switch(&mut self) {
        match self.cache.get(&self.cache_key()) {
            Some(cached) => {
                self.data = Some(Ok(cached.clone()));
                self.switching = false;
            }
            None => self.switching = true,
        }
    }

    fn source(&self) -> String {
        let mut source = format!("board page {}", self.key);
        if let Some(tab) = &self.tab {
            source.push_str(&format!(" --tab {tab}"));
        }
        for (k, v) in &self.args {
            source.push_str(&format!(" --arg {k}={v}"));
        }
        source
    }

    fn skeleton_shapes(&self) -> Vec<Shape> {
        let tab = self.tab.clone().unwrap_or_default();
        self.shapes
            .get(&tab)
            .cloned()
            .unwrap_or_else(|| loading::default_shapes(&self.key, &tab))
    }

    fn set_tab(&mut self, tab: String, cx: &mut Context<Self>) {
        self.tab = Some(tab);
        self.begin_switch();
        self.reload(cx);
    }

    /// Fetch the page again; while it is live, keep refreshing it.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.last_reload = std::time::Instant::now();
        self.generation += 1;
        let generation = self.generation;
        self.loading = true;
        cx.notify();
        let (key, tab, args) = (self.key.clone(), self.tab.clone(), self.args.clone());
        let workspace = self.workspace.clone();
        self._load = cx.spawn(async move |this, cx| {
            loop {
                // The board poller knows the project's home; wait for its first read.
                let mut target = None;
                for _ in 0..100 {
                    target = cx.update(|cx| {
                        let workspace = workspace.upgrade()?;
                        let store = BoardStore::for_workspace(&workspace, cx)?;
                        let store = store.read(cx);
                        Some((store.bin.clone(), store.home.clone()?))
                    });
                    if target.is_some() {
                        break;
                    }
                    cx.background_executor().timer(Duration::from_millis(300)).await;
                }
                let Some((bin, home)) = target else {
                    this.update(cx, |this, cx| {
                        this.loading = false;
                        this.switching = false;
                        this.data = Some(Err("This project has no .mini-ork — run `mini-ork init` in it.".into()));
                        cx.notify();
                    })
                    .ok();
                    return;
                };
                let result = cx
                    .background_spawn(board::fetch_page(bin, home, key.clone(), tab.clone(), args.clone()))
                    .await;
                let refresh = this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return None;
                        }
                        this.loading = false;
                        if this.switching || this.data.is_none() {
                            this.fade += 1;
                        }
                        this.switching = false;
                        let refresh = match &result {
                            Ok(value) => value.get("refresh").and_then(Value::as_u64),
                            Err(_) => None,
                        };
                        if let (Ok(value), Some(graph)) = (&result, this.graph.clone()) {
                            if let Some(model) = this.args.get("run").and_then(|run| crate::graph::Model::from_page(value, run)) {
                                graph.update(cx, |graph, cx| graph.set_model(model, cx));
                            }
                        }
                        if let Ok(value) = &result {
                            this.cache.insert(this.cache_key(), value.clone());
                            let tab = this.tab.clone().unwrap_or_else(|| str_of(value, "tab"));
                            let shapes: Vec<Shape> = arr(value, "sections")
                                .iter()
                                .map(|s| Shape {
                                    kind: str_of(s, "type"),
                                    full: s.get("full").and_then(Value::as_bool).unwrap_or(false),
                                    titled: !str_of(s, "title").is_empty(),
                                })
                                .collect();
                            if !shapes.is_empty() {
                                this.shapes.insert(tab, shapes);
                            }
                        }
                        this.data = Some(result.map_err(|e| e.to_string()));
                        cx.notify();
                        refresh
                    })
                    .ok()
                    .flatten();
                // Pages follow the board (see `follow_board`); only a page that
                // asks for it refreshes on a timer.
                let seconds = refresh.unwrap_or(0);
                if seconds == 0 {
                    return;
                }
                cx.background_executor().timer(Duration::from_secs(seconds)).await;
            }
        });
    }

    /// A button/row/chip action: `set` and tab switches stay in this page;
    /// everything else goes through `store::perform` after this update ends.
    fn action_handler(
        &self,
        action: Option<&Value>,
        cx: &mut Context<Self>,
    ) -> Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>> {
        let action = action?.clone();
        if action.is_null() {
            return None;
        }
        let page = cx.entity().downgrade();
        let workspace = self.workspace.clone();
        Some(Box::new(move |_, window, cx| {
            let (action, page, workspace) = (action.clone(), page.clone(), workspace.clone());
            window.defer(cx, move |window, cx| {
                if let Some(question) = action.get("confirm").and_then(Value::as_str) {
                    let answer = window.prompt(
                        gpui::PromptLevel::Warning,
                        question,
                        None,
                        &["Yes", "Cancel"],
                        cx,
                    );
                    let mut action = action.clone();
                    if let Some(map) = action.as_object_mut() {
                        map.remove("confirm");
                    }
                    let window_handle = window.window_handle();
                    cx.spawn(async move |cx| {
                        if answer.await.ok() == Some(0) {
                            window_handle
                                .update(cx, |_, window, cx| {
                                    if let Some(workspace) = workspace.upgrade() {
                                        store::perform(&action, &workspace, Origin::Page(page), window, cx);
                                    }
                                })
                                .ok();
                        }
                    })
                    .detach();
                    return;
                }
                if let Some(workspace) = workspace.upgrade() {
                    store::perform(&action, &workspace, Origin::Page(page), window, cx);
                }
            });
        }))
    }

    pub(crate) fn button(&self, id: impl Into<ElementId>, b: &Value, height: f32, size: f32, cx: &mut Context<Self>) -> AnyElement {
        let label = str_of(b, "label");
        let kind = b.get("kind").and_then(Value::as_str).unwrap_or("default").to_string();
        let handler = self.action_handler(b.get("do"), cx);
        p::button(id, label, &kind, height, size, handler)
    }

    fn render_header(&self, page: &Value, cx: &mut Context<Self>) -> AnyElement {
        let title = str_of(page, "title");
        let sub = str_of(page, "sub");
        let chips = arr(page, "chips");
        let actions = arr(page, "actions");
        let mono = p::mono_font(cx);
        h_flex()
            .items_start()
            .gap(px(12.))
            .flex_wrap()
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(240.))
                    .gap(px(3.))
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .flex_wrap()
                            .items_center()
                            .child(self.texts.text("page-title", &title, Look::new(17., c(p::TEXT)).bold(true), cx))
                            .children(chips.iter().map(|chip| {
                                div()
                                    .px(px(7.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(c(p::BORDER2))
                                    .text_size(px(11.5))
                                    .font(mono.clone())
                                    .text_color(p::named(&str_of(chip, "c")))
                                    .child(str_of(chip, "t"))
                            })),
                    )
                    .when(!sub.is_empty(), |this| {
                        this.child(self.texts.text("page-sub", &sub, Look::new(12.5, c(p::SUB)), cx))
                    }),
            )
            .child(
                h_flex().gap(px(6.)).flex_wrap().children(
                    actions
                        .iter()
                        .enumerate()
                        .map(|(i, b)| self.button(("page-action", i), b, 26., 12.5, cx)),
                ),
            )
            .into_any_element()
    }

    fn render_tabs(&self, page: &Value, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tabs = arr(page, "tabs");
        if tabs.is_empty() {
            return None;
        }
        let current = self.tab.clone().unwrap_or_else(|| str_of(page, "tab"));
        Some(
            h_flex()
                .gap(px(2.))
                .border_b_1()
                .border_color(c(p::BORDER))
                .children(tabs.iter().enumerate().map(|(i, t)| {
                    let key = str_of(t, "key");
                    let on = key == current;
                    div()
                        .id(("page-tab", i))
                        .px(px(10.))
                        .py(px(6.))
                        .text_size(px(12.5))
                        .text_color(c(if on { p::TEXT } else { p::SUB }))
                        .border_b_2()
                        .border_color(if on { c(p::BLUE) } else { p::ca(0x00000000) })
                        .cursor_pointer()
                        .hover(|s| s.text_color(c(p::TEXT)))
                        .child(str_of(t, "label"))
                        .on_click(cx.listener(move |this, _, _, cx| this.set_tab(key.clone(), cx)))
                }))
                .into_any_element(),
        )
    }

    /// Sections in the design's two-column grid: `full` spans both; `col`
    /// pins a column (list/detail layouts); the rest flow two to a row.
    fn render_sections(&self, page: &Value, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let sections = arr(page, "sections");
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut group: Vec<(usize, &Value)> = Vec::new();
        let flush = |group: &mut Vec<(usize, &Value)>, rows: &mut Vec<AnyElement>, this: &Self, window: &mut Window, cx: &mut Context<Self>| {
            if group.is_empty() {
                return;
            }
            let pinned = group.iter().any(|(_, s)| s.get("col").is_some());
            if pinned {
                let (left, right): (Vec<_>, Vec<_>) =
                    group.iter().partition(|(_, s)| s.get("col").and_then(Value::as_u64) != Some(2));
                rows.push(
                    h_flex()
                        .items_start()
                        .gap(px(12.))
                        .child(v_flex().flex_1().min_w_0().gap(px(12.)).children(
                            left.iter().map(|(i, s)| this.render_section(*i, s, window, cx)),
                        ))
                        .child(v_flex().flex_1().min_w_0().gap(px(12.)).children(
                            right.iter().map(|(i, s)| this.render_section(*i, s, window, cx)),
                        ))
                        .into_any_element(),
                );
            } else {
                for pair in group.chunks(2) {
                    let mut row = h_flex().gap(px(12.));
                    for (i, s) in pair {
                        row = row.child(div().flex_1().min_w_0().flex().child(this.render_section(*i, s, window, cx)));
                    }
                    if pair.len() == 1 {
                        row = row.child(div().flex_1().min_w_0());
                    }
                    rows.push(row.into_any_element());
                }
            }
            group.clear();
        };
        for (i, s) in sections.iter().enumerate() {
            if s.get("full").and_then(Value::as_bool).unwrap_or(false) {
                flush(&mut group, &mut rows, self, window, cx);
                rows.push(self.render_section(i, s, window, cx));
            } else {
                group.push((i, s));
            }
        }
        flush(&mut group, &mut rows, self, window, cx);
        v_flex().gap(px(12.)).children(rows).into_any_element()
    }

    fn render_section(&self, index: usize, s: &Value, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let kind = str_of(s, "type");
        match kind.as_str() {
            "dag" => return self.render_dag(index, s, window, cx),
            "inspector" => return self.render_inspector(index, s, cx),
            "triage" => return self.render_triage(index, s, cx),
            "callout" => return self.render_callout(index, s, cx),
            "hero" => return self.render_hero(index, s, cx),
            _ => {}
        }
        let title = str_of(s, "title");
        let note = str_of(s, "note");
        let mut actions = arr(s, "actions");
        if kind == "markdown" && !str_of(s, "path").is_empty() {
            actions.push(serde_json::json!({ "label": "Open", "kind": "ghost", "do": { "path": str_of(s, "path") } }));
        }
        let body = match kind.as_str() {
            "table" => self.render_table(index, s, cx),
            "kv" => self.render_kv(index, s, cx),
            "list" => self.render_list(index, s, cx),
            "bars" => self.render_bars(s, cx),
            "flow" => self.render_flow(index, s, cx),
            "code" => self.render_code(index, s, cx),
            "pills" => self.render_pills(s, cx),
            "chips" => self.render_chips(index, s, cx),
            // Full document, capped in height and scrolled inside.
            "markdown" => self
                .texts
                .nested_scroll(
                    format!("s{index}-md-box"),
                    div()
                        .id(SharedString::from(format!("s{index}-md-box")))
                        .max_h(px(640.))
                        .overflow_y_scroll()
                        .pr(px(6.))
                        .child(self.texts.markdown(format!("s{index}-md"), str_of(s, "text"), Look::new(13., c(p::BODY)), cx)),
                )
                .into_any_element(),
            _ => self.texts.text(
                format!("s{index}-unknown"),
                &format!("This section ({kind}) needs a newer mini-ork IDE."),
                Look::new(12., c(p::MUTED)),
                cx,
            ),
        };
        let has_head = !title.is_empty() || !actions.is_empty();
        card()
            .flex_1()
            .px(px(14.))
            .py(px(12.))
            .gap(px(10.))
            .when(has_head, |this| {
                this.child(
                    h_flex()
                        .gap(px(10.))
                        .flex_wrap()
                        .items_center()
                        .child(div().flex_1().min_w(px(120.)).child(self.texts.text(
                            format!("s{index}-title"),
                            &title,
                            Look::new(13., c(p::TEXT)).bold(true),
                            cx,
                        )))
                        .children(actions.iter().enumerate().map(|(i, b)| {
                            self.button(SharedString::from(format!("s{index}-a{i}")), b, 24., 12., cx)
                        })),
                )
            })
            .when(!note.is_empty(), |this| {
                this.child(div().mt(px(-4.)).child(self.texts.text(format!("s{index}-note"), &note, Look::new(12., c(p::MUTED)), cx)))
            })
            .child(body)
            .into_any_element()
    }

    fn render_table(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let cols = arr(s, "cols");
        let head = arr(s, "head");
        let rows = arr(s, "rows");
        let mono = p::mono_font(cx);
        let sized = |el: Div, i: usize| -> Div {
            match cols.get(i) {
                Some(col) => match col.get("w").and_then(Value::as_f64) {
                    Some(w) => el.w(px(w as f32)).flex_none(),
                    None => el
                        .flex_grow(col.get("fr").and_then(Value::as_f64).unwrap_or(1.0) as f32)
                        .flex_basis(px(0.))
                        .min_w(px(col.get("min").and_then(Value::as_f64).unwrap_or(0.0) as f32)),
                },
                None => el.flex_1().min_w_0(),
            }
        };
        let header = h_flex()
            .gap(px(10.))
            .px(px(6.))
            .pt(px(2.))
            .pb(px(6.))
            .border_b_1()
            .border_color(c(p::BORDER))
            .children(head.iter().enumerate().map(|(i, h)| {
                sized(div(), i)
                    .text_size(px(11.))
                    .text_color(c(p::SUB))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(h.as_str().unwrap_or_default().to_string())
            }));
        let body = rows.iter().enumerate().map(|(r, row)| {
            let handler = self.action_handler(row.get("do"), cx);
            let selected = row.get("sel").and_then(Value::as_bool).unwrap_or(false);
            let cells = arr(row, "cells");
            let clickable = handler.is_some();
            let row_text = cells.iter().map(|cell| str_of(cell, "t")).collect::<Vec<_>>().join("\t");
            let line = h_flex()
                .id(SharedString::from(format!("s{index}-r{r}")))
                .gap(px(10.))
                .px(px(6.))
                .py(px(5.))
                .rounded(px(5.))
                .when(selected, |this| this.bg(c(p::ROW_SELECTED)))
                .hover(|style| style.bg(c(p::HOVER)))
                // Right-click copies the row, wherever it is.
                .on_mouse_down(gpui::MouseButton::Right, move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(row_text.clone()));
                })
                .children(cells.iter().enumerate().map(|(i, cell)| {
                    let colour = p::named(&str_of(cell, "c"));
                    let text = str_of(cell, "t");
                    if clickable {
                        // A row that opens something keeps plain text, so a
                        // click is a click; right-click copies it.
                        let mut el = sized(div(), i).text_size(px(12.5)).text_color(colour).truncate().child(text);
                        if bool_of(cell, "mono") {
                            el = el.font(mono.clone());
                        }
                        if bool_of(cell, "b") {
                            el = el.font_weight(gpui::FontWeight::SEMIBOLD);
                        }
                        el.into_any_element()
                    } else {
                        sized(div(), i)
                            .overflow_hidden()
                            .child(self.texts.text(
                                format!("s{index}-r{r}-c{i}"),
                                &text,
                                Look::new(12.5, colour).mono(bool_of(cell, "mono")).bold(bool_of(cell, "b")).nowrap(),
                                cx,
                            ))
                            .into_any_element()
                    }
                }));
            match handler {
                Some(handler) => line.cursor_pointer().on_click(handler).into_any_element(),
                None => line.into_any_element(),
            }
        });
        v_flex().mx(px(-6.)).child(header).children(body).into_any_element()
    }

    fn render_kv(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .flex_wrap()
            .gap(px(8.))
            .children(arr(s, "items").iter().enumerate().map(|(i, item)| {
                let sub = str_of(item, "sub");
                v_flex()
                    .w(px(150.))
                    .flex_grow(1.)
                    .max_w(px(260.))
                    .gap(px(1.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(6.))
                    .bg(c(p::BG))
                    .border_1()
                    .border_color(c(p::BORDER))
                    .child(self.texts.text(format!("s{index}-k{i}"), str_of(item, "k"), Look::new(11., c(p::SUB)), cx))
                    .child(self.texts.text(
                        format!("s{index}-v{i}"),
                        str_of(item, "v"),
                        Look::new(14., p::named(&str_of(item, "c"))).bold(true),
                        cx,
                    ))
                    .when(!sub.is_empty(), |this| {
                        this.child(self.texts.text(format!("s{index}-ks{i}"), &sub, Look::new(11., c(p::SUB)).mono(true), cx))
                    })
            }))
            .into_any_element()
    }

    fn render_list(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap(px(2.))
            .children(arr(s, "items").iter().enumerate().map(|(i, item)| {
                let sub = str_of(item, "sub");
                let acts = arr(item, "acts");
                let title = self.texts.text(
                    format!("s{index}-i{i}"),
                    str_of(item, "t"),
                    Look::new(12.5, p::named(&str_of(item, "tc"))).mono(bool_of(item, "mono")),
                    cx,
                );
                h_flex()
                    .items_start()
                    .gap(px(9.))
                    .px(px(2.))
                    .py(px(5.))
                    .border_b_1()
                    .border_color(c(p::ROW_RULE))
                    .child(
                        div()
                            .flex_none()
                            .w(px(14.))
                            .mt(px(1.))
                            .text_size(px(12.))
                            .text_color(p::named(&str_of(item, "mc")))
                            .child(str_of(item, "m")),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(1.))
                            .child(title)
                            .when(!sub.is_empty(), |this| {
                                this.child(self.texts.text(format!("s{index}-is{i}"), &sub, Look::new(11.5, c(p::SUB)), cx))
                            }),
                    )
                    .when(!acts.is_empty(), |this| {
                        this.child(h_flex().flex_none().gap(px(5.)).children(acts.iter().enumerate().map(
                            |(a, b)| self.button(SharedString::from(format!("s{index}-i{i}-a{a}")), b, 22., 11.5, cx),
                        )))
                    })
            }))
            .into_any_element()
    }

    fn render_bars(&self, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let mono = p::mono_font(cx);
        v_flex()
            .gap(px(6.))
            .children(arr(s, "items").iter().map(|item| {
                let pct = item.get("pct").and_then(Value::as_f64).unwrap_or(1.0).clamp(1.0, 100.0);
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(p::text(str_of(item, "label"), 12., p::MUTED).w(px(160.)).flex_none().truncate())
                    .child(
                        div()
                            .flex_1()
                            .h(px(8.))
                            .rounded(px(4.))
                            .bg(c(p::BG))
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(pct as f32 / 100.))
                                    .rounded(px(4.))
                                    .bg(p::named(&str_of(item, "c"))),
                            ),
                    )
                    .child(
                        p::text(str_of(item, "val"), 11.5, p::TEXT)
                            .w(px(72.))
                            .flex_none()
                            .text_right()
                            .font(mono.clone()),
                    )
            }))
            .into_any_element()
    }

    fn render_flow(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let mut row = h_flex().flex_wrap().items_center().gap(px(6.));
        for (i, item) in arr(s, "items").iter().enumerate() {
            if i > 0 {
                row = row.child(p::text("→", 13., p::DIM));
            }
            let sub = str_of(item, "sub");
            row = row.child(
                v_flex()
                    .px(px(9.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(c(p::BORDER2))
                    .bg(c(p::BG))
                    .child(self.texts.text(
                        format!("s{index}-f{i}"),
                        str_of(item, "label"),
                        Look::new(12.5, p::named(&str_of(item, "c"))),
                        cx,
                    ))
                    .when(!sub.is_empty(), |this| {
                        this.child(self.texts.text(format!("s{index}-fs{i}"), &sub, Look::new(10.5, c(p::SUB)), cx))
                    }),
            );
        }
        row.into_any_element()
    }

    fn render_code(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        code_box(&self.texts, &format!("s{index}-code"), arr(s, "lines"), cx).into_any_element()
    }

    fn render_pills(&self, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let mono = p::mono_font(cx);
        h_flex()
            .flex_wrap()
            .gap(px(6.))
            .children(arr(s, "items").iter().map(|item| {
                div()
                    .px(px(7.))
                    .py(px(1.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(c(p::BORDER2))
                    .text_size(px(11.5))
                    .font(mono.clone())
                    .text_color(p::named(&str_of(item, "c")))
                    .child(str_of(item, "t"))
            }))
            .into_any_element()
    }

    fn render_chips(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .flex_wrap()
            .gap(px(6.))
            .children(arr(s, "items").iter().enumerate().map(|(i, item)| {
                let on = bool_of(item, "on");
                let handler = self.action_handler(item.get("do"), cx);
                let chip = h_flex()
                    .id(SharedString::from(format!("s{index}-chip{i}")))
                    .h(px(24.))
                    .px(px(10.))
                    .items_center()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(c(if on { p::DIM } else { p::BORDER2 }))
                    .when(on, |this| this.bg(c(p::SELECTED)))
                    .text_size(px(12.))
                    .text_color(c(if on { p::TEXT } else { p::MUTED }))
                    .child(str_of(item, "t"));
                match handler {
                    Some(handler) => chip.cursor_pointer().on_click(handler).into_any_element(),
                    None => chip.into_any_element(),
                }
            }))
            .into_any_element()
    }

    fn render_dag(&self, index: usize, s: &Value, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mono = p::mono_font(cx);
        // Columns keep their width so a wide DAG overflows and scrolls.
        let mut row = h_flex().flex_none().items_center().gap(px(10.));
        for (ci, column) in arr(s, "cols").iter().enumerate() {
            if ci > 0 {
                row = row.child(p::text("→", 14., p::DIM).flex_none());
            }
            let nodes = column.as_array().cloned().unwrap_or_default();
            row = row.child(v_flex().flex_none().gap(px(8.)).children(nodes.iter().enumerate().map(|(ni, node)| {
                let state = str_of(node, "state");
                let selected = bool_of(node, "sel");
                let (mark, colour) = match state.as_str() {
                    "done" => ("✓", p::GREEN),
                    "running" => ("●", p::BLUE),
                    "failed" => ("✗", p::RED),
                    "skipped" => ("–", p::DIM),
                    _ => ("○", p::DIM),
                };
                let status = match state.as_str() {
                    "running" => "working".to_string(),
                    other => other.to_string(),
                };
                let cost = str_of(node, "cost");
                let status = if cost.is_empty() { status } else { format!("{status} · {cost}") };
                let lane = str_of(node, "lane");
                let border = if selected {
                    c(p::BLUE)
                } else if state == "running" {
                    p::ca(0x74ade880)
                } else {
                    c(p::BORDER2)
                };
                // Any node opens the run graph full screen on that node's stream.
                let node_id = str_of(node, "id");
                let el = v_flex()
                    .id(SharedString::from(format!("s{index}-n{ci}-{ni}")))
                    .w(px(128.))
                    .px(px(9.))
                    .py(px(7.))
                    .gap(px(2.))
                    .rounded(px(7.))
                    .border_1()
                    .border_color(border)
                    .bg(c(if state == "pending" { p::BG } else { p::NODE_BG }))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(p::text(mark, 10., colour))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(c(p::TEXT))
                                    .child(str_of(node, "label")),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .child(p::lane_mark(&lane, 11.))
                            .child(p::text(lane, 11., p::MUTED).truncate()),
                    )
                    .child(p::text(status, 11., colour).font(mono.clone()));
                el.cursor_pointer()
                    .hover(|s| s.border_color(c(p::BLUE)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_node_tab(node_id.clone(), window, cx)
                    }))
                    .into_any_element()
            })));
        }
        let legend = str_of(s, "legend");
        let scroller = div()
            .id(("dag-scroll", index))
            .w_full()
            .overflow_x_scroll()
            .track_scroll(&self.dag_scroll)
            .pt(px(36.))
            .pb(px(12.))
            .child(row);
        let full_screen = div()
            .id(("dag-full", index))
            .absolute()
            .top(px(8.))
            .right(px(8.))
            .h(px(24.))
            .px(px(9.))
            .flex()
            .items_center()
            .rounded(px(5.))
            .border_1()
            .border_color(c(p::BORDER2))
            .bg(c(p::BG))
            .text_size(px(12.))
            .text_color(c(p::MUTED))
            .cursor_pointer()
            .hover(|s| s.bg(c(p::HOVER)).text_color(c(p::TEXT)))
            .tooltip(ui::Tooltip::text("Full screen · inspect every node"))
            .child("⤢ Full screen")
            .on_click(cx.listener(|this, _, window, cx| this.open_graph(None, window, cx)));
        card()
            .relative()
            .p(px(16.))
            .child(full_screen)
            .child(
                div().relative().w_full().child(scroller).custom_scrollbars(
                    Scrollbars::always_visible(ScrollAxes::Horizontal)
                        .tracked_scroll_handle(&self.dag_scroll),
                    window,
                    cx,
                ),
            )
            .when(!legend.is_empty(), |this| this.child(p::text(legend, 11.5, p::SUB).mt(px(12.))))
            .into_any_element()
    }

    fn render_inspector(&self, index: usize, s: &Value, cx: &mut Context<Self>) -> AnyElement {
        let state = str_of(s, "state");
        let (mark, colour) = match state.as_str() {
            "done" => ("✓", p::GREEN),
            "running" => ("●", p::BLUE),
            "failed" => ("✗", p::RED),
            _ => ("○", p::DIM),
        };
        let lines = arr(s, "lines");
        let _ = p::mono_font(cx);
        let note = str_of(s, "steer_note");
        let left = card()
            .flex_1()
            .min_w_0()
            .px(px(14.))
            .py(px(12.))
            .gap(px(10.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(p::text(mark, 13., colour))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(c(p::TEXT))
                            .child(str_of(s, "label")),
                    )
                    .child(p::text(format!("{} · {}", str_of(s, "role"), state), 12., p::SUB)),
            )
            .child(
                div()
                    .id(("inspector-out", index))
                    .min_h(px(90.))
                    .max_h(px(220.))
                    .overflow_y_scroll()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(6.))
                    .bg(c(p::BG))
                    .border_1()
                    .border_color(c(p::BORDER))
                    .when(lines.is_empty(), |this| this.child(p::text("No output yet.", 11.5, p::DIM)))
                    .children(lines.iter().enumerate().map(|(i, l)| {
                        self.texts.text(
                            format!("insp-{index}-{i}"),
                            str_of(l, "t"),
                            Look::new(11.5, p::named(&str_of(l, "c"))).mono(true),
                            cx,
                        )
                    })),
            )
            .when(!note.is_empty(), |this| {
                this.child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(c(p::TEXT))
                                .child("Steer this worker"),
                        )
                        .child(p::text(note, 12., p::MUTED)),
                )
            });
        let right = card()
            .w(px(260.))
            .flex_none()
            .px(px(14.))
            .py(px(12.))
            .gap(px(8.))
            .children(arr(s, "items").iter().enumerate().map(|(i, item)| {
                v_flex()
                    .child(p::text(str_of(item, "k"), 11., p::SUB))
                    .child(self.texts.text(
                        format!("insp-{index}-v{i}"),
                        str_of(item, "v"),
                        Look::new(12.5, p::named(&str_of(item, "c"))).mono(true),
                        cx,
                    ))
            }));
        h_flex().items_start().gap(px(12.)).child(left).child(right).into_any_element()
    }
}

impl MiniOrkPage {
    /// Open the run graph full screen, optionally on one node's stream.
    /// A DAG node opens in its own tab (the full-screen graph is the
    /// "⤢ Full screen" button).
    pub(crate) fn open_node_tab(&mut self, node: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.args.get("run").cloned() else {
            return;
        };
        let Some(Ok(page)) = self.data.clone() else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let model = crate::graph::Model::from_page(&page, &run);
        let (title, handle) = (self.title_hint.clone(), cx.entity().downgrade());
        window.defer(cx, move |window, cx| {
            crate::node_tab::open_node_tab(&workspace, run, node, title, model, handle, window, cx);
        });
    }

    pub(crate) fn open_graph(&mut self, node: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.args.get("run").cloned() else {
            return;
        };
        let Some(Ok(page)) = self.data.clone() else {
            return;
        };
        let Some(model) = crate::graph::Model::from_page(&page, &run) else {
            return;
        };
        if let Some(graph) = self.graph.clone() {
            if let Some(node) = node {
                graph.update(cx, |graph, cx| graph.open_node(node, cx));
            }
            return;
        }
        let (workspace, page_handle) = (self.workspace.clone(), cx.entity().downgrade());
        let graph = cx.new(|cx| crate::graph::MiniOrkGraph::new(model, node, workspace, page_handle, window, cx));
        self._graph_sub = Some(cx.subscribe(&graph, |this, _, _: &DismissEvent, cx| {
            this.graph = None;
            this._graph_sub = None;
            cx.notify();
        }));
        graph.update(cx, |graph, cx| graph.focus(window, cx));
        self.graph = Some(graph);
        cx.notify();
    }

    /// Re-read when the board moved (mini-ork wrote to the project) and this
    /// page is the one in front — so what you look at is never stale.
    fn follow_board(&mut self, cx: &mut Context<Self>) {
        if self._store_sub.is_some() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(store) = BoardStore::for_workspace(&workspace, cx) else {
            return;
        };
        self.seen_board = store.read(cx).generation;
        self._store_sub = Some(cx.observe(&store, |this, store, cx| {
            let generation = store.read(cx).generation;
            if generation == this.seen_board {
                return;
            }
            this.seen_board = generation;
            let in_front = this
                .workspace
                .upgrade()
                .and_then(|w| w.read(cx).active_item(cx))
                .is_some_and(|item| item.item_id() == cx.entity_id());
            if (in_front || this.graph.is_some())
                && !this.loading
                && this.last_reload.elapsed() >= Duration::from_secs(2)
            {
                this.reload(cx);
            }
        }));
    }
}

fn card() -> Div {
    v_flex()
        .bg(c(p::CHROME))
        .border_1()
        .border_color(c(p::BORDER))
        .rounded(px(8.))
        .min_w_0()
}

/// A monospace block; every line selectable, and a Copy button for all of it.
pub(crate) fn code_box<T: 'static>(texts: &Texts, key: &str, lines: Vec<Value>, cx: &mut Context<T>) -> gpui::Stateful<Div> {
    let all = lines.iter().map(|l| str_of(l, "t")).collect::<Vec<_>>().join("\n");
    let block = v_flex()
        .id(SharedString::from(format!("{key}-box")))
        .max_h(px(480.))
        .overflow_y_scroll()
        .relative()
        .px(px(10.))
        .py(px(8.))
        .rounded(px(6.))
        .bg(c(p::BG))
        .border_1()
        .border_color(c(p::BORDER))
        // Clip sideways only: a plain overflow_hidden would also undo the
        // vertical scroll above.
        .overflow_x_hidden()
        .children(lines.iter().enumerate().map(|(i, l)| {
            texts.text(
                format!("{key}-{i}"),
                str_of(l, "t"),
                Look::new(11.5, p::named(&str_of(l, "c"))).mono(true).nowrap(),
                cx,
            )
        }))
        .when(!lines.is_empty(), |this| {
            this.child(
                div()
                    .id(SharedString::from(format!("{key}-copy")))
                    .absolute()
                    .top(px(4.))
                    .right(px(6.))
                    .px(px(6.))
                    .rounded(px(4.))
                    .text_size(px(11.))
                    .text_color(c(p::DIM))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(p::HOVER)).text_color(c(p::TEXT)))
                    .child("⧉ Copy")
                    .on_click(move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(all.clone()))),
            )
        });
    texts.nested_scroll(format!("{key}-box"), block)
}

pub(crate) fn str_of(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn bool_of(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn arr(v: &Value, key: &str) -> Vec<Value> {
    v.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

impl EventEmitter<()> for MiniOrkPage {}

impl Focusable for MiniOrkPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for MiniOrkPage {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        self.label().into()
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let mono = p::mono_font(cx);
        h_flex()
            .gap(px(7.))
            .child(
                div()
                    .text_size(px(11.5))
                    .font(mono)
                    .text_color(c(if params.selected { p::BLUE } else { p::SUB }))
                    .child(glyph(&self.key)),
            )
            .child(Label::new(self.label()).single_line().color(params.text_color()))
            .into_any_element()
    }

    fn tab_tooltip_text(&self, _: &App) -> Option<SharedString> {
        match self.args.get("run") {
            Some(run) if !self.title_hint.is_empty() => Some(format!("{} ({run})", self.title_hint).into()),
            Some(run) => Some(run.clone().into()),
            None => None,
        }
    }
}

impl Render for MiniOrkPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.texts.begin();
        self.follow_board(cx);
        if matches!(self.data, Some(Ok(_))) && !self.switching {
            if let Some(node) = self.pending_graph.take() {
                let page = cx.entity().downgrade();
                window.defer(cx, move |window, cx| {
                    page.update(cx, |page, cx| page.open_graph(node, window, cx)).ok();
                });
            }
        }
        let source = self.source();
        let bar = self.loading.then(|| loading::sweep_bar(("page-sweep", self.generation as usize)));
        let content = match self.data.clone() {
            None => v_flex()
                .gap(px(14.))
                .child(
                    div()
                        .text_size(px(17.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(c(p::TEXT))
                        .child(self.label()),
                )
                .children(bar)
                .child(loading::loading_line(source, cx))
                .child(loading::skeleton(&self.skeleton_shapes()))
                .into_any_element(),
            Some(Ok(page)) if self.switching => v_flex()
                .gap(px(14.))
                .child(self.render_header(&page, cx))
                .child(v_flex().children(self.render_tabs(&page, cx)).children(bar.map(|b| div().mt(px(-2.)).child(b))))
                .child(loading::loading_line(source, cx))
                .child(loading::skeleton(&self.skeleton_shapes()))
                .into_any_element(),
            Some(Err(error)) => v_flex()
                .gap(px(6.))
                .child(p::text("This page could not be read", 15., p::TEXT))
                .child(self.texts.text("page-error", &error, Look::new(12.5, c(p::RED)).mono(true), cx))
                .into_any_element(),
            Some(Ok(page)) => v_flex()
                .gap(px(14.))
                .child(self.render_header(&page, cx))
                .child(v_flex().children(self.render_tabs(&page, cx)).children(bar.map(|b| div().mt(px(-2.)).child(b))))
                .child(
                    div().child(self.render_sections(&page, window, cx)).with_animation(
                        ("page-fade", self.fade as usize),
                        Animation::new(Duration::from_millis(280)),
                        |el, delta| el.opacity(delta),
                    ),
                )
                .into_any_element(),
        };
        self.texts.end();
        div()
            .id("mini-ork-page")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(c(p::BG))
            .text_color(c(p::TEXT))
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(20.))
                    .pt(px(16.))
                    .pb(px(48.))
                    .child(div().max_w(px(1080.)).mx_auto().child(content)),
            )
            .when_some(self.graph.clone(), |this, graph| {
                this.child(
                    deferred(
                        anchored()
                            .position_mode(AnchoredPositionMode::Window)
                            .position(point(px(0.), px(0.)))
                            .child(graph),
                    )
                    .with_priority(100),
                )
            })
    }
}

/// Open a page (or bring its tab forward and switch it to `tab`).
pub fn open_page(
    workspace: &Entity<Workspace>,
    key: String,
    tab: Option<String>,
    args: BTreeMap<String, String>,
    window: &mut Window,
    cx: &mut App,
) {
    if key == "run" {
        if let Some(run) = args.get("run").cloned() {
            open_run(workspace, run, String::new(), tab, window, cx);
        }
        return;
    }
    workspace.update(cx, |workspace, cx| {
        let existing = workspace
            .items_of_type::<MiniOrkPage>(cx)
            .find(|page| page.read(cx).key == key);
        if let Some(existing) = existing {
            workspace.activate_item(&existing, true, true, window, cx);
            if tab.is_some() || !args.is_empty() {
                existing.update(cx, |page, cx| {
                    if tab.is_some() {
                        page.tab = tab;
                    }
                    page.args.extend(args);
                    page.begin_switch();
                    page.reload(cx);
                });
            }
            return;
        }
        let weak = cx.entity().downgrade();
        let page = cx.new(|cx| MiniOrkPage::new(key, tab, args, String::new(), weak, cx));
        workspace.add_item_to_active_pane(Box::new(page), None, true, window, cx);
    });
}

/// Open a run's tab (or bring it forward).
pub fn open_run(
    workspace: &Entity<Workspace>,
    run_id: String,
    title: String,
    tab: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    workspace.update(cx, |workspace, cx| {
        let existing = workspace
            .items_of_type::<MiniOrkPage>(cx)
            .find(|page| page.read(cx).key == "run" && page.read(cx).args.get("run") == Some(&run_id));
        if let Some(existing) = existing {
            workspace.activate_item(&existing, true, true, window, cx);
            if tab.is_some() {
                existing.update(cx, |page, cx| {
                    page.tab = tab;
                    page.begin_switch();
                    page.reload(cx);
                });
            }
            return;
        }
        let weak = cx.entity().downgrade();
        let mut args = BTreeMap::new();
        args.insert("run".to_string(), run_id);
        let page = cx.new(|cx| MiniOrkPage::new("run".into(), tab, args, title, weak, cx));
        workspace.add_item_to_active_pane(Box::new(page), None, true, window, cx);
    });
}

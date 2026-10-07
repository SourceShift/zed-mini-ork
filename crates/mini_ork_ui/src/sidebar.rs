//! The left edge of the mini-ork IDE: the activity bar and the Threads board.
//!
//! It fills Zed's pluggable workspace sidebar slot (`workspace::Sidebar`), so
//! it sits left of the docks, full height. Its top strip continues the title
//! bar (project, branch) under the window's traffic lights.

use std::{sync::Arc, time::Duration};

use gpui::{
    AnyElement, App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, Pixels,
    Render, ScrollHandle, SharedString, Subscription, Task, WeakEntity, Window, WindowControlArea, px,
};
use ui_input::{ERASED_EDITOR_FACTORY, ErasedEditor, ErasedEditorEvent};
use ui::{Tooltip, prelude::*, utils::{TRAFFIC_LIGHT_PADDING, platform_title_bar_height}};
use workspace::{MultiWorkspace, MultiWorkspaceEvent, Sidebar, SidebarEvent, SidebarSide, Workspace};

use crate::{
    board::{self, Board, Run},
    page::{self, MiniOrkPage},
    palette::{self as p, c},
    store::{self, BoardStore},
};

pub const ACTIVITY_WIDTH: f32 = 42.;
const BOARD_WIDTH: f32 = 272.;
const BOARD_MIN: f32 = 200.;

/// The activity bar, top to bottom: key, glyph, tooltip.
const ACTIVITIES: [(&str, &str, &str); 10] = [
    ("threads", "▤", "Threads (task board)"),
    ("orch", "✦", "Orchestrator & intake"),
    ("verify", "✓", "Verification & safety"),
    ("recipes", "▥", "Recipes, specs & epics"),
    ("autos", "◷", "Automations & scheduling"),
    ("lanes", "$", "Models, lanes & cost"),
    ("learn", "❖", "Learning & memory"),
    ("context", "◫", "Context (ContextNest)"),
    ("nodes", "⬡", "Nodes & sandboxes"),
    ("setup", "⚙", "Setup & health"),
];

pub struct MiniOrkSidebar {
    multi_workspace: WeakEntity<MultiWorkspace>,
    focus_handle: FocusHandle,
    board_open: bool,
    board_width: f32,
    selected_run: Option<String>,
    bound: Option<(EntityId, EntityId)>,
    /// Full-text search over runs, and runs paged in beyond the board's first 50.
    search: Arc<dyn ErasedEditor>,
    query: String,
    results: Option<Vec<Run>>,
    extra: Vec<Run>,
    total: Option<u64>,
    has_more: bool,
    loading_more: bool,
    list_scroll: ScrollHandle,
    _search: Task<()>,
    _search_sub: Subscription,
    _store: Option<Subscription>,
    _workspace: Option<Subscription>,
    _multi: Subscription,
}

impl MiniOrkSidebar {
    pub fn new(
        multi_workspace: Entity<MultiWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let factory = ERASED_EDITOR_FACTORY.get().expect("editor factory is set at start-up");
        let search = factory(window, cx);
        search.set_placeholder_text("Search runs — feature, kickoff, results…", window, cx);
        let search_sub = search.subscribe(
            Box::new({
                let this = cx.entity().downgrade();
                let editor = search.clone();
                move |event, _window, cx| {
                    if event == ErasedEditorEvent::BufferEdited {
                        let text = editor.text(cx);
                        this.update(cx, |this, cx| this.set_query(text, cx)).ok();
                    }
                }
            }),
            window,
            cx,
        );
        let multi = cx.subscribe(&multi_workspace, |this, _, event: &MultiWorkspaceEvent, cx| {
            if matches!(event, MultiWorkspaceEvent::ActiveWorkspaceChanged { .. }) {
                this.bound = None;
                cx.notify();
            }
        });
        // The sidebar is the IDE's activity bar: keep it open.
        let handle = multi_workspace.downgrade();
        cx.defer(move |cx| {
            handle
                .update(cx, |multi_workspace, cx| {
                    if !multi_workspace.sidebar_open() {
                        multi_workspace.open_sidebar(cx);
                    }
                })
                .ok();
        });
        Self {
            multi_workspace: multi_workspace.downgrade(),
            focus_handle: cx.focus_handle(),
            board_open: true,
            board_width: BOARD_WIDTH,
            selected_run: None,
            bound: None,
            search,
            query: String::new(),
            results: None,
            extra: Vec::new(),
            total: None,
            has_more: true,
            loading_more: false,
            list_scroll: ScrollHandle::new(),
            _search: Task::ready(()),
            _search_sub: search_sub,
            _store: None,
            _workspace: None,
            _multi: multi,
        }
    }

    fn target(&self, cx: &App) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        let workspace = self.workspace(cx)?;
        let store = BoardStore::for_workspace(&workspace, cx)?;
        let store = store.read(cx);
        Some((store.bin.clone(), store.home.clone()?))
    }

    /// A new search: debounce, then fetch the first page.
    fn set_query(&mut self, text: String, cx: &mut Context<Self>) {
        let query = text.trim().to_string();
        if query == self.query {
            return;
        }
        self.query = query.clone();
        self.extra.clear();
        self.has_more = true;
        self.total = None;
        if query.is_empty() {
            self.results = None;
            self._search = Task::ready(());
            cx.notify();
            return;
        }
        let Some((bin, home)) = self.target(cx) else {
            return;
        };
        self.loading_more = true;
        cx.notify();
        self._search = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let page = cx.background_spawn(board::fetch_runs(bin, home, query.clone(), 0, 50)).await;
            this.update(cx, |this, cx| {
                if this.query != query {
                    return;
                }
                this.loading_more = false;
                match page {
                    Ok(page) => {
                        this.has_more = page.has_more;
                        this.total = page.total;
                        this.results = Some(page.runs);
                    }
                    Err(error) => {
                        this.has_more = false;
                        this.results = Some(Vec::new());
                        log::warn!("mini-ork run search: {error}");
                    }
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// The next 50 — of the search results, or of all runs past the board's first page.
    fn load_more(&mut self, cx: &mut Context<Self>) {
        if self.loading_more || !self.has_more {
            return;
        }
        let Some((bin, home)) = self.target(cx) else {
            return;
        };
        let base = BoardStore::for_workspace(&match self.workspace(cx) { Some(w) => w, None => return }, cx)
            .and_then(|s| s.read(cx).board.as_ref().map(|b| b.runs.len()))
            .unwrap_or(0);
        let offset = match &self.results {
            Some(results) => results.len(),
            None => base + self.extra.len(),
        };
        let query = self.query.clone();
        self.loading_more = true;
        cx.notify();
        self._search = cx.spawn(async move |this, cx| {
            let page = cx.background_spawn(board::fetch_runs(bin, home, query.clone(), offset, 50)).await;
            this.update(cx, |this, cx| {
                this.loading_more = false;
                if this.query != query {
                    return;
                }
                match page {
                    Ok(page) => {
                        this.has_more = page.has_more && !page.runs.is_empty();
                        this.total = page.total.or(this.total);
                        match &mut this.results {
                            Some(results) => results.extend(page.runs),
                            None => this.extra.extend(page.runs),
                        }
                    }
                    Err(error) => {
                        this.has_more = false;
                        log::warn!("mini-ork runs page: {error}");
                    }
                }
                cx.notify();
            })
            .ok();
        });
    }

    pub fn board_open(&self) -> bool {
        self.board_open
    }

    pub fn toggle_board(&mut self, cx: &mut Context<Self>) {
        self.board_open = !self.board_open;
        cx.emit(SidebarEvent::SerializeNeeded);
        cx.notify();
    }

    /// Open the Threads board (no-op when it is already open).
    pub fn show_board(&mut self, cx: &mut Context<Self>) {
        if !self.board_open {
            self.toggle_board(cx);
        }
    }

    fn workspace(&self, cx: &App) -> Option<Entity<Workspace>> {
        Some(self.multi_workspace.upgrade()?.read(cx).workspace().clone())
    }

    /// Re-render when the active workspace's board or active tab changes.
    fn bind(&mut self, cx: &mut Context<Self>) -> Option<(Entity<Workspace>, Entity<BoardStore>)> {
        let workspace = self.workspace(cx)?;
        let store = BoardStore::for_workspace(&workspace, cx)?;
        let ids = (workspace.entity_id(), store.entity_id());
        if self.bound != Some(ids) {
            self.bound = Some(ids);
            self._store = Some(cx.observe(&store, |_, _, cx| cx.notify()));
            self._workspace = Some(cx.subscribe(&workspace, |_, _, event: &workspace::Event, cx| {
                if matches!(event, workspace::Event::ActiveItemChanged) {
                    cx.notify();
                }
            }));
        }
        Some((workspace, store))
    }

    fn active_page_key(workspace: &Entity<Workspace>, cx: &App) -> Option<String> {
        let item = workspace.read(cx).active_item(cx)?;
        let page = item.to_any_view().downcast::<MiniOrkPage>().ok()?;
        Some(page.read(cx).key.clone())
    }

    fn render_strip(&self, board: Option<&Board>, workspace: Option<Entity<Workspace>>, window: &Window) -> AnyElement {
        let height = platform_title_bar_height(window);
        let fullscreen = window.is_fullscreen() || window.is_simple_fullscreen();
        let header = board.map(|b| b.header.clone()).unwrap_or_default();
        let project = if header.project.is_empty() {
            board.map(|b| b.project.clone()).unwrap_or_default()
        } else {
            header.project.clone()
        };
        h_flex()
            .id("mini-ork-sidebar-strip")
            .window_control_area(WindowControlArea::Drag)
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
            .flex_none()
            .h(height)
            .w_full()
            .bg(c(p::CHROME))
            .border_b_1()
            .border_color(c(p::BORDER))
            .when(!fullscreen && cfg!(target_os = "macos"), |this| this.pl(px(TRAFFIC_LIGHT_PADDING + 2.)))
            .when(fullscreen || !cfg!(target_os = "macos"), |this| this.pl(px(12.)))
            .gap(px(10.))
            .text_size(px(12.5))
            .whitespace_nowrap()
            .overflow_hidden()
            .when(self.board_open && !project.is_empty(), |this| {
                this.children(crate::title_bar::left_items(&header, &project, workspace, true))
            })
            .into_any_element()
    }

    fn render_activity_bar(
        &self,
        workspace: &Option<Entity<Workspace>>,
        needs_you: u64,
        active_key: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mono = p::mono_font(cx);
        let mut bar = v_flex()
            .flex_none()
            .w(px(ACTIVITY_WIDTH))
            .h_full()
            .bg(c(p::CHROME))
            .border_r_1()
            .border_color(c(p::BORDER))
            .items_center()
            .py(px(6.))
            .gap(px(2.));
        for (i, (key, glyph, tooltip)) in ACTIVITIES.iter().enumerate() {
            if *key == "setup" {
                bar = bar.child(div().flex_1());
            }
            let on = if *key == "threads" {
                self.board_open
            } else {
                active_key.as_deref() == Some(*key)
            };
            let key = key.to_string();
            let workspace = workspace.clone();
            let button = div()
                .id(SharedString::from(format!("activity-{key}")))
                .relative()
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .when(on, |this| this.bg(c(p::SELECTED)))
                .text_size(px(15.))
                .font(mono.clone())
                .text_color(c(if on { p::TEXT } else { p::SUB }))
                .cursor_pointer()
                .hover(|style| style.bg(c(p::HOVER)).text_color(c(p::TEXT)))
                .tooltip(Tooltip::text(*tooltip))
                .child(*glyph)
                .when(key == "threads" && needs_you > 0, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(2.))
                            .right(px(1.))
                            .min_w(px(14.))
                            .h(px(14.))
                            .px(px(3.))
                            .rounded(px(7.))
                            .bg(c(p::YELLOW))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(9.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(c(p::BG))
                            .child(needs_you.to_string()),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if key == "threads" {
                        this.toggle_board(cx);
                        return;
                    }
                    let (key, workspace) = (key.clone(), workspace.clone());
                    window.defer(cx, move |window, cx| {
                        if let Some(workspace) = workspace {
                            page::open_page(&workspace, key, None, Default::default(), window, cx);
                        }
                    });
                }));
            bar = bar.child(button);
            if i == 0 {
                bar = bar.child(div().w(px(20.)).h(px(1.)).my(px(4.)).bg(c(p::BORDER)));
            }
        }
        bar.into_any_element()
    }

    fn render_board(
        &self,
        board: Option<&Board>,
        error: Option<String>,
        workspace: &Option<Entity<Workspace>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mono = p::mono_font(cx);
        let runs: Vec<Run> = match &self.results {
            Some(results) => results.clone(),
            None => {
                let mut runs = board.map(|b| b.runs.clone()).unwrap_or_default();
                let seen: std::collections::HashSet<String> = runs.iter().map(|r| r.id.clone()).collect();
                runs.extend(self.extra.iter().filter(|r| !seen.contains(&r.id)).cloned());
                runs
            }
        };
        let counts = board.map(|b| b.counts.clone()).unwrap_or_default();
        let header = {
            let workspace = workspace.clone();
            h_flex()
                .flex_none()
                .h(px(36.))
                .pl(px(12.))
                .pr(px(8.))
                .gap(px(8.))
                .border_b_1()
                .border_color(c(p::BORDER))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(c(p::TEXT))
                        .child("Threads"),
                )
                .child(p::text(
                    match (self.total, self.results.is_some()) {
                        (Some(total), _) => format!("{} of {total}", runs.len()),
                        _ => runs.len().to_string(),
                    },
                    12.,
                    p::SUB,
                ))
                .child(div().flex_1())
                .child(
                    div()
                        .id("board-new-thread")
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .text_size(px(15.))
                        .text_color(c(p::MUTED))
                        .cursor_pointer()
                        .hover(|s| s.bg(c(p::HOVER)).text_color(c(p::TEXT)))
                        .tooltip(Tooltip::text("New thread"))
                        .child("+")
                        .on_click(move |_, window, cx| {
                            if let Some(workspace) = workspace.clone() {
                                store::new_thread(&workspace, "", window, cx);
                            }
                        }),
                )
        };
        let search_row = h_flex()
            .flex_none()
            .h(px(30.))
            .mx(px(10.))
            .mt(px(8.))
            .px(px(8.))
            .gap(px(6.))
            .rounded(px(5.))
            .border_1()
            .border_color(c(p::BORDER2))
            .bg(c(p::BG))
            .child(p::text("⌕", 12., p::SUB))
            .child(div().flex_1().min_w_0().text_size(px(12.5)).child(self.search.render(window, cx)));
        let tiles = [
            (counts.needs_you, "need you", p::YELLOW),
            (counts.working, "working", p::BLUE),
            (counts.failed, "failed", p::RED),
            (counts.done, "done", p::GREEN),
        ];
        let count_grid = h_flex()
            .flex_none()
            .gap(px(4.))
            .px(px(10.))
            .py(px(8.))
            .border_b_1()
            .border_color(c(p::BORDER))
            .children(tiles.into_iter().map(|(n, label, colour)| {
                v_flex()
                    .flex_1()
                    .items_center()
                    .py(px(4.))
                    .rounded(px(5.))
                    .bg(c(p::BG))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(c(colour))
                            .child(n.to_string()),
                    )
                    .child(p::text(label, 10.5, p::SUB))
            }));

        let now = now_epoch();
        let groups = [
            ("needs_you", "Needs you"),
            ("working", "Working"),
            ("queued", "Queued"),
            ("failed", "Failed"),
            ("done", "Done"),
        ];
        let mut list = v_flex().px(px(6.)).pt(px(6.)).pb(px(16.));
        if let Some(error) = error.filter(|_| runs.is_empty()) {
            list = list.child(p::text(error, 12., p::SUB).p(px(8.)));
        } else if board.is_none() {
            list = list.child(p::text("Loading…", 12., p::SUB).p(px(8.)));
        } else if runs.is_empty() && self.results.is_some() && !self.loading_more {
            list = list.child(p::text(format!("No runs match “{}”.", self.query), 12., p::SUB).p(px(8.)));
        } else if runs.is_empty() && self.results.is_none() {
            list = list.child(
                p::text("No threads yet. Press + and say what you want done.", 12., p::SUB).p(px(8.)),
            );
        }
        for (state, label) in groups {
            let rows: Vec<_> = runs.iter().filter(|run| run.state == state).collect();
            if rows.is_empty() {
                continue;
            }
            let (mark, colour) = p::state_mark(state);
            list = list.child(
                h_flex()
                    .gap(px(6.))
                    .px(px(8.))
                    .pt(px(10.))
                    .pb(px(4.))
                    .text_size(px(11.))
                    .text_color(c(p::SUB))
                    .child(div().text_color(c(colour)).child(mark))
                    .child(format!("{} · {}", label.to_uppercase(), rows.len())),
            );
            for run in rows {
                let selected = self.selected_run.as_deref() == Some(run.id.as_str());
                let sub_colour = match state {
                    "failed" => p::RED,
                    "needs_you" => p::YELLOW,
                    _ => p::SUB,
                };
                let mut sub = run.step.clone();
                if (run.added > 0 || run.removed > 0) && !sub.contains('+') {
                    let change = format!("+{} −{}", run.added, run.removed);
                    sub = if sub.is_empty() { change } else { format!("{sub} · {change}") };
                }
                if sub.is_empty() {
                    sub = run.recipe.clone();
                }
                let (id, title) = (run.id.clone(), run.display_title());
                let workspace = workspace.clone();
                list = list.child(
                    h_flex()
                        .id(SharedString::from(format!("board-run-{}", run.id)))
                        .items_start()
                        .gap(px(8.))
                        .w_full()
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .when(selected, |this| this.bg(c(p::SELECTED)))
                        .hover(|s| s.bg(c(p::HOVER)))
                        .cursor_pointer()
                        .child(p::text(mark, 12., colour).w(px(14.)).flex_none().mt(px(1.)))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(1.))
                                .child(p::text(title.clone(), 13., p::TEXT).truncate())
                                .child(p::text(sub, 11.5, sub_colour).truncate())
                                .children(span_line(run.started_at, run.ended_at, state).map(|line| {
                                    p::text(line, 10.5, p::DIM).font(mono.clone()).truncate()
                                })),
                        )
                        .child(
                            v_flex()
                                .flex_none()
                                .items_end()
                                .gap(px(1.))
                                .text_size(px(11.))
                                .text_color(c(p::SUB))
                                .font(mono.clone())
                                .child(format!("${:.2}", run.cost_usd))
                                .child(age(run.started_at, now)),
                        )
                        .when_some(span_tooltip(run.started_at, run.ended_at), |this, tip| {
                            this.tooltip(Tooltip::text(tip))
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selected_run = Some(id.clone());
                            cx.notify();
                            let (id, title, workspace) = (id.clone(), title.clone(), workspace.clone());
                            window.defer(cx, move |window, cx| {
                                if let Some(workspace) = workspace {
                                    page::open_run(&workspace, id, title, None, window, cx);
                                }
                            });
                        })),
                );
            }
        }
        if self.loading_more {
            list = list.child(p::text("Loading…", 11.5, p::SUB).px(px(8.)).py(px(6.)));
        } else if self.has_more && !runs.is_empty() {
            list = list.child(
                div()
                    .id("board-load-more")
                    .mx(px(8.))
                    .my(px(6.))
                    .py(px(5.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(c(p::BORDER2))
                    .text_center()
                    .text_size(px(11.5))
                    .text_color(c(p::MUTED))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(p::HOVER)).text_color(c(p::TEXT)))
                    .child("Load more")
                    .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
            );
        }
        let scroll = self.list_scroll.clone();
        v_flex()
            .flex_none()
            .w(px(self.board_width))
            .h_full()
            .bg(c(p::CHROME))
            .border_r_1()
            .border_color(c(p::BORDER))
            .child(header)
            .child(search_row)
            .child(count_grid)
            .child(
                div()
                    .id("board-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.list_scroll)
                    // Near the bottom, fetch the next page.
                    .on_scroll_wheel(cx.listener(move |this, _, _, cx| {
                        let (offset, max) = (scroll.offset().y, scroll.max_offset().y);
                        if f32::from(max) > 0. && f32::from(-offset) >= f32::from(max) - 240. {
                            this.load_more(cx);
                        }
                    }))
                    .child(list),
            )
            .into_any_element()
    }
}

fn local(epoch: i64) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::TimeZone as _;
    chrono::Local.timestamp_opt(epoch, 0).single()
}

/// `21:03 → 21:41`, `21:03 → running`, with the date when it is not today
/// (and the end's date only when it differs from the start's).
fn span_line(started: Option<i64>, ended: Option<i64>, state: &str) -> Option<String> {
    let start = local(started?)?;
    let today = chrono::Local::now().date_naive();
    let fmt = |t: &chrono::DateTime<chrono::Local>, same_day_as: chrono::NaiveDate| {
        if t.date_naive() == same_day_as {
            t.format("%H:%M").to_string()
        } else {
            t.format("%b %-d %H:%M").to_string()
        }
    };
    let head = fmt(&start, today);
    let tail = match ended.and_then(local) {
        Some(end) if state != "working" => fmt(&end, start.date_naive()),
        _ if state == "working" => "running".to_string(),
        _ => "—".to_string(),
    };
    Some(format!("{head} → {tail}"))
}

fn span_tooltip(started: Option<i64>, ended: Option<i64>) -> Option<String> {
    let start = local(started?)?;
    let mut text = format!("Started {}", start.format("%Y-%m-%d %H:%M:%S"));
    if let Some(end) = ended.and_then(local) {
        let secs = (end - start).num_seconds().max(0);
        let took = if secs >= 3600 {
            format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
        } else {
            format!("{}m {:02}s", secs / 60, secs % 60)
        };
        text.push_str(&format!(" · Finished {} · {took}", end.format("%Y-%m-%d %H:%M:%S")));
    }
    Some(text)
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn age(started_at: Option<i64>, now: i64) -> String {
    let Some(started) = started_at else {
        return String::new();
    };
    match (now - started).max(0) {
        0..60 => "now".into(),
        secs @ 60..3600 => format!("{}m", secs / 60),
        secs @ 3600..86400 => format!("{}h", secs / 3600),
        secs => format!("{}d", secs / 86400),
    }
}

impl EventEmitter<SidebarEvent> for MiniOrkSidebar {}

impl Focusable for MiniOrkSidebar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Sidebar for MiniOrkSidebar {
    fn width(&self, _cx: &App) -> Pixels {
        if self.board_open {
            px(ACTIVITY_WIDTH + self.board_width)
        } else {
            px(ACTIVITY_WIDTH)
        }
    }

    fn set_width(&mut self, width: Option<Pixels>, cx: &mut Context<Self>) {
        self.board_width = match width {
            Some(width) => (f32::from(width) - ACTIVITY_WIDTH).max(BOARD_MIN),
            None => BOARD_WIDTH,
        };
        self.board_open = true;
        cx.notify();
    }

    fn has_notifications(&self, cx: &App) -> bool {
        self.workspace(cx)
            .and_then(|workspace| BoardStore::for_workspace(&workspace, cx))
            .and_then(|store| store.read(cx).board.as_ref().map(|b| b.counts.needs_you > 0))
            .unwrap_or(false)
    }

    fn side(&self, _cx: &App) -> SidebarSide {
        SidebarSide::Left
    }

    fn serialized_state(&self, _cx: &App) -> Option<String> {
        Some(serde_json::json!({ "board_open": self.board_open, "board_width": self.board_width }).to_string())
    }

    fn restore_serialized_state(&mut self, state: &str, _window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(state) {
            if let Some(open) = value.get("board_open").and_then(|v| v.as_bool()) {
                self.board_open = open;
            }
            if let Some(width) = value.get("board_width").and_then(|v| v.as_f64()) {
                self.board_width = (width as f32).max(BOARD_MIN);
            }
            cx.notify();
        }
    }
}

impl Render for MiniOrkSidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bound = self.bind(cx);
        let (workspace, board, error) = match &bound {
            Some((workspace, store)) => {
                let store = store.read(cx);
                (Some(workspace.clone()), store.board.clone(), store.error.clone())
            }
            None => (None, None, None),
        };
        let active_key = workspace.as_ref().and_then(|w| Self::active_page_key(w, cx));
        let needs_you = board.as_ref().map_or(0, |b| b.counts.needs_you);
        let strip = self.render_strip(board.as_ref(), workspace.clone(), window);
        let activity = self.render_activity_bar(&workspace, needs_you, active_key, cx);
        let board_column = self
            .board_open
            .then(|| self.render_board(board.as_ref(), error, &workspace, window, cx));
        v_flex()
            .id("mini-ork-sidebar")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(c(p::CHROME))
            .text_color(c(p::TEXT))
            .child(strip)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(activity)
                    .children(board_column),
            )
    }
}

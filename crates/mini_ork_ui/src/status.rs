//! Status bar items: the board toggle and the live mini-ork counters on the
//! left, the mini-ork version on the right.

use gpui::{App, Context, Entity, Render, SharedString, Subscription, WeakEntity, Window, px, relative};
use ui::{Tooltip, prelude::*};
use workspace::{HideStatusItem, ItemHandle, StatusItemView, Workspace};

use crate::{
    board::Lane,
    page,
    palette::{self as p, c},
    sidebar::MiniOrkSidebar,
    store::BoardStore,
};

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        let weak = cx.entity().downgrade();
        let Some(store) = BoardStore::for_workspace(&cx.entity(), cx) else {
            return;
        };
        let left = cx.new(|cx| MiniOrkStatus::new(weak, store.clone(), cx));
        let right = cx.new(|cx| MiniOrkVersion::new(store, cx));
        workspace.status_bar().update(cx, |status_bar, cx| {
            status_bar.add_left_item(left, window, cx);
            status_bar.add_right_item(right, window, cx);
        });
    })
    .detach();
}

pub struct MiniOrkStatus {
    workspace: WeakEntity<Workspace>,
    store: Entity<BoardStore>,
    _store: Subscription,
}

impl MiniOrkStatus {
    fn new(workspace: WeakEntity<Workspace>, store: Entity<BoardStore>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            workspace,
            store,
            _store: sub,
        }
    }
}

fn toggle_board(workspace: &WeakEntity<Workspace>, cx: &mut App) {
    with_sidebar(workspace, cx, |sidebar, cx| sidebar.toggle_board(cx));
}

/// Open the Threads board, where working and failed runs are grouped.
fn show_board(workspace: &WeakEntity<Workspace>, cx: &mut App) {
    with_sidebar(workspace, cx, |sidebar, cx| sidebar.show_board(cx));
}

fn with_sidebar(
    workspace: &WeakEntity<Workspace>,
    cx: &mut App,
    f: impl FnOnce(&mut MiniOrkSidebar, &mut Context<MiniOrkSidebar>),
) {
    let Some(workspace) = workspace.upgrade() else {
        return;
    };
    let Some(mw) = workspace.read(cx).multi_workspace().and_then(|mw| mw.upgrade()) else {
        return;
    };
    let sidebar = mw
        .read(cx)
        .sidebar()
        .and_then(|s| s.to_any().downcast::<MiniOrkSidebar>().ok());
    if let Some(sidebar) = sidebar {
        sidebar.update(cx, f);
        mw.update(cx, |_, cx| cx.notify());
    }
}

impl Render for MiniOrkStatus {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self
            .store
            .read(cx)
            .board
            .as_ref()
            .map(|b| b.header.clone())
            .unwrap_or_default();
        let item = |id: &'static str, label: String, colour: u32, tip: &'static str, key: &'static str, tab: Option<&'static str>, workspace: WeakEntity<Workspace>| {
            div()
                .id(id)
                .h(px(20.))
                .px(px(7.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .text_size(px(12.))
                .text_color(c(colour))
                .cursor_pointer()
                .hover(|s| s.bg(c(p::HOVER)))
                .tooltip(Tooltip::text(tip))
                .child(label)
                .on_click(move |_, window, cx| {
                    let workspace = workspace.clone();
                    if key == "threads" {
                        show_board(&workspace, cx);
                        return;
                    }
                    window.defer(cx, move |window, cx| {
                        if let Some(workspace) = workspace.upgrade() {
                            page::open_page(&workspace, key.into(), tab.map(String::from), Default::default(), window, cx);
                        }
                    });
                })
        };
        let w = self.workspace.clone();
        let nodes = if header.nodes.configured > 0 {
            format!("⬡ {} nodes", header.nodes.configured)
        } else {
            "⬡ local".to_string()
        };
        let lanes: Vec<AnyElement> = header.lanes.iter().take(5).map(|lane| lane_item(lane, cx)).collect();
        h_flex()
            .gap(px(2.))
            .child(
                div()
                    .id("status-board")
                    .h(px(20.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .text_size(px(12.))
                    .text_color(c(p::MUTED))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(p::HOVER)))
                    .tooltip(Tooltip::text("Toggle task board"))
                    .child("▤")
                    .on_click({
                        let w = w.clone();
                        move |_, _, cx| toggle_board(&w, cx)
                    }),
            )
            .child(div().w(px(1.)).h(px(14.)).mx(px(4.)).bg(c(p::BORDER2)))
            .child(item(
                "status-working",
                format!("● {} working", header.working),
                if header.working > 0 { p::BLUE } else { p::SUB },
                "Working runs — on the Threads board",
                "threads",
                None,
                w.clone(),
            ))
            .when(header.needs_you > 0, |this| {
                this.child(item(
                    "status-needs",
                    format!("✋ {} need you", header.needs_you),
                    p::ORANGE,
                    "Opens the Inbox",
                    "inbox",
                    None,
                    w.clone(),
                ))
            })
            .child(item(
                "status-failed",
                format!("✗ {}", header.failed),
                if header.failed > 0 { p::RED } else { p::SUB },
                "Failed runs — on the Threads board",
                "threads",
                None,
                w.clone(),
            ))
            .child(item(
                "status-budget",
                format!("{} / ${:.0}", money(header.today_usd), header.cap_usd),
                p::MUTED,
                "Daily budget",
                "lanes",
                Some("cost"),
                w.clone(),
            ))
            .child(item(
                "status-scheduler",
                format!("◷ scheduler {}", if header.scheduler_on { "on" } else { "off" }),
                if header.scheduler_on { p::MUTED } else { p::YELLOW },
                "Scheduler",
                "autos",
                Some("schedulers"),
                w.clone(),
            ))
            .child(item(
                "status-cn",
                "◫ ContextNest".to_string(),
                if header.contextnest { p::MUTED } else { p::DIM },
                "Context",
                "context",
                None,
                w.clone(),
            ))
            .child(item("status-nodes", nodes, p::MUTED, "Nodes", "nodes", None, w))
            .when(!lanes.is_empty(), |this| {
                this.child(div().w(px(1.)).h(px(14.)).mx(px(4.)).bg(c(p::BORDER2)))
                    .children(lanes)
            })
    }
}

/// One lane's last-24h health: its name, then a 28×4 bar split green for the
/// success share and red for the failure share. A lane failing half its calls
/// (over at least three) gets a red name.
fn lane_item(lane: &Lane, cx: &App) -> AnyElement {
    let calls = lane.calls;
    let failed = lane.failed.min(calls);
    let failure = if calls > 0 { failed as f32 / calls as f32 } else { 0. };
    let success = if calls > 0 { (calls - failed) as f32 / calls as f32 } else { 0. };
    let name_colour = if calls >= 3 && failure >= 0.5 { p::RED } else { p::SUB };
    let mut tip = format!("{} calls · {} failed · ${:.2}", calls, failed, lane.usd);
    if !lane.last_error.is_empty() {
        tip.push_str(&format!(" · {}", lane.last_error));
    }
    h_flex()
        .id(SharedString::from(format!("status-lane-{}", lane.lane)))
        .h(px(20.))
        .px(px(5.))
        .gap(px(4.))
        .items_center()
        .rounded(px(4.))
        .tooltip(Tooltip::text(tip))
        .child(
            div()
                .font(p::mono_font(cx))
                .text_size(px(10.5))
                .text_color(c(name_colour))
                .child(lane.lane.clone()),
        )
        .child(
            h_flex()
                .w(px(28.))
                .h(px(4.))
                .flex_none()
                .rounded(px(2.))
                .overflow_hidden()
                .bg(c(p::BORDER2))
                .child(div().h_full().w(relative(success)).flex_none().bg(c(p::GREEN)))
                .child(div().h_full().w(relative(failure)).flex_none().bg(c(p::RED))),
        )
        .into_any_element()
}

impl StatusItemView for MiniOrkStatus {
    fn set_active_pane_item(&mut self, _: Option<&dyn ItemHandle>, _: &mut Window, _: &mut Context<Self>) {}

    fn hide_setting(&self, _cx: &App) -> Option<HideStatusItem> {
        None
    }
}

pub struct MiniOrkVersion {
    store: Entity<BoardStore>,
    _store: Subscription,
}

impl MiniOrkVersion {
    fn new(store: Entity<BoardStore>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&store, |_, _, cx| cx.notify());
        Self { store, _store: sub }
    }
}

impl Render for MiniOrkVersion {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let version = self
            .store
            .read(cx)
            .board
            .as_ref()
            .map(|b| b.header.version.clone())
            .unwrap_or_default();
        let label: SharedString = if version.is_empty() {
            "mini-ork".into()
        } else {
            format!("mini-ork {version}").into()
        };
        div().px(px(7.)).text_size(px(12.)).text_color(c(p::MUTED)).child(label)
    }
}

impl StatusItemView for MiniOrkVersion {
    fn set_active_pane_item(&mut self, _: Option<&dyn ItemHandle>, _: &mut Window, _: &mut Context<Self>) {}

    fn hide_setting(&self, _cx: &App) -> Option<HideStatusItem> {
        None
    }
}

fn money(value: Option<f64>) -> String {
    value.map(|v| format!("${v:.2}")).unwrap_or_else(|| "—".to_string())
}

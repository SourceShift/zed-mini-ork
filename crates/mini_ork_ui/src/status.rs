//! Status bar items: the board toggle and the live mini-ork counters on the
//! left, the mini-ork version on the right.

use gpui::{App, Context, Entity, Render, SharedString, Subscription, WeakEntity, Window, px};
use ui::{Tooltip, prelude::*};
use workspace::{HideStatusItem, ItemHandle, StatusItemView, Workspace};

use crate::{
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
            .child(item("status-needs", format!("✋ {}", header.needs_you), p::YELLOW, "Needs you", "verify", Some("inbox"), w.clone()))
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
    }
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

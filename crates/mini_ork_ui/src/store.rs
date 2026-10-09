//! One `mini-ork board` poller per workspace, shared by the sidebar, the title
//! bar and the status bar — plus the one place page actions are carried out.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    time::Duration,
};

use gpui::{
    App, AppContext as _, Context, Entity, EntityId, Global, SharedString, Task, TaskExt as _,
    WeakEntity, Window,
};
use serde_json::Value;
use workspace::{OpenOptions, Toast, Workspace, notifications::NotificationId};

use crate::board::{self, Board};

/// The board is re-read when mini-ork writes to the project's `.mini-ork`
/// (state.db, its WAL, run folders) — never on a fixed beat. Reads are at least
/// `MIN_GAP` apart so a busy run coalesces into one read per gap; a slow safety
/// read covers anything the watcher misses.
const MIN_GAP: Duration = Duration::from_millis(1500);
const MIN_GAP_INACTIVE: Duration = Duration::from_secs(10);
const SAFETY_READ: Duration = Duration::from_secs(60);
const WATCH_LATENCY: Duration = Duration::from_millis(300);

pub struct BoardStore {
    workspace: WeakEntity<Workspace>,
    pub home: Option<PathBuf>,
    pub bin: PathBuf,
    pub board: Option<Board>,
    pub error: Option<String>,
    /// Each run's state at the previous read, to notice runs that finish.
    last_states: HashMap<String, String>,
    /// Bumped on every fresh read; pages re-read themselves when it moves.
    pub generation: u64,
    _watcher: Option<std::sync::Arc<dyn fs::Watcher>>,
    _poll: Task<()>,
}

#[derive(Default)]
struct Stores(HashMap<EntityId, Entity<BoardStore>>);

impl Global for Stores {}

pub fn init(cx: &mut App) {
    cx.observe_new(|_workspace: &mut Workspace, window, cx| {
        let id = cx.entity().entity_id();
        let weak = cx.entity().downgrade();
        let store = cx.new(|cx| BoardStore::new(weak, cx));
        cx.default_global::<Stores>().0.insert(id, store);
        cx.on_release(move |_workspace, cx| {
            if cx.has_global::<Stores>() {
                cx.global_mut::<Stores>().0.remove(&id);
            }
        })
        .detach();
        if let Some(window) = window {
            startup_layout(window, cx);
        }
    })
    .detach();
}

/// The design's layout once the window has loaded: the mini-ork thread in the
/// left dock, nothing in the right dock. `MINI_ORK_IDE_OPEN` (comma-separated
/// `key[:tab]`, `run:<id>[:tab][:graph[:node]]` or `run:latest…`) also opens pages —
/// for checking the IDE without clicking through it.
fn startup_layout(window: &mut Window, cx: &mut Context<Workspace>) {
    cx.spawn_in(window, async move |workspace, cx| {
        // The first board read takes longer than loading the panels.
        for _ in 0..200 {
            let ready = workspace
                .update(cx, |_, cx| {
                    BoardStore::for_workspace(&cx.entity(), cx)
                        .map(|store| {
                            let store = store.read(cx);
                            store.board.is_some() || store.error.is_some()
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(true);
            if ready {
                break;
            }
            cx.background_executor().timer(Duration::from_millis(300)).await;
        }
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_panel::<agent_ui::AgentPanel>(window, cx);
                workspace
                    .right_dock()
                    .update(cx, |dock, cx| dock.set_open(false, window, cx));
                let Ok(spec) = std::env::var("MINI_ORK_IDE_OPEN") else {
                    return;
                };
                let latest = BoardStore::for_workspace(&cx.entity(), cx).and_then(|store| {
                    let store = store.read(cx);
                    let board = store.board.as_ref()?;
                    board
                        .runs
                        .iter()
                        .max_by_key(|run| run.started_at.unwrap_or_default())
                        .map(|run| (run.id.clone(), run.display_title()))
                });
                let entity = cx.entity();
                window.defer(cx, move |window, cx| {
                    for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                        let parts: Vec<&str> = entry.split(':').collect();
                        if parts[0] == "run" && parts.len() >= 2 {
                            let tab = parts.get(2).map(|t| t.to_string());
                            let (id, title) = if parts[1] == "latest" {
                                match latest.clone() {
                                    Some(run) => run,
                                    None => continue,
                                }
                            } else {
                                (parts[1].to_string(), String::new())
                            };
                            crate::page::open_run(&entity, id.clone(), title, tab, window, cx);
                            if parts.get(3) == Some(&"graph") {
                                let node = parts.get(4).map(|n| n.to_string());
                                let page = entity
                                    .read(cx)
                                    .items_of_type::<crate::page::MiniOrkPage>(cx)
                                    .find(|page| page.read(cx).args.get("run") == Some(&id));
                                if let Some(page) = page {
                                    page.update(cx, |page, _| page.pending_graph = Some(node));
                                }
                            }
                        } else {
                            let tab = parts.get(1).map(|t| t.to_string());
                            crate::page::open_page(&entity, parts[0].to_string(), tab, BTreeMap::new(), window, cx);
                        }
                    }
                });
            })
            .ok();
    })
    .detach();
}

impl BoardStore {
    pub fn for_workspace(workspace: &Entity<Workspace>, cx: &App) -> Option<Entity<BoardStore>> {
        cx.try_global::<Stores>()?
            .0
            .get(&workspace.entity_id())
            .cloned()
    }

    fn project_root(&self, cx: &App) -> Option<PathBuf> {
        let workspace = self.workspace.upgrade()?;
        let project = workspace.read(cx).project().read(cx);
        let worktree = project.visible_worktrees(cx).next()?;
        Some(worktree.read(cx).abs_path().to_path_buf())
    }

    fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            let (changed_tx, mut changed_rx) = futures::channel::mpsc::unbounded::<()>();
            let mut watched: Option<PathBuf> = None;
            loop {
                let Ok((bin, root)) = this.update(cx, |this, cx| (this.bin.clone(), this.project_root(cx)))
                else {
                    break;
                };
                let result = match root {
                    Some(root) => {
                        cx.background_spawn(async move {
                            let Some(home) = board::resolve_home(&root).await else {
                                return Err(anyhow::anyhow!(
                                    "This project has no .mini-ork — run `mini-ork init` in it."
                                ));
                            };
                            let board = board::fetch_board(bin, home.clone()).await?;
                            Ok((home, board))
                        })
                        .await
                    }
                    None => Err(anyhow::anyhow!("Open a project folder to see its mini-ork state.")),
                };
                let home = result.as_ref().ok().map(|(home, _)| home.clone());
                let updated = this.update(cx, |this, cx| {
                    match result {
                        Ok((home, board)) => {
                            this.home = Some(home);
                            this.announce_finished(&board, cx);
                            this.board = Some(board);
                            this.error = None;
                            this.generation += 1;
                        }
                        Err(error) => this.error = Some(error.to_string()),
                    }
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
                // Watch the home once it is known (and again if the project moves).
                if let Some(home) = home.filter(|home| watched.as_ref() != Some(home)) {
                    // A project opened in the IDE joins the project switcher's list.
                    let remembered = home.clone();
                    cx.background_spawn(async move { crate::projects::remember_home(&remembered) }).detach();
                    let fs = cx.update(|cx| <dyn fs::Fs>::global(cx));
                    let (mut events, watcher) = fs.watch(&home, WATCH_LATENCY).await;
                    this.update(cx, |this, _| this._watcher = Some(watcher)).ok();
                    let tx = changed_tx.clone();
                    cx.background_spawn(async move {
                        use futures::StreamExt as _;
                        while let Some(batch) = events.next().await {
                            if batch.iter().any(|event| relevant(&event.path))
                                && tx.unbounded_send(()).is_err()
                            {
                                break;
                            }
                        }
                    })
                    .detach();
                    watched = Some(home);
                }
                let active = cx.update(|cx| cx.active_window().is_some());
                cx.background_executor()
                    .timer(if active { MIN_GAP } else { MIN_GAP_INACTIVE })
                    .await;
                // Wait for the next change (or the safety read), then drain the
                // burst so one read covers it.
                {
                    use futures::{FutureExt as _, StreamExt as _};
                    let mut next = changed_rx.next().fuse();
                    let mut safety = cx.background_executor().timer(SAFETY_READ).fuse();
                    futures::select_biased! {
                        _ = next => {},
                        _ = safety => {},
                    }
                }
                while changed_rx.try_recv().is_ok() {}
            }
        });
        Self {
            workspace,
            home: None,
            bin: board::mini_ork_bin(),
            board: None,
            error: None,
            last_states: HashMap::new(),
            generation: 0,
            _watcher: None,
            _poll: poll,
        }
    }

    /// Read the board again now (after an action).
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let (Some(home), bin) = (self.home.clone(), self.bin.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(board::fetch_board(bin, home)).await;
            this.update(cx, |this, cx| {
                if let Ok(board) = result {
                    this.announce_finished(&board, cx);
                    this.board = Some(board);
                    this.generation += 1;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// A toast for every run that was working at the previous read and has
    /// finished now — "Show" opens its tab.
    fn announce_finished(&mut self, board: &Board, cx: &mut Context<Self>) {
        let first_read = self.last_states.is_empty();
        let finished: Vec<board::Run> = board
            .runs
            .iter()
            .filter(|run| run.state != "working")
            .filter(|run| self.last_states.get(&run.id).is_some_and(|state| state == "working"))
            .cloned()
            .collect();
        self.last_states = board
            .runs
            .iter()
            .map(|run| (run.id.clone(), run.state.clone()))
            .collect();
        if first_read {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        for run in finished {
            let verb = match run.state.as_str() {
                "needs_you" => "needs you",
                "failed" => "failed",
                _ => "finished",
            };
            let title = run.display_title();
            let mut message = format!("mini-ork {verb} · {}", short(&title, 80));
            if run.added > 0 || run.removed > 0 {
                message.push_str(&format!(" · +{} −{}", run.added, run.removed));
            }
            let (run_id, weak) = (run.id.clone(), workspace.downgrade());
            let toast = Toast::new(
                NotificationId::named(SharedString::from(format!("mini-ork-run-{}", run.id))),
                message,
            )
            .on_click("Show", move |window, cx| {
                if let Some(workspace) = weak.upgrade() {
                    crate::page::open_run(&workspace, run_id.clone(), title.clone(), None, window, cx);
                }
            })
            .autohide();
            workspace.update(cx, |workspace, cx| workspace.show_toast(toast, cx));
        }
    }
}

/// Writes that can change what the IDE shows: the database and run folders —
/// not lock files, caches or the IDE's own scratch.
fn relevant(path: &std::path::Path) -> bool {
    let text = path.to_string_lossy();
    if text.ends_with(".lock") || text.contains("/__pycache__/") || text.ends_with("-shm") {
        return false;
    }
    text.contains("state.db") || text.contains("/runs/") || text.contains("/worktrees/")
        || text.ends_with("automations.json")
}

pub fn short(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut: String = text.chars().take(limit).collect();
    format!("{cut}…")
}

/// Where an action came from, so a page can refresh itself afterwards.
pub enum Origin {
    Page(WeakEntity<crate::page::MiniOrkPage>),
}

/// Carry out a page action (see `mini_ork/ide_pages/spec.py`).
pub fn perform(
    action: &Value,
    workspace: &Entity<Workspace>,
    origin: Origin,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(args) = action.get("cli").and_then(Value::as_array) {
        let args: Vec<String> = args.iter().filter_map(|a| a.as_str().map(String::from)).collect();
        let with_home = action.get("home").and_then(Value::as_bool).unwrap_or(true);
        run_cli(args, with_home, workspace, origin, cx);
    } else if let Some(key) = action.get("page").and_then(Value::as_str) {
        let tab = action.get("tab").and_then(Value::as_str).map(String::from);
        let args = string_map(action.get("args"));
        // `page run <tab> --arg run=R --arg node=N` is the "inspect this node"
        // gesture — a story step, a flow-map node, a run-dock row. Open the
        // node's own tab (the run's DAG up top with the node selected, the
        // node's own detail below) rather than dropping `node` and merely
        // switching the run page's tab. Every other `page` action is untouched.
        if key == "run" {
            if let (Some(run_id), Some(node)) = (args.get("run").cloned(), args.get("node").cloned()) {
                if !node.is_empty() {
                    crate::node_tab::open_node_tab(
                        workspace,
                        run_id,
                        node,
                        String::new(),
                        None,
                        WeakEntity::new_invalid(),
                        window,
                        cx,
                    );
                    return;
                }
            }
        }
        crate::page::open_page(workspace, key.to_string(), tab, args, window, cx);
    } else if let Some(set) = action.get("set") {
        let Origin::Page(page) = origin;
        page.update(cx, |page, cx| page.set_args(string_map(Some(set)), cx)).ok();
    } else if let Some(run_id) = action.get("run").and_then(Value::as_str) {
        let title = action.get("title").and_then(Value::as_str).unwrap_or_default().to_string();
        let tab = action.get("tab").and_then(Value::as_str).map(String::from);
        crate::page::open_run(workspace, run_id.to_string(), title, tab, window, cx);
    } else if let Some(path) = action.get("path").and_then(Value::as_str) {
        let path = PathBuf::from(path);
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_abs_path(path, OpenOptions::default(), window, cx)
                .detach_and_log_err(cx);
        });
    } else if let Some(path) = action.get("reveal").and_then(Value::as_str) {
        cx.reveal_path(&PathBuf::from(path));
    } else if let Some(url) = action.get("url").and_then(Value::as_str) {
        cx.open_url(url);
    } else if let Some(text) = action.get("thread").and_then(Value::as_str) {
        new_thread(workspace, text, window, cx);
    } else if let Some(root) = action.get("project").and_then(Value::as_str) {
        crate::projects::switch_to(workspace, PathBuf::from(root), window, cx);
    } else if action.get("project_pick").and_then(Value::as_bool) == Some(true) {
        crate::projects::pick(workspace, window, cx);
    }
}

fn string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), v)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A new mini-ork thread in the Agent panel; `text` becomes its first prompt.
pub fn new_thread(workspace: &Entity<Workspace>, text: &str, window: &mut Window, cx: &mut App) {
    if let Ok(action) = cx.build_action(
        "agent::NewExternalAgentThread",
        Some(serde_json::json!({ "agent": "mini-ork" })),
    ) {
        window.dispatch_action(action, cx);
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    let prompt = agent_ui::ExternalSourcePrompt::new(&text);
    workspace.update(cx, |workspace, cx| {
        if let Some(panel) = workspace.panel::<agent_ui::AgentPanel>(cx) {
            panel.update(cx, |panel, cx| {
                panel.new_agent_thread_with_external_source_prompt(prompt, window, cx)
            });
        }
    });
}

/// Run `mini-ork <args>`; report the outcome as a toast and refresh.
pub fn run_cli(args: Vec<String>, with_home: bool, workspace: &Entity<Workspace>, origin: Origin, cx: &mut App) {
    let Some(store) = BoardStore::for_workspace(workspace, cx) else {
        return;
    };
    let (Some(home), bin) = ({
        let store = store.read(cx);
        (store.home.clone(), store.bin.clone())
    }) else {
        return;
    };
    let label = args.join(" ");
    let weak_workspace = workspace.downgrade();
    cx.spawn(async move |cx| {
        let outcome = cx.background_spawn(board::run_action(bin, home, args, with_home)).await;
        cx.update(|cx| {
            let message = match outcome {
                Ok(done) => format!("mini-ork {label}: {done}"),
                Err(error) => format!("mini-ork {label} failed: {error}"),
            };
            if let Some(workspace) = weak_workspace.upgrade() {
                let toast = Toast::new(NotificationId::named(SharedString::from("mini-ork-action")), message)
                    .autohide();
                workspace.update(cx, |workspace, cx| workspace.show_toast(toast, cx));
            }
            store.update(cx, |store, cx| store.refresh(cx));
            let Origin::Page(page) = origin;
            page.update(cx, |page, cx| page.reload(cx)).ok();
        });
    })
    .detach();
}

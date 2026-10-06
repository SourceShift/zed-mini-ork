//! The mini-ork dock panel: runs, review, learnings, automations, recipes.

use std::{path::PathBuf, time::Duration};

use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Pixels, Render, SharedString, Task, WeakEntity, Window, actions, px,
};
use ui::{AgentThreadStatus, Divider, ThreadItem, prelude::*};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

use crate::board::{self, Board};

actions!(
    mini_ork_panel,
    [
        /// Toggles focus on the mini-ork panel.
        ToggleFocus
    ]
);

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const TEXT_LIMIT: usize = 280;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<MiniOrkPanel>(window, cx);
        });
    })
    .detach();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Runs,
    Review,
    Learnings,
    Automations,
    Recipes,
}

impl Section {
    const ALL: [Section; 5] = [
        Section::Runs,
        Section::Review,
        Section::Learnings,
        Section::Automations,
        Section::Recipes,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::Runs => "Runs",
            Section::Review => "Review",
            Section::Learnings => "Learnings",
            Section::Automations => "Automations",
            Section::Recipes => "Recipes",
        }
    }
}

pub struct MiniOrkPanel {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    position: DockPosition,
    section: Section,
    root: Option<PathBuf>,
    home: Option<PathBuf>,
    bin: PathBuf,
    board: Option<Board>,
    error: Option<String>,
    selected_run: Option<String>,
    card: Option<String>,
    notice: Option<String>,
    _poll: Task<()>,
}

impl MiniOrkPanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |_workspace, _window, cx| {
            let weak = cx.entity().downgrade();
            cx.new(|cx| Self::new(weak, cx))
        })
    }

    /// The first folder of the project. Read on every refresh: a window opens
    /// before its folders are added, so the panel must not capture it once.
    fn project_root(&self, cx: &App) -> Option<PathBuf> {
        let workspace = self.workspace.upgrade()?;
        let project = workspace.read(cx).project().read(cx);
        let worktree = project.visible_worktrees(cx).next()?;
        Some(worktree.read(cx).abs_path().to_path_buf())
    }

    fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let Ok((bin, root)) = this.update(cx, |this, cx| {
                    this.root = this.project_root(cx);
                    (this.bin.clone(), this.root.clone())
                }) else {
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
                let updated = this.update(cx, |this, cx| {
                    match result {
                        Ok((home, board)) => {
                            this.home = Some(home);
                            this.board = Some(board);
                            this.error = None;
                        }
                        Err(error) => this.error = Some(error.to_string()),
                    }
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
                cx.background_executor().timer(POLL_INTERVAL).await;
            }
        });
        Self {
            workspace,
            focus_handle: cx.focus_handle(),
            position: DockPosition::Left,
            section: Section::Runs,
            root: None,
            home: None,
            bin: board::mini_ork_bin(),
            board: None,
            error: None,
            selected_run: None,
            card: None,
            notice: None,
            _poll: poll,
        }
    }

    fn select_run(&mut self, run_id: String, cx: &mut Context<Self>) {
        if self.selected_run.as_deref() == Some(run_id.as_str()) {
            self.selected_run = None;
            self.card = None;
            cx.notify();
            return;
        }
        self.selected_run = Some(run_id.clone());
        self.card = Some("Loading…".into());
        cx.notify();
        let (Some(home), bin) = (self.home.clone(), self.bin.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let card = cx
                .background_spawn(board::fetch_run_card(bin, home, run_id.clone()))
                .await;
            this.update(cx, |this, cx| {
                if this.selected_run.as_deref() == Some(run_id.as_str()) {
                    this.card = Some(card.unwrap_or_else(|error| error.to_string()));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn act(&mut self, args: Vec<String>, cx: &mut Context<Self>) {
        let (Some(home), bin) = (self.home.clone(), self.bin.clone()) else {
            return;
        };
        let label = args.join(" ");
        self.notice = Some(format!("Running `mini-ork {label}`…"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let outcome = cx.background_spawn(board::run_action(bin, home, args)).await;
            this.update(cx, |this, cx| {
                this.notice = Some(match outcome {
                    Ok(done) => format!("`mini-ork {label}`: {done}"),
                    Err(error) => format!("`mini-ork {label}` failed: {error}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_header(&self) -> impl IntoElement {
        let title = self
            .board
            .as_ref()
            .map(|board| board.project.clone())
            .filter(|project| !project.is_empty())
            .unwrap_or_else(|| "mini-ork".to_string());
        let counts = self.board.as_ref().map(|board| {
            let c = &board.counts;
            format!(
                "● {} working · ✋ {} need you · ✓ {} done · ✗ {} failed",
                c.working, c.needs_you, c.done, c.failed
            )
        });
        v_flex()
            .px_2()
            .pt_2()
            .pb_1()
            .gap_0p5()
            .child(Label::new(title).size(LabelSize::Default))
            .when_some(counts, |this, counts| {
                this.child(Label::new(counts).size(LabelSize::Small).color(Color::Muted))
            })
            .when_some(self.notice.clone(), |this, notice| {
                this.child(Label::new(notice).size(LabelSize::Small).color(Color::Accent))
            })
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let review_count = self.board.as_ref().map_or(0, |board| board.workspaces.len());
        h_flex()
            .px_1()
            .pb_1()
            .gap_0p5()
            .flex_wrap()
            .children(Section::ALL.into_iter().map(|section| {
                let label = if section == Section::Review && review_count > 0 {
                    format!("{} {}", section.label(), review_count)
                } else {
                    section.label().to_string()
                };
                Button::new(SharedString::from(format!("mini-ork-tab-{}", section.label())), label)
                    .label_size(LabelSize::Small)
                    .toggle_state(self.section == section)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        cx.notify();
                    }))
            }))
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(error) = &self.error {
            return self.message(error.clone()).into_any_element();
        }
        let Some(board) = self.board.as_ref() else {
            return self.message("Loading mini-ork…".into()).into_any_element();
        };
        match self.section {
            Section::Runs => self.render_runs(board, cx).into_any_element(),
            Section::Review => self.render_review(board, cx).into_any_element(),
            Section::Learnings => self.render_learnings(board).into_any_element(),
            Section::Automations => self.render_automations(board, cx).into_any_element(),
            Section::Recipes => self.render_recipes(board).into_any_element(),
        }
    }

    fn message(&self, text: String) -> impl IntoElement {
        div()
            .p_2()
            .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
    }

    fn render_runs(&self, board: &Board, cx: &mut Context<Self>) -> impl IntoElement {
        if board.runs.is_empty() {
            return v_flex().child(self.message(
                "No runs yet. Start one from a mini-ork thread in the Agent panel.".into(),
            ));
        }
        let now = now_epoch();
        v_flex().gap_px().children(board.runs.iter().map(|run| {
            let id = run.id.clone();
            let selected = self.selected_run.as_deref() == Some(run.id.as_str());
            let title = if run.title.is_empty() {
                run.id.clone()
            } else {
                run.title.clone()
            };
            let mut item = ThreadItem::new(SharedString::from(format!("mini-ork-run-{}", run.id)), title)
                .status(status_for(&run.state))
                .timestamp(age(run.started_at, now))
                .selected(selected)
                .on_click(cx.listener(move |this, _, _, cx| this.select_run(id.clone(), cx)));
            if run.added > 0 {
                item = item.added(run.added as usize);
            }
            if run.removed > 0 {
                item = item.removed(run.removed as usize);
            }
            v_flex()
                .child(item)
                .when(selected, |this| this.child(self.render_run_detail(run, cx)))
        }))
    }

    fn render_run_detail(&self, run: &board::Run, cx: &mut Context<Self>) -> impl IntoElement {
        let meta = format!(
            "{} · step {} · ${:.2}",
            run.recipe,
            if run.step.is_empty() { "—" } else { run.step.as_str() },
            run.cost_usd
        );
        let mut buttons = h_flex().gap_1();
        if run.has_workspace {
            let merge_id = run.id.clone();
            let discard_id = run.id.clone();
            buttons = buttons
                .child(
                    Button::new(SharedString::from(format!("merge-{}", run.id)), "Merge")
                        .style(ButtonStyle::Filled)
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.act(vec!["board".into(), "merge".into(), merge_id.clone()], cx)
                        })),
                )
                .child(
                    Button::new(SharedString::from(format!("discard-{}", run.id)), "Discard")
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.act(vec!["board".into(), "discard".into(), discard_id.clone()], cx)
                        })),
                );
        }
        if run.state == "working" {
            let stop_id = run.id.clone();
            buttons = buttons.child(
                Button::new(SharedString::from(format!("stop-{}", run.id)), "Stop")
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.act(vec!["board".into(), "stop".into(), stop_id.clone()], cx)
                    })),
            );
        }
        let card = self.card.clone().unwrap_or_default();
        v_flex()
            .px_3()
            .py_1()
            .gap_1()
            .child(Label::new(meta).size(LabelSize::XSmall).color(Color::Muted))
            .child(buttons)
            .children(
                card.lines()
                    .filter(|line| !line.trim().is_empty() && !line.starts_with("| ---"))
                    .take(40)
                    .map(|line| {
                        Label::new(clean_markdown(line))
                            .size(LabelSize::XSmall)
                            .color(if line.starts_with('#') {
                                Color::Default
                            } else {
                                Color::Muted
                            })
                    }),
            )
    }

    fn render_review(&self, board: &Board, cx: &mut Context<Self>) -> impl IntoElement {
        if board.workspaces.is_empty() {
            return v_flex().child(self.message(
                "Nothing waiting for review. Finished runs arrive as agent edits in your project — review them with Review Changes in the Agent panel.".into(),
            ));
        }
        v_flex().gap_2().p_2().children(board.workspaces.iter().map(|ws| {
            let merge_id = ws.run_id.clone();
            let discard_id = ws.run_id.clone();
            let title = board
                .runs
                .iter()
                .find(|run| run.id == ws.run_id)
                .map(|run| run.title.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| ws.run_id.clone());
            v_flex()
                .gap_0p5()
                .child(Label::new(title).size(LabelSize::Small))
                .child(
                    Label::new(format!(
                        "{} → {} · +{} −{} · {} file{}",
                        ws.branch,
                        ws.base,
                        ws.added,
                        ws.removed,
                        ws.files.len(),
                        if ws.files.len() == 1 { "" } else { "s" }
                    ))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
                )
                .children(ws.files.iter().take(12).map(|file| {
                    Label::new(file.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                }))
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new(SharedString::from(format!("review-merge-{}", ws.run_id)), format!("Merge into {}", ws.base))
                                .style(ButtonStyle::Filled)
                                .label_size(LabelSize::Small)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.act(vec!["board".into(), "merge".into(), merge_id.clone()], cx)
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("review-discard-{}", ws.run_id)), "Discard")
                                .label_size(LabelSize::Small)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.act(vec!["board".into(), "discard".into(), discard_id.clone()], cx)
                                })),
                        ),
                )
        }))
    }

    fn render_learnings(&self, board: &Board) -> impl IntoElement {
        if board.learnings.is_empty() {
            return v_flex().child(self.message(
                "No learnings yet — mini-ork records them as runs finish.".into(),
            ));
        }
        v_flex().gap_2().p_2().children(board.learnings.iter().map(|learning| {
            let kind = match learning.kind.as_str() {
                "gradient" => "Failure mode",
                "record" => "Learning",
                "pattern" => "Pattern",
                _ => "Note",
            };
            v_flex()
                .gap_0p5()
                .child(
                    Label::new(format!(
                        "{kind} · {} · confidence {:.0}%",
                        learning.target,
                        learning.confidence * 100.0
                    ))
                    .size(LabelSize::XSmall)
                    .color(Color::Accent),
                )
                .child(Label::new(truncate(&learning.text)).size(LabelSize::Small))
                .when(!learning.suggestion.is_empty(), |this| {
                    this.child(
                        Label::new(format!("→ {}", truncate(&learning.suggestion)))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                })
        }))
    }

    fn render_automations(&self, board: &Board, cx: &mut Context<Self>) -> impl IntoElement {
        let scheduler = if board.scheduler.installed {
            match &board.scheduler.last_tick {
                Some(tick) => format!("Scheduler on — last tick {tick}."),
                None => "Scheduler on — no tick yet.".to_string(),
            }
        } else {
            "Scheduler off — automations do not fire on their own.".to_string()
        };
        let mut list = v_flex().gap_2().p_2().child(
            h_flex()
                .gap_1()
                .child(Label::new(scheduler).size(LabelSize::XSmall).color(Color::Muted))
                .when(!board.scheduler.installed, |this| {
                    this.child(
                        Button::new("mini-ork-scheduler-on", "Turn on")
                            .label_size(LabelSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.act(vec!["automations".into(), "scheduler".into(), "install".into()], cx)
                            })),
                    )
                }),
        );
        if board.automations.is_empty() {
            return list.child(self.message(
                "No automations yet. In a mini-ork thread, ask to run a recipe on a schedule.".into(),
            ));
        }
        list = list.children(board.automations.iter().map(|automation| {
            let run_id = automation.id.clone();
            let toggle_id = automation.id.clone();
            let enabled = automation.enabled;
            v_flex()
                .gap_0p5()
                .child(
                    h_flex()
                        .gap_1()
                        .child(Label::new(if enabled { "●" } else { "⏸" }).size(LabelSize::Small).color(if enabled { Color::Success } else { Color::Muted }))
                        .child(Label::new(automation.name.clone()).size(LabelSize::Small))
                        .child(Label::new(automation.recipe.clone()).size(LabelSize::XSmall).color(Color::Muted)),
                )
                .child(
                    Label::new(format!(
                        "{} · next {} · last {}",
                        automation.when, automation.next, automation.last_run
                    ))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new(SharedString::from(format!("auto-run-{}", automation.id)), "Run now")
                                .label_size(LabelSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.act(vec!["automations".into(), "run".into(), run_id.clone()], cx)
                                })),
                        )
                        .child(
                            Button::new(
                                SharedString::from(format!("auto-toggle-{}", automation.id)),
                                if enabled { "Pause" } else { "Resume" },
                            )
                            .label_size(LabelSize::XSmall)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let verb = if enabled { "pause" } else { "resume" };
                                this.act(vec!["automations".into(), verb.into(), toggle_id.clone()], cx)
                            })),
                        ),
                )
        }));
        list
    }

    fn render_recipes(&self, board: &Board) -> impl IntoElement {
        v_flex().gap_2().p_2().children(board.recipes.iter().map(|recipe| {
            v_flex()
                .gap_0p5()
                .child(
                    h_flex()
                        .gap_1()
                        .child(Label::new(recipe.id.clone()).size(LabelSize::Small))
                        .child(
                            Label::new(format!(
                                "{} · grade {} · {} runs · {}% ok",
                                recipe.source,
                                recipe.grade_letter.clone().unwrap_or_else(|| "—".into()),
                                board::value_text(&recipe.runs),
                                board::value_text(&recipe.success_pct)
                            ))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                        ),
                )
                .child(
                    Label::new(truncate(&recipe.description))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
        }))
    }
}

fn status_for(state: &str) -> AgentThreadStatus {
    match state {
        "working" => AgentThreadStatus::Running,
        "needs_you" => AgentThreadStatus::WaitingForConfirmation,
        "failed" => AgentThreadStatus::Error,
        _ => AgentThreadStatus::Completed,
    }
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
    let secs = (now - started).max(0);
    match secs {
        0..60 => "now".into(),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    }
}

fn truncate(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= TEXT_LIMIT {
        return text.to_string();
    }
    let cut: String = text.chars().take(TEXT_LIMIT).collect();
    format!("{cut}…")
}

fn clean_markdown(line: &str) -> String {
    line.trim_start_matches('#')
        .replace("**", "")
        .replace('`', "")
        .trim()
        .to_string()
}

impl EventEmitter<PanelEvent> for MiniOrkPanel {}

impl Focusable for MiniOrkPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MiniOrkPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("mini-ork-panel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(self.render_header())
            .child(self.render_tabs(cx))
            .child(Divider::horizontal())
            .child(
                div()
                    .id("mini-ork-panel-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(self.render_body(cx)),
            )
    }
}

impl Panel for MiniOrkPanel {
    fn persistent_name() -> &'static str {
        "MiniOrkPanel"
    }

    fn panel_key() -> &'static str {
        "MiniOrkPanel"
    }

    fn position(&self, _: &Window, _: &App) -> DockPosition {
        self.position
    }

    fn position_is_valid(&self, _: DockPosition) -> bool {
        true
    }

    fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
        self.position = position;
        cx.notify();
    }

    fn default_size(&self, _: &Window, _: &App) -> Pixels {
        px(380.)
    }

    fn icon(&self, _: &Window, _: &App) -> Option<IconName> {
        Some(IconName::ListTodo)
    }

    fn icon_tooltip(&self, _: &Window, _: &App) -> Option<&'static str> {
        Some("mini-ork")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn starts_open(&self, _: &Window, _: &App) -> bool {
        true
    }

    fn activation_priority(&self) -> u32 {
        8
    }
}

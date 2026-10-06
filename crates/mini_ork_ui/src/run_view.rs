//! A mini-ork run as an editor tab: kickoff, steps, cost, changed files, the
//! diff and every artifact the run wrote — each file opens in the editor.

use std::{path::PathBuf, time::Duration};

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, Render, SharedString, Task,
    TaskExt as _, WeakEntity, Window,
};
use ui::{Divider, prelude::*};
use workspace::{OpenOptions, Workspace, item::Item};

use crate::board::{self, RunDetail};

const REFRESH_WHILE_RUNNING: Duration = Duration::from_secs(5);
const KICKOFF_LINES: usize = 120;
const DIFF_LINES: usize = 1500;

pub struct MiniOrkRunView {
    pub(crate) run_id: String,
    title: String,
    home: PathBuf,
    bin: PathBuf,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    detail: Option<Result<RunDetail, String>>,
    notice: Option<String>,
    _load: Task<()>,
}

impl MiniOrkRunView {
    pub fn new(
        run_id: String,
        title: String,
        home: PathBuf,
        bin: PathBuf,
        workspace: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let load = {
            let (run_id, home, bin) = (run_id.clone(), home.clone(), bin.clone());
            cx.spawn(async move |this, cx| {
                loop {
                    let detail = cx
                        .background_spawn(board::fetch_run_detail(bin.clone(), home.clone(), run_id.clone()))
                        .await;
                    let running = detail
                        .as_ref()
                        .map(|d| !matches!(d.card.status.as_str(), "published" | "failed" | "rolled_back"))
                        .unwrap_or(false);
                    if this
                        .update(cx, |this, cx| {
                            this.detail = Some(detail.map_err(|error| error.to_string()));
                            cx.notify();
                        })
                        .is_err()
                        || !running
                    {
                        break;
                    }
                    cx.background_executor().timer(REFRESH_WHILE_RUNNING).await;
                }
            })
        };
        Self {
            run_id,
            title,
            home,
            bin,
            workspace,
            focus_handle: cx.focus_handle(),
            detail: None,
            notice: None,
            _load: load,
        }
    }

    fn open(&self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_abs_path(path, OpenOptions::default(), window, cx)
                .detach_and_log_err(cx);
        });
    }

    fn act(&mut self, args: Vec<String>, cx: &mut Context<Self>) {
        let (home, bin) = (self.home.clone(), self.bin.clone());
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

    fn heading(title: &'static str) -> impl IntoElement {
        div()
            .pt_4()
            .pb_1()
            .child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
    }

    fn render_detail(&self, detail: &RunDetail, cx: &mut Context<Self>) -> impl IntoElement {
        let card = &detail.card;
        let verdict = card.verdict.clone().unwrap_or_else(|| "—".into());
        let verdict_color = match verdict.as_str() {
            "pass" => Color::Success,
            "fail" | "failed" => Color::Error,
            _ => Color::Muted,
        };
        let running = !matches!(card.status.as_str(), "published" | "failed" | "rolled_back");
        let run_dir = self.home.join("runs").join(&self.run_id);

        let mut actions = h_flex().gap_1();
        if detail.workspace.is_some() {
            let (merge_id, discard_id) = (self.run_id.clone(), self.run_id.clone());
            let base = detail.workspace.as_ref().map(|w| w.base.clone()).unwrap_or_default();
            actions = actions
                .child(
                    Button::new("run-merge", format!("Merge into {base}"))
                        .style(ButtonStyle::Filled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.act(vec!["board".into(), "merge".into(), merge_id.clone()], cx)
                        })),
                )
                .child(Button::new("run-discard", "Discard").on_click(cx.listener(move |this, _, _, cx| {
                    this.act(vec!["board".into(), "discard".into(), discard_id.clone()], cx)
                })));
        }
        if running {
            let stop_id = self.run_id.clone();
            actions = actions.child(Button::new("run-stop", "Stop").on_click(cx.listener(move |this, _, _, cx| {
                this.act(vec!["board".into(), "stop".into(), stop_id.clone()], cx)
            })));
        }
        actions = actions.child(
            Button::new("run-folder", "Open run folder").on_click(move |_, _, cx| cx.reveal_path(&run_dir)),
        );

        let costs = card
            .cost_by_stage
            .iter()
            .map(|(stage, cost)| format!("{stage} ${cost:.2}"))
            .collect::<Vec<_>>()
            .join("  ·  ");

        let mut body = v_flex()
            .gap_0p5()
            .child(Label::new(self.title.clone()).size(LabelSize::Large))
            .child(
                h_flex()
                    .gap_2()
                    .child(Label::new(format!("{} · {}", card.recipe, card.detail)).color(Color::Muted))
                    .child(Label::new(format!("verdict {verdict}")).color(verdict_color))
                    .child(Label::new(format!("${:.2}", card.cost_total)).color(Color::Muted))
                    .child(Label::new(self.run_id.clone()).size(LabelSize::Small).color(Color::Muted)),
            )
            .child(div().pt_2().child(actions))
            .when_some(self.notice.clone(), |this, notice| {
                this.child(Label::new(notice).size(LabelSize::Small).color(Color::Accent))
            });

        // Kickoff
        body = body.child(Self::heading("KICKOFF"));
        if let Some(path) = detail.kickoff.path.clone() {
            body = body.child(
                Button::new("run-open-kickoff", "Open the kickoff file")
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(move |this, _, window, cx| this.open(PathBuf::from(&path), window, cx))),
            );
        }
        let kickoff_lines: Vec<&str> = detail.kickoff.text.lines().collect();
        body = body.children(kickoff_lines.iter().take(KICKOFF_LINES).map(|line| {
            Label::new(line.to_string()).size(LabelSize::Small).buffer_font(cx).color(Color::Muted)
        }));
        if kickoff_lines.len() > KICKOFF_LINES || detail.kickoff.truncated {
            body = body.child(Label::new("… (open the kickoff file for the rest)").size(LabelSize::Small).color(Color::Muted));
        }

        // Steps
        body = body.child(Self::heading("STEPS")).children(card.steps.iter().map(|step| {
            let (mark, color) = match step.state.as_str() {
                "done" => ("✓", Color::Success),
                "failed" | "error" => ("✗", Color::Error),
                "running" | "working" => ("●", Color::Accent),
                _ => ("·", Color::Muted),
            };
            h_flex()
                .gap_2()
                .child(Label::new(mark).color(color))
                .child(Label::new(step.name.clone()))
                .child(
                    Label::new(format!(
                        "{} · {}{}",
                        step.kind,
                        step.lane,
                        step.seconds.map(|s| format!(" · {s}s")).unwrap_or_default()
                    ))
                    .color(Color::Muted),
                )
        }));
        if !costs.is_empty() {
            body = body
                .child(Self::heading("COST"))
                .child(Label::new(costs).color(Color::Muted));
        }

        // Files changed
        if !card.files.is_empty() {
            body = body.child(Self::heading("FILES CHANGED")).children(card.files.iter().enumerate().map(|(i, file)| {
                let row = h_flex()
                    .id(SharedString::from(format!("run-file-{i}")))
                    .gap_2()
                    .child(Label::new(file.path.clone()).color(if file.abs.is_some() { Color::Accent } else { Color::Default }))
                    .child(Label::new(format!("+{}", file.added)).color(Color::Created))
                    .child(Label::new(format!("−{}", file.removed)).color(Color::Deleted));
                match file.abs.clone() {
                    Some(abs) => row
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| this.open(PathBuf::from(&abs), window, cx)))
                        .into_any_element(),
                    None => row.into_any_element(),
                }
            }));
        }

        // Diff
        if !detail.diff.is_empty() {
            let lines: Vec<&str> = detail.diff.lines().collect();
            body = body.child(Self::heading("DIFF")).child(
                v_flex().children(lines.iter().take(DIFF_LINES).map(|line| {
                    let color = if line.starts_with("+++") || line.starts_with("---") {
                        Color::Default
                    } else if line.starts_with('+') {
                        Color::Created
                    } else if line.starts_with('-') {
                        Color::Deleted
                    } else if line.starts_with("@@") {
                        Color::Accent
                    } else {
                        Color::Muted
                    };
                    Label::new(line.to_string()).size(LabelSize::Small).buffer_font(cx).color(color)
                })),
            );
            if lines.len() > DIFF_LINES {
                body = body.child(
                    Label::new(format!("… {} more lines", lines.len() - DIFF_LINES))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                );
            }
        }

        // Artifacts, grouped
        if !detail.artifacts.is_empty() {
            body = body.child(Self::heading("ARTIFACTS"));
            let mut current = String::new();
            for (i, artifact) in detail.artifacts.iter().enumerate() {
                if artifact.group != current {
                    current = artifact.group.clone();
                    body = body.child(
                        div().pt_2().child(Label::new(current.clone()).size(LabelSize::Small).color(Color::Default)),
                    );
                }
                let abs = artifact.abs.clone();
                body = body.child(
                    h_flex()
                        .id(SharedString::from(format!("run-artifact-{i}")))
                        .gap_2()
                        .cursor_pointer()
                        .child(Label::new(artifact.path.clone()).color(Color::Accent))
                        .child(Label::new(size_text(artifact.size)).size(LabelSize::Small).color(Color::Muted))
                        .on_click(cx.listener(move |this, _, window, cx| this.open(PathBuf::from(&abs), window, cx))),
                );
            }
        }
        body
    }
}

fn size_text(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

impl EventEmitter<()> for MiniOrkRunView {}

impl Focusable for MiniOrkRunView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for MiniOrkRunView {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        let mut title: String = self.title.chars().take(40).collect();
        if self.title.chars().count() > 40 {
            title.push('…');
        }
        format!("mini-ork: {title}").into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::ListTodo))
    }

    fn tab_tooltip_text(&self, _: &App) -> Option<SharedString> {
        Some(format!("{} ({})", self.title, self.run_id).into())
    }
}

impl Render for MiniOrkRunView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.detail {
            None => Label::new("Loading the run…").color(Color::Muted).into_any_element(),
            Some(Err(error)) => Label::new(error.clone()).color(Color::Error).into_any_element(),
            Some(Ok(detail)) => {
                let detail = detail.clone();
                self.render_detail(&detail, cx).into_any_element()
            }
        };
        v_flex()
            .id("mini-ork-run-view")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(
                div()
                    .id("mini-ork-run-view-body")
                    .size_full()
                    .overflow_y_scroll()
                    .p_4()
                    .child(body)
                    .child(Divider::horizontal()),
            )
    }
}

/// Open the run's tab, or bring it forward when it is already open.
pub fn open_run_tab(
    workspace: &Entity<Workspace>,
    run_id: String,
    title: String,
    home: PathBuf,
    bin: PathBuf,
    window: &mut Window,
    cx: &mut App,
) {
    workspace.update(cx, |workspace, cx| {
        let existing = workspace
            .items_of_type::<MiniOrkRunView>(cx)
            .find(|view| view.read(cx).run_id == run_id);
        if let Some(existing) = existing {
            workspace.activate_item(&existing, true, true, window, cx);
            return;
        }
        let weak = cx.entity().downgrade();
        let view = cx.new(|cx| MiniOrkRunView::new(run_id, title, home, bin, weak, cx));
        workspace.add_item_to_active_pane(Box::new(view), None, true, window, cx);
    });
}

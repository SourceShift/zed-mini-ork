# Orca → mini-ork parity matrix

Every capability Orca ships, classified against what mini-ork has. Evidence is
a path on each side; where mini-ork has no implementation the row says what was
searched, so a later reader can tell "absent" from "not looked for".

Statuses are defined in `README.md`. Two conventions matter when reading:

- **parity by inheritance.** The IDE is a Zed fork. Git staging, terminal
  splits, the file finder and the diff view come from the host editor. Those
  rows are `parity`, with the Zed crate named as the evidence — no mini-ork
  code implements them.
- **the unit differs.** Orca organises around a *worktree you attach agents
  to*; mini-ork organises around a *run that owns its worktree*
  (`mini_ork/workspaces.py`, `--workspace worktree|in-place`). A row that says
  "worktree creation" is `partial` because the verb exists but the mental model
  a user brings is rejected at step one.

Counts: **14 parity · 35 partial · 1 cli-only · 14 missing · 4 not-applicable**
across 68 rows. Rows 1–49 come from the renderer inventory of `sources/orca`
(`src/renderer`, `src/preload`, `src/shared`); rows 50–59 from the orchestration
inventory of the same checkout (`AGENTS.md`, the `orca` verb surface); rows
60–68 from both plus the fork's `crates/`.

## 1. App shell and navigation

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 1 | Multi-view shell with a left sidebar, workbench, right sidebar | `src/renderer/src/app-shell/AppWorkspaceShell.tsx:91-248` | Zed workbench + 12-entry activity bar (`crates/mini_ork_ui/src/sidebar.rs`) + 20 pages (`mini_ork/ide_pages/__init__.py`) | parity |
| 2 | Command palette with worktree, tab, project, settings, task rows | `components/use-worktree-jump-palette-list-entries.ts:40-231` | `⌘J` jump palette lists runs and pages only (`crates/mini_ork_ui/src/jump.rs`) | partial — no settings deep-links, no task or file rows |
| 3 | File quick-open | `components/QuickOpen.tsx` | Zed file finder | parity (inherited) |
| 4 | Recent-tab switcher overlay | `components/tab-bar/RecentTabSwitcher.tsx` | Zed tab commands in the palette; no overlay | partial |
| 5 | Back/forward worktree history | `TitlebarLeftControls.tsx:26-153` | Zed per-pane nav history; nothing across runs | partial |
| 6 | Empty-workspace landing with add-project / create-workspace | `components/Landing.tsx:264-300` | `setup` page; `onboard`/`init` are shell-only and the setup page never emits them | partial — the landing exists, its two buttons do not |
| 7 | Crash report capture | `AppRootSurfaces.tsx:129-388` | Zed `crates/crashes` | parity (inherited, different sink) |
| 8 | Update card | `AppRootSurfaces.tsx:129-388` | Zed `crates/auto_update_ui` | parity (inherited) |

## 2. Workspaces and worktrees

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 9 | New-workspace composer (project, agent, name, branch, note, setup script, create-more) | `components/NewWorkspaceComposerModal.tsx` | The `orch` page drafts a kickoff from one line (`orch.py`, `kickoff new`) and starts a run per kickoff row with the picked recipe and workspace (`orch.py`, `board start --kickoff … --recipe … --workspace …`); a worktree is minted by the run (`mini_ork/workspaces.py`). No project/branch/setup-script fields, no create-more | partial |
| 10 | Worktree list: host badge, status dot, unread, pin | `sidebar/WorktreeList.tsx`, `WorktreeCardReviewDetailSection.tsx` | `runs` page lists runs with state; no pin, sleep or unread state | partial |
| 11 | Worktree context menu (rename, open in editor, copy path, pin, mark read, sleep, delete) | `sidebar/WorktreeContextMenuView.tsx:158-381` | `board discard`, `board kill` emitted by `changes`/`outcome` pages; no rename, pin, sleep | partial |
| 12 | Workspace board (Kanban drawer, drag, lane resize) | `sidebar/WorkspaceKanbanDrawer.tsx`, `WorkspaceKanbanLaneGrid.tsx` | `kanban` page (`mini_ork/ide_pages/kanban.py`) | parity |
| 13 | Add repo: clone URL, nested repos, non-git folder, add-from-folder | `sidebar/AddRepoDialog.tsx`, `NonGitFolderDialog.tsx` | The `setup` page's readiness tab emits `onboard --dry-run --workspace <project>` and, in a real terminal, `onboard --workspace <project>` (`setup.py`). It onboards the project the page belongs to; no clone-URL, nested-repo or add-from-folder dialog | partial |
| 14 | Start a workspace from an issue / PR / Linear / GitLab item | `task-page/TaskPage.tsx`, `SourceBar.tsx` | `pm` providers github/jira/linear/local (`mini_ork/pm/providers`), `intake` polling; no per-item start action, no GitLab provider | partial (GitLab: missing) |
| 15 | `orca.yaml` setup and archive hooks (`run` / `skip` / `inherit`) | `NewWorkspaceComposerModal.tsx`, `components/worktree-creation/*` | Nothing: `rg 'scripts\.setup\|archive hook\|hooks\.yaml' mini_ork/` returns no hits | missing |
| 16 | Parent/child worktree lineage | `SidebarAgentsList.tsx` child rows | Epic children exist (`epics` CLI) but the relation is not shown on a run or worktree | partial |
| 17 | External worktrees visible in the current workspace | `sidebar/WorktreeList.tsx` | `changes` page lists the project's other git worktrees (`mini_ork/ide_pages/changes.py`) | partial |
| 18 | Space manager: disk usage per workspace, bulk delete | `workspace-space/WorkspaceSpacePage.tsx` | Nothing; `sandbox-gc` reclaims sandboxes, not worktrees | missing |

## 3. Tabs, panes and workbench

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 19 | Terminal tabs, split right/down, focus, equalize | `terminal-pane/TerminalPane.tsx` | Zed `crates/terminal_view` | parity (inherited) |
| 20 | Terminal context menu: copy/paste/select-all, clear, set title, search | `terminal-pane/TerminalContextMenu.tsx` | Zed terminal menu | parity (inherited) |
| 21 | Floating terminal panel (drag, resize, maximize) | `floating-terminal/FloatingTerminalPanel.tsx` | Zed dock positions only; no floating panel | partial |
| 22 | Quick Commands: saved terminal commands run in the active host | `tab-bar/TabBarQuickCommandsMenu.tsx`, `settings/QuickCommandsPane` | Zed `tasks.json` tasks; mini-ork `recipes` page is a different concept (workflow recipes) | partial |
| 23 | PTY sessions survive app restart via a daemon | `orcad` (bundled CLI, `orca orchestration` verbs) | None; a run's processes die with the dispatcher | missing |
| 24 | Browser pane: address bar, profiles, element grab, annotate, send to agent | `browser-pane/*`, `browser-pane/annotate/*` | No webview crate in the fork (`ls crates \| rg web\|browser` finds only `gpui_web`, `web_search`) | missing |
| 25 | Mobile emulator pane | `emulator-pane/EmulatorPane.tsx` | Nothing | missing |
| 26 | Editor / changes / diff tabs, markdown preview | `editor/ChangesModeView.tsx`, `DiffViewer.tsx` | Zed `editor`, `markdown_preview`, `git_ui`, plus the `changes` page | parity |
| 27 | Conflict review surface | `editor/EditorConflictReviewSurface.tsx` | Zed merge-conflict editor | parity (inherited) |

## 4. Right sidebar

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 28 | File explorer with create/rename/delete, search in files | `right-sidebar/useFileExplorerTree.ts` | Zed project panel | parity (inherited) |
| 29 | Source control: stage/unstage/discard, commit message with AI generation, commit/push/sync | `right-sidebar/source-control/commit/commit-area.tsx` | Zed `git_ui` panel; the `changes` page's only git verbs are `board merge` and `board discard` | partial — no PR create/link, no per-file discard from mini-ork's page |
| 30 | CI checks panel with rerun | `right-sidebar/checks-panel*` | Nothing | missing |
| 31 | Ports panel (forwarded ports, open/copy) | `right-sidebar/local-port-row.tsx` | Nothing | missing |
| 32 | Plugin-contributed sidebar panels | `right-sidebar/PluginPanel.tsx` | Zed extensions model (language/theme, not panels) | not-applicable — different extension contract |
| 33 | Session search / AI Vault: search indexed agent history, resume, open log | `right-sidebar/AiVaultPanel.tsx`, `AiVaultSessionActionMenuItems.tsx:62-165` | ContextNest: `cn search`, `cn_session`, `cn_file`, `context` pages (`mini_ork/ide_pages/cn_session.py`, `search.py`) | partial — corpus is mini-ork's own sessions, no "resume" action |
| 34 | Pull-request page: conversation, checks, files, reviewers, merge | `pull-request-page/page/tabs-shell.tsx:86-223` | No PR page; review gates live in `verify` and `review` CLI | partial |
| 35 | Inline diff notes, notes shelf, "send notes to agent" | `diff-comments/DiffCommentPopover.tsx:43-230`, `editor/DiffNotesSendMenu.tsx:62-102` | `review findings` CLI only; no note creation surface | cli-only |

## 5. Status bar

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 36 | Provider usage roster and per-account switchers | `status-bar/UsageRosterPanel.tsx`, `ClaudeSwitcherMenu.tsx` | `lanes` page shows usage and emits `usage-report`, plus `lifetime summary` and `lifetime conductor-history` from its header (`lanes.py`); `providers` has `status`/`configure` with no page action | partial |
| 37 | Resource manager: per-workspace CPU/mem, session inventory, kill | `status-bar/WorkspaceSpaceManagerPanel.tsx`, `use-resource-usage-actions.ts:222` | Nothing; `nodes` page covers remote nodes, not local processes | missing |
| 38 | Status-bar segment visibility menu | `status-bar/StatusBarVisibilityMenu.tsx` | Zed status bar items are not user-selectable | missing |
| 39 | Agent-completion notifications, sounds, macOS permission card | `NotificationCardStack.tsx`, `hooks/agent-hook-completion-notifications.ts` | Zed `crates/notifications` plus the `inbox` page; mini-ork run completion is not wired to a desktop toast | partial |

## 6. Feature pages

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 40 | Tasks page over GitHub / GitLab / Linear / Jira, start workspace from item | `task-page/TaskPage.tsx`, `settings/TasksPane.tsx` | `kanban` sources pane emits `pm add\|test\|remove` for github/linear/jira and shows each source's declared/enabled/skipped/errored state, which is what `pm sources` prints — the pane supersedes the verb, so no button is owed; `orch` emits `intake once`; no row starts a run from an item; GitLab has no provider | partial (GitLab: missing) |
| 41 | Automations page and editor (prompt, project, agent, schedule, precheck) | `automations/AutomationEditorDialog.tsx` | `autos` page emits run/pause/resume/remove/scheduler; `mini_ork/automations.py` has `update` but the CLI has no `automations edit` | partial — no editor, one verb short |
| 42 | Automation runs dashboard with host filter | `automations/AutomationRunsDashboard.tsx` | `automations list` + `autos` page rows; no run history view | partial |
| 43 | Artifacts page: preview, publish public link, copy/open, delete | `artifacts/ArtifactsPage.tsx`, `ArtifactPublishButton.tsx` | Run/node artifacts (`ide_pages/node_artifacts.py`, `board artifacts`); no share link (would need an account and a host) | partial |
| 44 | Skills: browse, install to target agents, share bundle, freshness update | `skills/SkillsPage.tsx`, `SkillDetailDialog.tsx:81-184` | `skills list/show/validate/promote` CLI + Zed `crates/agent_skills`; no mini-ork skills page, no share | partial |
| 45 | Activity inbox: thread list/detail, scope filters, clear unread | `activity/ActivityPrototypePage.tsx` | `inbox` page (`mini_ork/ide_pages/inbox.py`, 1126 lines) | parity |
| 46 | Agent dashboard: Kanban of agent sessions, terminal preview | `dashboard/AgentDashboardDrawer.tsx`, `dashboard-popout/AgentKanbanBoard.tsx` | `kanban`, `orch`, `runs`, `dags` pages | parity |
| 47 | Orca Mobile: pairing QR, paired devices, relay | `mobile/MobilePage.tsx`, `MobileHeroPairingStep.tsx` | Nothing; no mobile client | not-applicable — no mobile target exists |
| 48 | Notification center and permission card | `NotificationCardStack.tsx` | Zed notifications + `inbox` page | partial |
| 49 | Global search across worktrees, docs, tasks | `cmd-j/palette-filter.ts`, `use-worktree-jump-palette-list-entries.ts` | `⌘J` over runs/pages; `search` page over ContextNest | partial |

## 7. Orchestration (Orca's core, and the closest comparison)

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 50 | Orchestration runs bound to a coordinator agent | `orca orchestration` verbs; `AGENTS.md` orchestration section | `run`/`orch` pages, `board start\|node\|gate`, dispatcher (`mini_ork/cli/board.py`) | parity |
| 51 | Typed inter-agent messages (status, dispatch, worker_done, merge_ready, escalation, handoff, decision_gate, question, heartbeat) and `@all`/`@idle` groups | `orca orchestration send` verb docs, `AGENTS.md` | `concord send/inbox/ack/claims/violations` + `threads`; one untyped message shape | partial — no message taxonomy, no broadcast groups |
| 52 | Tasks: create, list, update, dispatch with a preamble | `orca task` verbs | `epics ingest\|split\|priority\|ready`, `specs ingest\|lint`, `board node`; no preamble concept | partial |
| 53 | Supervised workers: start, show, read, stop, abandon, release, retain, list | `orca worker` verbs | Dispatcher supervises runs; the `runs` page emits `reap --dry-run`/`reap` and `supervise --once --dry-run`/`supervise --once` (`runs.py`), the `run` page emits `repair --dry-run`/`repair` for a failed run (`run.py`); `board kill`, `board node`, `board steer`; no release/retain/abandon vocabulary | partial |
| 54 | Decision gates raised and answered by an agent | `orca gate` verbs, `NativeChatApprovalCard.tsx` | `board gate approve\|reject` emitted by the `verify` page; `board answer` CLI | parity |
| 55 | Reset orchestration state | `orca orchestration reset` | Nothing | missing |
| 56 | Agent message inbox with acks and overlap claims | `orca message` verbs, `concord` in mini-ork | `concord inbox` partially reachable (`orch.py:1093`); acks and claims have no page | partial |

## 8. Remote and multi-host

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 57 | Serve, hosts, environments, SSH worktrees, mobile relay | `orca serve\|hosts\|environments` verbs; `settings/servers` pane | `serve` CLI + `nodes` page (docker/host node agents, `mini_ork/cli/nodes.py`) | partial — no pairing, no SSH worktree model |
| 58 | `orcad` supervisor daemon that keeps sessions alive | bundled CLI, `orca orchestration` | None | missing |
| 59 | SSH remote workspaces with port forwarding | `right-sidebar/local-port-row.tsx`, `settings/ssh` pane | Nothing | missing |

## 9. Agents

| # | Orca capability | Orca evidence | mini-ork | Status |
|---|---|---|---|---|
| 60 | Agent accounts/profiles per provider, account switching | `settings/AgentsPane.tsx`, `ClaudeSwitcherMenu.tsx` | `lanes` page + `providers configure` + `config/agents.yaml` overlays (`MO_NODE_SETTINGS`); no switcher | partial |
| 61 | Routing and lane assignment per worktree | `AgentRuntimeSetting.tsx` | Lane routing (`config/agents.yaml`, `MO_ROUTING_POLICY`) with a read-only `lanes` page | partial |
| 62 | Default agent, permissions mode, launch defaults, MCP config | `settings/AgentsPane.tsx`, `NativeChatSessionOptionPickers.tsx` | `features set\|unset\|set-env`, `mcp add\|list\|rm` CLI; `setup` page toggles features only | partial |
| 63 | Rich agent chat: tool runs, subagent roster, background tasks, approvals, composer with queue/slash/skills | `native-chat/*` (see `native-chat-types.ts:43-258`) | Zed `crates/agent_ui`, `acp_thread`, `acp_tools`; mini-ork thread cards drawn from ACP `_meta` (`agent_ui` commit `f06b8d3719`) | partial — host chat exists, mini-ork-specific run context is thin |
| 64 | Rewind a conversation turn | `NativeChatRewindAction.tsx` | Nothing in the fork's ACP surface | missing |
| 65 | Planning approval, plan-approval markdown card | `native-chat-interactive-prompt.ts:45-105` | mini-ork `plan` CLI is shell-only; Zed approval cards cover tool permissions, not plans | partial |

## 10. Platform features Orca has that mini-ork does not target

| # | Orca capability | Why | Status |
|---|---|---|---|
| 66 | Voice / dictation (`settings/voice`, dictation in composer) | The fork has no dictation surface and mini-ork has no speech pipeline; adding one is a host-editor feature, not mini-ork's product axis. Revisit if the fork ever ships dictation. | not-applicable |
| 67 | Computer use / browser automation (`settings/computer-use`, `browser-use`) | Requires a webview and a driver. The fork has no webview crate, so this is a missing platform, not a missing button; recorded again on rows 24/25 rather than pretending a page action would close it. | missing |
| 68 | Mobile app and relay | No mini-ork mobile client exists; Orca's page exists only to pair one. If a mobile client is ever built, the pairing page becomes a `cli-only` gap against `serve`. | not-applicable |

## What this says about cost

The cheap set is the last `cli-only` row (35) plus the `partial` rows that
are one verb or one button away (9, 10, 11, 17, 36, 41, 42, 53, 56, 61): the
engine verb already works and only a page action or a single CLI verb is
missing. What is left, in order of value per line changed:

1. **Authoring, row 41** — an `automations edit` verb (the module already has
   `update`), and a review surface for the drafts the agent produces
   (`mcp:draft_kickoff`, `mcp:draft_recipe`). Row 9's composer now drafts and
   starts; it cannot show a draft produced elsewhere.
2. **Tasks, row 40** — no row starts a run from a task item; GitLab has no
   provider. (`pm sources` needs no button: the `kanban` sources pane renders
   the same declared/enabled/skipped/errored state.)
3. **Worktrees, rows 10/11/17** — rename, pin, sleep and unread are states
   mini-ork does not model; the `runs` list is the closest surface and has no
   context menu. These need a decision before a button.
4. **Routing, rows 53/56/61** — release/retain/abandon vocabulary, acks and
   overlap claims, per-worktree lane assignment. Each is a new verb family, not
   an emitted action.

Closed this pass (the pages now emit the verb that already worked):

- **Onboarding, row 13** — `onboard --dry-run` and `onboard` on the `setup`
  page's readiness tab.
- **Spend, row 36** — `lifetime summary` and `lifetime conductor-history` on
  the `lanes` header.
- **Failure forensics, rows 50/53** — `reap`, `supervise` on the `runs` header;
  `repair` on a failed run's own header; the run's graph patches as a table
  with `graph-patch validate`/`apply` per row.
- **Verifier calibration** — unlabelled `verifier_results` rows on the
  `verify` gates tab, each labelable with `verifier annotate` (false positive /
  false negative). No Orca row maps to it; it is mini-ork's own loop and it had
  no caller at all.

The `partial` set splits into two kinds and should not be planned together:

- **one verb or one button away** — rows 9, 10, 11, 17, 36, 41, 42, 53, 56, 61.
- **needs a design decision first** — rows 14 (GitLab provider), 18 (worktree
  GC), 22 (quick commands vs recipes), 23 (daemon), 43 (share links need a
  host), 51 (message taxonomy), 63 (how much run context belongs in chat).

The `missing` set is Orca's platform bet, not its feature list: a browser, an
emulator, a daemon, SSH workspaces, a mobile relay. Each is a product axis
mini-ork can decline explicitly; none closes with a page action.

# Agent thread — the design pass

Reference: `mini-ork IDE.dc.html`, screen "Agent thread" (`t1`, the
dark-mode run). Builds on `2026-10-18-thread-cards-design.md`; nothing in
that design is reopened here.

## What the design shows, and where each piece lives

| Design element | Lives in | Status after this pass |
|---|---|---|
| Thread header mark + state (`✓ … ready to review · +38 −4`) | Engine `_emit_title_state_update` → Zed thread title; mini-ork rail's Threads board | already there (title text); the mark is the board's |
| `code-fix` tag, `↻ Replay` | — | not done; Replay is the mock's simulation control |
| **Plan strip** (`Plan · run-a41c ▬▬ 9/9`, node chips) | Engine `AgentPlanUpdate` (Z6) → `ThreadView::render_plan_summary` | bar + `done/total` count added; chips are the expanded list |
| Task bubble | Zed user message | already there |
| `Explored the project · 5 reads and searches` groups | — | not done (stock tool calls stay flat) |
| Orchestrator prose, `Learning applied: g-118` | Engine agent message | engine-owned text |
| **Run card**: `✓ run <id> [recipe] worktree … $cost · time ⤢`, node rows with lane dot / status / cost, `Open run` / `Certify` | `agent_ui::thread_card::render_run_card` | done (see below); `Certify` is not offered by the engine yet |
| Files changed · Keep / Reject | Zed edited-files bar (`render_edited_files`) | already there |
| Composer pickers (Orchestrate / Opus / code-fix / New worktree) | Engine ACP session modes + config options → Zed `mode_selector`, `config_options_view` | already there |
| Footer `thread $0.47` | Engine `usage_update` cost → Zed `render_token_usage` | already there |
| Footer `$4.12 of $50 today` | mini-ork title bar / status bar | already there (outside the thread) |

## The run card

The engine's run marker (`<run_id>:parent`) is opened before any node runs,
so its card has no rows; every row arrives as a flat `node` sibling card,
and the IDE suppresses those entries. The card now **folds** the siblings:

- `collect_siblings` takes every tool call whose id starts with `<run_id>:`
  (or whose card names the marker as `parent`), and `fold_steps` merges them
  into the marker's own `steps` — one row per node, first-seen order, a later
  card (or an equal-or-higher `revision`) updating its row in place while
  keeping fields it leaves out (the lane only the start card named).
- A card of **any kind** sitting on the marker is drawn as the run card. The
  engine's review and lane-repair offers land on the marker with a `review`
  card and replace its sticky `_meta`; the rows survive and the offer's
  buttons join them.
- Header: status mark, `run <id>` (mono), recipe chip, `workspace branch`,
  `$cost · duration` lifted out of the `kv` rows; `⤢` and `Open run` open
  the run page; a node row opens the node's tab.
- Lane dots use the design's family hues (`lane_color`), the same ones the
  mini-ork pages use; `shell` has no family and draws muted.
- A **working** node shows the last four non-empty lines of its live text
  under its row (`tail_lines`, each line clipped at 160 chars). The engine
  folds a node's stream into the content chunks of its tool call (design
  §2.7), so the host reads them off the suppressed sibling entry; the node
  tab has the whole stream.
- A `command` action puts `/<command>` in the composer and focuses it
  (`agent::PrefillMessageEditor`); a protected draft is never overwritten.

### The navigation seam

`mini_ork_ui` depends on `agent_ui`, so the card cannot call the page
openers. It dispatches **by name** — `mini_ork::OpenRun {run, title}` and
`mini_ork::OpenNode {run, node, title}` — through `App::build_action`, and
only draws those buttons when `cx.all_action_names()` lists the action. The
actions are registered on every workspace by `mini_ork_ui::run_panel::init`.
A build without `mini_ork_ui` draws the same card minus navigation.

### The engine's close

`_close_marker_update` re-emits the marker's close (`completed` / `failed`)
with a run card built by the same `run_card` builder from the **final
snapshot** — rows, verdicts, cost — on all three close paths (follower,
delivered-to-Zed, race). The close is recorded on the thread like the open,
so a replay shows the finished run without re-deriving it.

## Decided against, for now

- **Grouping consecutive read-only tool calls** into one `Explored …` row.
  It reshapes stock Zed's thread rendering for every agent (and its tests)
  for one visual gain; a later pass can gate it on the mini-ork agent.
- **`Certify this change`** on a published run. The ACP agent advertises no
  `/certify` command (the `mini_ork.certify` package is CLI-only today), so
  the button would send a command nothing answers. The `command` action
  path is ready for it once the engine offers one.
- **Per-node cost.** `llm_calls` still carries no node column (design §2.6);
  the row's cost column draws itself as soon as a card supplies `cost`.

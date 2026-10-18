# Judge panel: inline thread cards for the mini-ork IDE

## Context

The mini-ork IDE is this Zed fork (`/Volumes/docker-ssd/ps/zed-mini-ork`,
branch `mini-ork`). The engine is the mini-ork checkout at
`/Volumes/docker-ssd/ps/mini-ork`. The thread view is Zed's ACP Agent panel
(`crates/agent_ui/src/conversation_view/thread_view.rs`). mini-ork talks to it
over ACP; every run, node and approval currently renders as a generic tool
call with a title, a status and, for approvals, a permission row.

The redesign asks for three inline comps:

- **A — kickoff draft**: header `✋ <task> · kickoff ready to check ·
  [<recipe>]`; user task bubble; collapsible exploration group; a **Kickoff
  draft** card (file path, TASK / IN SCOPE / PROVES SUCCESS sections,
  checklist rows, note) with Start run / Save only / Change something /
  Discard.
- **B — run success**: header `✓ <task> · +A −D · published · [<recipe>]`;
  run card (run id, recipe, workspace, cost, duration, node rows with lane
  dot, model, status, cost); prose blocks (Where it landed / Not committed /
  Checks / Cost).
- **C — failure**: header `✗ <task> · <gate> rc=1 · [<recipe>]`; run card
  with failed node row and inline stderr; failure card (`✗ <gate> failed
  (rc=1)`, node, attempt, checkpoint, Healer / Recovery planner / Related
  learning, STDERR block) with Recover / Resume from &lt;node&gt; / Open
  forensics.

The proposed decision is a **data-driven card envelope** carried in ACP
`_meta`, a small block vocabulary, and one renderer-registry hook in
`agent_ui` whose renderers live in the `mini_ork_ui` crate. No new ACP
message type. Nothing has been implemented yet.

Your job: verify each premise against the live repositories and grade each
proposed solution. Be adversarial. Where a solution is weak, name a better
alternative in its verdict. Do not edit repository source.

## Pending tasks (premises to verify)

### T1 — ACP `_meta` is available end to end

Claim: `ToolCallStart`, `ToolCallProgress` and permission `ToolCallUpdate`
can all carry `_meta`; Rust stores it on `acp_thread::ToolCall` and the engine
persists it, so a card payload can ride an ordinary tool call.

Receipts to check:

- `crates/acp_thread/src/acp_thread.rs` — `pub struct ToolCall`, the
  `meta: Option<acp_v2::Meta>` field, `ToolCall::from_acp`, `update_fields`,
  and `upsert_tool_call_patch`.
- `crates/agent_servers/src/acp.rs` — where incoming tool-call updates are
  applied to a thread.
- `mini_ork/acp/agent.py` — `_open_run_marker`, `_build_event_updates`,
  `_emit`, and every `ToolCallStart` / `ToolCallProgress` construction.
- `mini_ork/acp/threads.py` — `_record_thread_update` /
  `ThreadStore.append`; check whether `_meta` survives
  `model_dump(mode="json", by_alias=True, exclude_none=True)`.

Questions the judge must answer: is `_meta` actually accepted by the Python
ACP dataclasses used by this engine version? Does Rust preserve it through
upserts (does a later update without `_meta` erase the earlier one)? Are
routed run updates recorded to the thread store or skipped?

### T2 — A permission request can exist without a preceding `ToolCallStart`

Claim: the engine can push a card by calling `request_permission` alone
(today `_offer_node_review` does exactly this with
`tool_call_id = f"{run_id}:review"`), and a custom renderer can read the
options and answer them.

Receipts to check:

- `crates/acp_thread/src/acp_thread.rs` — `request_tool_call_authorization`,
  `upsert_tool_call_inner`, `ToolCallAuthorization` (`pub options`,
  `pub kind`), `authorize_tool_call`.
- `mini_ork/acp/agent.py` — `_ask_node_review`, `_offer_lane_repair`,
  `_offer_kickoff_draft_approval`, `_offer_run_review`.
- `crates/agent_ui/src/conversation_view/thread_view.rs` —
  `render_main_agent_awaiting_permission`, `render_any_tool_call`,
  `ToolCallLayout::Floating`.

Questions: does `upsert_tool_call_inner` create a new entry from a
permission-only update, and with what title/kind/status? Can a renderer
access the `PermissionOptions` and respond without duplicating the stock
permission UI? Does the floating awaiting-permission row show the same tool
call a second time?

### T3 — Run nodes are flat tool calls, not a hierarchy

Claim: node tool calls (`tool_call_id = node_id`, routed ids such as
`<run_id>:<node>`) are siblings of the run marker, so a run card cannot
implicitly contain its nodes. Grouping and per-node lane/model/cost/attempt
must be carried in metadata, and standalone node rows must be suppressed
inside the card.

Receipts to check:

- `mini_ork/acp/agent.py` — `_open_run_marker`, `_build_event_updates`,
  `_routes`, `_follow_in_thread`, and the node_start/node_end emitters.
- `crates/acp_thread/src/acp_thread.rs` — `entries`,
  `AgentThreadEntry::ToolCall`, `index_for_tool_call`; confirm there is no
  parent/child relation.
- `crates/agent_ui/src/conversation_view/thread_view.rs` — how tool calls
  are laid out in the list and whether any grouping exists.

Questions: is a stable run id available on every node update? Are lane,
model, cost and attempt obtainable at emit time without another disk read?
Would a `parent` key in `_meta` be enough, and what happens if updates
arrive out of order or after thread reload?

### T4 — Native diff, terminal and live-output surfaces can be delegated

Claim: a custom card renderer can own the card shell while delegating diffs,
terminals and live text to the existing native renderers; replacing the
whole tool-call renderer naively would lose edits, terminal output and
streaming text.

Receipts to check:

- `crates/acp_thread/src/acp_thread.rs` — `ToolCall::diffs`,
  `tool_call.terminals`, tool content types.
- `crates/agent_ui/src/conversation_view/thread_view.rs` —
  `render_tool_call`, `render_any_tool_call`,
  `render_tool_call_label`, diff and terminal rendering paths.
- `mini_ork/acp/live.py` — `_TEXT_CAP`, `_TOOL_OUTPUT_CAP`, event kinds
  (`text`, `thought`, `tool`, `tool_output`).

Questions: can an `AgentThreadEntry::ToolCall` be rendered by two elements
(shell + attachment) without double-rendering the stock body? Is there a
supported way for `mini_ork_ui` to call `agent_ui`'s private/sibling
renderers, or must the hook pass callbacks? What is lost if the card shows
only a summary plus a link?

### T5 — Generic failure recovery is not offered today

Claim: the engine offers recovery only for lane failures (`_offer_lane_repair`
requires `needs_change.kind == "lane"`), node review and run review. A
generic run failure closes the marker `failed` and emits prose naming
`/recover`; there is no permission card with Recover / Resume from node /
Open forensics. Comps C cannot be built without a new engine surface.

Receipts to check:

- `mini_ork/acp/agent.py` — `_follow_in_thread`, `_offer_lane_repair`,
  `_offer_node_review`, `_offer_run_review`, `_build_refusal_message`,
  `_offer_thread_review`.
- `mini_ork/recovery/` — `retry_hint`, `retry_notify`, `needs_change`
  classification; what actions exist for a non-lane failure.
- `mini_ork/acp/task_state.py` — the `needs_you` detail and pause sentinels.

Questions: what failure classes exist and which currently get buttons? What
does `/recover` accept (`--strategy`, `--from-node`, `--lane`,
`--ack-change`)? Is "Open forensics" backed by a stable artifact path? Is
inactive-turn gating deliberate, and what would relax it safely?

### T6 — Card state survives history replay, and legacy threads stay readable

Claim: thread JSONL stores raw ACP updates, so `_meta` round-trips on
`session/load`; titles are persisted as separate records; old threads with no
cards must fall back to stock tool calls. Duplicate, partial and out-of-order
updates must not corrupt the card.

Receipts to check:

- `mini_ork/acp/threads.py` — record types (`update`, `title`, `costs`,
  `config`, `user`), `load_session` replay, id whitelist.
- `mini_ork/acp/agent.py` — `_load_thread_session`, `_replaying`,
  `_record_thread_update`, `_emit_title_state_update`.
- `crates/agent_ui/src/agent_panel.rs` — thread load path
  (`load_agent_thread`, `AgentThreadSource`), `crates/agent/src/db.rs` for
  the local thread store distinction.
- `crates/acp_thread/src/acp_thread.rs` — update application order and
  status precedence.

Questions: on reopen, are routed run/node updates present, or only the
thread-level marker? Does a second `ToolCallProgress` without `_meta` erase
the card? What is the cheapest correct fallback for unknown `v`/`kind`?

## Proposed solutions (grade these)

### P1 — `_meta` envelope v2 plus one registry hook

`_meta["mini_ork"] = {v: 2, id, revision, parent, phase, status, header,
blocks, actions, fallback_text}`. `agent_ui` gains one hook at
`render_any_tool_call`: if a registered renderer claims the tool call, it
renders; otherwise stock rendering continues. Renderers live in
`mini_ork_ui/src/thread_cards.rs` and register from `mini_ork_ui::init`
(after `agent_ui::init`). Unknown `v`/`kind` or malformed payload renders
`fallback_text` plus the stock tool call. Rationale: one fork touch point,
data-driven, pages-style reuse.

### P2 — Action model richer than permission options

`actions[]` entries: `permission` (option id, answered through
`authorize_tool_call`), `prompt` (free text, e.g. "Request changes…"),
`command` (send a slash command such as `/recover …`), `link` (open a path
such as forensics). Rationale: comps C needs actions the permission API
cannot express; a failure card must remain actionable after the turn ended.

### P3 — Explicit linkage and grouping

Every card carries `parent` (e.g. `<run_id>:parent`), `node`, `group`
(`race` / `epic` / `explore`), plus lane, model, cost, attempt and stderr.
The renderer draws run cards as containers for node rows and suppresses the
matching standalone tool-call rows. Rationale: ACP is flat; grouping must be
declared, not inferred.

### P4 — Attachment delegation and suppression flags

The envelope may reference attachments (`diff`, `terminal`, `live`, `file`)
that the host renders with its native widgets. Flags:
`suppress_stock_permission`, `suppress_plan_checklist`,
`compact_in_floating_layout`. Rationale: avoid reimplementing Zed's diff,
terminal and streaming surfaces; avoid duplicate approval rows.

### P5 — New failure-recovery offer in the engine

Add `_offer_failure_recovery` in `mini_ork/acp/agent.py`: on a non-lane
terminal failure, emit one permission card with failure metadata (failed
node, rc, attempt, checkpoint, stderr excerpt, forensics path) and options
`recover`, `resume:<node>`, `forensics` (and later/leave). Decide whether
the offer is gated on an active turn; if gated, a notice card with `command`
actions is the fallback. Rationale: comps C is impossible without it.

### P6 — Versioning, caps and replay hardening

Cap header/detail/block sizes (text excerpt, block count, total payload);
validate on both sides; dedupe by `id` + `revision`; keep the last good card
on a malformed update; render legacy threads unchanged; add tests for
reopen, out-of-order updates and unknown versions. Rationale: the JSONL is
append-only and replayed; a bad payload must never blank a run.

## What the panel should return

- For each T1–T6: does the premise hold in the live tree? Cite receipts.
- For each P1–P6: is the solution sound, and what would you substitute?
- A reconciled execution order and the disputes a human must settle.

Improvements are expected: if a simpler or more robust design exists, name
it and say what it replaces. The goal is to make the final design cheap to
extend to every stage of a mini-ork run, including failure notifications
with revive options.

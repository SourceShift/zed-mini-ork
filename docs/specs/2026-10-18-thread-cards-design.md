# Inline thread cards for the mini-ork IDE — implementation design

Status: revised after the three-lane judge panel
(`run-1792348221-71710`, minimax + GLM + deepseek).
Supersedes the premises in `20261018-thread-cards-panel-kickoff.md` where the
panel found them false; those corrections are binding below.

## 1. What ships

The mini-ork agent currently renders every run, node, kickoff draft and
approval as a stock ACP tool call. This design replaces the stock body for
those tool calls with a data-driven card, drawn inside the thread:

- **Kickoff draft** (comp A) — header, task bubble, exploration group, a draft
  card with TASK / IN SCOPE / PROVES SUCCESS and Start run / Save only /
  Change something / Discard.
- **Run success** (comp B) — header with diff stat and publish state, a run
  card with node rows, and prose blocks (Where it landed / Not committed /
  Checks / Cost).
- **Run failure** (comp C) — header with the failing gate and rc, a run card
  with the failed row and inline stderr, a failure card naming node, attempt,
  checkpoint, healer and related learning, plus Recover / Resume from <node> /
  Open forensics.

The payload is not a new ACP message type. It is a JSON object carried in the
standard `_meta` field of the existing tool-call updates. The engine owns the
content; the IDE owns the drawing.

## 2. Panel corrections (binding)

These were verified against the live trees and must not be re-litigated by the
implementation:

1. **Routed run internals are not persisted in the thread JSONL.** `_emit`
   records only updates whose `session_id` is the thread itself; a routed run's
   tool calls are re-projected from run data on load
   (`mini_ork/acp/agent.py:5485`, `:6236`, `:6289`). Therefore every card must
   be **projector-derived** — built by the same pure builder on the live path
   and on `_replay_run_in_thread`. A payload written once at emit time will
   diverge after reopen.
2. **The engine is ACP v1.** `ToolCallStart` / `ToolCallProgress` are v1
   dataclasses. v1 `_meta` is sticky: an update without `_meta` keeps the
   earlier value (`crates/acp_thread/src/acp_thread.rs:1447-1471`). We do not
   emit v2-only null clears; we may therefore attach the card once per tool
   call and let later progress updates omit it.
3. **A permission request can create a card on its own.** The client calls
   `request_tool_call_authorization_with_id` with the full `ToolCallUpdate`,
   which upserts an entry when absent
   (`crates/agent_servers/src/acp.rs:6475`,
   `crates/acp_thread/src/acp_thread.rs:5689`, `:5313`). The type is
   `PermissionRequest` with a private payload; the public accessors are
   `legacy_options()` and `legacy_kind()`, and the response path is
   `authorize_permission_request` / `authorize_tool_call`
   (`crates/acp_thread/src/acp_thread.rs:1061-1096`, `:5779`). There is no
   `ToolCallAuthorization` type.
4. **A v1 progress update for an unknown id creates a synthetic failed
   "Tool call not found" entry**
   (`crates/acp_thread/src/acp_thread.rs:5210-5234`). Cards must never rely on
   a progress-before-start order for a new id.
5. **Node tool calls are flat siblings of the run marker.** There is no
   parent/child relation in `AgentThreadEntry`. Grouping and suppression must
   be declared in the payload and honoured by the host.
6. **At node-emit time only the lane is free.** `node_start` carries
   `model_lane`; `node_end` carries duration, verdict, artifact path and finish
   reason (`mini_ork/observability/node_events.py:339-394`). Model id, per-node
   cost and attempt are not available without a schema enrichment. Version 1
   cards render what exists and omit what does not.
7. **Live text has no standalone renderer.** It is folded into the content
   chunks of `ToolCallProgress` (`mini_ork/acp/agent.py:4990-5090`). A card
   must delegate that content to the host rather than reimplement it.
8. **`epic` and `explore` are not real group names.** Only `race` exists as a
   grouped mode. Do not invent group semantics; render `parent` only.
9. **Forensics is conditionally present.** `<run_dir>/diagnosis.json` is
   written by the diagnoser, not by every failure
   (`mini_ork/recovery/diagnose.py:1003`). An "Open forensics" action is
   enabled only when a target exists.

## 3. Card envelope

One JSON object under `_meta["mini_ork"]`. The namespace is owned by this
design; other ACP servers may ship their own.

```json
{
  "v": 1,
  "kind": "run",
  "id": "run-1791292465-2b0f:parent",
  "revision": 4,
  "parent": null,
  "phase": "execute",
  "status": "in_progress",
  "header": {
    "mark": "check",
    "title": "Add dark-mode line to CHANGELOG",
    "meta": "+4 -0",
    "detail": "published",
    "tag": "docs"
  },
  "blocks": [],
  "actions": [],
  "suppress_stock": true,
  "fallback_text": "run run-1791292465-2b0f (docs) · published"
}
```

Fields:

- `v` — card schema major version. Unknown or missing ⇒ fallback.
- `kind` — `kickoff | run | node | failure | review | notice`. Unknown ⇒ fallback.
- `id` — the tool-call id the card belongs to, unprefixed. Routed updates are
  prefixed by `_emit` (`<run_id>:...`); the renderer uses the final entry id
  for actions and does not trust this field for identity.
- `revision` — monotonically increasing per id. The renderer keeps the highest
  revision it has seen and ignores older or equal ones (out-of-order safety).
- `parent` — the run marker id (`<run_id>:parent`) when this card belongs to a
  run; `null` otherwise. Linkage is **prefix-first**: an entry whose id starts
  with `<run_id>:` belongs to that run; `parent` is a cross-check, not the
  primary key.
- `phase` — `classify | plan | execute | verify | publish | recovery | done`.
- `status` — `pending | in_progress | completed | failed | cancelled`.
- `header` — what the collapsed row shows. `mark` is one of `check | cross |
  hand | dot | none`.
- `blocks[]` — ordered content, vocabulary below.
- `actions[]` — ordered buttons, vocabulary below.
- `suppress_stock` — when true, the host does not draw the stock tool-call body
  for this entry.
- `fallback_text` — plain text used when the card cannot be rendered. It must
  be meaningful on its own; never a placeholder.

### Blocks

| `type` | shape | use |
|---|---|---|
| `text` | `{type, text, tone?}` | prose; tone `muted` for secondary |
| `kv` | `{type, rows: [{k, v}]}` | Where it landed / Not committed / Cost |
| `items` | `{type, items: [{mark, text, status?}]}` | checklist rows, verdicts |
| `files` | `{type, files: [{path, label?, status?}]}` | IN SCOPE paths; status `new` |
| `steps` | `{type, steps: [{id, label, status, lane?, model?, cost?, detail?, stderr?}]}` | run node rows |
| `code` | `{type, lang, text}` | stderr excerpt |
| `section` | `{type, title, text}` | kickoff sections |

`status` values used for colouring: `ok | warn | bad | new | muted`.
Unknown block types are skipped, not fatal.

### Actions

| `kind` | shape | host behaviour |
|---|---|---|
| `permission` | `{id, label, kind, option_id, style?}` | answer the existing permission request; disabled when the entry has no `authorization_id()` |
| `command` | `{id, label, kind, command}` | send the slash command into the thread |
| `prompt` | `{id, label, kind, placeholder?}` | focus the composer; the user's next message continues |
| `link` | `{id, label, kind, target}` | open a file or reveal a directory; disabled when the target is absent |

`style` is `primary | danger | ghost`. Permission options remain the source of
truth for approvals: the action list repeats them for layout, but the response
still goes through `authorize_permission_request`.

### Caps and validation

- Header title ≤ 200 chars; block count ≤ 12; text block ≤ 4000 chars; code
  block ≤ 8000 chars; total serialized payload ≤ 64 KiB.
- The builder clamps; the renderer validates and falls back on malformed input.
- A malformed update never blanks a card: the renderer keeps the last good
  card for that id.

## 4. Where the payload is built

A new pure module `mini_ork/acp/thread_cards.py` owns:

```python
CARD_KEY = "mini_ork"
CARD_VERSION = 1

def build_card(*, kind, card_id, revision, parent=None, phase="execute",
               status="in_progress", header=None, blocks=None, actions=None,
               suppress_stock=True, fallback_text="") -> dict: ...
def with_meta(update, card): ...          # returns model_copy with _meta
def run_card(snapshot, *, run_id, recipe, worktree) -> dict: ...
def node_card(event, *, run_id, recipe) -> dict: ...
def kickoff_card(draft_path, markdown, *, recipe) -> dict: ...
def failure_card(run_id, snapshot, hint, *, diagnosis_path) -> dict: ...
def review_card(run_id, snapshot, options) -> dict: ...
```

Rules:

- No filesystem I/O inside the builders except `kickoff_card`, which receives
  the markdown and path as arguments.
- Every builder is deterministic for the same input so live and replay emit
  byte-identical cards.
- The module imports nothing from `mini_ork.acp.agent`; `agent.py` imports it.

## 5. Engine integration points (Phase 1)

Target: `/Volumes/docker-ssd/ps/mini-ork` (recipe `framework-edit`; the recipe
produces a reviewable diff and does not apply it to `main`).

1. `_open_run_marker` (`agent.py:3865`) — attach `run_card` to the
   `ToolCallStart` for `<run_id>:parent`.
2. `_build_event_updates` (`agent.py:4959`) — accept the run context and attach
   `node_card` to each `node_start` / `node_end`. Keep the ids unprefixed; the
   router adds the prefix. A node card carries `parent=<run_id>:parent`, the
   lane from `payload["model_lane"]`, and the status from the event type.
3. `_project_snapshot` / `_replay_run_in_thread` — call the same builders on
   replay so a reopened thread reconstructs the card. This is the correction
   that matters most.
4. `_offer_kickoff_draft_approval` (`agent.py:3424`) — add `kickoff_card` meta
   to its `ToolCallUpdate`, parsed from `.mini-ork/kickoff-drafts/<id>.md`:
   TASK, IN SCOPE (paths), PROVES SUCCESS (checklist), note, and the four
   permission actions.
5. `_offer_node_review` (`:4155`), `_offer_run_review` (`:4427`),
   `_offer_lane_repair` (`:4588`) — attach `review_card` / `notice` meta to
   their updates so the host can draw them consistently.
6. New `_offer_failure_recovery` — called from `_follow_in_thread` after the
   lane-repair branch and before the marker is closed `failed`. It emits one
   permission card on `<run_id>:failure` with:
   - failed node, rc/verdict, attempt, checkpoint (from `retry_hint` /
     `diagnosis` when present),
   - an `code` block with the stderr excerpt,
   - options `recover`, `resume:<node>`, `forensics`, `leave`,
   - a `link` action for `diagnosis.json` only when it exists.
   It obeys the same active-turn gate as the other offers: live buttons inside
   an active turn; otherwise a notice message that names the exact
   `mini-ork recover <run_id>` command. Never raises into the follower.
7. Do not change `PROTOCOL_VERSION`, do not add an ACP message type, and do not
   touch the run JSONL format beyond carrying `_meta` in existing updates.

## 6. IDE integration (Phase 2)

Target: `/Volumes/docker-ssd/ps/zed-mini-ork` (branch `mini-ork`).

The renderer ships **in `agent_ui`**, not in `mini_ork_ui`, for one structural
reason: the `mini_ork_ui` crate uses workspace dependencies and only builds as
a member of the Zed workspace, so a change confined to its own repo cannot be
compiled or tested there. A one-repo change in `agent_ui` is the only way to
hand this to a single verifiable mini-ork run. The crate seam is preserved for
later: the registry below accepts an override from `mini_ork_ui` without
touching the hook again.

New `crates/agent_ui/src/thread_card.rs`:

- Typed envelope (`ThreadCard`, `CardHeader`, `CardBlock`, `CardAction`) with
  `serde` defaults, unknown fields ignored, and a `parse` that returns
  `Option<ThreadCard>` (never panics on malformed JSON).
- A generic renderer for the block vocabulary in section 3. It is
  server-agnostic: any ACP server that emits the same envelope gets cards.
- `pub trait ThreadCardRenderer` with `namespace()`, `render(ctx) ->
  Option<AnyElement>`, `suppress_entry(meta) -> bool`; the built-in envelope
  renderer is the default registration.
- `ThreadCardHost` closes over the private stock renderers so a card can
  delegate diff / terminal / live content instead of reimplementing them.
- A global registry consulted in `render_any_tool_call`
  (`thread_view.rs:8756`) before stock rendering and in `render_entry`
  (`:6747`) for suppression.
- The diff, terminal and permission rows remain host-owned; permission actions
  answer through the existing authorization path.

A later extraction may move the drawing into `mini_ork_ui` behind a registered
override; that is a refactor, not a prerequisite.

The run must also prepend the two `README.md` review lines required by the
project rules and add the new hook marker to `script/mini-ork-invariants.txt`
in the same commit.

## 7. Failure and recovery model

Recovery is a first-class card, not prose:

- A run that ends `failed` for a non-lane reason gets `_offer_failure_recovery`.
- The card's `recover` action maps to `mini-ork recover <run_id>`; a
  `resume:<node>` action maps to `recover <run_id> --from-node <node>`. The
  engine handles the decision in-process, exactly as `_handle_lane_repair_decision`
  does, so the follower keeps owning the marker.
- `--force` / `--ack-change` gates are not bypassed by a button; if the
  recovery command refuses, the engine surfaces the refusal text in the card
  and leaves the marker open.
- `forensics` opens `<run_dir>/diagnosis.json` when present, else the run
  directory; when neither exists the action is disabled.

## 8. Open decisions and defaults

The panel left these open. This design chooses a default; the implementer may
revisit only with evidence.

| Question | Default |
|---|---|
| Active-turn gating for failure cards | Keep the existing gate; an inactive thread gets a notice message with the exact command |
| Forensics surface | `diagnosis.json` if present, else the run dir; disabled otherwise |
| Free-text "Change something" | `prompt` action focuses the composer; no argv, no new engine round-trip |
| Artifact destination | Run dir under `.mini-ork/runs/<run_id>/`; no new repo files |
| Authoritative branch | Engine `main` at the run base; IDE `mini-ork`; UI repo `main` |
| Per-node provenance | Render lane + duration + verdict now; omit model/cost until `llm_calls` carries a node id |
| Suppression ownership | Host-side `suppress_entry`; the engine only declares `parent` |
| ACP pinning | v1; do not upgrade as part of this work |

## 9. Acceptance

Phase 1 is accepted when:

- A new run marker carries a valid `_meta["mini_ork"]` card with `v: 1`.
- Node start/end updates carry cards whose `parent` is the run marker.
- Reopening the thread re-projects identical cards from run data.
- A synthetic non-lane failure produces a failure card with the four actions
  (or the notice fallback outside an active turn).
- A v1 progress update without `_meta` does not erase the card.
- Unknown `v` / `kind` and malformed payloads fall back without raising.
- The kickoff's verification commands pass in the patched copy.

Phase 2 repeats the same fixture in the IDE: comps A, B and C draw from the
payload, stock rows are suppressed, diffs and terminals still render, and an
old thread with no cards is unchanged.

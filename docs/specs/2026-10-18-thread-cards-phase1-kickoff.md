# Phase 1 — thread card payloads in the mini-ork engine

Target repo: `/Volumes/docker-ssd/ps/mini-ork` (the engine; Python).
Reference design: `/Volumes/docker-ssd/ps/zed-mini-ork/docs/specs/2026-10-18-thread-cards-design.md`.
This kickoff is the acceptance bar. Do not edit the design doc.

## Objective

Emit a versioned, projector-derived card payload in ACP `_meta` for mini-ork
kicks, runs, nodes, reviews and failures, so the IDE can render the three
inline comps without a new ACP message type.

## Why the payload must be projector-derived

Routed run internals are **not** recorded in the thread JSONL
(`mini_ork/acp/agent.py:_emit`, `:6236`). On `session/load`,
`_load_thread_session` re-projects each run from its own data
(`mini_ork/acp/agent.py:6289`, `_replay_run_in_thread`). A card written only
at live-emit time disappears or diverges after reopen. The same pure builders
must therefore be called on both paths.

## Scope

Create `mini_ork/acp/thread_cards.py`. Edit `mini_ork/acp/agent.py`. Add
tests under `tests/unit/acp/`. Do not change the ACP protocol version, do not
add a protocol message type, and do not change the thread JSONL record shapes.

### 1. `mini_ork/acp/thread_cards.py` — pure builders

Module-level constants:

```python
CARD_KEY = "mini_ork"
CARD_VERSION = 1
```

Public functions with these exact names and keyword-only arguments (the blind
spec tests import them):

```python
def build_card(*, kind: str, card_id: str, revision: int,
               parent: str | None = None, phase: str = "execute",
               status: str = "in_progress",
               header: dict | None = None,
               blocks: list[dict] | None = None,
               actions: list[dict] | None = None,
               suppress_stock: bool = True,
               fallback_text: str = "") -> dict:
```

Returns:

```json
{
  "v": 1, "kind": "...", "id": "...", "revision": 0,
  "parent": null, "phase": "execute", "status": "in_progress",
  "header": {"mark": "none", "title": "", "meta": "", "detail": "", "tag": ""},
  "blocks": [], "actions": [], "suppress_stock": true,
  "fallback_text": ""
}
```

Rules: clamp header title to 200 chars, block count to 12, total serialized
size to 64 KiB (drop trailing blocks first, then code blocks, and keep
`fallback_text`); unknown keys in `header` are ignored; `fallback_text` is
never empty when a title exists.

```python
def with_meta(update, card: dict):
```

The ACP Python models expose the wire `_meta` field as the attribute
**`field_meta`** (alias `_meta`); there is no `meta` attribute. Return:

```python
update.model_copy(update={"field_meta": {CARD_KEY: card}})
```

so `model_dump(mode="json", by_alias=True)` emits
`"_meta": {"mini_ork": {...}}`. Verify the round-trip in a test before using
it. Return the update unchanged when it has no `field_meta` attribute. Must not
mutate the input.

```python
def run_card(snapshot: dict, *, run_id: str, recipe: str,
             worktree: dict | None = None) -> dict:
```

Builds `kind="run"`, `id=f"{run_id}:parent"`, `parent=None`. Header:
title from the run's task/first user prompt when available, otherwise
`run <run_id>`; `detail` is the run status; `tag` is the recipe. Blocks:
- `kv` rows for workspace mode, branch, cost and duration when present,
- `steps` rows derived from `snapshot["events"]` node_start/node_end pairs,
  each with `id`, `label` (node type), `status`, `lane` from
  `payload["model_lane"]` and `detail` from `finish_reason` / `verdict`,
- `text` for an agent-visible summary only when the snapshot carries one.
Do not invent per-node cost or model.

```python
def node_card(event: dict, *, run_id: str, recipe: str,
              revision: int = 0) -> dict:
```

Builds `kind="node"`, `id` = the node id from the event payload, `parent =
f"{run_id}:parent"`. Status `in_progress` for `node_start`, the verdict or
`completed` for `node_end`. `suppress_stock=True`. One `steps` block with the
single row. The card must be safe when `payload_json` is a JSON string or a
dict.

```python
def kickoff_card(draft_path: str, markdown: str, *, recipe: str) -> dict:
```

Builds `kind="kickoff"`, `id` supplied by the caller through `build_card`
(the permission call owns the id), `status="pending"`. Parse the markdown:
- `files` block from fenced or backticked paths under an `IN SCOPE` heading
  (label `new` when the path does not exist),
- `section` blocks for `TASK`, `PROVES SUCCESS` and any note section,
- `items` rows for checklist lines (`- [ ]`, `- [x]`, `-`) under
  `PROVES SUCCESS`.
Missing sections are omitted, never fatal. Actions mirror the four permission
options the caller passes: `run`, `save`, `change`, `discard`.

```python
def failure_card(run_id: str, snapshot: dict, hint: dict | None, *,
                 diagnosis_path: str | None) -> dict:
```

Builds `kind="failure"`, `id=f"{run_id}:failure"`, `parent=f"{run_id}:parent"`,
`status="failed"`. Blocks: `text` naming the failed gate, `kv` rows for node,
attempt, checkpoint, healer and related learning when present in `hint`,
`code` with the stderr excerpt (cap 8000 chars), and `link` action for
`diagnosis_path` only when it is a non-empty existing path. Actions:
`permission` with option ids `recover`, `resume:<node>`, `forensics`, `leave`.
Never raise: a missing or malformed hint yields a minimal card.

```python
def review_card(run_id: str, snapshot: dict,
                options: list[dict]) -> dict:
```

Builds `kind="review"`, `id=f"{run_id}:parent"` by default, with `items` for
the run's review state and `permission` actions built from `options`
(`option_id`, `label`). Used by node review, run review and lane repair.

### 2. `mini_ork/acp/agent.py` integration

- `_open_run_marker` (`:3865`): attach a `run_card` to the `ToolCallStart` via
  `with_meta`. It must still emit when the snapshot is not yet readable: use a
  minimal card built from `run_id` + `recipe` and the `_run_status` cache.
- `_build_event_updates` (`:4959`): accept an optional run context
  (`run_id`, `recipe`) and attach `node_card` to each `ToolCallStart` /
  `ToolCallProgress`. Keep ids unprefixed — `_emit` adds the route prefix.
  Existing callers without the context keep today's behaviour.
- `_project_snapshot` (`:5539`) and `_replay_run_in_thread`: pass the run
  context so live and replay derive the same cards. This is the
  reopen-reconstruction requirement.
- `_offer_kickoff_draft_approval` (`:3424`): read
  `.mini-ork/kickoff-drafts/<draft_id>.md`, call `kickoff_card`, attach its
  meta to the `ToolCallUpdate`. Keep the existing four options and handlers.
- `_offer_node_review` (`:4155`), `_offer_run_review` (`:4427`) and
  `_offer_lane_repair` (`:4588`): attach `review_card` meta to their
  `ToolCallUpdate`; keep their current option ids and decision handlers.
- New `_offer_failure_recovery(thread_id, run_id)`: called from
  `_follow_in_thread` after the lane-repair branch and before the marker is
  closed `failed`. Build `failure_card` from `_read_snapshot(run_id)`,
  `mini_ork.recovery.retry_hint.load_or_compute(..., write=False)` and
  `<run_dir>/diagnosis.json`. Offer permission options `recover`,
  `resume:<node>`, `forensics`, `leave` on tool-call id
  `f"{run_id}:failure"`. Handle the decision in the same follower:
  - `recover` → start the same detached recover path the lane repair uses,
    then re-enter `_await_resume_started` and continue following;
  - `resume:<node>` → recover with `--from-node <node>`;
  - `forensics` → emit a message naming the path, leave the card open;
  - `leave`/dismissed → close the marker `failed`.
  Outside an active turn or with no connection, emit one notice message that
  names `mini-ork recover <run_id>` and close as today. Never raise into the
  follower. Dedupe per run incarnation exactly like `_lane_repair_emitted`.
- `_emit` (`:5485`) already rewrites `tool_call_id` and preserves `meta`
  (`model_copy`); do not change it.

### 3. Constraints

- Python 3.13; no new dependencies.
- No `print` in library code; no bare `except:`; no `assert` for control flow.
- A v1 update without `_meta` must not erase an earlier card: emit the card on
  the first update for an id and let it be sticky. Do not emit explicit nulls.
- Never emit a `ToolCallProgress` for an id the client has not seen unless it
  is a permission request (`request_permission`), which upserts safely.
- All new code paths must be no-ops on failure (best-effort card, never a
  broken run).

### 4. Tests

Add `tests/unit/acp/test_thread_cards.py` covering:

- `build_card` shape, clamping and fallback text.
- `with_meta` does not mutate and round-trips through
  `ToolCallStart(...).model_dump(mode="json", by_alias=True, exclude_none=True)`.
- `node_card` accepts both dict and JSON-string payloads; parent is the run
  marker; `node_end` carries the verdict.
- `kickoff_card` parses TASK / IN SCOPE / PROVES SUCCESS and tolerates a
  minimal document.
- `failure_card` includes the four actions, the stderr excerpt and the
  forensics link only when the file exists.
- Live and replay paths produce equal cards for the same snapshot.
- Unknown version/kind and malformed payload do not raise.

Add `tests/unit/acp/test_acp_failure_recovery.py` for the new offer: options,
active-turn gate, decision handling, dedupe, and no-raise behaviour. (The
existing ACP suites live directly in `tests/unit/`, not in a subdirectory.)

## Verification commands

- `pytest tests/unit/acp/test_thread_cards.py -q`
- `pytest tests/unit/test_acp_lane_repair.py -q`
- `pytest tests/unit/test_acp_review_flow.py -q`
- `python -m py_compile mini_ork/acp/thread_cards.py mini_ork/acp/agent.py`

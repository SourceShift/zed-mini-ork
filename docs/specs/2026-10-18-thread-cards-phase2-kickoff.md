# Phase 2 — draw the inline thread cards in the IDE

Target repo: `/Volumes/docker-ssd/ps/zed-mini-ork` (the Zed fork; Rust).
Reference design: `/Volumes/docker-ssd/ps/zed-mini-ork/docs/specs/2026-10-18-thread-cards-design.md`.
The engine side (Phase 1) ships the payload; this phase only draws it.
Do not edit the design doc. Do not edit the engine repo.

## Objective

When a tool call carries `_meta["mini_ork"]` (a card envelope written by the
mini-ork ACP server), draw that card inside the thread instead of the stock
tool-call body. Three comps must render from payload alone:

- **A — kickoff draft**: header `✋ Rate limit POST /api/upload · kickoff ready
  to check · [code-fix]`, a TASK `section`, IN SCOPE `files` (new paths in
  green), PROVES SUCCESS `items`, and four permission buttons.
- **B — run success**: header `✓ Add dark-mode line to CHANGELOG · +4 −0 ·
  published · [docs]`, a `steps` block of node rows (mark, node, lane dot,
  status, cost), then `kv` prose blocks (Where it landed / Not committed /
  Checks / Cost).
- **C — failure**: header `✗ Migrate users.email to citext · rollback-proof.sh
  rc=1 · [db-migration]`, a `steps` block with the failed row and its inline
  stderr, a `code` block with STDERR, `kv` rows (node, attempt, checkpoint,
  healer, related learning), and buttons Recover / Resume from `<node>` / Open
  forensics.

Unknown `v`, unknown `kind`, unknown block types and malformed JSON all fall
back without panicking: the stock body still renders and `fallback_text` is
available to it.

## Mechanism

### 1. New module `crates/agent_ui/src/thread_card.rs`

Register it as `mod thread_card;` in `crates/agent_ui/src/agent_ui.rs`
(alphabetical position among the other `mod` lines).

#### 1.1 Data model

`serde` types that mirror the envelope in the design's section 3. The field
names and defaults are fixed; unknown fields are ignored.

```rust
pub const CARD_KEY: &str = "mini_ork";
pub const CARD_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ThreadCard {
    #[serde(default)] pub v: u32,
    #[serde(default)] pub kind: String,
    #[serde(default, rename = "id")] pub card_id: String,
    #[serde(default)] pub revision: i64,
    #[serde(default)] pub parent: Option<String>,
    #[serde(default)] pub phase: String,
    #[serde(default)] pub status: String,
    #[serde(default)] pub header: CardHeader,
    #[serde(default)] pub blocks: Vec<CardBlock>,
    #[serde(default)] pub actions: Vec<CardAction>,
    #[serde(default)] pub suppress_stock: bool,
    #[serde(default)] pub fallback_text: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CardHeader {
    #[serde(default)] pub mark: String,   // check | cross | hand | dot | none
    #[serde(default)] pub title: String,
    #[serde(default)] pub meta: String,
    #[serde(default)] pub detail: String,
    #[serde(default)] pub tag: String,
}
```

Blocks are an internally tagged enum on `type`, with the variants
`text | kv | items | files | steps | code | section` and a catch-all
`#[serde(other)] Unknown` so an unknown type is skipped rather than failing
the whole card. Shapes:

```rust
Text    { text: String, tone: Option<String> }
Kv      { rows: Vec<KvRow> }            // KvRow { k: String, v: String }
Items   { items: Vec<ItemRow> }         // ItemRow { mark: String, text: String, status: Option<String> }
Files   { files: Vec<FileRow> }         // FileRow { path: String, label: Option<String>, status: Option<String> }
Steps   { steps: Vec<StepRow> }         // StepRow { id, label, status, lane?, model?, cost?, detail?, stderr? }
Code    { lang: Option<String>, text: String }
Section { title: String, text: String }
```

Actions are tagged on `kind`:

```rust
Permission { id: String, label: String, option_id: String, option_kind: String, style: Option<String> }
Prompt     { id: String, label: String, placeholder: Option<String> }
Link       { id: String, label: String, target: String }
Command    { id: String, label: String, command: String }
```

Parsing entry points:

```rust
impl ThreadCard {
    /// Returns None when the envelope is absent, a different version, or invalid.
    pub fn from_meta(meta: Option<&acp_v2::Meta>) -> Option<ThreadCard>;
    /// The text to show when the card cannot be drawn.
    pub fn fallback(&self) -> &str;
}
```

`from_meta` must return `None` — never panic — for: no key, `v != 1`, empty
`kind`, non-object payload. A card whose `blocks` all fail still parses (blocks
collapse to `Unknown`), so the header can draw.

#### 1.2 View context handed to a renderer

The renderer must not reach into `ThreadView`'s private state. Everything it
can act on is resolved by the host first:

```rust
pub struct ThreadCardView<'a> {
    pub session_id: &'a acp_v1::SessionId,
    pub tool_call_id: String,
    pub entry_ix: usize,
    /// The pending permission request for this tool call, when one exists.
    pub permission: Option<ResolvedPermission>,
    /// The workspace, for `link` actions.
    pub workspace: Option<gpui::Entity<Workspace>>,
}

pub struct ResolvedPermission {
    pub request_id: acp_thread::PermissionRequestId,
    /// option_id -> option_kind, exactly the options the server offered.
    pub options: Vec<(String, String)>,
}
```

#### 1.3 Registry

```rust
pub trait ThreadCardRenderer: Send + Sync {
    /// The `_meta` key this renderer owns.
    fn namespace(&self) -> &'static str;
    /// `None` = this renderer declines; the next one is tried.
    fn render(
        &self,
        card: &ThreadCard,
        view: &ThreadCardView<'_>,
        window: &gpui::Window,
        cx: &gpui::App,
    ) -> Option<gpui::AnyElement>;
    /// When true the entry is not drawn in the inline list at all.
    fn suppress_entry(&self, _card: &ThreadCard) -> bool {
        false
    }
}

pub fn register_renderer(renderer: Box<dyn ThreadCardRenderer>);
pub fn render(meta: Option<&acp_v2::Meta>, view: &ThreadCardView<'_>, window: &Window, cx: &App) -> Option<AnyElement>;
pub fn suppresses_entry(meta: Option<&acp_v2::Meta>) -> bool;
```

The registry is a `OnceLock<Mutex<Vec<Box<dyn ThreadCardRenderer>>>>`. The
built-in envelope renderer is registered once, lazily, on first use; a renderer
registered later is checked first, so `mini_ork_ui` (or any other crate) can
override the drawing without touching the hook. Renderers are tried in
reverse registration order and only the one whose `namespace()` owns a key
present in `meta` is consulted.

The built-in renderer's namespace is `CARD_KEY`; it is the only renderer in
this phase, so the registry exists to keep the seam, not to add policy.

#### 1.4 Drawing

A card is a bordered, rounded container:

- **Header row** — `mark` as a small glyph (`✓`, `✗`, `✋`, `·`), `title` in
  normal weight, `meta` and `detail` in muted text separated by `·`, `tag` as
  a small muted pill on the right. `mark` unknown or `none` draws no glyph.
- **Blocks** in order:
  - `text` — paragraph; `tone: "muted"` uses the muted text colour.
  - `kv` — two-column rows, key muted, value normal.
  - `items` — one row per item, mark glyph coloured by `status`
    (`ok → success`, `warn → warning`, `bad → error`, `new → modified`,
    `muted → muted`; unknown → default text colour).
  - `files` — mono path; `status: "new"` in the added/success colour.
  - `steps` — one row per step: status glyph, label, lane dot with `lane`
    text, `detail` muted, `cost` right-aligned. When `stderr` is present it
    renders under the row in a mono, wrapped, error-tinted block.
  - `code` — `lang` as a caption, body in mono inside a subtle inset
    container; cap the drawn text at what the payload carries.
  - `section` — bold title, then body text.
- **Actions row** under the blocks:
  - `permission` — a button. Disabled (muted, no handler) when
    `view.permission` is `None`. When enabled it dispatches the existing
    `crate::AuthorizeToolCall` action with `tool_call_id`, `session_id`,
    `request_id`, `option_id` and `option_kind` from `view.permission` and
    the action payload. Resolve `option_kind` from `view.permission.options`
    by `option_id`; when the option is not offered, skip the button.
    `style: "primary"` uses the accent fill, `"danger"` the error colour,
    otherwise the standard button look.
  - `link` — opens `target` in the workspace (`open_abs_path` when it is a
    file, reveal/`open` the directory when it is a directory). Disabled when
    the path does not exist.
  - `prompt` — dispatches a new `FocusMessageEditor` action (see 2.3).
  - `command` — for this phase, draw it disabled with the stock tooltip; the
    engine does not emit it yet.

Read colours from `cx.theme().colors()`, never hard-code a hex value. Use the
existing `button`/`Button` styling from `ui` so cards look native.

#### 1.5 Suppression

`suppress_entry` returns true for `kind == "node"` so node rows are not also
drawn as their own tool calls when the run card already lists them in `steps`.
Nothing else is suppressed in this phase. A card with `suppress_stock == false`
is still drawn; the flag is advisory to the host and unused in this phase.

### 2. Wire the hook into `crates/agent_ui/src/conversation_view/thread_view.rs`

2.1 **`render_any_tool_call` (:8756)** — before the `is_subagent` / terminals
branch, try the registry:

```rust
let view = thread_card::ThreadCardView {
    session_id: active_session_id,
    tool_call_id: tool_call.id.0.to_string(),
    entry_ix,
    permission: self.resolved_permission_for(tool_call, active_session_id, cx),
    workspace: Some(self.workspace.clone()),
};
if let Some(card) = thread_card::render(tool_call.meta.as_ref(), &view, window, cx) {
    return div().w_full().id(container_id).child(card);
}
```

Keep the wrapper and its `container_id` — the element-id uniqueness comment
above it still applies. Terminals and diffs stay host-owned: the hook only wins
when a card is present, and a card that declines falls through unchanged.

2.2 **`render_entry` (:6747), tool-call arm (:7021)** — before drawing the
entry, `thread_card::suppresses_entry(tool_call.meta.as_ref())` returns
`Empty.into_any()`. Do this before the cancelled-tool-call check.

2.3 **`FocusMessageEditor`** — a new action in `thread_card.rs`
(`#[derive(Action)] #[action(namespace = agent)] pub struct FocusMessageEditor;`)
handled by `ThreadView`: register `.on_action(cx.listener(...))` next to the
existing `handle_authorize_tool_call` registration at :13143 and implement a
handler that focuses `self.message_editor.focus_handle(cx)` and notifies. A
`prompt` action dispatches it via `window.dispatch_action`.

2.4 **Permission resolution** — a helper on `ThreadView`:

```rust
fn resolved_permission_for(
    &self,
    tool_call: &ToolCall,
    session_id: &acp_v1::SessionId,
    cx: &App,
) -> Option<thread_card::ResolvedPermission>
```

It uses the same lookup as `permission_action_target` (:3261):
`conversation.threads.get(session_id)?.read(cx).permission_request_for_tool(&tool_call.id)`,
then `request.legacy_options()` for the option list. It must never panic and
returns `None` when there is no pending request.

### 3. Constraints

- Rust only. No new crates, no new workspace dependencies; `serde`,
  `serde_json`, `acp_thread`, `agent-client-protocol` and `gpui` are already
  dependencies of `agent_ui`.
- No `unwrap()` / `expect()` on parsed payload data; propagate or fall back.
  `expect` is acceptable only for a `OnceLock` initialisation that cannot fail.
- No panics on malformed input, missing paths or a missing workspace.
- Do not change the wire protocol, `acp_thread`, `acp_v1`/`acp_v2`, or any
  behaviour for tool calls without a `mini_ork` meta key.
- Keep the diff inside the files listed below.
- `README.md` already starts with the two `> [!IMPORTANT]` review lines. Leave
  them exactly as they are; never remove or reorder them.
- `script/mini-ork-invariants.txt` must gain a record for each new fork edit
  (two records, tab-separated):
  `path	crates/agent_ui/src/thread_card.rs	mini_ork` and
  `path	crates/agent_ui/src/conversation_view/thread_view.rs	thread_card::`.
  The literal must be absent from the Zed base tag, so pick the marker text
  carefully.

## Files in scope

- `crates/agent_ui/src/thread_card.rs` (new)
- `crates/agent_ui/src/agent_ui.rs` (one `mod` line)
- `crates/agent_ui/src/conversation_view/thread_view.rs` (hook, suppression,
  action handler, permission resolution)
- `script/mini-ork-invariants.txt` (two records)

## Tests

Add `#[cfg(test)] mod tests` inside `thread_card.rs` covering:

- `from_meta` returns `None` for absent/mistyped/`v: 2`/empty-kind payloads,
  and a card for a valid payload.
- unknown block `type` is skipped, other blocks still parse.
- `suppress_entry` is true for `kind: "node"` and false for `"run"`.
- every comp fixture parses: kickoff (A), run success (B), failure (C) — build
  them as JSON string constants that mirror the design's section 3 examples.
- an action with a `permission` kind whose `option_id` is not in the offered
  options yields no button (test the pure selection helper, not the drawing).

## Verification commands

- `./script/mini-ork-check`
- `cargo test -p agent_ui --lib thread_card`
- `cargo check -p zed --bin zed`

## Success criteria

- `./script/mini-ork-check` passes (this also runs `cargo ui-check`).
- `cargo test -p agent_ui --lib thread_card` passes.
- `cargo check -p zed --bin zed` passes, proving the hook call sites compile.
- A thread entry with no `mini_ork` meta renders exactly as before.
- A thread entry with comp A, B or C meta renders the card and not the stock
  body.
- A `node` card is not drawn as its own entry.

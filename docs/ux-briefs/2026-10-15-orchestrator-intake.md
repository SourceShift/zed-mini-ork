# UI/UX Brief — Orchestrator & intake

| Field | Value |
|---|---|
| **brief_type** | `redesign` (not an adjustment pass — see [Why now](#1-why-now)) |
| **Surface** | `mini-ork IDE` → activity rail `✦` → page key `orch` |
| **Route equivalent** | `mini-ork board page orch [--tab session\|kickoffs\|races\|planner\|coord\|intake]` |
| **Page builder** | `mini-ork` repo → `mini_ork/ide_pages/orch.py` (604 lines) |
| **Renderer** | `mini-ork-ui` repo → `src/page.rs`, `src/sections.rs` (generic renderer, one for all pages) |
| **Date** | 2026-10-15 |
| **Author** | engineering handover (auto-derived from code + live page JSON) |
| **Status** | ready for kickoff |

---

## 1. Why now

This page is the front door of the product — it is where work *enters* mini-ork — and it currently behaves as a read-only report. Every tab can tell you what happened; almost none of them let you do the thing they describe. The single action available on all six tabs is `New thread`, which throws you out of the page into a chat.

Two defects make this more than taste:

1. **Promised affordances that aren't wired.** `History · import threads` says *"Opening one replays it; the next prompt continues the same conversation"* (`orch.py:150`) — but only *run* rows carry a click handler. Thread rows are inert (`orch.py:137-139`).
2. **Silent failure.** The run reaper's failure is merged into the page payload's `errors` map (`mini_ork/cli/board_cmd.py:1043-1044`) and the renderer **never reads `errors`** — verified by grep across `crates/mini_ork_ui/src/`. Live evidence on this machine: every tab of the local project reports

   ```json
   "errors": {"reaper": "OperationalError: no such table: task_runs"}
   ```

   while the UI shows a clean page whose History table reads *"No threads or runs yet"*. The reaper is dead and the page reads as *"no data"* rather than *"broken"*.

The redesign has to answer: **what does the operator come here to do, and can they finish it without leaving?**

---

## 2. What this page is — the job to be done

The page's own subtitle enumerates six things, which is itself the diagnosis:

> *"How work gets in: the orchestrator thread, kickoffs, races, the planner, multi-agent coordination, and the tickets that start runs on their own."*
> — `orch.py:598-600`

Two real jobs hide inside that list, and they are different jobs with different urgency:

**JTBD-A — Dispatch (high frequency, seconds):**
> *When I have a task — in my head, in a ticket, or in a saved kickoff — I want to start it with the right recipe, lane and workspace, so I can get back to what I was doing.*

**JTBD-B — Triage the intake chain (low frequency, high stakes):**
> *When nothing is coming in or nothing is starting, I want to see which link in the chain is broken and fix it here, so I stop being the thing that unblocks it.*

There is a distant third job — *"which model should I use for this class of work?"* (Races, Coordination) — which is a decision-support job, not a dispatch job.

Everything below is measured against those.

### Who uses it

| | |
|---|---|
| **Primary** | The owner/operator of a mini-ork project. One person. Technical: writes kickoff markdown, reads YAML, knows what `MO_ORCHESTRATOR_LANE` is. |
| **Context** | Desktop, focused, single window. The IDE *is* the workspace. |
| **Frequency** | Job A several times a day. Job B once a week, usually when something has already gone wrong. |
| **Not a user** | Team members, non-technical stakeholders, anyone on mobile. There is no auth or tenancy — it is a local operator console. |

---

## 3. Current state — what ships today

Six tabs (`orch.py:22-24`), rendered through one generic two-column section grid.

### 3.1 Session — *what the orchestrator thread will be*

```
┌─ Orchestrator & intake                                    [New thread] ─┐
│ ▪ mini-ork acp                                                          │
│                                                                         │
│ ┌ Thread defaults ────────────────────────────────────────────────────┐ │
│ │  Orchestrator lane                     opus                         │ │
│ │                                        MO_ORCHESTRATOR_LANE   ← env  │ │
│ │  Recipe                                code-fix                     │ │
│ │  Workspace                             New worktree per task        │ │
│ │                                                                     │ │
│ │  note: "The thread is a control plane — the orchestrator decides…"  │ │
│ └─────────────────────────────────────────────────────────────────────┘ │
│ ┌ MCP context server · 19 tools ─────┐  ┌ History · import threads ───┐ │
│ │ list_runs run_detail learnings     │  │ No threads or runs yet      │ │
│ │ cost lanes describe_recipe …       │  │                             │ │
│ │ list_recipes start_run workspaces  │  │                             │ │
│ │ run_status stop_run certify …      │  │                             │ │
│ └────────────────────────────────────┘  └─────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
    ☝ read-only, every value. No picker, no toggle, nothing clickable.
```

- `Thread defaults` — a `kv` of three env-derived values (`orch.py:94-97`). The `sub` field literally prints the env var name (`MO_ORCHESTRATOR_LANE`) as a hint that this is config, but offers no way to change it.
- `MCP context server · 19 tools` — a `pills` wall of tool names, grey = read-only, blue = control (`orch.py:104-113`). Pure reference documentation living at the top of the product's front door.
- `History · import threads` — a merged, time-sorted list of the last 8 threads and 8 runs (`orch.py:129-150`). Only run rows open.

### 3.2 Kickoffs — *saved markdown tasks*

```
┌ Kickoffs · .mini-ork/kickoffs/                    [New kickoff] [as list]┐
│ ▤  my-task.md                                                           │
│    saved · lint clean · code-fix (default) · ran as run-1791… · failed  │
│                                   [Open] [Open run] [Start run]         │
│                                                                         │
│ note: "Lint checks: paths that do not exist, no success section, …"     │
└─────────────────────────────────────────────────────────────────────────┘
```

The richest tab, and the closest thing to a working console (`orch.py:189-232`) — it already lints each kickoff against its recipe and reports the first finding. Its gaps: **no create-with-content**, **no edit**, **no delete**, **no re-lint**, and the row never shows *why* a lint failed beyond the first message.

`New kickoff` opens a thread with the literal string `"/kickoff "` — trailing space, no argument (`orch.py:232`). The page knows the folder, the recipe and the lint rules; it hands none of that over.

### 3.3 Races — *one task, several models*

```
┌ Races                                                    [New race] ───┐
│ task          lanes        verified  kept            cost   age        │
│ No races yet — /race <task> runs one task on several models            │
│                                                                        │
│ note: default lanes from MO_RACE_LANES… A race costs ~one run per model│
└────────────────────────────────────────────────────────────────────────┘
```

Reads `/race`-launched runs grouped by kickoff within 180s (`orch.py:254-297`). Columns include a `kept` state that can read `undecided · 2 open` — but there is **no keep/discard action**, even though `mini-ork board discard` exists and run rows elsewhere carry exactly that button. The empty state explains the feature and gives you no way to invoke it except typing a slash command in a chat.

### 3.4 Classify & plan — *what the pipeline decided*

```
┌ Classification ────────────────────────────────────────────────────────┐
│ No run has been classified yet                                         │
└────────────────────────────────────────────────────────────────────────┘
```

When populated (`orch.py:312-365`): a `kv` of the classifier's output (`task_class`, `recipe`, `risk`, `confidence`) with an `Open run` action, then the plan's steps as a list, then open planner questions as list items whose action is `Answer` → `S.thread(f"Answer for {run_id}: ")`.

Two structural problems: it reports on **the newest run that happens to be classified**, not on a run *you* chose, and there is no picker to look at any other. And the "Planner questions" list is the one genuinely urgent, blocking thing this page can surface — a run is *stopped waiting for a human* — and it is buried on tab 4.

### 3.5 Coordination — *who else is working right now*

```
┌ Principals · concord ─────────────────────┐ ┌ Spawn tree ────────────┐
│ ContextNest is not reachable              │ │ No run has spawned     │
│ Concord principals, hot-file claims and   │ │ children yet           │
│ --owns violations live in ContextNest…    │ │                        │
└───────────────────────────────────────────┘ └────────────────────────┘
```

Three tables (principals, hot-file claims, `--owns` violations) gated on a ContextNest health check with a 0.5s timeout, plus a spawn tree as raw ASCII (`orch.py:377-439`). When ContextNest is down it degrades to a single explanation list — good — but there is no *start it* action, only prose telling you to.

### 3.6 Intake — *tickets that start runs by themselves*

```
┌ Sources ────────────────────────────────────────────────────────────┐
│ source   state       trigger                  recipe  mirror  synced │
│ jira     skipped     no trigger configured    —       0       never  │
│ linear   skipped     no trigger configured    —       0       never  │
│ local    registered  no trigger configured    —       0       never  │
│                                                                      │
│ note: "All four rule keys are opt-in and ANDed…"                     │
└──────────────────────────────────────────────────────────────────────┘
┌ ⚠ Nothing will start ────────────────────────────────────────────────┐
│ No registered source has a trigger. Add trigger_label,               │
│ trigger_assignee or trigger_states under a source in config/pm.yaml  │
└──────────────────────────────────────────────────────────────────────┘
┌ Recent arrivals ─────────────────────────────────────────────────────┐
│ source      action      issue       thread       when                 │
│ (empty)                                                               │
└──────────────────────────────────────────────────────────────────────┘
```

Page actions: `Preview a pass` (dry-run), `Poll now` (confirm), `Open threads` (`orch.py:585-597`).

This tab is the best-designed of the six — it has a real diagnostic callout, real verbs, and a confirm on the destructive one. Its gap: the callout tells you to edit `config/pm.yaml` and gives you **no button to open it**, and no per-source action (no "sync this source now", no "test trigger"). The blocking state and the fix live one click apart and the click isn't offered.

---

## 4. Heuristic audit

Only the violations that matter here.

| Heuristic | Finding | Evidence |
|---|---|---|
| **Match between system and real world** | The title `Orchestrator & intake` names two of the six things inside. The subtitle names all six. A page that must introduce itself by enumeration has no spine. | `orch.py:598-600` |
| **Visibility of system status** | Page-level `errors` are computed, shipped, and never rendered. A dead data source is indistinguishable from an empty one. | `board_cmd.py:1043`; no `errors` read in `crates/mini_ork_ui/src/` |
| **Recognition rather than recall** | Three primary values (orchestrator lane, recipe, workspace) are shown as dead text. To change any of them the operator must recall an env var name that the UI prints as a subtitle and then edit it elsewhere. | `orch.py:94-97` |
| **Consistency and standards** | Other pages in this same product use `composer` (free-text + Send) — the thread page (`thread.py:275`), run steering (`run.py:1162`), env editing (`setup.py:393`). This page, the *authoring* page, uses it zero times. | `grep S.composer mini_ork/ide_pages/*.py` |
| **Error prevention** | `Poll now` correctly confirms before starting runs (`orch.py:591-593`). Good — and the only place this pattern is used on the page. `New race` / `Start run` do not confirm, though both cost money. | `orch.py:224`, `orch.py:297` |
| **Aesthetic and minimalist design** | The 19-tool MCP pill wall is reference documentation occupying prime space above the fold on the default tab. | `orch.py:104-113` |
| **Help and documentation** | Notes are well-written and frequent — this page's prose is a strength. But notes carry the burden that buttons should: *"Add `trigger_label` … in `config/pm.yaml`"* is an instruction with no affordance attached. | `orch.py:522-529` |

---

## 5. Information architecture today

```
orch
├── Session      → configure the thread · reference (MCP tools) · history
├── Kickoffs     → author + lint + start saved tasks
├── Races        → compare N models on one task, then pick
├── Classify&plan→ inspect what the pipeline decided + answer blocking questions
├── Coordination → live fleet state (Concord) + spawn tree
└── Intake       → configure ticket sources + see what arrived
```

Six tabs, five mental models: *configure*, *reference*, *author*, *compare*, *observe*. Two tabs (Session, Coordination) are static configuration readouts; two (Kickoffs, Intake) are actual consoles; one (Races) is a decision surface with no decision action; one (Planner) is a monitoring surface buried at position 4.

Tags that would help a designer argue the regrouping: **Novice-first** here is the Intake callout (`orch.py:522`) — the only place the page says "here is why nothing works". That is arguably the *default* state of a new project, and it is the last tab.

---

## 6. Data + state inventory

The renderer has exactly **four** page-level states (`page.rs:1955-2015`) plus per-section degradation. Designers must model all of them; the current page models two.

| State | When | What the user sees today | File |
|---|---|---|---|
| **First load** | no data yet, no cache | `sweep_bar` + `loading_line("board page orch --tab …")` + a skeleton shaped from the *last* known section shapes for that tab, else `loading::default_shapes` | `page.rs:1955-1970`, `page.rs:370-376` |
| **Switch** | tab/filter changed, cached copy exists | previous content instant, sweep bar over it, then a fade-in (280ms) | `page.rs:1971-1977`, `page.rs:2007-2011` |
| **Page error** | the whole read failed | *"This page could not be read"* + the raw error string in red mono | `page.rs:1978-1982` |
| **Loaded** | normal | header + tabs + sections | `page.rs:1983-2014` |
| **Section error** *(per section)* | one source threw | `S.guarded` swaps that section for a `list` with one `bad("Could not read this", "<ExcType>: <msg>")` | `spec.py:746-761` |
| **Silent error** *(leak)* | reaper failure, or any key in `errors` with no matching section | **nothing at all** | `board_cmd.py:1043` |

Per-tab empty and populated states:

| Tab | Loading | Empty | Partial | Populated |
|---|---|---|---|---|
| Session | skeleton: kv + pills + table | `No threads or runs yet` (a table row, not an empty state) | MCP server import fails → `guarded` list | defaults + 19 pills + merged thread/run history |
| Kickoffs | skeleton: list | `No saved kickoffs — .mini-ork/kickoffs/ is empty — /kickoff in a thread drafts one` | one kickoff's `read_text` fails → skipped silently; >25 files → note says "showing 25 newest of N" | N rows with lint verdict + up to 3 actions each |
| Races | skeleton: table | `No races yet — /race <task> runs one task on several models` | lane parse fails → `?` in the lanes cell | grouped race rows, `verified k/n`, `kept`, summed cost, age |
| Planner | skeleton: list | `No run has been classified yet` | no `plan.json` but a profile exists → classification only | kv + plan list + open questions |
| Coord | skeleton: list + code | ContextNest down → warn list; no spawns → code box saying so | each of 3 concord calls is `guarded` separately with a 0.5s timeout | 3 tables + spawn tree |
| Intake | skeleton: table | 3 source rows always exist (sources are static); arrivals table empty | per-source `skipped_reason` / `sync_error` in the `note` column | sources + callout + arrivals ledger |

**Edge cases that need designing, currently unhandled:**

- A project with **zero kickoffs and zero runs** — the state of every new project. Today: three separate "nothing here yet" sentences on three tabs.
- **Everything is red at once** (reaper dead + ContextNest down + no trigger): today three unrelated tabs each explain a third of it. No aggregate.
- **A run waiting on a human** (`profile_gate` open question, `orch.py:355-364`) — the most time-critical state on the page, reachable only by opening the right tab.
- **A race with `undecided · N open`** — the page displays the decision and offers no decision.

---

## 7. Action inventory — what the page *can* do vs what it *does*

The renderer's full action vocabulary (`spec.py:59-69`) is available to this page. Measured against it, the page uses a small and skewed slice:

| Action verb | Available? | Used on orch? | Count |
|---|---|---|---|
| `thread` — new mini-ork thread, prefilled prompt | yes | **yes, and it is nearly the whole action surface** | 1 page-level (`New thread`) + 4 section-level (`New kickoff`, `New race`, `Answer`, `Start run`) |
| `cli` — run any `mini-ork <args>`, then refresh | yes | yes | 3, all on Intake |
| `run` — open a run's tab | yes | yes | 3 |
| `path` — open a file in the editor | yes | yes | 3 (`Open` on kickoff, plan, +1) |
| `page` — switch page/tab | yes | yes | 1 (`Open threads`) |
| `run` | yes | yes | — |
| `set` — re-fetch this page with merged args | yes | **no** | 0 |
| `url` | yes | no | 0 |
| `reveal` | yes | no | 0 |
| `project` / `project_pick` | yes | no | 0 |
| `composer` — free-text input + Send, wired to a CLI prefix | yes | **no** | 0 |
| `chips` — clickable filter/state chips | yes | **no** | 0 |

**The asymmetry to design against:** the page has 27 buttons across 6 tabs and 23 of them are either `thread` (5) or navigation (18). `S.thread("/kickoff ")` does not dispatch anything — it opens a chat with a dangling slash command and no argument. Every "create" affordance on this page is a handoff out of the page.

---

## 8. Constraints the design must live inside

These are not preferences — they are what the renderer accepts.

**The renderer is generic and shared.** `MiniOrkPage` draws *every* page from the same JSON (`page.rs:1-3`). Section types available to this page: `table`, `kv`, `list`, `bars`, `flow`, `code`, `pills`, `chips`, `dag`, `inspector`, `markdown`, `hero`, `triage`, `callout`, `story`, `files`, `findings`, `checks`, `agents`, `composer`, `columns`, `document` (`spec.py:20-57`). Anything outside this set is a change to the shared renderer, which affects all 13 page types — price it explicitly.

**Layout.** Sections are cards in a two-column grid, gap 12px. `full: true` spans both columns; `col: 1` / `col: 2` pins to a column for list/detail layouts; otherwise sections flow two to a row (`page.rs:666-717`). The page content is left-aligned full-width with a `px-5 pt-4 pb-12` gutter (`page.rs:2029-2038`).

**Colours are names, never hex** (`spec.py:71-73`): `text body muted sub dim blue green red yellow purple cyan orange`, plus lane families as `fam:<lane>`. They map onto the reader's active editor theme; `MINI_ORK_PALETTE=orca` pins the neutral dark set. Design in both light and dark.

**Design tokens** (`palette.rs`): title 13px · badge 10px · pill 9px · card radius 10 · button radius 6. Activity rail is 42px wide, 12 icons (`sidebar.rs:52`, `sidebar.rs:73-86`). Run-state marks are fixed: `●` working / `✋` needs-you / `✗` failed / `✓` done / `○` queued.

**Button kinds:** `primary`, `default`, `danger`, `warn`, `ghost` (`spec.py:68`). Row buttons render at 22px height, section-head buttons at 24px, page-header buttons at 26px (`page.rs:894`, `page.rs:1007`, `page.rs:627`).

**Confirmations** exist as a modal prompt on any action carrying `confirm` (`page.rs:521-547`).

**Accessibility baseline today:**
- The page root calls `track_focus` (`page.rs:2019`), but **tab strips are click-only** — `on_click` with no keyboard handler, no roving tabindex, no focus ring (`page.rs:644-659`).
- Selectable text is routed through `Texts` so most content can be copied and, in places, is right-click copyable (`page.rs:1000`).
- Contrast is theme-dependent by construction — the page cannot guarantee a ratio. Flag any design that depends on `DIM` (`0x5f6672`) against `BG` on a light theme.
- Hit targets: 22px row actions are below a comfortable 32px; hover is the only affordance that a row is clickable (no cursor change on some rows).

---

## 9. The ask — design questions to answer

Deliverable: a redesign of the `orch` page as a **console**, that keeps the existing section vocabulary.

**Q1 — What is the spine?** Pick the organising principle. Candidates, in the order the code suggests them:
- by **job** (Dispatch · Intake health · Compare models · Configure) — argues for 3–4 groups and demotes reference material;
- by **lifecycle of a task** (draft kickoff → classify → plan → race → run → steer) — argues for one vertical flow and kills the tab metaphor;
- by **urgency** (`needs you now` · `running` · `configured` · `reference`) — argues for a triage-led page like the Intake tab's callout, promoted to the top.

The code most supports the third: the only urgent thing the page can surface today (a run stopped at `profile_gate`) is buried on tab 4, and the only explanatory thing (why nothing starts) is on tab 6.

**Q2 — Where does authoring happen?** The page has a folder of kickoffs, a lint engine, a recipe resolver and four CLI verbs. Should a kickoff be drafted *in* the page (a `composer` bound to the kickoff path, plus an inline lint result), or is handing off to a thread genuinely correct? If the handoff stays, design the handoff so it carries context — the current pattern (`S.thread("/kickoff ")`) carries none.

**Q3 — How does a decision get made here, not elsewhere?** A race with `undecided · 3 open` is a decision waiting for a human. Design the decision: what does the operator see to choose, and what are the buttons (keep this one / discard the rest)? Same question for an open planner question and for an `undecided` intake source.

**Q4 — How do the three "defaults" become controllable?** Lane / recipe / workspace appear as read-only text with an env var printed underneath. Options range from a picker per value to a "these come from `<file>` — open it" affordance. Decide, and decide where the *source of truth* disclosure lives.

**Q5 — What is the page's resting state on a brand-new project?** Today: three "nothing yet" sentences on three tabs, and one diagnostic callout on the last tab. Design the zero state as a first-run checklist or equivalent — the operator needs to know the four things to do, in order.

**Q6 — How is a broken data source shown?** The payload already carries `errors`, and the renderer drops it. Design the treatment for "this section's source failed" (distinct from "this section is empty"), and say what the *page-level* summary looks like when three sources are down.

**Q7 — Is six tabs right?** If the spine changes, tab count changes. If tabs remain, propose labels that name the *job* rather than the subsystem — `Session` and `Coordination` currently name data structures.

---

## 10. Success metrics

Proposed — confirm with the owner before locking.

| Kind | Metric | Target |
|---|---|---|
| **User outcome** | Time from "I have a kickoff" to "a run is dispatched" | < 15 s, without opening a thread |
| **User outcome** | Time to answer *"is intake healthy?"* from a cold open | < 5 s |
| **Behavioral** | Share of `orch` page sessions ending in a dispatched run rather than a thread handoff | > 50% |
| **Behavioral** | Share of `orch` page sessions that switch tabs 3+ times before acting (a proxy for "the answer wasn't where I looked") | < 20% |
| **Technical** | First contentful paint for `orch` — the read shells out to `mini-ork board page`, so it is not instant | budget to be set by engineering; today's worst case is the Intake tab (SQLite + 3 source probes) |
| **Technical** | Every entry in `payload.errors` is visible in the UI | 100% (today: 0%) |

---

## 11. In scope / out of scope

**In scope**
- The `orch` page: its tab set, section composition, hierarchy, per-tab states, empty states, action placement, and copy.
- Composition using the existing section vocabulary in `mini_ork/ide_pages/spec.py`.
- The activity-rail entry for `✦` (its label, tooltip, and whether it should carry a badge).

**Out of scope**
- Any change to the shared renderer in `crates/mini_ork_ui` — including new section types, keyboard-navigable tabs, and rendering the `errors` map. Propose these as named renderer tickets if the design needs them; they affect all 13 pages.
- The orchestrator thread itself (its chat panel, model picker, steering), which lives in Zed's agent panel.
- Any other page (`runs`, `verify`, `recipes`, `lanes`, `inbox`, `kanban`, `task`). Cross-page moves — e.g. "promote planner questions to the `inbox` page" — are a product decision, not part of this brief.
- Backend/CLI work: new subcommands, new data. The design may assume new *actions* mapped onto existing verbs; it may not assume new verbs.

---

## 12. Appendix — file map and how to see it

**Page builder (engine)**
| File | What it contributes |
|---|---|
| `mini_ork/ide_pages/orch.py` | The whole page: 6 tab builders, `build()` at `:565` |
| `mini_ork/ide_pages/spec.py` | The section + action vocabulary the designer composes from |
| `mini_ork/ide_pages/header.py` | Shared page header helpers |
| `mini_ork/cli/board_cmd.py:1000-1044` | Where `payload["errors"]` is assembled (and where the reaper key is merged) |
| `mini_ork/intake/poll.py` | `source_statuses()` — the real shape behind the Intake table |
| `mini_ork/acp/history.py`, `mini_ork/acp/threads.py` | `list_runs` / `ThreadStore` behind the History table |

**Renderer (mini-ork-ui crate)**
| File | What it contributes |
|---|---|
| `src/page.rs:584-631` | Page header: title, chips, desc/sub, actions |
| `src/page.rs:633-662` | Tab strip |
| `src/page.rs:666-772` | The two-column section grid (`full` / `col`) |
| `src/page.rs:905-1158` | `table`, `kv`, `list` sections |
| `src/page.rs:1160-1269` | `bars`, `flow`, `code`, `pills`, `chips` |
| `src/sections.rs:130-224` | `callout` — the tone-coloured diagnostic card |
| `src/sections.rs:226-337` | `hero` |
| `src/sections.rs:474-562` | `composer` — the only free-text input |
| `src/palette.rs` | Colour names, type scale, radii, theme mapping |
| `src/sidebar.rs:73-86` | The 12 activity-rail entries |

**How to see the real page (read-only, no build)**
```bash
H=/path/to/project/.mini-ork
mini-ork board page orch                --home $H | jq   # tabs, sections, actions
mini-ork board page orch --tab intake   --home $H | jq
```
Run it against a project with history for populated states, and against a fresh `mini-ork init` project for the zero state. **Attach screenshots of both** — this brief contains wireframes derived from live JSON, not pixels.

**Related prior art in this product** — worth a designer's 10 minutes, because these already solve parts of the problem: the `intake` tab's callout treatment (`orch.py:520-530`), the `thread` page's composer (`ide_pages/thread.py:275`), and the `run` page's steering composer (`ide_pages/run.py:1162`).

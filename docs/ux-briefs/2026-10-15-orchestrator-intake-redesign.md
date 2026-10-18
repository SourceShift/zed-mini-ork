# Orchestrator & intake — redesign: journeys, FE and BE design

Companion to [the brief](2026-10-15-orchestrator-intake.md). That document says *what is wrong*. This one says *what to build*.

> **Build contract:** the engine-side spec for this design lives in the mini-ork repo at
> `specs/orchestrator-page-redesign.spec.md` (SourceShift/mini-ork, `main`). It carries the
> EARS requirements, acceptance criteria, error-handling table and the phased TODO. **Line
> anchors in this document are stale** — `mini_ork/ide_pages/orch.py` grew from 604 to 791
> lines after this was written. The spec cites functions, not lines; use it when implementing.

| | |
|---|---|
| **Page** | `mini-ork IDE` → rail `✦` → `board page orch` |
| **Engine** | `mini_ork/ide_pages/orch.py` (`mini-ork` repo) |
| **Renderer** | `crates/mini_ork_ui` (`mini-ork-ui` repo) — shared by all 13 pages |
| **Date** | 2026-10-15 |
| **Rule applied** | Everything here composes from the section vocabulary in `mini_ork/ide_pages/spec.py`. Renderer changes are listed separately, never assumed. |

---

## 0. Method, and one constraint that shapes everything

The page is JSON. The engine composes it, the renderer draws it. So "FE design" here means **which sections the engine emits**, and "BE design" means **what data and verbs the engine needs to emit them**.

Three facts from the recon drive most of the design:

1. **The IDE has no dropdown, no modal form, and no inline form.** The only input is `composer` — one text field whose contents become the last argv element of a CLI call (`sections.rs:474-562`). Every "pick a value" affordance must therefore be built from `chips` + `S.set_args(...)`: clicking a chip re-fetches the page with a new arg, and the engine draws the *next* state. This is a reload-driven picker. It works, it is already used in the product, and it is the pattern this design leans on.
2. **Several BE primitives exist and are simply unreferenced by this page.** `orchestrator_lanes()`, `recipes_catalog.list_recipes()`, `kickoff_lint`'s `fix` field, `web.control.launch_run`, the per-source `intake once --source`, `concord send|ack|stop|inbox|admit`, `board merge|discard`. Roughly half the "missing BE" is wiring, not building.
3. **The engine can fix its own invisible-errors defect without touching the renderer.** `payload.errors` is shipped and never drawn. The engine can turn its own `errors` dict into a `callout` section before returning. That is a ~10-line change with no shared-renderer risk.

---

## 1. The ten journeys

Ranked by frequency × consequence. Every one is a thing an engineer owning a mini-ork project actually does.

| # | Journey | Frequency | Cost of the page failing today |
|---|---|---|---|
| **J1** | Start a run from a saved kickoff, with overrides | several/day | Bounced to a thread; retype the task |
| **J2** | See whether a ticket that arrived on its own started a run | several/day | None — the ledger doesn't link to the run |
| **J3** | Draft a new kickoff and get it lint-clean | daily | No authoring path at all |
| **J4** | Find out why nothing is starting | weekly | Answer exists, last tab, no fix button |
| **J5** | Answer a run that is blocked waiting on me | weekly | Buried on tab 4; bounce to a thread |
| **J6** | Decide a race — keep one run, discard the rest | weekly | Decision shown, no decision action |
| **J7** | Set the orchestrator's lane / default recipe / workspace | monthly | Read-only text naming an env var |
| **J8** | See and drive the live fleet (principals, claims) | weekly | Three tables, zero actions |
| **J9** | Review recent classifications and correct the router | monthly | Newest-run-only, no picker |
| **J10** | Resume an earlier orchestrator conversation | weekly | Row is inert; note promises replay |

---

## 2. Page architecture

The six current tabs are five mental models (`configure`, `reference`, `author`, `compare`, `observe`) wearing a tab strip. Journeys 1–4 are all "get work in". This design regroups by job and hoists urgency out of the tabs entirely.

```
┌ Orchestrator ──────────────────────────────────────────────────────────────┐
│ ✦ Orchestrator   [3 sources · 1 wired] [scheduler on] [ContextNest ✓]      │
│                                          [Start a run]  [New thread]       │
│                                                                             │
│ ⚠ 2 runs are waiting for you · 1 race undecided · intake synced 3d ago      │
│   [Answer in Decide]  [Decide the race]  [Poll now]                         │
├─────────────────────────────────────────────────────────────────────────────┤
│ Dispatch │ Intake │ Decide │ Fleet                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│  … tab content …                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

- **Header chips** are the project-health rollup — four facts an engineer wants on every visit. Today the header carries `mini-ork acp`, which is a version tag nobody acts on.
- **The attention strip** is one `triage` section, prepended to *every* tab's section list by `build()`, and omitted entirely when there is nothing to say. This is the single highest-leverage change on the page: it is the only element that answers "what needs me" without a tab hunt. `triage` already renders `counts` and a `menu` of actions (`sections.rs:22`).
- **Four tabs, by job:**

| Tab | Journeys | What it is |
|---|---|---|
| **Dispatch** | J1, J3, J7 | The front door: pick or write a kickoff, set overrides, start it |
| **Intake** | J2, J4 | The ingestion chain: sources, triggers, arrivals, chain health |
| **Decide** | J5, J6, J9 | Everything where a human judgement unblocks the machine |
| **Fleet** | J8, J10 | Live coordination, orchestrator history, reference |

Sections that survive from today: the Intake sources table, the arrivals ledger, the spawn tree, the MCP pills (demoted), the history table.

---

## 3. Per-journey design

Each entry: the job, what happens today, the FE composition, the BE work, and the acceptance test.

---

### J1 — Start a run from a saved kickoff, with overrides

> *I have `fix-auth-timeout.md` saved. I want it running on `opus` in a fresh worktree, now, without a chat.*

**Today.** `Kickoffs` tab lists saved kickoffs with a lint verdict. `Start run` calls `S.thread(f"Start a run from the kickoff {path}")` (`orch.py:224`) — opens a chat, starts nothing. There is no recipe or workspace override anywhere on the page.

**FE design** — *Dispatch tab.*

```
┌ Start a kickoff ────────────────────────────────────────────────────────────┐
│ Recipe:    [ code-fix (from kickoff) ] [ epic-runner ] [ code-review ]      │
│ Workspace: [ worktree ] [ in-place ]                                        │
│                                                      note: overrides this    │
│                                                      start only             │
├─────────────────────────────────────────────────────────────────────────────┤
│ ▤ fix-auth-timeout.md                                                       │
│   code-fix · lint clean · last run-64879-1 ✓ done · 2d                     │
│                          [Start]  [Open]  [Open run]  [Log]                │
│ ▤ add-linear-sync.md                                                        │
│   ⚠ no success section — add "## Success" … · code-fix · never run          │
│                          [Start]  [Open]  [Lint]                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

- Recipe and workspace rows are `chips` sections; each chip is `S.set_args(recipe="epic-runner")`. The selected chip is `on: true`. The engine reads `args.get("recipe")` when composing every `Start` button.
- `Start` is `S.cli("board", "start", path, "--recipe", recipe, "--workspace", workspace)` — **primary, no confirm** (starting a run is the point of the page; confirming it would be parody).
- The row's `sub` shows the *first* lint finding with its `fix`, truncated. `Lint` opens the full finding list for that kickoff (see J3).

**BE design.**

| Item | Status |
|---|---|
| `board start <kickoff.md> [--recipe R] [--workspace M] [--set K=V] [--dry-run]` | **MISSING — the critical one.** Wrap `mini_ork.web.control.launch_run(home, recipe, kickoff_markdown, run_id, extra_env)` (`web/control.py:626`), which already spawns `bin/mini-ork run <recipe> <kickoff>` detached, validates recipe/run-id against a safe charset, stages the kickoff at `runs-inbox/<run_id>.md` and returns `run_id` immediately. Today it is reachable only over HTTP (`web/routes/runs.py:33`) and from the ACP agent; the IDE's action vocabulary has no HTTP POST, so a CLI wrapper is the only bridge. |
| Recipe list for the chips | **Exists** — `mini_ork.recipes_catalog.list_recipes(home)`. |
| Workspace values | **Exists** — `worktree` \| `in-place` (`_WORKSPACE_LABELS`, `orch.py:26`). |
| Kickoff rows carry the last run's status + log path | **Partial** — `runs_by_path` (`orch.py:195-198`) already resolves the run; the log path (`<home>/runs/<id>/…`) is not emitted. Add `log_path` to the row so `Log` can `S.open_path` it. |
| Kickoff row shows the first finding's `fix`, not just `msg` | **Exists, unused** — `kickoff_lint.lint` returns `{"sev","msg","fix"}` (`kickoff_lint.py:221`); `orch.py:212-217` drops `fix`. |

**Done when** — from a cold page, an engineer picks a kickoff, changes the recipe chip, clicks `Start`, and a run appears on the board within a second, with no thread opened.

---

### J2 — See whether a ticket that arrived on its own started a run

> *A Linear card got labelled. Did mini-ork pick it up? If not, why?*

**Today.** `Intake` tab shows a sources table and an arrivals ledger. The ledger's `thread` column is mono text with no link; there is no run column, and `intake_events` has no `run_id` (`threads/store.py:96-108` — the table has `thread_id`, `issue_uid`, `action`, `detail`, no run).

**FE design** — *Intake tab.*

```
┌ Recent arrivals ────────────────────────────────────────────────────────────┐
│ source  action     issue        run              when                        │
│ linear  launched   LIN-482      run-64879-1 →    now      [Open run]        │
│ linear  duplicate  LIN-481      —                4m                          │
│ jira    skipped    JIRA-90      —                1h       [Why?]            │
├─────────────────────────────────────────────────────────────────────────────┤
│ ▤ 3 arrived today · 1 launched · 1 folded as duplicate · 1 skipped           │
└─────────────────────────────────────────────────────────────────────────────┘
```

- `run` becomes a real link (`S.open_run`) the moment the ledger knows it.
- `Why?` on a `skipped` row opens the source's trigger config — because "skipped" is the normal, correct state and the engineer needs to be able to confirm *why* without reading code.

**BE design.**

| Item | Status |
|---|---|
| Link an arrival to its run | **MISSING.** Two options: (a) add `run_id` to `intake_events` and set it in `intake/dispatch.py` where the run is launched — a one-column migration against an existing table, done with the same `CREATE TABLE IF NOT EXISTS` + `ALTER TABLE` pattern the store already uses; or (b) join `thread_id` → the thread's run. (a) is honest and cheap; (b) is a guess because a thread can start several runs. **Recommend (a).** |
| Per-arrival `Why?` target | **Exists** — `S.open_path(home / "config" / "pm.yaml")`. |
| Day rollup | **Exists as data** — `_rows(... FROM intake_events WHERE created_at >= …)`, aggregated by `action`. |

**Done when** — an engineer can answer "did my ticket start a run" from the ledger row alone, in one glance, without opening a terminal.

---

### J3 — Draft a new kickoff and get it lint-clean

> *Nothing saved matches this task. I want to write it, see what's wrong with it, fix it, and run it — here.*

**Today.** No authoring path. `New kickoff` opens a thread with the literal string `"/kickoff "` (`orch.py:232`).

**FE design** — *Dispatch tab, above the kickoff list.*

```
┌ New kickoff ────────────────────────────────────────────────────────────────┐
│ [ # What should happen, and how you'll know it worked…        ]  [Draft]    │
│ note: creates .mini-ork/kickoffs/<slug>.md and opens it in the editor       │
├─────────────────────────────────────────────────────────────────────────────┤
│ Lint · fix-auth-timeout.md                              [Re-lint] [Open]    │
│ ✗ scope path does not exist: src/auth/session.ts                            │
│   → create it, or mark the line "(new)"                                     │
│ ⚠ no success section                                                        │
│   → add "## Success" with the commands that must pass                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

- Drafting is a `composer` (one text field). The typed text becomes the draft's title line; the file is created from a template and opened. Honest limitation: a composer cannot hold a multi-paragraph kickoff — the draft is a *stub*, and `Open` is where the real writing happens. That is a fair split and avoids a renderer change.
- The lint panel is a `code` section listing **every** finding as `{glyph} {msg} → {fix}`, for the kickoff currently selected in the list above (`set_args(kickoff=<path>)`).
- `Re-lint` is `S.set_args(kickoff=<path>, t=<epoch>)` — the cache-buster forces a re-read. (The page already uses `set_args` with a timestamp as a cache-buster elsewhere; see `setup.py:581`.)

**BE design.**

| Item | Status |
|---|---|
| `kickoff new <text>` → writes `.mini-ork/kickoffs/<slug>.md` from a template, prints the path | **MISSING.** Small. Slugify, refuse to overwrite, seed the front matter (`recipe:`) and a `## Success` stub so a fresh draft lints clean-ish rather than red. |
| Every finding with its `fix` | **Exists, unused** — same as J1. |
| `Re-lint` cache-buster | **Exists** — page `args` are already threaded through `build(home, tab, args)`. |

**Done when** — an engineer can go from "blank page" to "a kickoff file open in the editor, with a known list of what is still wrong with it" without leaving the IDE.

---

### J4 — Find out why nothing is starting

> *I labelled a ticket an hour ago. Nothing. What is broken?*

This is the journey the page currently *almost* serves — the Intake callout is genuinely good (`orch.py:520-530`) — and it fails on two counts: it is the last tab, and its remedy is prose.

**FE design** — *Intake tab, and the attention strip.*

```
┌ Intake chain ────────────────────────────────────────────────────────────┐
│ ✓ linear is registered                          [Open config]            │
│ ✗ linear has no trigger                          → add trigger_label     │
│ – jira is disabled in config                     [Enable]                │
│ ✓ scheduler is installed and running             [Status]                │
│ ✗ the run reaper is failing                      → task_runs unreadable  │
│                                            1 failing · 1 pending · 2 passed│
└──────────────────────────────────────────────────────────────────────────┘
```

- Built from the `checks` section type, which already renders exactly this: rows with state, a detail, and a `passing/failing/pending/na` summary (`spec.py:543`).
- Each row's action is the CLI that fixes it: `Open config` → `S.open_path(config/pm.yaml)`; `Poll now` → `S.cli("intake","once" , confirm=…)`; `Status` → `S.cli("automations","scheduler","status")`.

**BE design.**

| Item | Status |
|---|---|
| Source registration / trigger / skip reason | **Exists** — `intake.poll.source_statuses()` + `_SOURCE_STATE` (`orch.py:447-452`). |
| Scheduler state | **Exists** — `mini-ork automations scheduler status --json`. Not read by this page today. |
| Reaper failure | **Exists but invisible.** `board_cmd.py:1040-1044` merges the reaper error into `payload["errors"]`; the renderer never draws `errors`. |
| Chain-health section | **MISSING (engine-only).** Add `_chain_health(home)` returning a `checks` section; and — the important half — **turn the `errors` dict into a `callout` at the end of `build()`**, so any error the engine captured becomes visible regardless of which key it was filed under. This fixes the defect for every tab of every page, engine-side, with no renderer change. |

**Done when** — a new project that has never polled anything shows, on the default tab, a checklist that names the one missing thing and the button that adds it.

---

### J5 — Answer a run that is blocked waiting on me

> *A run stopped because the kickoff was ambiguous. It is costing wall-clock every minute it waits.*

**Today.** The planner questions live on tab 4 (`Classify & plan`), as `list` items whose only action is `S.thread(f"Answer for {run_id}: ")` (`orch.py:360-361`). The run is genuinely stopped — `profile_gate` sets `profile_status = "needs_answers"` — and the page treats it as a footnote.

**FE design** — *attention strip + Decide tab.*

```
⚠ 1 run is waiting for you                                    [Go to Decide]
┌ Blocked on you ───────────────────────────────────────────────────────────┐
│ ? run-64879-1 · code-fix · waiting 42m                                    │
│   "Should the migration be reversible, or is a one-way schema drop OK?"   │
│   [ answer…                                        ]  [Send]              │
│   note: answers are saved to profile-answers.json; the run resumes        │
│ ? run-64880-3 · epic-runner · waiting 3m                      [Dismiss]   │
│   "Which of the two epics should land first?"                             │
└───────────────────────────────────────────────────────────────────────────┘
```

- Each question gets its **own** `composer` — that is what composers are for, and `thread.py:275` already proves the pattern. The composer's `cli` is `["board","answer", run_id, "--text"]`, so the typed text becomes the answer.
- The strip's `triage` counts (`sections.rs:22`) give "N waiting" without opening the tab. The `icon` is `?` and the `tone` escalates with age.

**BE design.**

| Item | Status |
|---|---|
| Reading questions | **Exists** — `web.control.get_profile(home, run_id)` returns `human_questions` (`web/control.py:334`). |
| Writing answers and unblocking | **Exists** — the answer writer already persists `profile-answers.json`, sets `profile_status="ready"`, raises `confidence` to ≥0.9, clears `human_questions`, and returns a suggested re-run command (`web/control.py:~390-415`). Reachable over HTTP only. |
| `board answer <run_id> --text T` | **MISSING.** Wrap the above. Return the suggested re-run so the page can offer `Start again` as a follow-up button. |
| Scanning **all** runs for `profile_status == "needs_answers"` | **MISSING (engine-only).** `_planner` today inspects only the newest run with a `run_profile.json` (`orch.py:316`). The Decide tab needs a scan over recent runs. Cheap: reuse `_rows(home, "SELECT id FROM task_runs ORDER BY created_at DESC LIMIT 60")` and test each run dir. |

**Done when** — a stopped run is visible from every tab, and answering it is one sentence and one click, in the page.

---

### J6 — Decide a race

> *Four models ran the same kickoff overnight. One won. Keep it, throw away the other three.*

**Today.** The `Races` tab shows a `kept` column that can read `undecided · 3 open` (`orch.py:283`) and offers no way to decide. The verbs exist: `mini-ork board merge <run_id>` commits and lands a run's worktree, `board discard <run_id>` drops it (`board_cmd.py:511-515`).

**FE design** — *Decide tab.*

```
┌ Races ────────────────────────────────────────────────────────────────────┐
│ ▤ fix-auth-timeout · started 9h ago · 4 contestants · $3.84 total          │
│   ● opus      run-64879-a   ✓ verified  ✓ 18/18 checks   $1.40   [Review]  │
│   ● sonnet    run-64879-b   ✗ 12/18 checks               $0.62   [Review]  │
│   ● glm       run-64879-c   ✗ verifier refuted           $0.55   [Review]  │
│   ● deepseek  run-64879-d   ✓ verified  ✓ 17/18 checks   $1.27   [Review]  │
│                          [Keep opus] [Keep deepseek] [Discard all]         │
│   note: Keep merges the winner and discards the rest — 3 worktrees freed   │
└───────────────────────────────────────────────────────────────────────────┘
```

- One `list` per race, not one table row per race. The contestants are the decision input and they belong on screen.
- `Keep <lane>` is a composite action — see BE. It carries a `confirm` naming exactly what will be merged and what will be thrown away.
- `Review` opens the contestant's run tab (`S.open_run(run_id, title, tab="changes")`) so the engineer can look before deciding.

**BE design.**

| Item | Status |
|---|---|
| Grouping contestants into races | **Exists** — `_races` already groups by kickoff within a 180 s window (`orch.py:254-297`). |
| `board merge` / `board discard` | **Exist** — `board_cmd.py:511-515`. |
| Per-contestant verdict, not just `status == published` | **Partial** — `verified` today counts `status == "published"` (`orch.py:280`). The check counts live in the run dir's verifier output; reuse whatever `verify.py` reads. |
| `board race-keep <run_id>` — merge one, discard its siblings, atomically enough to report what failed | **MISSING.** The `cli` action is a single argv list with no chaining, so `Keep` cannot be expressed as `merge && discard && discard`. Options: a composite verb, or N buttons and a lecture. **Recommend the composite verb** — it is the only one that can confirm with real numbers ("merge run-a, discard b/c/d, free 3 worktrees") and report a partial failure honestly. |

**Done when** — the decision presented by the page can be executed by the page.

---

### J7 — Set the orchestrator's lane, default recipe and workspace

> *Opus is overkill for my backlog grooming. Make the orchestrator run on sonnet.*

**Today.** `Thread defaults` is a read-only `kv` whose `sub` field prints the env var *name* as an apology (`orch.py:94-101`).

**FE design** — *Dispatch tab footer, or Fleet → Orchestrator.*

```
┌ Thread defaults ─────────────────────────────────────────────────────────────┐
│ Orchestrator lane   [ opus ✓ ] [ sonnet ] [ haiku ]        note: from the    │
│ Recipe              [ code-fix ✓ ] [ epic-runner ] [ … ]   lane map;         │
│ Workspace           [ worktree ✓ ] [ in-place ]            click to change   │
│                                                                              │
│ note: these are the defaults the orchestrator proposes. Model is still the   │
│ only picker inside a thread.                    [Open config/agents.yaml]    │
└──────────────────────────────────────────────────────────────────────────────┘
```

- Three `chips` groups. A click emits `S.cli("features","set-env","MO_ORCHESTRATOR_LANE","sonnet", home=False)` — `home=False` because `features` rejects `--home` (`spec.py:102-113`, and `setup.py:458-488` already uses exactly this shape for the settings catalogue).
- The subtlety: **the persisted default is not the env var.** `default_lane` reads `MO_ORCHESTRATOR_LANE` first, then the `orchestrator` role in the home's lane map (`acp_orchestrator/config.py:88-110`). `features set-env` writes `.mini-ork/config/features.yaml`'s `env:` section, which `features/store` applies via `os.environ.setdefault` at startup — so it *does* win, and it is the only writable path the IDE has. The note should say **"set here, overrides the lane map"** so nobody debugs a phantom.

**BE design.**

| Item | Status |
|---|---|
| Lane option list | **Exists, unused** — `acp_orchestrator.config.orchestrator_lanes()` returns every Claude-CLI lane, ordered opus → sonnet → rest (`config.py:51-86`). |
| Recipe option list | **Exists, unused** — `recipes_catalog.list_recipes(home)`. |
| Workspace values | **Exists** — two, hardcoded legitimately. |
| Writing the value | **Exists** — `features set-env <NAME> <value>` accepts any valid env name that is not a credential and not feature-owned (`features/store.py:231-244`). All three names pass. |
| Showing *where* a value came from | **MISSING (engine-only, small).** Read back the precedence chain and label each chip row `env · features.yaml · agents.yaml · default`. Without it, "why is it still opus" is unanswerable. |

**Done when** — changing the orchestrator's model is three clicks and the page tells you which layer you just wrote to.

---

### J8 — See and drive the live fleet

> *Two agents are editing the same file. Who holds the claim, and can I stop one?*

**Today.** The `Coordination` tab shows principals, hot-file claims and `--owns` violations as three tables with no actions (`orch.py:384-414`), plus a spawn tree drawn as raw ASCII (`orch.py:423-439`).

**FE design** — *Fleet tab.*

```
┌ Principals · concord ──────────────────────────────────────────────────────┐
│ principal          pid     state   inbox   what                            │
│ epic-4821-impl     41928   active  2       run-64879-a · implementer       │
│                             [Send] [Inbox] [Stop]                          │
├────────────────────────────────────────────────────────────────────────────┤
│ Claims on hot files                                                        │
│ src/auth/session.ts   epic-4821-impl   expires 4m          [Admit / Defer] │
├────────────────────────────────────────────────────────────────────────────┤
│ ⚠ 1 scope violation                                                        │
│ epic-4821-impl edited src/db/migrate.ts — caller epic-4830-root            │
│                                          [Open the edit] [Stop the holder] │
├────────────────────────────────────────────────────────────────────────────┤
│ Spawn tree                       note: root epic-4821 → 3 children          │
│ epic-4821  epic-runner                                                     │
│ ├─ run-64879-b  code-fix  ✓                                                │
│ └─ run-64879-c  code-fix  ●                                                │
└────────────────────────────────────────────────────────────────────────────┘
```

- Every row gets actions, because every one has a verb: `concord send <principal>`, `concord inbox <principal>`, `concord stop <principal>`, `concord admit <epic>` (`concord --help`). **All of them exist today and none are wired.**
- `Send` is a `composer` (`["concord","send", principal_id]`), consistent with J5.
- When ContextNest is unreachable the tab keeps its current good behaviour — one explanation list — and gains the missing `Start ContextNest` reference (`S.open_path` on the compose file, or a `url` action if `mini-ork serve` is up).

**BE design.**

| Item | Status |
|---|---|
| Principals / claims / violations payloads | **Exist** — `cn_client.coord_list_principals`, `coord_hot_claims`, `coord_owns_violations` (`orch.py:384-408`). |
| Every action verb | **Exists** — `mini-ork concord send\|inbox\|ack\|stop\|admit`. |
| Principal → run mapping (the `what` column) | **MISSING (engine-only).** The principal id is not linked to a run anywhere on the page. `epic-4821-impl` should say which run it is. Tie it to `run_spawns` / the run's principal field. |
| Remediation for a violation | **MISSING (engine-only).** `Open the edit` needs the violating path — already in the payload (`v["path"]`); `Stop the holder` needs a confirm. Both are wiring. |

**Done when** — a coordination problem can be diagnosed *and* resolved from this tab.

---

### J9 — Review recent classifications and correct the router

> *This task was routed to `code-fix` but it is really an `epic-runner` job. Why did it misclassify, and how do I stop it?*

**Today.** The `Classify & plan` tab reports on the newest run that happens to hold a `run_profile.json` (`orch.py:312-320`) — no picker, no history, no correction path.

**FE design** — *Decide tab.*

```
┌ Classifications ─────────────────────────────────────────────────────────────┐
│ run          task_class    recipe        conf    actual outcome             │
│ run-64879-1  bug-fix       code-fix      0.91    ✓ done        [Review]     │
│ run-64878-4  feature       epic-runner   0.62    ✗ failed      [Review]     │
│ run-64877-9  chore         code-fix      0.88    ✓ done        [Review]     │
│                                                        note: 2 of 12 below   │
│                                                        confidence 0.7       │
├──────────────────────────────────────────────────────────────────────────────┤
│ Classify & plan · run-64878-4                              [Open run]        │
│ task_class bug-fix · recipe code-fix · risk medium · confidence 0.62         │
│ ── Plan ──                                                                   │
│ 1 · extract the config loader into its own module                            │
│ 2 · add the migration …                                                      │
└──────────────────────────────────────────────────────────────────────────────┘
```

- A `table` of classifications with a confidence column, sorted so the low-confidence ones are visible — this is the router-tuning surface, and it is currently unreachable.
- Selecting a row (`S.set_args(run_id=…)`) drives the detail panel below, so the plan shown is the plan you clicked, not "the newest one".

**BE design.**

| Item | Status |
|---|---|
| Per-run profile | **Exists** — `run_profile.json`, read by `_planner` (`orch.py:322-332`). |
| Scan over recent runs | **MISSING (engine-only).** Same scan as J5; share the helper. |
| Plan artefact | **Exists** — `plan_artifact` / `plan_steps` (`orch.py:333-354`). |
| **Correction path** ("this should have been a `chore`") | **MISSING (BE + product).** The engine has prefs (`mini-ork prefs set <key> <value> --scope task_class`) and this is exactly the operator-preference mechanism. Design: a `Correction` composer bound to `["prefs","set", …]` with the run's task class pre-filled. **Flag this to the owner as a product decision** — it writes durable behaviour, not a run-local setting. |

**Done when** — the router's recent decisions are reviewable, ordered by doubt, and correctable from the page where you noticed the mistake.

---

### J10 — Resume an earlier orchestrator conversation

> *We discussed a plan for the import rewrite last week. Pick it back up.*

**Today.** `History · import threads` claims *"Opening one replays it; the next prompt continues the same conversation"* (`orch.py:150`) — and thread rows have no `do` at all (`orch.py:137-139`). Only run rows open.

**FE design** — *Fleet tab.*

```
┌ Pick up where you left off ──────────────────────────────────────────────────┐
│ ▤ Rework the import pipeline       thread · $2.10 · 3d ago   [Open]        │
│ ◇ run-64879-1 fix-auth-timeout     run    · $1.40 · 2d ago   [Open run]    │
│ ▤ Backlog grooming                 thread · $0.40 · 5d ago   [Open]        │
│                                       note: opening a thread continues it — │
│                                       the next prompt is the same conversation│
└──────────────────────────────────────────────────────────────────────────────┘
```

- The thread rows get their action. `mini_ork/ide_pages/thread.py` exists and is the target (`S.page_link("thread", …)`); the engine needs whatever arg `thread.py` keys on.
- Type glyphs (`▤` thread, `◇` run) instead of the current `sub` text, so the eye sorts them.

**BE design.**

| Item | Status |
|---|---|
| Thread list + per-thread cost | **Exists** — `ThreadStore.list_threads()` + `_thread_cost` (`orch.py:129-139`). |
| Opening a thread | **Missing link.** Resolve the arg `thread.py` expects and emit `S.page_link("thread", **{...})` per row. |
| MCP tool reference | **Exists** — demote the 19-pill wall (`orch.py:104-113`) into a collapsed `code` or move it to Fleet; it is reference documentation and should not occupy the default tab's top-right quadrant. |

**Done when** — the note is true.

---

## 4. BE work package

Everything the ten journeys need, ordered by whether it is wiring or building.

### 4.1 New CLI verbs (5)

| Verb | Wraps | Serves | Size |
|---|---|---|---|
| `board start <kickoff.md> [--recipe R] [--workspace M] [--set K=V] [--dry-run]` | `web.control.launch_run` (`web/control.py:626`) | J1, J2, J3 | S |
| `board answer <run_id> --text T` | the profile-answer writer (`web/control.py:~390-415`) | J5 | S |
| `board race-keep <run_id> [--keep-winner]` | `workspaces.merge` + `workspaces.discard` | J6 | M — needs honest partial-failure reporting |
| `kickoff new <text>` | new | J3 | S |
| *(no verb)* — per-source sync reuses `intake once --source S` | — | J1, J2, J4 | — |

### 4.2 Engine payload work (no new verbs, no renderer change)

| Change | Serves | Notes |
|---|---|---|
| Render `errors` as a `callout` at the end of `build()` | J4 + every page | Fixes the invisible-reaper defect engine-side |
| `_chain_health(home)` → `checks` section | J4 | Sources × trigger × token × scheduler |
| `_attention(home)` → `triage` section, prepended to every tab | J5, J6, J4 | The urgency strip |
| Kickoff rows: emit `fix` on the first finding, plus `log_path` | J1, J3 | `kickoff_lint` already returns `fix` |
| Arrivals: join to run | J2 | Needs the `run_id` column — see 4.3 |
| Thread defaults → three `chips` groups + provenance labels | J7 | Options come from `orchestrator_lanes()` / `list_recipes()` |
| History thread rows → `S.page_link("thread", …)` | J10 | Resolve the arg `thread.py` keys on |
| Fleet rows → `acts` for send/inbox/stop/admit; principal → run | J8 | All verbs exist |
| Races → per-contestant verdicts + keep/discard buttons | J6 | Verdict readers exist in `verify.py` |
| Classifications → scan + picker + detail | J9 | Share the recent-run scan with J5 |

### 4.3 Schema (1 migration)

`intake_events` gains `run_id TEXT` (J2). Same store, same `threads/store.py`, additive. Existing rows keep `NULL`, which the UI must render as "—", not as an error.

### 4.4 Renderer changes — **out of scope, listed for costing**

The design deliberately avoids these. If the owner wants any of them, they affect all 13 pages, not this one:

| Change | Would buy | Serves |
|---|---|---|
| A `select`/`dropdown` section type | Real pickers instead of chips + reload | J1, J7, J9 |
| Rendering the page-level `errors` map | Defence in depth for 4.2 | all pages |
| Keyboard-navigable tab strips | Accessibility baseline | all pages |
| Row expansion inside `list` | Inline lint detail per kickoff | J1, J3 |

---

## 5. Sequencing

1. **Wiring pass** — no new verbs, no schema: kickoff `fix` + `log_path`, thread-default chips with provenance, history `do`, fleet `acts`, `errors` → callout, `_chain_health`. *This alone moves six of ten journeys from "read-only" to "partly actionable" and is the cheapest possible improvement to the page.*
2. **`board start` + `kickoff new`** — unlocks J1 and J3, the two highest-frequency journeys. Requires the Dispatch tab to exist, so it is also the tab restructure.
3. **`board answer` + the recent-run scan** — unlocks J5 and J9, which share the scan.
4. **`board race-keep` + the `run_id` column** — unlocks J6 and J2.
5. **Attention strip last**, once its four inputs exist.

---

## 6. Out of scope

- The orchestrator thread's chat panel, model picker and steering (Zed's agent panel, not this page).
- Other pages. Cross-page moves — promoting blocked runs to the `inbox` page — are a product call.
- New section types in the shared renderer (see 4.4).
- Any change to how recipes or lanes are *defined*; this design only changes how they are chosen.

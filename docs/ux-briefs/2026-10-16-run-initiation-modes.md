# Starting a run: offering every mode of mini-ork

**Date** 2026-10-16 · **Status** proposal for review · **Supersedes nothing** —
companion to `2026-10-15-orchestrator-intake.md` (what is wrong) and
`2026-10-15-orchestrator-intake-redesign.md` (the kickoff-centric redesign).

**The prompt this answers.** *"SDD is one mode of mini-ork, alongside a
per-feature loop and a doc-to-features loop. It is not 'the whole product'; it
is the part that is directly on BoWatt's problem. Find the user journeys for
initiating a mini-ork run in a way that offers all modes in a UI/UX-friendly and
comprehensive way through the UI."*

**The one-line answer.** Today the UI offers *recipes by name* and *kickoffs by
path*, never *modes by input*. Every mode is reachable only by a user who
already knows its recipe id. The fix is not a mode picker — it is to ask what
the user is **holding**, infer the mode from that, and show the inference on a
pre-flight sheet before anything starts. Seven inputs, one pre-flight, one
Start button.

### Decisions taken (2026-10-16)

Two forks were settled before this brief was accepted; the body below is
written to them.

| Decision | Taken | Consequence |
|---|---|---|
| **Unsure inference** | **Ask, and gate Start.** §10 Q2 | When no rule matches, the sheet says so and **Start is disabled** until a mode is chosen. A run costs money and writes a worktree; that is not a default. J1–J3, J8 unaffected (their inference always matches). |
| **Catalogue size** | **Curated families, "more…" for the rest.** §10 Q1 | Two chip levels — family, then that family's recipes — plus a "more…" that opens the existing Recipes board. Never 44 items at the moment of commitment. |

These resolve §10 Q1 and Q2. Q3 and Q4 remain open, and Q3 is now largely
answered by the first row: a gated sheet *is* the "gate, not just inform"
answer.

---

## 0. Method

Every claim below carries a `file:line` from one of three trees, read on
2026-10-16:

- **engine** — `/Volumes/docker-ssd/ps/mini-ork-worktrees/orch-per-run`
- **IDE** — `/Volumes/docker-ssd/ps/zed-mini-ork/crates/mini_ork_ui`
- **crate repo** — `SourceShift/mini-ork-ui` (same code, submodule pin)

Two things this brief deliberately does **not** do: it does not propose a new
Rust widget (the crate's primitives are sufficient — §7), and it does not
propose a new run engine. Every journey below ends in a verb the engine already
has, or in one small verb it is missing (§8, B1).

---

## 1. Ground truth: "mode" is not a thing the code knows

This matters before any design, because the first instinct — *add a mode
enum, expose a mode picker* — is not available.

**There is no `mode` concept for modes of work.** There is no `MODES` tuple, no
mode registry, no mode column. The word "mode" appears in the codebase meaning
other things entirely (`dry-run`/`live` run mode, dispatch modes, search modes,
gate modes), which is why searching for it does not find the three modes the
user means.

**The three modes are three recipes.** A recipe is a directory under
`recipes/<id>/` containing both `workflow.yaml` and `task_class.yaml` — that
pair is the whole predicate (`mini_ork/recipes_catalog.py:94-107`, enumerated by
`list_recipes` at `:174`).

| The user's name | Recipe id (= directory) | `task_class.name` |
|---|---|---|
| SDD | `spec-driven-delivery` | `spec_driven_delivery` |
| per-feature loop | `recursive-validate-impl` | `recursive_validate_impl` |
| doc-to-features loop | `doc-to-features-loop` | `doc_to_features_loop` |

The names are cross-walked in the recipes' own READMEs, and most explicitly in
`recipes/spec-driven-delivery/README.md:11-14`:

> "Use it when the input is a spec directory (`## Spec dir:` in the kickoff) …
> **For a single broad document, use `doc-to-features-loop`. For a single clear
> task, use `recursive-validate-impl` directly.**"

`docs/plans/2026-10-03-spec-driven-delivery.md:10-11` is the only design doc
that uses the word "mode" for these, and it states the gap the same way:

> "mini-ork can turn ONE markdown doc into a feature queue
> (`doc-to-features-loop`) and close-loop a single feature
> (`recursive-validate-impl`), but it has no mode that takes a DIRECTORY of .md
> spec files."

**There are not three. There are 44.** `recipes/` holds 44 directories. The
three named modes are simply the three the user has in mind, and the ones on
BoWatt's problem. Their long-horizon siblings — `goal-loop`, `epic-runner`,
`bdd-first-delivery`, `post-mvp-delivery`, `prompt-graph-loop`,
`recursive-self-improve`, `research-synthesis`, `frontier-llm-research` — are
the same kind of object. The set is open: it is data, not a schema, and a new
recipe appears by creating a directory.

**Do not be misled by `sdd-deliver`.** `docs/plans/2026-10-06-sdd-methodology-speckit.md:258`
proposes a recipe of that name and a `mini-ork run sdd-deliver` command. It does
not exist. It is a design proposal.

**What the user says is a mode, the code calls a recipe. So "offer all modes"
means "offer all recipes without making the user learn 44 names."** That is a
naming and framing problem, not a plumbing problem.

---

## 2. How a run starts today, and where the modes go missing

### 2.1 One verb, one required argument

Every run begins at `mini-ork run [<recipe>] <kickoff.md>` (`mini_ork/cli/main.py:133-143`).
The arg split is `main.py:1391-1411`:

```python
first = rest.pop(0)
if os.path.isfile(first):        # first arg is a kickoff file
    kickoff = first
    ... probed_class = _grep_kv(probe.stdout, "task_class")
        recipe = resolve_recipe(root, probed_class)      # INFERENCE
else:                            # first arg is a recipe name
    recipe = first
    kickoff = rest.pop(0)                                # EXPLICIT CHOICE
```

Both paths exist in one line of code. And **a kickoff is always required** —
there is no mode that takes nothing.

### 2.2 Inference is a keyword router the user cannot see

When the recipe is omitted, `mini_ork/cli/classify.py` scores every candidate
`task_class.yaml` against the kickoff text: keyword word-boundary hits count 1,
regex matches `+2`, class-name aliases `+3` (`classify.py:43-84`); highest score
wins, ties lex-first, and the fallback is `"generic"` (`classify.py:168-176`).
Candidates come from `config/task_classes/*.yaml` **plus** `recipes/*/task_class.yaml`
(`classify.py:87-96`).

So the mode is inferred from markers like `"Spec dir:"`, `"Source doc:"`,
`"Definition of Done"` — strings most users have never seen. This is a
reasonable engine design. It is a terrible *interface*, because the user gets
neither the question nor the answer.

### 2.3 In the IDE, the modes are effectively unreachable

Every IDE entry point is one button → one fixed verb. The two that start a run
from a kickoff are:

- `ide_pages/orch.py:224` — Kickoffs list, **Start run** →
  `S.thread(f"Start a run from the kickoff {path}")`. A thread prompt, no recipe.
- `ide_pages/recipes.py:161` — recipe detail, **Test** →
  `S.thread(f"/run {sel_id} {kickoff_path}")`. Recipe pinned *by id*.

And the recipe's default on the Kickoffs path is spelled out at
`ide_pages/orch.py:206`:

```python
recipe = declared or os.environ.get("MO_ACP_RECIPE") or "code-fix"
```

**This is the defect the user is pointing at.** A user who writes
`sdd.md`, drops it in `.mini-ork/kickoffs/`, and clicks **Start run** gets a
**`code-fix`** run — unless the file carries `recipe:` front-matter they have
never been told about. The classifier never runs on this path. Nothing warns.
The run starts. SDD is unreachable from the Kickoffs board by anyone who does
not already know its id.

### 2.4 The one place modes are shown is a name list

`ide_pages/recipes.py:184-231` groups recipes into task_class **families**
(`Code`, `Research`, …) as columns, one card per recipe — a real board, added
at IDE spec level 2 (`recipes.py:169-174`). It is the closest thing to a mode
picker that exists. But it is reached from a *Recipes* activity, it is titled
"Recipes", and each card asks "Run with this" → `/run <recipe_id> ` — **recipe
first, task second** (`recipes.py:89`). The user must pick the tool before
they have said what the job is.

### 2.5 Modes are never a thread-level choice, by design

`_build_config_options` returns exactly one option — the model
(`acp/agent.py:1300-1312`). Recipe and workspace pickers exist
(`_build_recipe_config_option`, `agent.py:1370-1403`; workspace at `agent.py:1405+`)
but are **deliberately not surfaced**, and the page says so in plain words
(`ide_pages/orch.py:98-101`):

> "The thread is a control plane — the orchestrator decides per request whether
> to answer, start a run, or edit directly. **Model is the only picker in the
> thread; Recipe and Workspace are the defaults the orchestrator proposes.**"

That is a defensible stance for *the thread*. It is not a stance about
*initiation*: the thread is where a run gets **proposed**, and this brief is
about the surface where a run gets **started**, which needs the recipe to be
visible precisely because it is about to be committed to.

### 2.6 What the user must currently know, per mode

| Mode | To start it today the user must know | Where they could learn it |
|---|---|---|
| SDD | the id `spec-driven-delivery`, **and** to put `## Spec dir:` in the kickoff | `recipes/spec-driven-delivery/README.md:104` |
| doc-to-features | the id `doc-to-features-loop`, **and** the `Source doc:` marker | the recipe README |
| per-feature loop | the id `recursive-validate-impl`, **and** to write machine-checkable success commands | the recipe README |
| any other of the 41 | its id | `README.md:305` lists **seven** of 44 |

`README.md:305` is the best user-facing summary that exists — a seven-row
need→recipe table. Seven rows for forty-four recipes, and it lives in a README,
not the UI.

---

## 3. The reframe: input-first, not recipe-first

The existing intake redesign is **kickoff-first**: it asks the user for a
kickoff, then a recipe, then starts. That is the correct shape *for the audience
that already writes kickoffs*. It cannot serve a user who holds a folder of
specs and has never heard the word.

Invert it. The user is not choosing a mode; the user is choosing **what to feed
it**. The mode is a consequence.

| What the user is holding | Mode | Recipe |
|---|---|---|
| A directory of `.md` spec files | SDD | `spec-driven-delivery` |
| One broad document (PRD, RFC, brief, wiki dump) | doc-to-features | `doc-to-features-loop` |
| One feature, with a way to check it | per-feature loop | `recursive-validate-impl` |
| A bug report, a stack trace, a red test | fix | `code-fix` |
| A goal with no end date | long-horizon | `goal-loop` |
| A roadmap of epics | epic delivery | `epic-runner` |
| A kickoff file they already wrote | *as declared* | front-matter `recipe:`, else classify |
| Nothing yet — just an intention | conversational | orchestrator drafts via `/kickoff`, then the same pre-flight |

Three properties make this the right spine:

1. **It matches the engine's own dispatch.** These are exactly the inputs each
   recipe's README names, and exactly the markers its `task_class.yaml`
   matches. The inference ladder is already implemented in `classify.py`; the
   UI is what is missing.
2. **It is honest about the missing ingredient.** Two of the three named modes
   need a *marker in the kickoff*, not just an input path. So an input-first
   intake must also **draft the kickoff** — the artifact becomes a consequence
   of the input, not a prerequisite for the user. (§8, B4.)
3. **It degrades to today.** A user who already has a kickoff gets the same
   page with the source chip pre-selected to *Kickoff*.

### The rule the design follows

> **Infer, then show, then let them override — never ask a question whose
> answer the system can already derive.**

Precedent, already in-tree and already the product's own voice: the Recipes
board infers a family from the recipe's own keywords (`_RECIPE_FAMILIES`,
`recipes.py:184-190`). Mode inference is the same move, one level up.

---

## 4. The journeys

Each journey: **trigger → holds → today → the walk → surfaces → missing**. All
seven converge on one object (§5).

### J1 — "I have a folder of specs" (SDD)

- **Trigger** the user has a `specs/` directory; this is BoWatt's shape.
- **Today** unreachable from the UI. `S.pick_files` is `files: true,
  directories: false` — hardcoded (`store.rs:623-628`). **There is no directory
  picker anywhere in the IDE.** The only route is the thread prompt at
  `recipes.py:267`: *"Ingest directory"* → `/kickoff ingest the specs directory
  with mini-ork specs ingest ` — which hands the user an empty sentence to
  finish by hand.
- **The walk**
  1. Dispatch page → chip **Spec folder**.
  2. **Pick folder…** → native directory picker.
  3. Engine runs `mini-ork specs ingest <dir>` (a deterministic pre-step, no
     model — `cli/specs.py`), then drafts a kickoff whose body carries
     `## Spec dir: <dir>`.
  4. The page re-fetches and renders `specs/` — the existing spec table
     (`recipes.py:242-269`): one row per spec card, with status and dep count.
  5. Pre-flight: *"will run as **spec-driven-delivery** — matched `Spec dir:`
     · 14 specs, 11 ready, 2 blocked, 1 draft"*.
  6. **Start run.**
- **Missing** a directory picker (F1); a kickoff drafter (B4); `specs ingest`
  reachable as a `cli` action rather than a sentence; the pre-flight (B2).

### J2 — "I have one big document" (doc-to-features loop)

- **Trigger** a PRD, an RFC, a pasted wiki page, a 40-page brief.
- **Today** `S.pick_files` *can* pick it — but nothing in the IDE offers a
  "turn this document into features" entry. `pick_files` has exactly one
  caller, `threads attach` (`ide_pages/thread.py:274`), which starts nothing.
- **The walk**
  1. Chip **Document**.
  2. **Pick file…** → the existing multi-file picker, single selection.
  3. Engine drafts a kickoff with `Source doc: <path>` and the document's
     headings quoted as the feature seed.
  4. Pre-flight: *"will run as **doc-to-features-loop** — matched `Source doc:`
     · will emit N features, then close-loop each"*, plus the recipe's own
     `flow` preview (`S.flow`, `recipes.py:93-107`).
  5. **Start run.**
- **Missing** the entry point; the drafter; the pre-flight.

### J3 — "I have one feature with a way to check it" (per-feature loop)

- **Trigger** "add retry to the dispatcher, and `pytest tests/unit/test_retry.py`
  must pass."
- **Today** reachable — this is what `code-fix`/`/run` degenerates to — but by
  accident, and the distinctive thing about the mode (its `Definition of Done`
  probes) is not mentioned anywhere in the intake.
- **The walk**
  1. Chip **Feature**.
  2. A **`composer`** (not a picker): *"describe the feature and how to check
     it"* → the typed line becomes the kickoff body.
  3. Pre-flight shows the mode, and **quotes back the check it extracted** — if
     it extracted none, it says so, because a per-feature-loop run with no
     check can only ever report vacuous success (the Recipes board already
     carries exactly this warning at `recipes.py:114`).
  4. **Start run.**
- **Missing** the composer entry point; probe extraction feedback.

### J4 — "I have a bug"

- **Trigger** a stack trace, a red test, a ticket.
- **Today** this is the only journey that works — because `code-fix` is the
  silent default (`orch.py:206`). The mode is right; the *feedback* is wrong:
  nothing tells the user which mode they got, so they cannot tell a good
  default from a silent guess.
- **The walk** chip **Bug** → `composer` or a pasted log via `pick_files` →
  pre-flight *"will run as **code-fix** — the default"* → Start.
- **Missing** only the pre-flight. J4 is the acceptance test that the new page
  did not break the one path that already worked.

### J5 — "I have a goal" (goal-loop)

- **Trigger** "keep improving the flaky test suite until it is green for a
  week." No end date, no single artifact.
- **Today** no surface at all; `recipes/goal-loop/` has **no README**.
- **The walk** chip **Goal** → `composer` → pre-flight shows the loop's stop
  condition *as the engine will interpret it*, and the cost band, because a
  long-horizon loop is the one mode where the user must understand the exit
  before starting.
- **Missing** everything but the recipe.

### J6 — "I have a roadmap" (epic delivery)

- **Trigger** a roadmap file, or a ticket queue.
- **Today** partially present, as thread prompts only: **Ingest roadmap** and
  **Split into kickoffs** (`recipes.py:315-317`), plus **Sync tickets**
  (`kanban.py:784`, `--dry-run`) and **Poll now** (`orch.py:780`).
- **The walk** chip **Roadmap** → pick the file → `epics ingest` → the epic
  table (`recipes.py:283-317`) renders inline → pre-flight names
  `epic-runner` and shows how many epics are dispatchable *now* versus waiting
  on dependencies.
- **Missing** the chip; the pre-flight; and the honest distinction the UI
  currently blurs — `epics ingest` **queues**, it does not start
  (`commands.py:980`, `kanban.py:781-783`). The page should say "queued, the
  scheduler will dispatch" and offer **Dispatch now** (`task.py:419-421`) as a
  separate, confirmed button.

### J7 — "I have nothing yet"

- **Trigger** an intention.
- **Today** correct and already good: any plain prompt opens an orchestrator
  thread (`agent.py:1862-1867`), and `/kickoff` drafts a kickoff with the
  orchestrator, checks it, then starts (`commands.py:939-974`).
- **The walk** the Dispatch page keeps a **"Not sure yet"** affordance →
  `S.thread("/kickoff ")` → the conversation happens in the Agent panel → the
  orchestrator proposes → **the same pre-flight sheet renders in the page**,
  so the final commit to a mode is visible and overridable in the same place as
  J1–J6.
- **Missing** only the handoff back into the page. Today the conversation is a
  one-way door: the orchestrator can call `start_run` and the run is simply
  gone from the page's point of view.

### J8 — "I already have a kickoff"

- **Trigger** a written kickoff.
- **Today** `orch.py:224` — works, but silently defaults to `code-fix` (§2.3).
- **The walk** chip **Kickoff** (default selected when the page has kickoffs) →
  the existing kickoff list → select → pre-flight shows *which* recipe the
  kickoff implies and *why* (front-matter, or the classifier's top match), with
  a **Change mode** chips row → Start.
- **Missing** the pre-flight's recipe resolution. **This is the single
  highest-value fix in the brief**: it converts the one silent wrong mode into a
  visible one.

### The cross-cutting object: the pre-flight sheet

Every journey above ends in the same shape. This is the deliverable the user's
question is really asking for — the thing that makes "all modes offered" true
without a mode picker:

**Matched — the common case:**

```
┌ Will run as ────────────────────────────────────────────────────────┐
│  spec-driven-delivery          family: Code · source: engine       │
│  matched  ## Spec dir:  in the kickoff                             │
│  [ Change mode ▾ ]  ← chips: Code · Research · Product · Ops · …   │
├ Input ─────────────────────────────────────────────────────────────┤
│  specs/ · 14 specs — 11 ready, 2 blocked, 1 draft                  │
├ Flow ──────────────────────────────────────────────────────────────┤
│  ingest → per_spec_dispatcher → implementer → verifier → …         │
├ Run ───────────────────────────────────────────────────────────────┤
│  workspace: worktree   lane: (orchestrator default)   ~$0.40–1.20  │
│  [ Start run ]   [ Dry run ]   [ Edit kickoff ]                    │
└────────────────────────────────────────────────────────────────────┘
```

**Unmatched — Start is gated (decision of 2026-10-16):**

```
┌ Will pick a mode ───────────────────────────────────────────────────┐
│  no rule matched  ·  classifier's best guess was generic (score 0) │
│  this run will not start until you choose one                      │
│  [ Change mode ▾ ]  ← chips: Code · Research · Product · Ops · …   │
├ Input ─────────────────────────────────────────────────────────────┤
│  kickoff.md · lint clean                                           │
├ Flow ──────────────────────────────────────────────────────────────┤
│  (shown once a mode is chosen — a flow preview is a mode preview)  │
├ Run ───────────────────────────────────────────────────────────────┤
│  workspace: worktree   lane: (orchestrator default)   — cost band  │
│  [ Start run ] ← DISABLED   [ Dry run ]   [ Edit kickoff ]         │
└────────────────────────────────────────────────────────────────────┘
```

Five properties make this the right object:

- **It is the last screen before an irreversible spend.** A run costs money and
  writes to a worktree. The recipe, the workspace and the lane are exactly the
  three things that are about to be committed to, and all three are currently
  invisible at the moment of commitment.
- **It turns inference into consent.** "Matched `Spec dir:`" is the difference
  between a system that guessed and a system that told you it guessed. And when
  nothing matched, the cheap `Start` is *withheld* rather than spent — the
  asymmetry is deliberate: the sheet costs a click, a wrong run costs money and
  a worktree.
- **The gate only ever fires on a miss.** J1–J3 and J8 all match a marker by
  construction (§3), so the seven journeys a new user walks are never gated.
  Only a hand-written kickoff with no marker — today's silent `code-fix` case —
  is stopped, which is precisely the case that should be.
- **It is the same for all seven journeys**, so there is one screen to build,
  not seven.
- **It costs almost nothing to build**, because every field already exists:
  recipe + family (`recipes.py:184`), lint (`orch.py:207`),
  `S.flow` (`recipes.py:93-107`), workspace mode (`orch.py:93`), lane
  (`orch.py:87`), family colour (`recipes.py:69`).

---

## 5. How a "mode picker" is built with no dropdown

The binding constraint, inherited from the 10-15 brief §0 and re-verified:
**the IDE has no dropdown, no modal form, and no inline form.** The input
surface is `composer` — one line of text that becomes the last argv element of
a CLI call (`ide_pages/spec.py:581-598`; rendered `sections.rs:474-562`) — plus
`chips`, whose `do` is almost always `set_args`.

So all three selection problems are solved with the primitives that exist:

| Problem | Primitive | Why it works |
|---|---|---|
| Pick the **source shape** | `S.chips(..., do=S.set_args(source="spec-dir"))` (`spec.py:348-351`, `spec.py:127-128`) | A chip re-fetches the page with new args — a reload-driven picker, which is the crate's native idiom |
| Pick the **file or folder** | `S.pick_files(...)` → `store.rs:608-646` | Native OS picker; needs a `directories` flag (§8, F1) |
| Pick the **mode** | chips over families, then chips over that family's recipes, both `set_args(recipe=…)` | Two levels, ~8 + ≤10 items; never 44 at once |
| Describe the **work** | `S.composer("describe the feature…", ["board", "start", "--stdin"])` | Typed line becomes the kickoff body |
| **Override anything** | the same chips, one level down | Nothing is hidden behind a modal |
| **Not sure yet** | `S.thread("/kickoff ")` (`spec.py:193-194`) | Falls through to the orchestrator, which is already built (§2.5, J7) |

**Curate the list; do not enumerate 44** (decided 2026-10-16). The families are
the first chip level (`_RECIPE_FAMILIES`, `recipes.py:184-190` — `Code`,
`Research`, `Product`, …), the recipes in the matching family are the second,
and a **"more…"** chip opens the existing Recipes board for anything outside the
curated set. Offering 44 recipes at the moment of initiation is the same failure
as offering one, in the other direction: it moves the decision to the user at
the worst possible time.

`★ Insight ─────────────────────────────────────`
The reload-driven picker is not a workaround for a missing dropdown — it is a
*different interaction model*, and it fits this problem better. A dropdown
hides the state behind a transient popover; `chips + set_args` re-renders the
whole page with the new selection, so the pre-flight sheet below it updates live
as the user tries modes. Trying "spec-driven-delivery" and then "doc-to-features"
is a two-click comparison of the resulting flow, cost band and lint, with no
commitment. That is a better mode-chooser than a `<select>`, which would show
only a name.
`─────────────────────────────────────────────────`

---

## 6. Page architecture

One new tab, or — preferred — the existing Intake page (`PAGES`, `ide_pages/__init__.py:18-39`)
re-cut into four sections top to bottom. Everything below is page JSON; no Rust
renderer changes.

1. **What are you starting from?** — a `chips` row of the seven shapes from §3,
   plus **Not sure yet**. `do: S.set_args(source=…)`. Exactly one `on`.
2. **Pick it** — differs by chip and nothing else differs by chip:
   - *Spec folder / Roadmap* → `pick_files(..., directories=True)` once F1 lands
   - *Document / Bug / Kickoff* → `pick_files(...)` or the existing kickoff list
   - *Feature / Goal* → `composer`
   - Nothing picked yet → a `pills` row of recent candidates (the kickoffs list,
     the spec indexes) so a repeat user clicks instead of picking
3. **Will run as** — the pre-flight sheet of §4. Recipe, family, the matched
   marker, the flow preview, the input summary. Plus **Change mode**, two
   levels of chips (family → recipe) and **more…**.
4. **Run** — workspace mode, lane, cost band, and the primary action.
   **Start is disabled when the mode is unchosen** (i.e. classification
   returned `generic`/nothing and no chip has been clicked). `btn(label, do=None)`
   already draws a disabled button (`spec.py:204`), so the gate is
   `do=None` versus `S.cli(...)` — no new mechanism.

The primary action is `S.thread(f"Start a run from the kickoff {path} as {recipe}")`
until `board start` exists (§8, B1), then `S.cli("board", "start", path, "--recipe", recipe)`.

**The page is stateless between starts.** Every selection is a `set_args` arg,
so the whole sheet is reconstructible from the query args and a page reload
loses nothing.

---

## 7. What the crate needs: exactly one flag

| Need | Verdict |
|---|---|
| chips, composer, thread, btn, table, flow, kv, pills, list | **exist** — `spec.py:56-69` lists the ten wired action verbs; `store::perform` (`store.rs:476-563`) handles every one |
| **directory picker** | **missing** — `store.rs:623-628` hardcodes `files: true, directories: false` |

`S.pick_files` (`spec.py:161-168`) carries `cli`, `prompt`, `home` but no
directory flag. Add `directories: bool = False` to the builder, thread it into
the `pick_files` action dict, and pass it to `cx.prompt_for_paths` at
`store.rs:623-628`. That is the entire crate change, and it unblocks J1 — the
mode the user says is on BoWatt's problem, whose input is a *directory*.

Everything else this brief proposes is `mini_ork/ide_pages/*.py` JSON.

---

## 8. Backend work package

Ordered by value per line. None of these is large.

**B1 — `board start <kickoff.md> [--recipe R] [--workspace M] [--set K=V] [--dry-run]`.**
The missing verb; already named in the 10-15 brief §J1. It wraps
`mini_ork.web.control.launch_run(home, recipe, kickoff_markdown, run_id, extra_env)`
(`web/control.py:642-727`), which is what every non-IDE launcher already calls.
Today the IDE starts runs only by *prompting a thread*; a real verb makes the
Start button a `S.cli` action, makes `--dry-run` free, and gives `board` the one
verb (`board_cmd.py:1032-1039`) it conspicuously lacks.

**B2 — `board preflight <kickoff.md> [--recipe R] [--json]`.**
The pre-flight sheet's data source, and the highest-value item here. Emits:
inferred recipe + the matched marker + the classifier's top-3 with scores
(`classify.py:43-84` already computes them), kickoff lint findings
(`kickoff_lint.lint`, used at `orch.py:207`), spec counts when a `## Spec dir:`
is present (`cli/specs.py`), the recipe's node list (`recipes/<id>/workflow.yaml`),
and the resolved workspace mode and lane. Read-only, so the sheet can re-render
on every chip click without side effects.

**It must return an explicit `resolved: bool`** — the gate's only input. The
page derives Start's enabled state from that one field, so the rule lives in
B2, not in two places in the page.

**B3 — explain the classifier, don't just run it.** `classify.py:168-176` picks
the top score and silences the rest, falling back to `"generic"`. That fallback
is exactly `resolved: false`, and the sheet must render it as a *question*, not
a default: "no rule matched — pick a mode" is honest; silently running `generic`
is not. Note the fallback is a *code*, not a missing value: `classify.py:168`
initializes `task_class, best = "generic", 0` and only replaces it on
`hits > best`, so a zero-scoring candidate never wins. The tell is `best == 0`
— so `resolved` must be computed where the score is known, not inferred from
the recipe string.

**B4 — a kickoff drafter.** Given `source=spec-dir|doc|feature|bug` plus a path,
emit the kickoff markdown with the marker the target recipe's `task_class.yaml`
matches — `## Spec dir:` for SDD, `Source doc:` for doc-to-features. The
markers are already declared in `recipes/*/task_class.yaml` (`matches.regex`),
so the drafter and the classifier can be held to the same table and cannot
drift. This is what makes "input-first" true: the user brings a folder, not a
marker.

**B5 — `specs ingest` and `epics ingest` as page actions.** Both exist as CLI
verbs but are reachable only through hand-finished sentences
(`recipes.py:267`, `recipes.py:315`). They become `S.cli` calls behind the
picker.

**B6 — make a raw failure classify as `code-fix`.** Required *by* the gate, not
independent of it — see R1b. `code-fix/task_class.yaml` matches on exactly five
keywords (`fix`, `bug`, `patch`, `update`, `refactor`) and an empty `regex: []`,
and its `path_globs: ["**/*"]` is **inert**: `_score` (`classify.py:43-84`) reads
only `keywords` and `regex`, never `path_globs`. So a pasted stack trace scores
**0** against every candidate and lands on `generic`. Today that is invisible —
`code-fix` is the silent default (`orch.py:206`) and the run starts anyway.
Under the gate it becomes a dead button on the most common journey (J4).

**The inert key is declared in 11 recipes and read by nothing at all.**
`grep -rn "path_globs" mini_ork/` returns **zero hits** — not "unscored by the
classifier" but absent from the entire engine. Only a test fixture echoes the
schema (`tests/unit/test_ide_pages_skills.py:43`). And `code-fix`'s value is
`"**/*"`, which is legible as a deliberate catch-all: the author meant *this
recipe matches anything*. It never did. That intent is the useful artifact here
— `path_globs` either becomes real (score it in `_score`, and `code-fix` then
genuinely becomes the fallback, no gate needed for J4) or it is removed from the
11 recipes that carry a lie.

The minimal fix is data, not code: add regexes to `code-fix/task_class.yaml` so
a failure shape classifies — `Traceback`, `^\s*File "`, `AssertionError`,
`exit code [1-9]`. This also fixes the classifier for every CLI user, who today
gets the same silent `generic` for the same input. **Do B6 with B2**, or the gate
will (correctly, and uselessly) refuse to start bug runs.

---

## 9. Sequencing

1. **B2 `preflight`** + **B3** + **B6** — the sheet *and* the matcher it gates
   on. Do this first: it makes J8 (the silent `code-fix`) correct immediately,
   *before* any page reshuffle, and it is the object all seven journeys need.
   **B6 ships in this step or the gate is unsafe** (R1b).
2. **F1 `directories`** — one flag; unblocks J1, the BoWatt path.
3. **B4 drafter** + **B5** — makes the input chips real.
4. **The Dispatch page reshuffle** (§6) — pure page JSON, now that every field
   it renders exists.
5. **B1 `board start`** — replaces the thread-prompt start with a verb, and
   makes `--dry-run` a button.

Steps 1–2 are independently shippable and each fixes a defect on its own. Steps
3–5 are the re-cut.

**The gate lands last, inside step 4.** B2's `resolved` field is inert until the
page consumes it, so the classifier can be surfaced (step 1) and corrected
(step 1) while Start still behaves as today. Flipping the page's Start to gated
is a one-line change in step 4 — deliberately, so it is revertible on its own if
R1b bites in practice.

---

## 10. Questions this brief cannot settle

**Resolved 2026-10-16** (see the table at the top):

1. ~~Curated families, or all 44?~~ → **curated, with "more…"** (§5).
2. ~~What does the pre-flight do when the classifier is unsure?~~ → **ask, and
   gate Start** (§4, §6).
3. Consequently: **gate, not merely inform.** The sheet withholds a button
   rather than relying on the user to read a line of text before spending.

**Still open:**

4. **Where do the 41 non-mode recipes belong on this page?** With the curated
   decision taken, the answer is *behind "more…"* — but that is a routing
   answer, not a design one. Whether they get a chip row of their own, or are
   simply the Recipes board one click away, is unresolved.
5. **Does the gate ever fire on a *matched* but low-confidence result?** Today's
   rule is binary: matched → start enabled, unmatched → gated. `classify.py`
   produces a *score*, so a middle band exists ("matched `Definition of Done`,
   score 3, second best `code-fix` score 2"). Whether a near-tie should warn
   rather than start is a real question and deliberately not answered here.

---

## 11. Risks

- **R1 — the pre-flight becomes a wizard.** The whole premise is that the user
  answers *one* question (what are you holding) and the rest is derived. If the
  sheet grows required fields, J4 — the journey that already works — gets
  slower, and the design has failed. J4 is the acceptance test.
- **R1b — the gate turns a working journey into a dead button.** This is the
  sharpest risk in the brief and it is now measured, not hypothetical. The
  gate's rule is "no match → no Start", and `code-fix`'s matcher is five
  generic words with no regex and an inert `path_globs` (B6). A bug report
  pasted as a stack trace therefore matches *nothing* and is **gated**. Today
  that input starts a `code-fix` run because `code-fix` is the default
  (`orch.py:206`); after the change it starts nothing until the user clicks a
  chip. **B6 is not optional to the gate** — ship them together or J4
  regresses. The general form of the risk: every journey whose recipe is
  currently reached by *defaulting* rather than by *matching* becomes a gated
  journey the day the gate lands. Audit which those are before shipping.
- **R1c — the gate's own false negatives are silent.** A user who cannot start a
  run gets no error explaining why unless the sheet says "no rule matched". The
  disabled button *must* be accompanied by the reason, in place (`spec.py:204`
  draws `do=None` disabled but carries no message of its own).
- **R2 — inference the user cannot audit.** The classifier's markers are
  strings in `task_class.yaml`. If `preflight` does not show *which* marker
  matched, this is a slightly more visible version of today's silent default.
  B3 is not optional to B2.
- **R3 — drafter/classifier drift.** B4 emits markers; `classify.py` matches
  them. Held to the same `matches.regex` table they cannot drift; held to
  duplicated string literals they will. This is the one place the work package
  has a correctness trap.
- **R4 — `board start` is a new launch path.** `launch_run` is already the
  single launcher behind web, scheduler, intake and automation; `board start`
  must call it, not re-implement the spawn. If it grows its own spawn, the two
  will diverge on env, on run-id minting and on cost accounting.
- **R5 — the page's own state.** Tying every selection to `set_args` means a
  page re-fetch per click. That is the crate's designed idiom, but it is worth
  confirming that `preflight` is cheap enough (it runs the classifier and lints
  the kickoff; `orch.py:167-181` already caches `find_recipe` for exactly this
  reason) to sit behind every chip.

---

## 12. Out of scope

- **`sdd-deliver`** and the spekkit methodology — a design proposal, not a
  recipe (`2026-10-06-sdd-methodology-speckit.md:258`).
- **Per-run orchestrator narration** — shipped separately; the Dispatch page is
  the front door, the Orchestrator thread (`run.py` actions,
  `/orchestrator <run>`) is the mid-run surface.
- **The Recipes activity as a whole** — it is the expert view of the same data
  and keeps its current shape.
- **New Rust interaction shapes.** §7 is one flag.

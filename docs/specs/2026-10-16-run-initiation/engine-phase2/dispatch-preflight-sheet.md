# Dispatch pre-flight sheet — choose the mode before spending

Depends on: none

Companion brief: `docs/ux-briefs/2026-10-16-run-initiation-modes.md` (§3–§6, §8 B4/B5, §9).

The IDE offers *recipes by name* and *kickoffs by path*, never *modes by input*.
A kickoff started from the Kickoffs list is dispatched as `code-fix` unless the
file carries `recipe:` front-matter, and nothing says so: the classifier never
runs on that path, and the page prints `code-fix (default)` in a subtitle a
reader learns to skip. This spec adds the layer that asks what the user is
holding, resolves it to a mode, shows the resolved run's shape, and refuses to
start an unresolved one.

**Two external prerequisites, both outside this spec dir, neither expressible as
a `Depends on:` slug.**

1. **The pre-flight resolver.** This spec consumes `preflight.resolve` — the
   module function from the sibling `engine/board-preflight` spec. Until that
   lands there is no `resolved`, no `score`, no `spec_dir`, and no sheet.
2. **The classifier's failure coverage.** `engine/codefix-classifier-coverage`
   makes a pasted stack trace resolve to `code-fix`. **Ship them together or
   journey J4 regresses**: the gate turns a run that works today by *defaulting*
   into a dead button. The general form of the risk is that every journey whose
   recipe is reached by defaulting rather than by matching becomes a gated
   journey the day the gate lands.

A third prerequisite is a *scheduling* one, not a technical one: a peer session
owns the Dispatch page's tab composition and has uncommitted edits to
`ide_pages/orch.py`. This spec adds sections to that page and must not fight it
for the same lines. It lands after the peer's work is committed.

## Inputs

- `mini_ork/ide_pages/orch.py` — the Dispatch page builder, its `_kickoffs`
  section and the `_thread_defaults` key/value block.
- `mini_ork/ide_pages/spec.py` — the section builders and the action vocabulary.
  Only these are used, all of them existing: `S.chips` (`{"t", "on", "do"}`),
  `S.kv`, `S.btn` (whose `do=None` draws it disabled), `S.set_args`,
  `S.thread`, `S.pick_files`, `S.open_path`.
- `preflight.resolve(kickoff_path, *, recipe=None, home=None) -> dict` — the
  resolver, called **in process**. The page builder must not shell out to its own
  CLI; it polls, and a fork per redraw is not a design.
- Page args: the Dispatch page's own selection state, written by `S.set_args`
  and read back on the next render. Existing mechanism, no new channel.

## Outputs

Three sections added to the Dispatch page, in this order. Nothing existing is
removed or reworded.

1. **Source chips** — "What are you starting from?". One chip per source:
   Kickoff file, Spec directory, Roadmap, A failure, A draft. Exactly one chip is
   `on` (the page arg's value, defaulting to Kickoff file). Each chip's `do` is
   `S.set_args` naming the source. This is a reload-driven picker: choosing a
   source re-renders the page with that source's affordances and nothing else
   changes. For the Spec directory and Kickoff file sources the affordance is a
   `S.pick_files` action with `directories` set for the directory source and
   unset for the file source.

2. **Pre-flight sheet** — one `S.kv` block, built from a single `resolve` call
   for the currently selected kickoff. Rows, in this order: Mode; Why (the
   matched markers and the score, or `nothing matched` when the score is 0);
   Kickoff lint (error and warning counts, first message); Specs (ready, blocked
   and draft counts, with the spec-dir row absent when no `## Spec dir:` is
   named); Nodes (the resolved recipe's node count); Workspace; Lane. When no
   kickoff is selected the sheet is replaced by a one-line hint and no `resolve`
   call is made.

3. **Mode chooser and gated Start** — when `resolved` is true, the Start button
   is `S.btn("Start run", S.thread(...), "primary")` exactly as today, and the
   sheet's Mode row carries a "change" chip whose `do` selects a different
   recipe. When `resolved` is false, the sheet grows a chip row of the candidate
   recipes (curated families first, with `more…` for the remainder) whose `do` is
   `S.set_args` setting the recipe override, and the Start button becomes
   `S.btn("Start run", None, "primary")` — drawn disabled by the existing
   `do=None` rule. A note under the row states that no mode matched and the run
   is not started until one is chosen. **Both halves are one change**: a
   disabled Start with no way to resolve it is a dead end, so the gate lands with
   the chip row that opens it, in the same spec.

The gate is deliberately the last of the three and is a one-line change to the
existing button — so it is revertible on its own if it proves to bite in
practice, without unwinding the sheet.

## Errors

- `resolve` raising must not blank the page. The sheet renders a single row
  naming the failure and the page's remaining sections are built as normal; the
  Start button is disabled, because an unresolvable kickoff is not a resolved
  one.
- A selected kickoff path that no longer exists is treated as no selection: the
  hint replaces the sheet and the Start button is absent rather than disabled.
- An empty fetch of the recipe catalogue leaves the chooser's chip row out and
  the Start button disabled, rather than an empty chip row.

## Edge cases

- **A kickoff that names its own recipe.** Front-matter `recipe:` wins, as it
  does today. The sheet's Why row then says `declared`, not a score, so the user
  can tell a declared mode from a matched one.
- **A tie in the classifier.** `candidates` lists both. The sheet's Why row
  reports the winner and says how many others shared the top score — a tie the
  user cannot see is a mode chosen by lexicographic accident.
- **The sheet on every poll.** `resolve` reads a kickoff file, scores it against
  44 recipes and scans a spec dir. That is a small, bounded read, but it must run
  once per render and not once per row — the sheet is built from one call whose
  result is passed to every row.
- **No kickoff selected, source is a failure.** The user pasted a failure but has
  not saved it as a kickoff. The sheet is the hint, and the affordance to save it
  is a `S.thread` draft — the source chips select an intake shape, they do not
  invent a file.
- **The peer's tab composition.** These sections mount wherever the Dispatch page
  already mounts its sections. If the peer's committed page has moved them, this
  spec follows the new host rather than reintroducing the old layout.

## Acceptance criteria

- AC1: `python3 -m pytest -q tests/unit/test_ide_pages_preflight_sheet.py`
  reports 1 passed.
- AC2: the page JSON for a page with no kickoff selected contains a source-chips
  section whose items include the text `Kickoff file`, and contains no key/value
  row whose label is `Mode` — assert both on the built dict, and assert that no
  `resolve` call was made by patching it to raise.
- AC3: for a kickoff whose body contains `## Spec dir:` pointing at
  `tests/fixtures/sdd_e2e/specs`, the built page contains a key/value row labelled
  `Mode` whose value is `spec-driven-delivery`, and a button labelled `Start run`
  whose action is not `None`.
- AC4: for a kickoff containing only `hello`, the built page contains a button
  labelled `Start run` whose action is `None`, and a chip whose action selects a
  recipe override — assert the chip is present, so the disabled button is not a
  dead end.
- AC5: for that same kickoff, after a recipe override is set in the page args,
  the `Start run` button's action is not `None` and the `Mode` row names the
  override.
- AC6: patching `preflight.resolve` to raise `OSError` leaves every other
  section of the page intact and still yields a `Start run` button whose action
  is `None` — assert the page contains no key/value row labelled `Mode`.
- AC7 (precondition, passes before work starts):
  `python3 -m pytest -q tests/unit/test_ide_pages_orch.py` reports 1 passed.

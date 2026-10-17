# spec-dir kickoff precedence — a spec directory resolves to spec-driven-delivery

Depends on: board-preflight

Companion brief: `docs/ux-briefs/2026-10-16-run-initiation-modes.md` (§11 R1b, §8 B3).

A kickoff whose entire purpose is to point a run at a directory of specs does not
resolve to `spec_driven_delivery`. Measured on the two kickoffs written for this
work package, from the engine checkout:

- **engine kickoff** — `docs` 5, `code_fix` 4, `spec_driven_delivery` 4.
- **crate kickoff** — `docs` 5, `spec_driven_delivery` 4, `recursive_validate_impl` 3.

`mini-ork classify` therefore reports `task_class=docs` for both. The
pre-flight gate cannot be shipped over that: the gate would either refuse a
legitimate journey or, on the inference path, confidently route a spec-driven
delivery into the documentation recipe.

The mechanism is two markers over-firing, and neither is a missing keyword:

1. **The class-alias bonus fires on a path.** `_score` adds +3 when the
   candidate's own class name appears as a word. `docs` is a word in any path
   that contains `/docs/`, so a spec directory under `docs/` hands the
   documentation class a three-point head start on the strength of where the
   files happen to live.
2. **A marker meant for an artifact fires on a template.** The `spec-index`
   regex is intended to detect the ingest artifact; it matched the literal
   `${MINI_ORK_RUN_DIR}/spec-index.json` in a success-criteria template.

Add that to `readme` in the kickoff's hard rules (+1) and `docs/` (+1), and
`docs` reaches 5 while the class the kickoff exists for reaches 4.

## Inputs

- `mini_ork/cli/classify.py` — `_score` (keyword word-boundary hits, regex +2,
  class-name alias +3) and the `task_class, best = "generic", 0` fallback.
- The candidate set, as `_candidates` resolves it: `config/task_classes/*.yaml`
  plus the recipes half.
- The kickoff text of a run that would be started.

## Outputs

The classifier resolves a kickoff by what it asks for, not by where its files
live. Concretely, all four of these hold at once:

- A kickoff carrying a `## Spec dir:` heading naming an existing directory
  resolves to `spec_driven_delivery`.
- A kickoff that is *about* documentation — and names no spec directory —
  still resolves to `docs`.
- A kickoff with no marker of any class still resolves to `generic`.
- A kickoff that names a path containing `/docs/` but asks for something else
  is not pushed toward `docs` by the path alone.

The fix is not "add keywords to `spec_driven_delivery`". Adding a keyword moves
the threshold for this pair and leaves the mechanism — a path earning a class
alias — in place for every other class whose name is also a directory word.

`board preflight` reports the runner-up alongside the winner, so a four-versus-
five near-miss is visible to the user rather than silently decided by the
lex-first tie-break.

## Errors

- A kickoff that cannot be read is the caller's error, unchanged.
- A regular expression in a candidate file that fails to compile is skipped,
  unchanged; this spec must not make a malformed candidate file fatal.

## Edge cases

- **Two classes legitimately close.** A kickoff that both documents a change and
  names a spec dir should resolve by the intent its heading declares, and the
  sheet should show both so the user can override. A near-tie must never be
  resolved silently.
- **A genuine documentation kickoff that quotes a spec path.** Passing `docs/`
  in prose must not be enough. Whatever guard is chosen must not simply drop the
  `docs` class: `docs` remains a real class with real kickoffs.
- **The alias bonus elsewhere.** `docs` is the instance, not the category. Any
  guard must be stated in terms that apply to every class whose name can appear
  as a path segment (`docs`, `recipe`, `epics`), not special-cased to `docs`.
- **`path_globs`.** Still declared by this class and unread — the sibling
  `codefix-classifier-coverage` spec settles that. This spec must not depend on
  which way that goes.
- **Regression on the recipe half.** All 45 `recipes/*/task_class.yaml` are
  mirrored into `config/task_classes/`, and `_candidates` prefers
  `$HOME/recipes` without merging the repo's. On this checkout the mirror makes
  the difference invisible, but a change here must not make the recipe half
  unreachable when the home has no `recipes/` directory.

## Acceptance criteria

- AC1: `python3 -m pytest -q tests/unit/test_classify_precedence.py` reports 1
  passed.
- AC2: for a kickoff carrying `## Spec dir:` naming an existing directory and
  containing the word `README`, `mini-ork classify <kickoff> --dry-run` prints
  `task_class=spec_driven_delivery`.
- AC3: for that same kickoff placed under a directory whose absolute path
  contains `/docs/`, `mini-ork classify <kickoff> --dry-run` still prints
  `task_class=spec_driven_delivery`.
- AC4: for a documentation kickoff in `tests/fixtures/` that names no spec
  directory, `mini-ork classify <kickoff> --dry-run` prints `task_class=docs`.
- AC5: for a kickoff containing only `hello`, `mini-ork classify <kickoff>
  --dry-run` prints `task_class=generic`.
- AC6: `python3 -c "from mini_ork.cli import classify; print(classify._score)"`
  exits 0 — the scoring entry point keeps its name and signature.
- AC7 (precondition, passes before work starts): `python3 -m pytest -q
  tests/unit/test_mini_ork_classify_py.py` reports 0 failed — the classifier's
  existing suite is green on the untouched tree, so a failure after this change
  is attributable to it. (It named `tests/unit/test_classify.py`, which the
  sibling `codefix-classifier-coverage` spec introduces; no spec introduces it
  here, so as written the precondition could never pass.)

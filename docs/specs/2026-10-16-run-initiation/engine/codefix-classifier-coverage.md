# code-fix classifier coverage — a pasted failure resolves to code-fix

Depends on: none

Companion brief: `docs/ux-briefs/2026-10-16-run-initiation-modes.md` (§8 B6, §11 R1b).

`board preflight` refuses to start a run whose mode is unresolved. That gate is
only safe while every journey that *should* match a recipe still matches one.
Today a raw failure does not: `recipes/code-fix/task_class.yaml` scores on five
prose keywords (`fix`, `bug`, `patch`, `update`, `refactor`) and an empty
`regex` list, so a kickoff that is a pasted stack trace scores `0`, lands on
`generic`, and would be refused by the gate. This spec closes that hole before
the gate ships.

The trap underneath it: `path_globs` is declared by 11 recipes, including
`code-fix` with a catch-all `["**/*"]`, and nothing reads it —
`grep -rn "path_globs" mini_ork/` returns zero hits. A kickoff that names a file
does not match the recipe that declares that path. Either the field becomes real
or it stops being declared; leaving it half-alive is what makes a reviewer
believe a match that cannot happen.

## Inputs

- `mini_ork/cli/classify.py` — the scorer (`_score`) and the fallback at
  `task_class, best = "generic", 0`.
- `config/task_classes/*.yaml` and `recipes/*/task_class.yaml` — the candidate
  fixtures, each optionally carrying `keywords`, `regex`, and `path_globs`.
- A kickoff's text: the body of the markdown of the run that would be started.

## Outputs

Two changes, one behaviour and one schema:

1. **`code-fix` scores on failure shapes, not just failure words.** Its
   `task_class.yaml` gains regexes for the shapes a pasted terminal or CI log
   actually contains: a Python `Traceback`, a stack frame line
   (`^\s*File "`), an `AssertionError`, and a non-zero exit line
   (`exit code [1-9]`). No new scoring rule — these are entries in the `regex`
   list `_score` already reads.
2. **`path_globs` is either honoured or withdrawn.** The choice is recorded in
   code; see Edge cases. Honouring it means `_score` adds a small score when a
   kickoff names a path matching one of the candidate's globs. Withdrawing it
   means the 11 declarations are deleted and the field is documented as
   rejected.

`board preflight` on a kickoff whose body is a stack trace then reports
`recipe: "code-fix"` and `resolved: true`.

## Errors

- A malformed regex in a `task_class.yaml` must not crash classification. It is
  skipped, and the skip is logged; the remaining patterns still score. A recipe
  file that fails to parse is already skipped by the loader and stays skipped.
- A glob that is not valid for the matcher is skipped the same way, never raised.

## Edge cases

- **Prose that merely mentions a bug.** "fix the flaky test" already scores 1 on
  the `fix` keyword and needs no regex. The new patterns are additive; the
  existing word-boundary behaviour must not regress. A tie between `code-fix`
  and another candidate still resolves exactly as it does today.
- **A trace from a different language.** Go and Rust panics do not match the
  four Python-flavoured patterns. That is accepted: `code-fix` remains a
  plausible match for them via the `fix`/`patch` keywords, and an unmatched
  non-Python trace is a finding for a later spec, not a silent `generic`.
- **No failure text at all.** A kickoff of prose with no words from any
  candidate's list still scores `0` and stays `generic`. That is the gate's
  job, deliberately unchanged.
- **The `path_globs` decision.** Honouring the field is the larger change —
  it makes 11 recipes match on a dimension they never matched on, which is a
  scoring change for every existing kickoff that happens to name a path. The
  recommendation is to honour it for `code-fix` only, where the catch-all is
  already the declared intent, and to withdraw the other ten. Whatever is
  chosen, `grep -rn "path_globs" mini_ork/` must stop returning zero hits: the
  field is either read or gone.

## Acceptance criteria

- AC1: `python3 -m pytest -q tests/unit/test_classify.py` reports 1 passed.
- AC2: `python3 -m pytest -q tests/unit/test_classify_failure_shapes.py` reports
  1 passed, covering all four new patterns and the prose non-regression.
- AC3: for a kickoff containing a Python `Traceback (most recent call last):`
  and a frame line, `mini-ork classify <kickoff>` reports `code-fix`.
- AC4: for a kickoff containing `AssertionError: expected 3 got 4`,
  `mini-ork classify <kickoff>` reports `code-fix`.
- AC5: for a kickoff containing `fix the flaky test`, `mini-ork classify
  <kickoff>` still reports `code-fix` with the same score it reports today —
  capture the today-value first and assert equality.
- AC6: for a kickoff containing only `hello`, `mini-ork classify <kickoff>`
  reports `generic`.
- AC7: `grep -rn "path_globs" mini_ork/` returns at least one hit — the field is
  read by the classifier — or returns nothing *and* no
  `recipes/*/task_class.yaml` declares it.
- AC8 (precondition, passes before work starts): `python3 -m pytest -q
  tests/unit/test_mini_ork_classify_py.py` reports 0 failed — the classifier's
  existing suite is green on the untouched tree, so a failure after this change
  is attributable to it. (It named `tests/unit/test_classify.py`, which this spec
  introduces at AC1; a precondition cannot cite a file that does not exist yet,
  and citing AC1's own command made the two criteria the same assertion.)

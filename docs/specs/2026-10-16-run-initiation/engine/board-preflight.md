# board preflight — resolve a kickoff to a mode before spending

Depends on: none

Companion brief: `docs/ux-briefs/2026-10-16-run-initiation-modes.md` (§4, §8 B2/B3).

A kickoff started from the IDE's Kickoffs list runs as `code-fix` unless the file
carries `recipe:` front-matter (`ide_pages/orch.py`, `_kickoffs_section`). The
classifier never runs on that path, so the mode is both guessed and invisible.
`board preflight` is the read-only verb that makes the guess visible before any
budget is spent, and it is the gate's only input.

The resolution must live in an **importable module function**, with the CLI verb
as a thin wrapper over it. The page layer renders the pre-flight sheet on every
poll, and a page builder that shells out to its own CLI would fork a subprocess
per redraw. The page calls the function in-process; the verb exists so a human
and a test can call the same thing from a shell.

## Inputs

- `kickoff`: path to a kickoff markdown file. Required.
- `--recipe R`: force a recipe; skips classification and reports `resolved: true`.
- `--json`: emit one JSON object on stdout. Without it, print a short
  human-readable summary.
- `--home H`: the state directory, as on every other `board` subverb.

## Outputs

- A module function that returns the dict described below, with no printing, no
  exit calls and no argument parsing — so `ide_pages` can import it. Its
  signature is `resolve(kickoff_path, *, recipe=None, home=None) -> dict`, and
  `resolve` returning is the same value the verb serialises.
- `mini-ork board preflight` as a thin wrapper: parse argv, call `resolve`,
  print. Every field below comes from the function's return value unchanged.

One JSON object on stdout (with `--json`):

- `kickoff_path` — resolved absolute path.
- `task_class`, `recipe` — the classifier's winner, or the `--recipe` override.
- `resolved` — `true` when `recipe` was chosen by a scoring candidate or given by
  `--recipe`; `false` when classification fell through to `generic`.
- `score` — the winning score. `0` means nothing matched.
- `matched` — the markers that earned the score, as strings, e.g.
  `regex:Spec dir:` or `keyword:spec directory`. Empty when `score` is 0.
- `candidates` — every candidate with a non-zero score, highest first, each
  `{"recipe", "task_class", "score"}`.
- `lint` — `{"errors": N, "warnings": N, "findings": [...]}`, the existing
  `mini_ork.kickoff_lint.lint` result for this kickoff.
- `spec_dir` — when the kickoff names a `## Spec dir:`, a deterministic scan of
  it: `{"path", "exists", "specs", "ready", "blocked", "draft", "errors"}`;
  otherwise `null`.
- `nodes` — the recipe's node names from `recipes/<recipe>/workflow.yaml`.
- `workspace` — the resolved workspace mode (`worktree` or `in-place`).
- `lane` — the resolved orchestrator lane, the same value `orch._thread_defaults`
  reports.

Exit codes: `0` on a readable kickoff (whatever `resolved` says — an unresolved
kickoff is a finding, not a usage error); `2` on a missing kickoff or a spec-dir
that does not exist.

## Errors

- Missing or unreadable kickoff → exit 2, message on stderr, nothing on stdout.
- Unknown `--recipe` → exit 2 naming the value.
- A kickoff whose `## Spec dir:` does not exist → `spec_dir.exists` is `false`,
  `spec_dir.errors` carries the reason, exit stays 0.

## Edge cases

- **No scoring candidate.** All 44 recipes score 0. `recipe` is `"generic"`,
  `resolved` is `false`, `score` is `0`, `candidates` is empty. This is the case
  the IDE gate must refuse.
- **A tie.** Two candidates share the top score. The winner is the one
  `resolve_recipe` picks today (lex-first); `candidates` still lists both, so the
  near-tie is visible.
- **`--recipe` with no kickoff match.** `resolved` is `true` and `score` is the
  overridden recipe's own score, not `-1`.
- **An empty kickoff file.** Scores 0 everywhere; `resolved` is `false`.

## Acceptance criteria

- AC1: `python3 -m pytest -q tests/unit/test_board_preflight.py` reports 1 passed.
- AC2: for a kickoff whose body contains `## Spec dir:` and a path to
  `tests/fixtures/sdd_e2e/specs`, `mini-ork board preflight <kickoff> --json`
  exits 0 and its `recipe` field is `spec-driven-delivery` and its `resolved`
  field is `true`.
- AC3: for a kickoff containing the single line `hello`, the same command exits 0
  with `resolved` `false` and `score` `0`.
- AC4: the same kickoff with `--recipe recursive-validate-impl` exits 0 with
  `resolved` `true` and `recipe` `recursive-validate-impl`.
- AC5: for a kickoff naming a nonexistent `## Spec dir:`,
  `mini-ork board preflight <kickoff> --json` exits 0 with `spec_dir.exists`
  `false`.
- AC6: `mini-ork board preflight /nonexistent.md` exits 2.
- AC7: `mini-ork board preflight <kickoff>` without `--json` prints no `{` on the
  first line.
- AC8: `python3 -c "from mini_ork.preflight import resolve; print(type(resolve))"`
  prints `<class 'function'>` and imports no CLI module as a side effect.
- AC9 (precondition, passes before work starts):
  `python3 -m pytest -q tests/unit/test_board_cmd.py` reports 1 passed.

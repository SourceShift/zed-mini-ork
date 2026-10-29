# Orca parity

Orca (`stablyai/orca`, "the ADE for a fleet of parallel agents") is the product
the mini-ork IDE is measured against. This directory records what Orca can do,
where mini-ork already does it, and which units of that work mini-ork still
lacks in its UI/UX.

## Sources of truth

| Source | What it proves | Provenance |
|---|---|---|
| `matrix.md` | the verdict for every Orca capability | derived from the three inventories below |
| `orca-cli.md` | the CLI verbs Orca ships | `orca --help` from `/Applications/Orca.app/Contents/Resources/bin/orca`, 2026-10-29 |
| `sources/orca` (untracked checkout) | the semantics behind those verbs and the renderer views | stablyai/orca `76d1b7bf29`, v1.4.214 |
| running app | what a user actually sees | Orca v1.4.222, launched 2026-10-29 |
| `../user-journeys/capabilities.json` | what mini-ork can do | `script/mo-surface`, engine `8d792bcc` |

The mini-ork column of `matrix.md` is anchored on page actions
(`mini_ork/ide_pages/*.py`), CLI verbs (`mini_ork/cli/*.py`, checked against
`capabilities.json`) and, for rows the host editor supplies, the fork's
`crates/`. A row marked `missing` names what was searched, so absence is
distinguishable from an unchecked area.

Marketing copy in `README.md` upstream is not used as evidence: the same
engine-vs-docs discipline the journey corpus follows applies here.

## Status vocabulary

| Status | Meaning |
|---|---|
| `parity` | mini-ork exposes the feature and the user can reach it from the IDE |
| `partial` | the engine supports it, the IDE exposes part of it, or the semantics differ |
| `cli-only` | the engine supports it, no page reaches it |
| `missing` | neither engine nor IDE has it |
| `not-applicable` | Orca's version exists for a platform mini-ork does not target (mobile app, voice) |

## Open question this document answers

For every Orca capability: is the gap an engine gap, a UI gap, or a deliberate
non-goal? A `cli-only` verdict is the cheap kind (a page action and a section),
a `missing` verdict needs a design decision, and `not-applicable` needs one
written sentence of justification so it is not re-litigated.

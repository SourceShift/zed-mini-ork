> [!IMPORTANT]
> Remove this line to confirm you've reviewed this PR before submitting.

# mini-ork IDE

**mini-ork IDE is [Zed](https://zed.dev) with the [mini-ork](https://github.com/SourceShift/mini-ork)
task operating system built in.** It is a desktop IDE for running AI task loops
you can see: every run is a board card, a graph of nodes, a transcript and a
verdict — not a terminal you have to babysit.

## What it adds to Zed

- **Threads board** in the workspace sidebar — runs grouped by state, with what
  needs you, today's spend against the cap, and the scheduler.
- **Pages as tabs** — Orchestrator, Runs, Changes, Verify, Recipes, Automations,
  Lanes & cost, Learning, Context, Nodes, Features, Setup, and one tab per run
  (DAG, Overview, Agents, Learnings, Artifacts).
- **A run graph** drawn from the recipe's real edges — typed, directed, with
  retries and escalations drawn as back-edges.
- **The mini-ork thread** — Zed's agent panel running the `mini-ork` agent, so
  you can ask for work and steer it from the editor.
- **Title and status bars** with the open project, spend, and the health of the
  services mini-ork depends on.

mini-ork stays the source of truth and a separate program: the IDE polls
`mini-ork board --json` and every button runs a mini-ork command. Pages are
data, so most page changes need no rebuild.

## Quick start

```bash
git clone --recurse-submodules git@github.com:SourceShift/zed-mini-ork.git
cd zed-mini-ork
script/mini-ork-build                    # first build is long
open -a "mini-ork IDE" ~/path/to/project
# or: script/mini-ork-ide ~/path/to/project
```

The crate that draws the shell is the `crates/mini_ork_ui` submodule; a clone
without `--recurse-submodules` does not build until
`git submodule update --init --recursive` has run. `script/mini-ork-build` and
`script/mini-ork-check` materialise it for you.

The app installs itself as **mini-ork IDE** (`dev.mini-ork.ide`) and runs on the
dev channel, so it sits next to an installed Zed without sharing its state. It
shares `~/.config/zed` for settings, keymaps and agent servers.

## Branches

- **`mini-ork`** — the branch users build, and the only one that receives fork
  changes.
- **`main`** — a byte-identical mirror of [zed-industries/zed](https://github.com/zed-industries/zed),
  for diffing against upstream.

`origin` is upstream Zed; this fork is pushed to `fork`
(`git@github.com:SourceShift/zed-mini-ork`). Sync pull requests always target
`mini-ork`.

```bash
script/mini-ork-pull        # update to the fork's latest, then rebuild
git submodule update --remote crates/mini_ork_ui   # move the UI crate forward
```

## Layout

| What | Where |
|---|---|
| Shell, board and pages | the [`mini_ork_ui`](https://github.com/SourceShift/mini-ork-ui) crate, vendored as the `crates/mini_ork_ui` submodule — edit it in its own repo |
| Fork hooks into Zed | `Cargo.toml`, `crates/zed/*`, `crates/ui/*`, `crates/inspector_ui/*`, `crates/icons/*`, `crates/worktree/*`, `crates/git_ui/*` |
| Tooling | `script/mini-ork-*`, `.github/workflows/mini-ork-*.yml` |

## Documentation

**[MINI-ORK.md](./MINI-ORK.md)** is the fork's manual: build and rebuild tips,
the submodule rules, how a new Zed release is carried over, and what protects
the fork's changes from being overwritten.

## Upstream

Zed is a high-performance, multiplayer code editor from the creators of
[Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).
Upstream's docs live at [zed.dev/docs](https://zed.dev/docs) and its source at
[zed-industries/zed](https://github.com/zed-industries/zed).

## Licence

Zed's editor crates are GPL-3.0-or-later, with Apache-2.0 components where
marked. This IDE, including the `mini_ork_ui` crate source in
[SourceShift/mini-ork-ui](https://github.com/SourceShift/mini-ork-ui), is
distributed under the same terms. mini-ork itself (Apache-2.0) is a separate
program the IDE runs.

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponsorship

Zed is developed by **Zed Industries, Inc.**, a for-profit company.

If you’d like to financially support the project, you can do so via GitHub Sponsors.
Sponsorships go directly to Zed Industries and are used as general company revenue.
There are no perks or entitlements associated with sponsorship.

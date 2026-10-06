# mini-ork IDE

Zed with mini-ork built in, drawn to the `mini-ork IDE.dc.html` design:

- **Activity bar + Threads board** on the left — they fill Zed's pluggable
  workspace sidebar slot, in place of Zed's Threads Sidebar.
- **The mini-ork thread** — Zed's Agent panel running the `mini-ork` agent, in
  the left dock.
- **Pages as tabs** — Orchestrator, Runs, Changes, Verify, Recipes,
  Automations, Lanes & cost, Learning, Context, Nodes, Setup, and one tab per
  run (DAG, Overview, Agents, Learnings, Artifacts).
- **Title bar and status bar** with what needs you, today's spend vs the cap,
  the scheduler, ContextNest and nodes.

The IDE layers its own defaults under your settings (no dock buttons in the
status bar); `~/.config/zed/settings.json` is shared with Zed and never written.

## Layout of the changes

| What | Where | Touches Zed's files? |
|---|---|---|
| Shell and pages | `crates/mini_ork_ui/` | no — a new crate |
| Hooks | `Cargo.toml`, `crates/zed/Cargo.toml`, `crates/zed/src/main.rs`, `crates/zed/src/zed.rs` | yes — 5 lines (`git diff $(cat MINI_ORK_BASE) -- Cargo.toml crates/zed`) |
| Tooling | `script/mini-ork-*`, `MINI-ORK.md`, `MINI_ORK_BASE` | no — new files |

Every page is data: `mini-ork board page <key> [--tab T] [--arg k=v]` returns
it as JSON (`mini_ork/ide_pages/spec.py` in mini-ork) and `page.rs` draws it, so a
page changes without rebuilding the IDE. The shell polls `mini-ork board --json`
(5 s while the IDE is active, 30 s otherwise) and every button runs a mini-ork
command (`board merge|discard|stop|kill|resume|gate`, `automations …`). mini-ork
stays the source of truth and a separate program.

`MINI_ORK_BIN` picks the launcher (default `~/.local/bin/mini-ork`).
`MINI_ORK_IDE_OPEN=runs,run:latest:dag` opens pages at start-up — for checking
the IDE without clicking through it.

## Build and run

```bash
script/mini-ork-build                 # utility priority, 4 jobs; refreshes the app
open -a "mini-ork IDE" ~/path/to/project   # or script/mini-ork-ide <folder>
```

`script/mini-ork-app` (run by `mini-ork-build`) assembles `target/mini-ork IDE.app`
around the build — its own name, bundle id `dev.mini-ork.ide` and icon, the binary
hard-linked so it costs no space — and links it into `~/Applications`, so it opens
from Spotlight, Launchpad and the Dock like any app.

`script/mini-ork-build` keeps the Mac usable while it builds: background QoS
(efficiency cores), 4 parallel jobs (`MINI_ORK_BUILD_JOBS`), no debug info. A
first build is long; after that, a change to `crates/mini_ork_ui` rebuilds in
about a minute. Avoid toggling features of core crates (e.g. GPUI
`test-support`): that recompiles most of Zed.

The dev channel makes it a separate instance next to an installed Zed. It
shares `~/.config/zed` (settings, keymap, agent servers — the `mini-ork` agent
from `mini-ork zed setup` works unchanged).

## Upgrading to a new Zed release

```bash
script/mini-ork-upgrade v1.23.0
```

It checks out the new tag, re-applies our commits (crate, hooks, tooling),
records the new base in `MINI_ORK_BASE` and builds. A stop during the
cherry-pick means a hook line conflicted or a Zed API the crate uses changed;
fix it, `git cherry-pick --continue`, then `script/mini-ork-upgrade v1.23.0 --resume`.

## Licence

Zed's editor crates are GPL-3.0-or-later; this IDE, including
`crates/mini_ork_ui`, is distributed under the same terms. mini-ork itself
(Apache-2.0) is a separate program the IDE runs.

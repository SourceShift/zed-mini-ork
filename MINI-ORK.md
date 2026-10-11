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
script/mini-ork-build                 # utility priority, one job per core (cap 8)
open -a "mini-ork IDE" ~/path/to/project   # or script/mini-ork-ide <folder>
```

`script/mini-ork-app` (run by `mini-ork-build`) assembles `target/mini-ork IDE.app`
around the build — its own name, bundle id `dev.mini-ork.ide` and icon, the binary
hard-linked so it costs no space — and links it into `~/Applications`, so it opens
from Spotlight, Launchpad and the Dock like any app.

`script/mini-ork-build` keeps the Mac usable while it builds: background QoS
(efficiency cores) and one parallel job per core, capped at 8
(`MINI_ORK_BUILD_JOBS` lowers it — the memory lever). A first build is long;
after that, a change to `crates/mini_ork_ui` rebuilds in about a minute. Avoid
toggling features of core crates (e.g. GPUI `test-support`): that recompiles
most of Zed.

### Speeding up rebuilds

The build is cached per profile. Anything that changes the *profile* — the
debug-info level, a feature toggle on a core crate — rehashes every unit and
rebuilds the whole 100 GB target dir. That is the upgrade path, not the
everyday one: set it once and leave it.

After that, one command rebuilds the app: `script/mini-ork-build`. It pins
debug info to `limited` (line tables only) instead of the profile's `full`,
because the zed binary carries DWARF from every crate and the linker merges all
of it — that merge, not the one-crate compile, is what the rebuild time goes
into. Backtraces still carry file:line. `MINI_ORK_BUILD_DEBUG=full` restores
locals at the cost of one full rebuild.

- `cargo ui-check` — type-check `crates/mini_ork_ui` alone, no codegen and no
  relink. Seconds instead of a minute; run it while editing, then
  `script/mini-ork-build` to look at the result.
- Page content is data, not code: `mini-ork board page <key>` returns JSON that
  `page.rs` draws, so page changes need no rebuild at all (see above).
- `sccache` wraps rustc (`~/.cargo/config.toml`, cache on the external SSD).
  It does nothing for an ordinary incremental rebuild, but it rescues the
  expensive ones: a wiped target dir, a profile flip, or a branch switch.
  `sccache --show-stats` reports hits.
- `script/mini-ork-build` links with `ld64.lld` (`brew install lld`) when it is
  present, which links the very large zed binary faster than Apple's `ld`;
  `MINI_ORK_LLD=0` falls back.

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

# mini-ork IDE

Zed with mini-ork built in: a **mini-ork panel** (runs, review, learnings,
automations, recipes) next to Zed's own Agent panel, Threads Sidebar, Git
panel and editor.

## Layout of the changes

| What | Where | Touches Zed's files? |
|---|---|---|
| Panels and their data | `crates/mini_ork_ui/` | no — a new crate |
| Hooks | `Cargo.toml`, `crates/zed/Cargo.toml`, `crates/zed/src/main.rs`, `crates/zed/src/zed.rs` | yes — 7 lines in one commit |
| Tooling | `script/mini-ork-*`, `MINI-ORK.md`, `MINI_ORK_BASE` | no — new files |

The panels show what `mini-ork board --json` returns and run mini-ork's own
commands (`mini-ork board merge|discard|stop`, `mini-ork automations …`) for
every button. mini-ork stays the source of truth and a separate program.

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

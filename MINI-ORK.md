# mini-ork IDE

Zed with mini-ork built in, drawn to the `mini-ork IDE.dc.html` design:

- **Activity bar + Threads board** on the left — they fill Zed's pluggable
  workspace sidebar slot, in place of Zed's Threads Sidebar.
- **The mini-ork thread** — Zed's Agent panel running the `mini-ork` agent, in
  the left dock.
- **Pages as tabs** — Orchestrator, Runs, Changes, Verify, Recipes,
  Automations, Lanes & cost, Learning, Context, Nodes, Features, Setup, and one
  tab per run (DAG, Overview, Agents, Learnings, Artifacts).
- **Title bar and status bar** with what needs you, today's spend vs the cap,
  the scheduler, ContextNest and nodes.

The IDE layers its own defaults under your settings (no dock buttons in the
status bar); `~/.config/zed/settings.json` is shared with Zed and never written.

## Layout of the changes

| What | Where | Touches Zed's files? |
|---|---|---|
| Shell and pages | the `mini_ork_ui` crate — its own repo, [SourceShift/mini-ork-ui](https://github.com/SourceShift/mini-ork-ui), vendored here as the `crates/mini_ork_ui` submodule | no — a new crate |
| Hooks and fork edits | `Cargo.toml`, `crates/zed/*`, `crates/ui/*`, `crates/inspector_ui/*`, `crates/icons/*`, `crates/git_ui/*`, `crates/worktree/*`, `assets/*`, `.cargo/config.toml` | yes — run `git diff --name-only "$(cat MINI_ORK_BASE)..HEAD"` for the exact list |
| Tooling | `script/mini-ork-*` (incl. `mini-ork-sync-ui`, `mini-ork-check`, `mini-ork-pull`), `.github/workflows/mini-ork-*.yml`, `MINI-ORK.md`, `MINI_ORK_BASE`, `.gitmodules` | no — new files |

The crate source is **not** in this repo: it lives in `SourceShift/mini-ork-ui`
and is a submodule at `crates/mini_ork_ui/`. The `mini-ork` repo vendors the same
crate at `ui/`, so a commit pushed to that repo reaches both. **Edit the crate in
its own repo** — the two superprojects each pin a commit, and editing inside
either submodule checkout leaves a detached HEAD the other will never see.

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

The `mini_ork_ui` crate is a member of this cargo workspace, and cargo refuses
to load the workspace when a member directory is missing — so a fresh clone
**will not build** (and rust-analyzer will show the whole project as broken)
until the submodule is materialised:

```bash
git clone --recurse-submodules git@github.com:SourceShift/zed-mini-ork.git
# or, in an existing clone:
git submodule update --init --recursive
```

`script/mini-ork-sync-ui` does the same thing for you; you do not normally run
it by hand — `script/mini-ork-build` and `script/mini-ork-check` call it first.

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
after that, a change to the crate rebuilds in about a minute. Avoid
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

- `script/mini-ork-check` — materialises the submodule, then type-checks the
  crate alone (`cargo ui-check`), no codegen and no relink. Seconds instead of a
  minute; run it while editing, then `script/mini-ork-build` to look at the
  result. (Use the script, not bare `cargo ui-check`: the alias cannot fetch the
  submodule, and cargo needs the crate present to load the workspace at all.)
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

## Updating the crate

```bash
git submodule update --remote crates/mini_ork_ui   # move onto mini-ork-ui main
git add crates/mini_ork_ui && git commit -m "mini-ork: bump mini_ork_ui"
```

Commit the new gitlink, then push to `fork` (see below). Never edit files
directly under `crates/mini_ork_ui/` and expect them to stick: it is a submodule,
and `script/mini-ork-sync-ui` resets it to the pinned commit before every build.

## Remotes

`origin` is **upstream Zed** (`git@github.com:zed-industries/zed`) — never push
there. This fork is pushed to **`fork`**
(`git@github.com:SourceShift/zed-mini-ork`) on the `mini-ork` branch.

## Staying current on Zed

Zed ships a stable release most weeks. `mini-ork` is the branch users build, and
it only ever fast-forwards, so a new Zed base reaches a user as a merge on that
branch:

```bash
script/mini-ork-pull        # git pull --ff-only, submodule, rebuild
```

Nothing needs to watch for releases: `.github/workflows/mini-ork-track-zed.yml`
re-bases our commits onto each new stable tag daily, pushes `mini-ork-<tag>` and
opens a pull request. It never builds — a full Zed build is hours and ~100 GB, so
the branch is prepared by a machine and merged by a human who has built and
launched it. `.github/workflows/mini-ork-release.yml` will build the app and
attach it to a release, but that job wants a bigger runner than the 3 vCPU
GitHub gives away (see the comment in the workflow).

Our delta is what decides how painful this is: currently ~62 commits, of which
**13 files and ~140 lines touch Zed's own code**. Everything else — `assets/`,
`script/`, this file, `.gitmodules` — is ours and cannot conflict. The crate is
not replayed at all; it is a submodule. Keep that file count small: prefer a new
crate over an edit to a Zed file.

The collision set is stable and small, which is what makes a weekly rebase
viable. Across `v1.22.0 → v1.23.2` upstream changed 298 files, but only four of
ours: `Cargo.toml`, `Cargo.lock`, `crates/zed/Cargo.toml`,
`crates/zed/src/zed.rs`. Two are manifests (take upstream, re-add our entries)
and `zed.rs` is the hook that pulls the crate's pages in. Everything else
replayed untouched.

## Upgrading to a new Zed release

```bash
script/mini-ork-upgrade v1.23.2              # rebase, build, stop
script/mini-ork-upgrade v1.23.2 --publish    # ...then move mini-ork onto it
```

It checks out the new tag, re-applies our commits (fork edits + tooling; the
crate is a submodule, not replayed), untracks any `crates/mini_ork_ui` a replay
left behind, converges the submodule, records the new base in `MINI_ORK_BASE`
and builds. A stop during the cherry-pick means a fork file conflicted or a Zed
API the crate uses changed; fix it, `git cherry-pick --continue`, then
`script/mini-ork-upgrade v1.23.2 --resume`.

`--publish` is what puts the release in front of users: it merges the branch into
`mini-ork` and pushes `fork`. It refuses to run without the build, because
publishing a base nobody launched is how a broken one reaches users. `--no-build`
is for the tracking workflow, which has nowhere to put a 100 GB target dir.

## Licence

Zed's editor crates are GPL-3.0-or-later; this IDE, including the `mini_ork_ui`
crate source in [SourceShift/mini-ork-ui](https://github.com/SourceShift/mini-ork-ui),
is distributed under the same terms. mini-ork itself (Apache-2.0) is a separate
program the IDE runs.

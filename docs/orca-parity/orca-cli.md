# Orca CLI surface (captured)

Provenance: `orca --help` from the installed bundle
(`/Applications/Orca.app/Contents/Resources/bin/orca`), captured 2026-10-29.
Orca app reports v1.4.222; the vendored source `sources/orca` is v1.4.214
(`76d1b7bf29`). 154 verbs across 31 sections.

Selectors, output options and the per-verb option lists are in the help text
and are not repeated here; this file is the inventory the parity matrix cites.

### Startup

- `open` — Launch Orca and wait for the runtime to be reachable
- `serve` — Start a headless Orca runtime server
- `status` — Show app/runtime/graph readiness

### Diagnostics

- `diagnostics memory` — Collect a memory snapshot for Orca and managed terminals

### Agent Discovery

- `agent-context` — Print the machine-readable command schema for agents

### Agent Sessions

- `search` — Search the full text of agent sessions on one Orca host

### Accounts

- `account add` — Add a managed agent account on this Orca host
- `account list` — List managed agent accounts on this Orca host
- `account select` — Select an OpenCode or Devin account for new launches
- `account rm` — Remove an OpenCode or Devin account and its private data

### Skills

- `skills installed` — List installed skill selectors
- `skills share` — Publish selected skills behind one unlisted link
- `skills list` — List version-matched skill guides bundled with this Orca CLI
- `skills get` — Print a version-matched skill guide as Markdown
- `skills install` — Install bundled Orca skills globally via the community skills CLI
- `skills update` — Update already-installed Orca skills via the community skills CLI

### Hosts

- `host list` — List targetable machines and how to name each one

### Environments

- `environment add` — Save a remote Orca runtime from a pairing code
- `environment list` — List saved remote Orca runtimes
- `environment show` — Show one saved remote Orca runtime
- `environment rm` — Remove a saved remote Orca runtime

### Environment Recipes

- `vm recipe doctor` — Validate a per-workspace environment recipe

### Automations

- `automations list` — List scheduled Orca automations
- `automations show` — Show one Orca automation
- `automations create` — Create a scheduled Orca automation
- `automations edit` — Edit an Orca automation
- `automations remove` — Remove an Orca automation and its run history
- `automations run` — Run an Orca automation now
- `automations runs` — List automation run history

### Projects

- `project list` — List durable projects known to Orca
- `project setups` — List project host setups
- `project setup-clone` — Make a project available on a host by cloning a repository
- `project setup-create` — Create independent project host setup metadata
- `project setup-update` — Update project host setup metadata
- `project setup-delete` — Remove a project host setup

### Repos

- `repo list` — List repos registered in Orca
- `repo add` — Add a project to Orca by filesystem path
- `repo show` — Show one registered repo
- `repo set` — Set whether non-Orca worktrees are shown for a repo
- `repo set-base-ref` — Set the repo's default base ref for future worktrees
- `repo search-refs` — Search branch/tag refs within a repo

### Worktrees

- `worktree list` — List Orca-managed worktrees
- `worktree show` — Show one worktree
- `worktree current` — Show the Orca-managed worktree for the current directory
- `worktree create` — Create a new Orca-managed worktree
- `worktree set` — Update Orca metadata for a worktree
- `worktree rm` — Remove a worktree from Orca and git
- `worktree ps` — Show a compact orchestration summary across worktrees

### Files

- `file open` — Open a file as a tab, view stays put (--focus brings you to it)
- `file diff` — Open a file diff as a tab, view stays put (--focus brings you to it)
- `file open-changed` — Open changed files as tabs, view stays put (--focus brings you there)

### Terminals

- `terminal list` — List live Orca-managed terminals
- `terminal show` — Show terminal metadata and preview
- `terminal read` — Read bounded terminal output
- `terminal send` — Send input to a live terminal
- `terminal wait` — Wait for a terminal condition (exit, tui-idle)
- `terminal create` — Create a terminal session in a worktree
- `terminal rename` — Set or clear the title of a terminal tab
- `terminal split` — Split an existing terminal pane
- `terminal switch` — Bring a terminal tab to the foreground
- `terminal focus` — Alias for terminal switch
- `terminal close` — Close one terminal, its whole tab with --tab, or all in a worktree

### Orchestration

- `orchestration run-create` — Create and bind a lightweight orchestration Run
- `orchestration run-use` — Bind this coordinator to an existing Run
- `orchestration run-list` — List lightweight orchestration Runs
- `orchestration run-show` — Show one lightweight orchestration Run
- `orchestration send` — Send an inter-agent message
- `orchestration check` — Check the bound Run mailbox
- `orchestration ask` — Ask the coordinator a blocking question
- `orchestration reply` — Reply to a message
- `orchestration inbox` — Show all messages across recipients
- `orchestration task-list` — List orchestration tasks
- `orchestration dispatch` — Dispatch a task to a terminal
- `orchestration gate-list` — List decision gates
- `orchestration reset` — Reset orchestration state

### Computer Use

- `computer capabilities` — Show computer-use provider capabilities
- `computer permissions` — Show or open computer-use permission setup
- `computer list-apps` — List running apps available to computer-use
- `computer list-windows` — List visible windows for a target app
- `computer get-app-state` — Capture a compact accessibility snapshot of an app
- `computer click` — Click an app element or window coordinate
- `computer scroll` — Scroll an app element
- `computer drag` — Drag between app elements or window coordinates
- `computer type-text` — Type literal text at the current app focus
- `computer press-key` — Press a single key such as Return or Escape
- `computer hotkey` — Press a shortcut combination such as CmdOrCtrl+A
- `computer paste-text` — Paste text through the native clipboard path
- `computer set-value` — Set the value of a settable app element

### Linear

- `linear` — Read and write Linear issues for agents

### Mobile Emulator (iOS Simulator)

- `emulator list` — List available/running emulators (Orca-managed + raw serve-sim)
- `emulator exec --command` — Raw serve-sim subcommand passthrough (no "serve-sim" prefix)
- `emulator kill` — Stop helper for device

### Browser Automation

- `tab create` — Create a new browser tab (navigates to --url)
- `tab list` — List open browser tabs
- `tab show` — Show one browser tab by page id
- `tab current` — Show the current browser tab
- `tab profile list` — List browser session profiles
- `tab profile create` — Create a browser session profile
- `tab profile delete` — Delete a browser session profile
- `tab profile set` — Switch a browser tab to a different profile
- `tab profile show` — Show the profile bound to a browser tab
- `tab profile use-default` — Switch a browser tab back to the default profile
- `tab profile clone` — Clone a browser tab into another profile
- `tab switch` — Switch the active browser tab by --index or --page
- `tab close` — Close a browser tab by --index/--page or the current tab
- `snapshot` — Accessibility snapshot with element refs (e.g. @e1, @e2)
- `goto` — Navigate the active tab to --url
- `click` — Click element by --element ref
- `fill` — Clear and fill input by --element ref with --value
- `type` — Type --input text at the current focus (no element needed)
- `select` — Select dropdown option by --element ref and --value
- `hover` — Hover element by --element ref
- `keypress` — Press a key (e.g. --key Enter, --key Tab)
- `scroll` — Scroll --direction (up/down) by --amount pixels
- `back` — Navigate back in browser history
- `reload` — Reload the active browser tab
- `screenshot` — Capture viewport screenshot (--format png|jpeg)
- `eval` — Evaluate --expression JavaScript in the page context
- `wait` — Wait for page idle or --timeout ms
- `check` — Check a checkbox by --element ref
- `uncheck` — Uncheck a checkbox by --element ref
- `focus` — Focus an element by --element ref
- `clear` — Clear an input by --element ref
- `drag` — Drag --from ref to --to ref
- `upload` — Upload --files to a file input by --element ref
- `dblclick` — Double-click element by --element ref
- `forward` — Navigate forward in browser history
- `scrollintoview` — Scroll --element into view
- `get` — Get element property (--what: text, html, value, url, title)
- `is` — Check element state (--what: visible, enabled, checked)
- `inserttext` — Insert text without key events
- `mouse move` — Move mouse to --x --y coordinates
- `mouse down` — Press mouse button
- `mouse up` — Release mouse button
- `mouse wheel` — Scroll wheel --dy [--dx]
- `find` — Find element by locator (--locator role|text|label --value <v>)
- `set device` — Emulate device (--name "iPhone 12")
- `set offline` — Toggle offline mode (--state on|off)
- `set headers` — Set HTTP headers (--headers '{"key":"val"}')
- `set credentials` — Set HTTP auth (--user <u> --pass <p>)
- `set media` — Set color scheme (--color-scheme dark|light)
- `clipboard read` — Read clipboard contents
- `clipboard write` — Write --text to clipboard
- `dialog accept` — Accept browser dialog (--text for prompt response)
- `dialog dismiss` — Dismiss browser dialog
- `storage local get` — Get localStorage value by --key
- `storage local set` — Set localStorage --key --value
- `storage local clear` — Clear localStorage
- `storage session get` — Get sessionStorage value by --key
- `storage session set` — Set sessionStorage --key --value
- `storage session clear` — Clear sessionStorage
- `download` — Download file via --selector to --path
- `highlight` — Highlight --selector on page
- `exec` — Run any agent-browser command (--command "...")

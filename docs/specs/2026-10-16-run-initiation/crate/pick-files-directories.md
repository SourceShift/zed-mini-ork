# pick_files can ask for a directory

Depends on: none

Companion brief: `docs/ux-briefs/2026-10-16-run-initiation-modes.md` (§7, §8 F1).

The IDE's only file-chooser action, `pick_files`, hardcodes the native picker to
`files: true, directories: false, multiple: true` at one site
(`crates/mini_ork_ui/src/store.rs`). Every journey that begins with "choose a
folder" — pointing a spec-driven run at its spec directory is the motivating one
— therefore has no widget to call. The engine-side builder
(`mini_ork/ide_pages/spec.py::pick_files`) emits only `cli`, `prompt` and
`home`, so even a crate that could ask for a directory would never be told to.

This spec is the **consumer half**: teach the action payload to carry those three
switches and default them to today's behaviour. The builder half lands with the
page layer that uses it, so this change ships nothing user-visible and cannot
regress any existing caller.

**Where this code lives, and where it is tested.** The canonical source is the
`SourceShift/mini-ork-ui` repository; `crates/mini_ork_ui/` in a `zed-mini-ork`
checkout is a git submodule of it, and `ui/` in the engine is the same submodule
uninitialised. The crate does **not** build standalone — its manifest is a member
of the Zed workspace and fourteen of its dependencies are Zed-internal path
crates. So the acceptance commands below run **from a `zed-mini-ork` checkout**,
never from inside the submodule. An edit here is inert until a superproject moves
onto the new commit (`git submodule update --remote <path>`) and the IDE is
rebuilt; pushing the crate alone changes nothing a user sees.

## Inputs

- `crates/mini_ork_ui/src/store.rs` — `fn pick_files`, reached from the action
  dispatch on `action.get("pick_files")`.
- The action payload, today `{"pick_files": {"cli": [<args…>], "prompt": "<text>",
  "home": <bool>}}`. Three optional keys are added by this spec:
  - `"files"` — include files in the picker. Default `true`.
  - `"directories"` — include directories in the picker. Default `false`.
  - `"multiple"` — allow more than one selection. Default `true`.
- `mini_ork/ide_pages/spec.py::pick_files` on the engine side is the only
  producer; it is out of scope here and gains the keys in the page-layer spec.

## Outputs

- `store.rs` gains a plain data type carrying the parsed request, constructed by
  a pure function — no `cx`, no `Window`, no `gpui::PathPromptOptions` — so the
  parsing is unit-testable without a GPUI app. It holds the argv prefix, the
  `home` flag, the prompt, and the three switches.
- `fn pick_files` consumes that struct instead of reading `Value` inline, and
  passes the three switches straight into `gpui::PathPromptOptions`.
- Existing payloads are byte-for-byte equivalent in behaviour: a payload that
  omits all three keys produces exactly today's `PathPromptOptions`.
- A payload whose `cli` is missing or empty still returns early and runs
  nothing, as today.

## Errors

- A key present with the wrong JSON type is ignored, not rejected: the request's
  value falls back to its default. This keeps the action channel forgiving, which
  is how every other field of this payload already behaves (`home` is read with
  `and_then(Value::as_bool).unwrap_or(true)`).
- A parsed request with an empty `cli` is `None` at the parse step, so the
  caller has nothing to run rather than a picker that opens into a no-op.

## Edge cases

- **Neither files nor directories.** A payload with `"files": false` and
  `"directories": false` sets both off in `PathPromptOptions`. The platform
  picker may then show nothing selectable. This is accepted — the builder is the
  party that must not emit it — but the parse step must not silently force
  `files` back on, because that would make the switch a lie.
- **Directories only.** `"files": false, "directories": true` is the new
  capability and the case the page layer exists for.
- **`"multiple": false`.** Passed through unchanged. Nothing in this spec
  depends on it; it is carried so the switch set is complete rather than
  growing a second time.
- **A cancelled picker.** Unchanged: the await returns `Ok(None)` and no CLI
  runs.
- **The README HARD RULE.** The crate's `README.md` opens with a
  `> [!IMPORTANT]` line and a removal instruction. Both lines are already
  present. They must be present in the final diff and must not be removed.

## Acceptance criteria

- AC1: from a `zed-mini-ork` checkout, `cargo test -p mini_ork_ui --lib` reports
  0 failed.
- AC2: from that checkout, `cargo test -p mini_ork_ui --lib pick_files` reports
  1 passed, and the new test module covers: all three keys omitted; each key set
  to its non-default value; a key present with a wrong JSON type; and a missing
  `cli`.
- AC3: from that checkout, `./script/clippy` reports no warning whose file path
  is `crates/mini_ork_ui/` that is not also present on the pre-change tree —
  capture the pre-change warnings first and compare.
- AC4: `head -2 crates/mini_ork_ui/README.md` prints the two `> [!IMPORTANT]`
  lines — the `[!IMPORTANT]` marker on line 1 and the removal instruction on
  line 2.
- AC5 (precondition, passes before work starts): `cargo test -p mini_ork_ui
  --lib` reports 0 failed on the unmodified tree, so any failure afterwards is
  attributable to this change.

## Probes

`spec-driven-delivery` builds every gate from the executable fences below, so
these are the acceptance criteria in the form the run can actually execute.
They run with cwd = a `zed-mini-ork` checkout (never inside the submodule) and
must be re-run by hand before claiming the spec is met.

Four of the five are **precondition**-tagged: AC2 is the only thing this change
turns from red to green, and AC1/AC3/AC4/AC5 are preservation invariants that
already hold on the untouched tree (the suite is green, no clippy diagnostic is
*added* to the ones the crate already has, the README rule is in place). `test-validity` rejects a probe
that passes before the work as a broken baseline unless it is tagged
precondition; tagging is what keeps those four honest rather than vacuous —
`smoke-live` re-runs all of them after the change, so they still have to hold.

```bash
# Probes run with cwd = a zed-mini-ork checkout. The crate is a git submodule
# and does NOT build standalone; script/clippy exists only at this root.
test -f script/clippy && test -d crates/mini_ork_ui && test -f Cargo.lock

# AC1 precondition: preservation — the whole crate's lib suite stays green.
# Passes on the untouched tree, so it must be tagged, not treated as evidence.
cargo test -p mini_ork_ui --lib 2>&1 | tail -25
echo "AC1 pass: mini_ork_ui --lib reports 0 failed"

# AC2: the new parse tests exist and pass. On the untouched tree the filter
# matches nothing ("0 passed"), which is what makes this the discriminating
# gate: the grep fails before the change and succeeds after it.
out="$(cargo test -p mini_ork_ui --lib pick_files 2>&1)" || { printf '%s\n' "$out" | tail -40; exit 1; }
printf '%s\n' "$out" | tail -20
printf '%s\n' "$out" | grep -E "test result: ok\. [1-9][0-9]* passed; 0 failed" >/dev/null
echo "AC2 pass: the pick_files parse tests pass with 0 failed"

# AC3 precondition: no NEW clippy warning for this crate. The crate is not
# clippy-clean today — 18 diagnostics already fail `--deny warnings` — so the
# criterion is set difference, not zero. The baseline is committed beside this
# spec; it keys on file + message, never on line/column, so ordinary edits do
# not churn it. Regenerate it with the same awk when the pre-change tree moves.
base=docs/specs/2026-10-16-run-initiation/crate/clippy-baseline.txt
log="$(mktemp -t ac3-clippy.XXXXXX)"
./script/clippy -p mini_ork_ui >"$log" 2>&1 || true   # findings are the signal, not the rc
awk '
  /^(error|warning): / { msg=$0; next }
  /^[[:space:]]+--> crates\/mini_ork_ui\// {
    loc=$0; sub(/^[[:space:]]+--> crates\/mini_ork_ui\//,"",loc); sub(/:[0-9]+:[0-9]+$/,"",loc);
    print loc "\t" msg
  }
' "$log" | sort -u > "$log.new"
grep -v '^#' "$base" | sed '/^$/d' | sort -u > "$log.base"
new="$(comm -13 "$log.base" "$log.new")"
if [ -n "$new" ]; then printf 'new clippy diagnostics:\n%s\n' "$new"; rm -f "$log" "$log.new" "$log.base"; exit 1; fi
rm -f "$log" "$log.new" "$log.base"
echo "AC3 pass: mini_ork_ui adds no clippy diagnostic beyond the baseline"

# AC4 precondition: the two README lines survive, byte for byte.
first="$(sed -n '1p' crates/mini_ork_ui/README.md)"
second="$(sed -n '2p' crates/mini_ork_ui/README.md)"
printf '%s\n%s\n' "$first" "$second"
test "$first" = "> [!IMPORTANT]"
test "$second" = "> Remove this line to confirm you've reviewed this PR before submitting."
echo "AC4 pass: README keeps its two [!IMPORTANT] lines"

# AC5 precondition: baseline — the crate's lib suite is green before any edit,
# so a later failure is attributable to this change.
cargo test -p mini_ork_ui --lib 2>&1 | tail -5
echo "AC5 pass: baseline mini_ork_ui --lib reports 0 failed"
```

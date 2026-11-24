# Contributing

## Checks

| When | Command |
|---|---|
| While working | `cargo xtask check` |
| Before pushing | `cargo xtask ci prepush` |
| With your game data | `cargo xtask ci tier1` |
| On a graphics device | `cargo xtask ci tier2` |

`ci prepush` runs every check that needs no data, cheapest first: the rules, the changelog, the
docs, clippy, clippy for Windows and Linux, the build and the tests. CI runs nothing on a Mac, so
this is the Mac's run. `cargo xtask install-hook` makes git run it before each push, and skip it
when it already passed on the commit being pushed with nothing uncommitted. `git push --no-verify`
skips it. `--list` shows what it would run.

**Clippy for Windows and Linux** (`cargo xtask cross-clippy`) catches code that only builds on one
system, such as an unguarded `std::os::unix`, before CI does. It needs each target's standard
library once: `rustup target add x86_64-pc-windows-msvc x86_64-unknown-linux-gnu`. Without it the
rows say NO-HOST with that command and prepush goes on. Nothing else is installed: the C a few
dependencies carry is stood in for by scripts xtask writes to Pyreal's cache folder, because clippy
never links. So this checks the Rust for each system and CI still builds the C.

**The changelog** (`cargo xtask changelog`). A branch that changes what players get, since its merge
base with `main`, adds to the Unreleased section of [CHANGELOG.md](CHANGELOG.md). What players get
is the code under `core/`, `pyreal/client/` and `pyreal/desktop/`, shaders and
options included, and `base-pack.toml`; tests, examples, fixtures, `Cargo.toml` files, Markdown,
`tools/`, docs and notes are not. On `main` itself it compares with `origin/main`, so a push of
`main` needs a changelog change too. A branch with nothing for players says so with a trailer on
any of its commits, a line of its own after a blank line in the message (a title is never a
trailer):

```text
Made the sweep print its shard count

Changelog: none
```

`PYREAL_CHANGELOG=none` does the same for one run.

## Heavy runs

**On battery**, `cargo xtask ci tier0`, `tier1`, `tier2`, `cargo xtask golden` and
`python3 build.py` stop with a note.
`--on-battery`, or `PYREAL_ON_BATTERY=1`, runs them anyway. `ci prepush` always runs.

**The machine lock.** One lock for the whole machine (its files are in `~/Library/Caches/pyreal`)
has two kinds of hold:

- **Heavy**, for work that loads the machine: `ci tier1`, `ci tier2`, `cargo xtask gate`,
  `cargo xtask gates`, `cargo xtask golden`, `python3 build.py` and engine-view's headless runs.
  A heavy hold never waits. It only marks the machine busy, so a timing run knows to wait.
- **Quiet**, for timing and profiling runs only: `cargo xtask lock --quiet -- <command>`. It waits
  until the heavy work that started before it has finished, then has the machine to itself. Heavy
  work that starts while it waits or runs still goes ahead at once, without delaying it, and says
  which timing run it may skew. It also waits for runs of older checkouts, which took one
  exclusive lock for everything.

A quiet run that waits says who it waits for, where, and for how long, listing every holder. A
hold goes when its holder exits, however it exits. `cargo xtask lock` says who holds it, and
`cargo xtask lock -- <command>` runs any command under a heavy hold (`--heavy` says so outright).
The processes a holder starts do not wait on it, so a test or render run inside a quiet hold is
part of the measurement. `PYREAL_MACHINE_LOCK=off` skips it. Python and shell tools take a heavy
hold with `python3 tools/machine-lock/machine_lock.py heavy -- <command>`.

**The retail comparisons** (`cargo xtask golden`, and rows of `ci tier2`). A few places from
`pyreal/client/tests/gpu/report-library.txt`, listed in `tools/xtask/src/golden.rs`, are drawn by
engine-view with retail's look and with Pyreal's over the base pack. The retail-look pictures must
match golden pictures within a small tolerance. Pyreal's must stay near retail's in coarse measures:
mean lightness over the frame and each ninth of it, colour cast and the lightness histogram. The
goldens come from the retail data, so they are never committed: they live in `PYREAL_GOLDEN_DIR`,
or in `Golden` beside the folder `PYREAL_TEST_DAT_DIR` names, one folder per graphics adapter.
Without them the row is NOT RUN. `cargo xtask golden --update` draws them again after a change to
the retail look that was meant. Each run leaves its pictures, and differences from the goldens that
failed, in `target/golden/`.

`cargo xtask` with no arguments lists every check. [docs/BUILDING.md](docs/BUILDING.md) says how to
build and run Pyreal.

## Tests

Tests build in the **`test-release` profile**: optimised, with debug assertions and overflow
checks on. A hand-typed command takes the same profile: `cargo test --profile test-release -p <crate>`.

A crate's integration tests are one binary per tier:

| Tier | Needs | Target |
|---|---|---|
| `cpu` | nothing but the crate | `tests/cpu/main.rs` |
| `dat` | game data | `tests/dat/main.rs` |
| `gpu` | a graphics device and game data | `tests/gpu/main.rs` |

**Your own game data.** The game's data files are never part of this repository. Tests read your
copies through variables only tests read. Set them in your environment, or once for every build
with an `[env]` entry in a `.cargo/config.toml` above the workspace.

- `PYREAL_TEST_DAT_DIR`: the folder holding `client_portal.dat`, `client_cell_1.dat`,
  `client_highres.dat` and `client_local_English.dat`.
- `PYREAL_TEST_PRETOD_DAT_DIR`: the February 2005 `portal.dat` and `cell.dat`, from before Throne of
  Destiny.
- `PYREAL_TEST_LAUNCH_DAT_DIR`: the October 1999 launch `portal.dat` and `cell.dat`.

The three sets of data files are the only ones Pyreal reads: files of other dates are not
supported.

`PYREAL_TEST_GOLDEN=1` rewrites golden files instead of comparing against them.
`DERETH_TEST_DAT_DIR`, `DERETH_TEST_PRETOD_DAT_DIR`, `DERETH_SETTINGS_DIR` and `DERETH_ART_PACK`,
the variables' Dereth names, are still read when the `PYREAL_*` name is unset.

**The art packs.** The app carries a base pack, `base-pack.dat`, drawn under the pack the player
chooses. `base-pack.toml` pins it to a commit of the pack repository
(`../dereth-enhanced-environment`); `cargo xtask base-pack build` packs that commit into `target/base-pack/base-pack.dat`, which a run
from the checkout reads and `build.py` bundles (`cargo xtask package pyreal --base-pack <file>`).
After a change to the pack repository is committed, `cargo xtask base-pack update` pins its
`HEAD`; commit the new `base-pack.toml`. `PYREAL_BASE_PACK` names another
base for a run, or `none`. The player's library is `art-packs.txt` in the settings folder (Data
Files, Add Pack); `PYREAL_ART_PACK` names a pack for one run (a test, a run from the checkout).
`cargo run --release -p pyreal-art -- open <pack>` says what a pack holds and how long it takes to
open.

**The engine.** `tools/engine-view` (`pyreal-engine-view`) draws a place with the engine alone,
to a PNG or in a window, and prints each pass's GPU time: the quickest way to work on the
renderer. The client hands the engine its world through `pyreal-scene`'s bridge; a headless client
run with `--report <line or file>` stands where a report says (`--size WxH` sets the frame) and its
log ends with the frame times.

**A run that leaves your settings alone.** `PYREAL_SETTINGS_DIR=<folder>` gives a run a settings
folder of its own: it imports nothing from the old Dereth folders and writes nothing outside it.
`--front-door` opens a headless run (`--headless --capture <png>`) on the world list, as the app
opens.

**Sessions in process.** The app starts and ends sessions in one process
(`App::begin_session`, `App::end_session`); `pyreal/testkit/tests/dat/shell/relogin.rs` logs in
and out repeatedly over socket-free endpoints and is the test to run after touching either.

## Commits

- The code is rustfmt-clean (`cargo fmt --all`).
- A behaviour change comes with the test that pins it.
- A change players notice adds its line to the Unreleased section of [CHANGELOG.md](CHANGELOG.md).

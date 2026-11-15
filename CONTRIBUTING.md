# Contributing

## Checks

| When | Command |
|---|---|
| While working | `cargo xtask check` |
| Before pushing | `cargo xtask ci prepush` |
| With your game data | `cargo xtask ci tier1` |
| On a graphics device | `cargo xtask ci tier2` |

`ci prepush` runs every check that needs no data, cheapest first: the rules, the docs, clippy, the
build and the tests. CI runs nothing on a Mac, so this is the Mac's run. `cargo xtask install-hook`
makes git run it before each push, and skip it when it already passed on the commit being pushed
with nothing uncommitted. `git push --no-verify` skips it. `--list` shows what it would run.

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
- `PYREAL_TEST_LAUNCH_FIXTURE`: the hash-pinned manifest of the 1999 dats, for the Classic
  interface's Launch-world art check (`cargo xtask ci tier1`).

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

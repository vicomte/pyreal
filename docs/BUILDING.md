# Building and running Pyreal

How to build Pyreal from source and run it from the checkout. [CONTRIBUTING.md](../CONTRIBUTING.md)
covers the checks, the tests and their game data, and [RELEASING.md](../RELEASING.md) how a
release is cut.

## Build and run

You need Rust 1.98 or newer. On a Mac, the Xcode command-line tools; on Linux, `libasound2-dev` and
`pkg-config`; on Windows, MSVC.

```bash
cargo build --workspace
```

```bash
cargo run -p pyreal-client
```

The app opens to its world list. Add a world (a name, a host and a port), sign in with an account
(the password can be kept in the Keychain, Credential Manager or the Secret Service), pick a
character and play; logging out goes back to the list. `-a <account> -v <password> -h <host>`
connects straight away, for development. `--dat-dir <dir>` names the folder of game data files.

`python3 build.py` builds `dist/<commit>/Pyreal.app`, signed and checked, from the settings in
`build.ini` (copy `build.ini.example`). Releases are built by GitHub Actions from a version tag.

`cargo run --release -p pyreal-engine-view -- --dat-dir <dir> --window` opens the engine's own
viewer on a place, with a free camera. `--help` lists the place, the time, the art pack and every
switch. A headless client run can stand where an `@report` line says and capture it:
`pyreal --headless --no-ui --report <file> --capture <png>`.

`pyreal --explore [place]` flies a free camera through the world with no server, from `@loc`'s
text, a report line or its `camera` field, or over Holtburg with none. The world list's Explore
panel does the same for each era's files. C copies the camera's place and R its report line.

The client keeps the last 30 seconds of frame timings. A frame longer than 100 ms (`--hitch-ms
<ms>` or `PYREAL_HITCH_MS` sets another, 0 none) is saved a second later as a hitch report in the
settings folder's `logs/hitches`, `hitch-YYYYMMDD-HHMMSS.txt`, at most one every ten seconds.
Explore's Save Timings button or F9 saves the last 30 seconds as `frames-YYYYMMDD-HHMMSS.txt`, and
its Open Folder button opens the folder; in a world the Save Frame Timings and Open Frame Timings
Folder keys (unbound until chosen) do the same. Each report is a small text file: why it was saved,
the client and device, the slowest frames taken apart (the frame loop's steps, the streaming and its
parts, uploads, ray tracing, pipelines, waiting on the device, what was made, the streaming's queues,
the camera's place and speed), then every frame as a table. `PYREAL_EXPLORE_FLY=<m/s>` flies
explore's camera straight on, up to 1 km/s, for measuring.

## Game data

The game's data files are never part of this repository, and no release carries them. Pyreal
reads three sets:

| Era | Files |
|---|---|
| October 1999 launch | `portal.dat`, `cell.dat` |
| February 2005 | `portal.dat`, `cell.dat` |
| End of Retail | `client_portal.dat`, `client_cell_1.dat`, `client_local_English.dat`, and `client_highres.dat` when present |

[ERAS.md](ERAS.md) says how the older eras are drawn.

## Platforms

The Mac is the main platform. Linux and Windows are kept building and are less tested. There is
no web build. A release runs every no-data test on Linux, clippy and the tests of the crates with
Windows code on Windows, and builds, packages and starts the client on each system. The Mac's
build, tests and clippy run on the Mac before each push: `cargo xtask ci prepush`, which
`cargo xtask install-hook` has git run for you ([CONTRIBUTING.md](../CONTRIBUTING.md)).

Lamp shadows and bounce light are traced with ray queries where the device has them. On Direct3D 12
the shaders are compiled by DXC, which the Windows build links in. If a driver draws them wrongly
or crashes, `PYREAL_NO_RAY_QUERIES=1` turns them off for the run, and the lamps are drawn without
shadows. If the graphics driver resets the card mid-game (a Windows TDR), Pyreal logs the reason,
closes and says what happened.

## Art packs and graphics settings

**Art packs.** The app carries the Enhanced Environment as its base pack. Add another pack's `.dat`
on the world list's Data Files panel (Add Pack) and choose it in Settings, Display (Selected Art
Pack), which switches at once; it is drawn over the base pack. A world's own served records (its
data patch) are kept per world in the cache folder and lie under the packs: retail, then the
world's records, then the base pack, then the one chosen.

**Graphics settings.** Settings, Display: the quality preset (Low, Medium, High or Ultra) sets the
rows below it, each of which switches one part of the engine: sun shadows, bounce light, ambient
occlusion, anti-aliasing, depth mapping, ground cover and its distance, the render scale (a share
of the screen's pixels, or Dynamic, which holds a chosen frame rate and rebuilds the picture with
MetalFX on a Mac), VSync and HDR Output. Each row's tooltip says what it does and what it costs.

## Where Pyreal keeps its settings

Everything the player saves (`UserPreferences.ini`, key maps, UI layouts, screenshots, journals
and `worlds.json`, the world list) lives in one folder:

| Platform | Folder |
|---|---|
| macOS | `~/Library/Application Support/Pyreal` |
| Windows | `%APPDATA%\Pyreal` |
| Linux | `$XDG_CONFIG_HOME/pyreal`, or `~/.config/pyreal` |

On the first run the old Dereth client's folder (`Dereth/client` beside it) is copied in, never
moved, and the old launcher's worlds, accounts and data folders are imported from
`Dereth/launcher/launcher-state.json`, which is only read. Saved passwords are found under the old
launcher's Keychain entries and kept as Pyreal's the first time each is used.

These win over the default, strongest first:

- `-prefs <file>` moves everything to that file's folder.
- `PYREAL_SETTINGS_DIR` names the folder outright (`DERETH_SETTINGS_DIR`, its old name, is read
  when it is unset).
- A `UserPreferences.ini` in the working directory makes a portable install.

A run that fails, panics or crashes leaves a log in `crash-logs/` in the settings folder, with the
account, password and ticket hidden.

## Layout

One Cargo workspace.

| Folder | What it holds |
|---|---|
| `core/` | The shared crates: the data files, the engines, the wire protocol, the game rules and the client SDK. |
| `pyreal/` | The client (`pyreal/client`), with its crates under `pyreal/client/crates/`: the engine, the launch logic, Horizon and the rest. |
| `tools/` | The `xtask` task runner, the art pack tool, the engine's viewer and the GPU probe ([tools/README.md](../tools/README.md)). |
| `fixtures/` | Recorded sessions and other shared test data. |
| `docs/` | Reference for maintaining the code: the data formats, the networking, the plugins and the rest ([docs/README.md](README.md)). |

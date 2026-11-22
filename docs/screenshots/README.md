# Screenshots

The pictures in the README and on pyrealclient.app come from this library. Every shot is defined
in [shots.json](shots.json) and drawn again from it by one command, so the whole set can be
refreshed whenever the art or the renderer changes.

Each shot is drawn twice at 3840 by 2160 from the same camera at the same time of day, with no
interface. On the left is the retail look, with no art pack and every Pyreal addition turned off.
On the right is Pyreal on its Ultra preset, at a fixed exposure chosen for the shot.

<!-- shots: begin -->
| Shot | Retail | Pyreal | Side by side |
|---|---|---|---|
| A forest in the morning (`forest`) | [retail](forest-retail.jpg) | [Pyreal](forest-pyreal.jpg) | [both](forest-compare.jpg) |
| A knight in Covenant armour fighting a Diamond Golem under the dawn sunbeams (`forest-god-rays`) | [retail](forest-god-rays-retail.jpg) | [Pyreal](forest-god-rays-pyreal.jpg) | [both](forest-god-rays-compare.jpg) |
| Wai Jhou's pagoda over its lagoon in the morning (`wai-jhou`) | [retail](wai-jhou-retail.jpg) | [Pyreal](wai-jhou-pyreal.jpg) | [both](wai-jhou-compare.jpg) |
| The sea off the shore near Stoneport at dawn (`stoneport-shore`) | [retail](stoneport-shore-retail.jpg) | [Pyreal](stoneport-shore-pyreal.jpg) | [both](stoneport-shore-compare.jpg) |
| Three fountains before a manor at dawn (`manor-fountains`) | [retail](manor-fountains-retail.jpg) | [Pyreal](manor-fountains-pyreal.jpg) | [both](manor-fountains-compare.jpg) |
| Ice trees near Fiun Outpost at noon (`fiun-outpost`) | [retail](fiun-outpost-retail.jpg) | [Pyreal](fiun-outpost-pyreal.jpg) | [both](fiun-outpost-compare.jpg) |
| Snow in the Mid North Mountains, a runner chased by a Hoary Mattekar and leaving footprints (`north-mountains-footprints`) | [retail](north-mountains-footprints-retail.jpg) | [Pyreal](north-mountains-footprints-pyreal.jpg) | [both](north-mountains-footprints-compare.jpg) |
| The desert outside Yaraq at Morntide, with a Water Golem (`yaraq-creatures`) | [retail](yaraq-creatures-retail.jpg) | [Pyreal](yaraq-creatures-pyreal.jpg) | [both](yaraq-creatures-compare.jpg) |
| A robed mage's Lightning Bolt flying at a Virindi Inquisitor on the Obsidian Plains (`obsidian-plains-battle`) | [retail](obsidian-plains-battle-retail.jpg) | [Pyreal](obsidian-plains-battle-pyreal.jpg) | [both](obsidian-plains-battle-compare.jpg) |
| Caulcano's lava at midnight, with fire elementals, a Magma Golem and a volcanic vent (`caulcano-creatures`) | [retail](caulcano-creatures-retail.jpg) | [Pyreal](caulcano-creatures-pyreal.jpg) | [both](caulcano-creatures-compare.jpg) |
<!-- shots: end -->

## Drawing the library

```bash
tools/screenshots/capture.py
```

draws every shot and then rewrites the README's pictures and the table above. Name slugs to draw
only those shots, give `--docs` to rewrite only the README and the table, or give `--draft DIR` to
draw a smaller copy somewhere else while staging a shot. The tool needs the viewer built in the
test-release profile (and the client, for a shot drawn by it), the base pack, ImageMagick, and the
data folders:

```bash
cargo build --profile test-release -p pyreal-engine-view
cargo xtask base-pack build
export PYREAL_TEST_DAT_DIR="<the End of Retail data folder>"
```

Each side is saved as a JPEG at quality 90 (lowered only to stay under 3 MB). `<slug>-compare.jpg`
puts both at 1920 by 1080 side by side, labelled, retail on the left.

## The two sides

**Retail** (`<slug>-retail.jpg`) is Pyreal's engine viewer, `engine-view`, drawing the data files
with nothing added. It is a close stand-in for the original client, not a capture of it. It uses
the client's own light (`--retail`), no art pack, and every Pyreal addition off: depth and surface
detail, shadows, ambient occlusion, bounce light, ground cover, lamp rays, the water surface,
volumetric clouds, silk cobwebs, drawn smoke and creature volumes, face detail, and Pyreal's weather
(its rain and snow, fresh snow cover and footprints). Outdoors it
draws no lamp light, because the viewer has no copy of the original's lamp light on the land.

**Pyreal** (`<slug>-pyreal.jpg`) is `engine-view` over the base pack, as near the Ultra preset as
the viewer goes, with every model at its finest level. The viewer loads the whole landscape out to
the Horizon draw distance before it draws, where the client run headless saves its picture before
the far land has come in. It also places creatures and characters, which the client cannot do
without a server. A shot can still ask for the headless client with `"pyreal": {"renderer":
"client"}`.

## A shot's entry

`shots.json` holds a list of shots. Each one has:

- `slug`: the files' name, `<slug>-retail.jpg`, `<slug>-pyreal.jpg` and `<slug>-compare.jpg`.
- `name`, `caption` and `alt`: a short name, a one-line caption, and the README picture's
  description.
- `order`: where it comes in the table and on the site.
- `site`: whether the site shows it. `site_top` marks the one the site opens with.
- `readme`: which picture the README shows at the shot's `<!-- shot: slug -->` marker
  (`compare` or `pyreal`), or `null` for none.
- `world`: `eor`, `2005` or `launch`.
- `camera`: the landblock, cell, position, yaw and pitch (the client's radians) and horizontal
  field of view. `player` is where the player stands, when the shot came from a report line.
- `time`, `clock` and `weather`: the time of day, the game clock (which picks the day's sky) and
  `clear`, `rain`, `storm`, `snow` or `blizzard`.
- `scene`: what is staged. `bodies` are creatures and characters, each placed `ahead` of and
  `right` of the camera (or `at` a block-local spot), with its `facing` (a compass heading,
  `camera`, or `{"toward": name}`), its `stance`, `motion` and `time` into it, its `dress`,
  `wear` and `translucency` as its weenie gives them, a `run` speed or `stride` that it ends at its place moving at, a `lift` over the ground,
  and `scripts` played on it. `objects` are what a server would place there (a volcanic vent), each a
  `setup` at a `block` and its block-local `x` and `y`. `frames` is how many frames are drawn before
  the picture.
- `exposure`: the Pyreal side's fixed exposure. No shot uses the eye's adaptation, so a dark scene
  is not caught still adjusting and a picture does not change with what the camera sees. The retail
  side uses the client's own fixed exposure.
- `retail` and `pyreal`: extra flags for one side; `pyreal.renderer` picks `client` over the default
  `engine-view`.
- `captured`: when the shot was last drawn, from which commit, with which base pack.

To add a shot, copy a report line in the client and run:

```bash
tools/screenshots/capture.py add <slug> "<the report line>"
```

It prints an entry with the camera, the time and the weather, its clock moved to a clear day,
ready to fill in and add to `shots.json`.

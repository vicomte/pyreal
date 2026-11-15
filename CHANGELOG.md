# Changelog

Every release of Pyreal, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the versions follow [Semantic Versioning](https://semver.org/). While the version is below
1.0.0, a minor version (0.2.0) may change settings, files or behaviour, and a patch version
(0.1.1) only fixes things. [RELEASING.md](RELEASING.md) says how a version is cut.

## [Unreleased]

## [0.1.2] - 2026-11-15

### Added

- Character creation has a Random button beside the name. It makes a name that suits the chosen heritage, following the game's own naming help.
- Ctrl+Shift+P (Command+Shift+P on a Mac) shows or hides the performance panel on every screen, including the sign-in and loading screens and explore. It does nothing while you type in a text box, and the keys list lets you change it.
- Explore mode has the game's Map and Settings buttons in the lower right, and the game's own keys for them, as bound. A click on the map moves the camera there.
- The Explore panel sets the cloud cover. Its Fog row is gone because Settings has it.
- Waypoints. Double-click the map, type `/way 41.6N 33.9E` with an optional name (`/way clear` clears it), or press Set Waypoint on a Journal page with a place. An arrow on the radar's rim points the way, and the Waypoints setting can also lay a ring with an arrow on the ground around your character, overland only. A Clear Waypoint button stands under the radar, and the waypoint clears when you come within 10 m of it. Each character keeps its own waypoint in each world.
- Portals in the world are drawn as a spiralling funnel of their own colour that always faces you, with arms of different widths, lengths and speeds, and they light the ground round them in that colour. Dust from round a portal drifts toward it, speeds up as it nears and turns purple as it enters. Every era's portals of every colour are drawn this way.
- A frame longer than 100 ms is saved as a hitch report in the settings folder's `logs/hitches`, with the 5 seconds before it and the second after. Explore's Save Timings button or F9 saves the last 30 seconds on request, and Open Folder opens the folder. `--hitch-ms` sets the threshold.

### Changed

- The world list's Explore panel shows one Explore button when only the End of Retail files are installed, a button for each era installed otherwise, and nothing with no data files.

### Fixed

- The small stones and pebbles lying among the grass, on sand and dirt and along roads have a rock texture. They were drawn in one flat colour.
- The beacons on top of the Empyrean lighthouses, Tou-Tou's and six others, shine as a giant white light. A star of light burns at the heart, a shaft of light rises from it into the sky, white wisps rise round it and it lights the top of the tower. They were drawn as a grey cloud blown off the tower.
- Braziers, the fires on top of beacon towers and other open fires burn as rolling fire shaped like the original game's, with a yellow heart cooling to red as it rises. They were drawn as a row of small flames.
- Flying fast no longer reloads the world every few blocks. Explore's fastest speed is now 1 km/s, down from 5 km/s, which the land keeps up with.
- A long Landscape Draw Distance no longer stops the world for two seconds at a time in fast flight. The far terrain already drawn serves the new square while a new one is made, the square is loaded ahead of a fast camera, and the far trees' work is kept small and drops what has been flown past.

## [0.1.1] - 2026-11-14

### Added

- Art packs made for Dereth read in Pyreal: a `[[cover]]` table grows as ground cover, a ground's `detail` picture lays a fine grain over it near the eye (`detail_size`, `detail_strength`), and `pebbles = false` keeps the pebbles and stones off its grounds.
- An art pack's models are drawn for objects the server makes too: a thing (not a creature, nor held) whose setup the pack has a model for is drawn as that model, and a part whose graphics object it has one for is drawn as that model.
- The map reads a world's places from the place store its status names, so Empyrean worlds list their portals, towns and dungeons, and admins also get the detailed marks. A world with no place store, such as an ACE server, uses Pyreal's built-in list for its era.
- Explore flies a free camera over Dereth with no server, from the world list for each era's files or with `--explore [place]`. It keeps above the ground, flies from 2 m/s to 1 km/s, sets the hour, the weather and the fog, and copies the camera's place or its report line.
- Art packs can draw a creature or character as a skinned body of their own (`[[skin]]`, with `[keep]`), played by clips for what the creature is doing or following its parts as the game animates them, and `pyreal-art skeleton` writes a setup as a body to rig against.

### Fixed

- Create stays available on character select after logging out of one world and into another.
- A world whose data files were changed (another set chosen for it, or the same set installed again) no longer refuses them. Its old overlay is thrown away and the world patches the files afresh.
- A command line that does not parse says what was wrong under the game's own message.

## 0.1.0 - 2026-11-14

Pyreal's first version. It lists everything changed since Pyreal was forked from
[Dereth](https://github.com/dereth-network/dereth) on 2026-11-09, starting from Dereth's combined
build. Everything Dereth's client already did at that point came with the fork and is Dereth's
work.

### The fork

- Pyreal is cut loose from Dereth's network and releases: it reports nothing to Dereth's hub and downloads no clients or updates from Dereth's releases.
- The repository holds the Asheron's Call client alone. Dereth's server (Empyrean), the web client, the Tauri launcher and the Asheron's Call 2 parts are removed.
- Horizon is the only interface.
- The crates are renamed `pyreal-*`, the program is `pyreal`, the product folder is `pyreal/`, and the `DERETH_*` variables are `PYREAL_*` (the old names of the settings, art pack and test variables are still read).
- Pyreal keeps its settings in a Pyreal folder, and on its first run copies the old Dereth client's settings and imports the old launcher's worlds, accounts and data folders.
- Pyreal has its own icon, a struck coin with a crest in relief, and a personal build can strike its own emblem into the coin.
- Pyreal has its own version, starting at 0.1.0, and its own release tags (`v0.1.0`).

### Rendering engine

- A new renderer, the engine, draws the world on wgpu: the game's assets in GPU pools, a bindless material table, GPU culling and deferred passes.
- The old renderer, its replay device and the old scene are removed. wgpu is the only graphics device: Metal on a Mac, Vulkan or Direct3D 12 on Linux and Windows, with no MoltenVK.
- Enhanced graphics are always on. The Graphics menu, Keep Retail Look, the renderer choice and the settings only the Standard look used are gone.
- Rooms and dungeons are drawn through a portal walk, and the ground is cut away inside a room.
- A room's portal walk carries on through the outdoors into other buildings' rooms, so joined buildings and rooms seen across a street draw in place.
- Far terrain is drawn as rings of growing coarseness out to the chosen draw distance.
- Objects hidden behind the ground are culled against a depth pyramid.
- Each part's level of detail is chosen from its distance, its size and the field of view.
- Translucent parts are drawn far to near, each object whole, and fade through a screen-door pattern that the temporal resolve smooths.
- Light is worked in physical units with auto-exposure, bloom and an AgX tone curve.
- Bloom is a soft glow round what is brighter than white, so lamps, flames, crystals and lava glow without a haze over the scene.
- The sky is a physical sky, with the air's scattering and the heavens drawn over it.
- The sun casts shadows from four stable cascades, and Shadow Distance (Short, Medium, Long or Very Long) chooses how far they reach.
- Tree shadows fade in toward the shadow distance.
- Lamps light each pixel through light clusters, and the nearest lamps' shadows are traced with ray queries where the device has them.
- Ambient occlusion and ray-traced bounce light, both steadied so they no longer crawl or flicker.
- Render Scale draws the world at a share of the screen's pixels, or at Dynamic, which holds a chosen frame rate. MetalFX rebuilds the picture on a Mac, and a temporal resolve with sharpening rebuilds it elsewhere.
- Particles are stepped by the client's own driver and drawn as instanced cards, lit like the translucent surfaces.
- The selected object gets an outline and a target ring, and a hovered or selected object brightens by its own shading.
- Only the current selection is outlined, so Escape's deselection takes the outline away.
- The land streams in block by block round the camera, with pictures decoded ahead on worker threads.
- Land freed while portal space is on screen keeps the destination's ground, so a teleport no longer lands on a bare plain with objects standing on nothing.
- HDR Output draws to the display's extended range where the display offers it, and the saved setting applies at start-up.
- Object Detail (Low, Medium, High or Ultra) pushes level-of-detail changes out and crossfades them.
- Trees change detail smoothly: near casters crossfade, far trees turn into cards before the edge of the loaded land, and the cards stand on the far terrain.
- Three draw distances past Extreme: Distant, Far and Horizon, reaching about 24 km of far terrain.
- Landscape Smoothing is back from the old client as a strength menu (Off, Low, Medium or High). Physics stays on the game's own triangles.
- Depth Mapping (Off, Low, Medium or High) gives surfaces relief: parallax with self-shadowing, ground blended by height and cliffs stood up.
- The terrain follows Depth Mapping, End of Retail's road gets relief worked out from its own picture, and every End of Retail ground has relief.
- Every surface reflects its surroundings by its roughness, and metal mirrors the sky above it.
- A polygon corner the data files bent toward a neighbour takes its face's normal, which ends the dark ellipses beside dungeon torches.
- Luminous surfaces light themselves as the original game drew them. Only see-through and additive ones glow past white.
- Molten rock glows from its hot texels day and night, pulsing slowly.
- Glacier ice is thick translucent ice that glows blue at night.
- Dungeon cobwebs are spun as fine silk with cross-threads and a faint gauze. Silk Cobwebs turns them back into the original cards.
- Torches, sconces, candles and braziers burn with flames drawn in code, each flickering with its lamp.
- Lighting is tuned toward the original game: sealed rooms and dungeons as bright as retail drew them, lamps summed in the original colours and kept warm, sunlit sand pale and bright, and a low sun dimming with the sky at dusk and dawn.
- The night is a dark blue-grey with a hint of purple, with warm lamp light on the streets.
- Portals' and spells' lights stay in a pool round them, and glowing creatures cast a soft light of their own colour.
- Wisps, rifts and the Virindi energy clusters draw as hot cores in soft halos, each carrying a lamp of its colour.
- Creatures of smoke, dust, steam, frost and acid are drawn as one shared ray-marched volume, lit and self-shadowed. Dark Vapor and the Shadow Vortex whirl into a dark eye.
- A storm creature's lightning grows jagged forking strikes that light its cloud from within and stays cheap while a Plasma Golem walks.
- Creatures of fire are drawn as flames on their parts, each lit by its own flickering lamp.
- Frost elementals are snow and ice, with a cold blue heart under white rime. Their snow is a full body at their size, and their glow no longer pools on the ground.
- Golems look like what they are made of: metal golems shine and mirror the sky, ice and gem golems refract in their own colour, water golems are rippling water, blood golems drip, obsidian shines and mud golems are glossy wet mud. Magma golems glow.
- Creatures keep their own effects when they cast, are struck or are summoned.
- Shadows are see-through in their own bodies with their armour solid, fray at the edges as smoke, pool darkness at their feet and have glowing eyes.
- Statics and scenery play their default animation, so the sea's gulls wheel again.
- A server object's particles and fades run every frame, so a portal's swirl shows again.
- Volumetric clouds return under Cloud Detail (Low, Medium or High), with shadows on the land, drifting at a believable pace.
- The weather drives the clouds: rain closes the sky over and a storm darkens and lowers it.
- `/clouds` sets the clouds' cover, speed, height, depth, size, detail, light, shade and shadows.
- God Rays show sunlight in the air in shafts through gaps in leaves and roofs, most with a low sun.
- Fog Density (Off to Heavy, Clear by default) and `/fog` set the haze.
- The moons from an art pack are round worlds with their own phases, lights on their night sides, a cloud shell and relief. The data files' moons are drawn as painted, and a moon's disc hides the stars behind it.
- Setting the time with `@tod` or `@day` shows the new hour at once.
- High-Res Textures draws `client_highres.dat` whenever it is installed. It is on by default.
- Ground Coloring, with the Classic forest floor.

### Water, weather and ground cover

- Water is a surface and pass of its own, with waves that rise and fall, a sea swell that turns in to the shore and breaks into surf, and a swash that runs up every shore in time with the waves. Water Detail sets how much of this is drawn.
- Lake shores curve along the water's edge, and dry hills no longer show water through them.
- The open sea darkens to navy offshore, and far water is shaded as the near water with no band at the horizon.
- Lakes, rivers and swamps lap slowly and gently, with a pond's short chop, and rivers flow.
- Bodies wade where water lies at the ground's height, and froth and a wake gather round them.
- Rain keeps off whatever stands under a roof and leaves puddles on open ground.
- Storms have a steady slant of rain, swaying trees and cover, a darker sky and lightning.
- Snow falls where rain would fall over snowy land, in fluttering flakes of many sizes, and a storm over snowy land is a blizzard. The original client's endless snowfall over snowy land is gone.
- Fresh snow lies on open ground, roofs and rocks while it snows and melts away after.
- Footprints are left in snowy ground by everyone walking there, and falling snow fills them in.
- Ground cover is placed on the GPU, tinted toward the ground under it, thinned toward roads, building walls and objects, and grows only on the ground its kind lists.
- Sparse wildflowers and pebbles grow among the cover, with a ground-tinted grass blade layer and Grass Height.
- Gusts of wind travel across the fields, each plant taking them a moment late with its own stiffness, and trees lean and sway.

### Art packs and the built-in pack

- Pyreal carries its own base art pack (the Enhanced Environment), pinned by `base-pack.toml`, and draws the player's chosen pack over it. Built-in draws the base pack alone.
- The base pack is Enhanced Environment 0.2.0, packed by the pack repository's own packer.
- The art pack library lives on the Data Files panel (Add Pack, each pack with its size and Remove), and Selected Art Pack switches packs at once.
- A world's served records (its data patch) lie under the packs and reach every reader, the spell table and the HUD's tables among them.
- Art packs of manifest format 2 are read, and packs are stored compressed record by record, so the base pack opens in about a third of a second.
- The start-up packs open on a thread, and the world list's first frame comes in about half the time.
- Pack materials say what they are made of (metal, gloss, clear, refraction, wet), so creatures' looks live in the art pack.
- Pack entries are matched to the pictures they were made from, so 1999 and 2005 worlds draw only entries made from their own era's pictures.
- Older eras get their own effects, materials chosen by palette, sharpened ground, a new Launch ground and relief for older dungeon pictures.

### Eras

- Pyreal reads three sets of data files: the October 1999 launch, February 2005 and End of Retail. Other dates are refused.
- The 1999 launch files get a folder of their own. A Launch world signs in with the 1999 files and a 2005 world with the 2005 files.
- A world added by hand takes the era its status reply names, so a 2005 Empyrean world starts as Infiltration.
- The character creator offers only the era's heritages and skills, and sends a custom character as the template it most resembles.
- Community Presets in the character creator for Launch, Infiltration and End of Retail, with the world's own templates and Custom on the Profession page.
- The February 2005 creation tables are read.
- The map's places are right for each era, with far more for administrators and the world's own when it publishes them. The landmarks are checked against the community wiki.

### Interface (Horizon) and the front door

- The launcher lives in the client: the app opens to Horizon's world list, with sign-in, data files and settings before any world.
- Sessions start and end in one process, so logging out goes back to the world list.
- Saved passwords are kept in the Keychain, Credential Manager or the Secret Service, and show as dots on the sign-in panel.
- A password is kept only once the server accepts it, and an imported world's old password is forgotten with the world.
- `pyreal://` links open on the Mac, at launch and while running, and a join link adds its world at start-up.
- The screens before the world float portal space's violet lights over its slate void, with the world list's buttons in its panel and the sign-in pinned to the right.
- Portal space is flown through as a tunnel in the original's slate blue with violet lights, drawn by a thread of its own and sharp at 4K. Portal Style chooses Original or Enhanced.
- Portal space ends as the original's does, and a stale position from the server no longer ends it early.
- Settings, Display is grouped into Quality, Screen & Window, Lighting & Shadows, Sky & Weather, Surfaces & Detail and Information, each row with a tooltip saying what it does and what it costs.
- The quality presets set Sun Shadows and Terrain Draw Distance too, and Model Detail Distance, which did nothing, is gone.
- The lamps, the physical sky, the water, the slope materials and the damp are always drawn, so their boxes are gone.
- Every world draws with its own look, so Terrain, Sky and Object Mode are gone.
- The window opens where it stood, at its saved size and full screen when it was. On a Mac the window's own button makes it full screen.
- A Mac window restored at its saved size keeps the interface at the right scale.
- On a Mac, Command takes the place of Control for shortcuts.
- The vitals stand at the top centre of the HUD and the EXP bar under the hotbars, as wide as them.
- Status Bar Style shows a vital as its name and what remains, or what remains of its maximum. Large numbers are shortened to thousands, millions or billions.
- Hotbar Rows (1 to 4) and Spell Bar Rows (1 to 5), each row with its own keys and each slot showing its key.
- Each spell keeps the spell bar slot it is dropped on, per world, character and tab.
- The stance button shows each stance's glyph, green in peace and red in a fighting stance.
- The Plugins screen lists installed plugins, installs them from a file or folder, and manages plugin sources.
- The world map is far more detailed, and an administrator's map marks where they stand.
- Administrators have an Admin Panel with Find and Spawn, a Places filter whose rows teleport, and a Teleport button on the map.
- The world list reorders with up and down arrows, lists Known Worlds from Dereth's directory and the community list, and Save waits for a name, host and port.
- Character Select reads Enter with the character's name, and Delete and Remove are red.
- Settings opened from the world list takes its own clicks, and Data Files and Plugins close with one Done button.
- The Data Files panel wraps its warnings and status inside the panel.
- Train asks first, the fellowship list scrolls, trade lights follow the trade, split stacks sell and trade as split, and party bars stay live.
- The Loot window, shop costs and plurals, Sell Item splits, the spellbook's search and school filter, unusable skills, EXP at the cap, allegiance rank and tooltips are fixed.
- Friends, Vassals, Squelches, Contracts and the Loot grid scroll and clip.
- Buy and Add to List take the stack size shown after a purchase or on reopening a vendor.
- Sell Item takes a split stack by the same rule as every other gesture that splits.
- An examine window closed under an inscription gives the keyboard back.
- The camera never flips or spins, View Combat Target frames the target, and camera input is released at log off.
- International keys, mouse Back and Forward, trackpad wheel scrolling and surrogate pairs work, and PrintScreen fires once.
- Appraisal, chat and date text fixes.
- A tell from a squelched character is drawn as retail draws it, and the server refuses squelched tells.
- Inventory, trade, fellowship, allegiance, object and HUD state fixes.

### Network and Dereth Network

- Login no longer reports the world's player count to Dereth's hub.
- Dereth Network conformance scenarios built from Empyrean's message shapes, with a networking doc.
- Half-received messages no longer pile up until new messages are dropped, and a refused first fragment leaves nothing behind.
- Packets before a connection is set up are checksum-checked and dropped when corrupt.
- Packed lists reserve no more than the bytes left to read, and blobs for undrained queues are dropped.
- A refused data patch record returns to the world list with its reason, records are capped at 64 MB, and repeated ids complete every revision.
- Appearance updates keep a creature's motion, and the server's stance changes are not echoed back.
- Ending a session ends its journey, requests and sightings, and the next session starts clean.
- Port and number switches parse as the original client's did, a port outside 1 to 65535 is refused, and conflicting switches are refused.

### Plugins

- A plugin's chat line never runs a command that opens a file, and plugins may not save dot files.
- A plugin id made of dots names no folder outside the plugins folder.
- A wasm plugin has at most one memory and bounded tables, is charged per instruction, and a bad answer pointer is an error.
- A plugin's task errors are limited per frame, `chat.print` is cut at 4,096 characters, and loot profile patterns are bounded.
- A plugin's recall, allegiance and squelch lines follow the world's policy, and `/die`, `/pklite` and `/sepaku` need a gesture.
- `.met` and `.usd` tables nest at most 100 deep, and plugin places with a non-finite coordinate are refused.
- Failed Lua modules are remembered, hex subscriptions fire, and peers are counted by client.
- The plugin shelf re-reads only changed folders and manifests.
- The plugin template downloads the plugin tool from Pyreal's releases.

### Audio

- Sound Style, Enhanced by default, plays the game's sounds on an engine of Pyreal's own: many more sounds at once, placed round you and following what makes them, sounds behind you a little duller, echoes indoors and smooth looping ambience. Original plays them as the original game did.
- Each volume slider works, Interface Sounds included.
- The world's background sounds, the rain and wind of bad weather and storm thunder play again.
- Sound opens in the device's own format and recovers when the device changes, with fewer clicks, a mono mix and steadier panning.
- A failed sound is remembered, and the movie and MP3 players are sturdier.

### Data files

- Corrupt or hostile data files, pictures and packs are refused before they can hang the client or ask for gigabytes.
- A looping texture chain, an oversized JPEG, an overflowing glTF accessor and an out-of-range manifest value are refused.
- A data file's directory is a sorted list, which halves the End of Retail cell directory's memory.
- BC7 pictures use mode 5 where it holds them closer, and flat colours are exact.
- The data file writer stops at 2 GiB, and a sound claiming under 100 Hz is refused.
- Landscape ids at the far edge of the world no longer wrap into the first block.

### World, physics and simulation

- Sphere collisions and sunk walkable checks follow ACE's, with guards against invalid numbers.
- A route to an interior the outdoor window can't reach ends as disconnected, and a used portal brings its routes back.
- A motion cycle with no animations no longer stalls, and per-tick animation work stops allocating.
- A container inside itself no longer overflows the stack.
- House payment arithmetic wraps without panicking.
- A released world's physics never touches the next session's world.

### Platform, settings and CI

- Pyreal is desktop first. The web client, its relay, its WebSocket endpoints and the wasm32 code paths are removed.
- Windows links DXC into the build for its shaders, so Direct3D 12 gets ray queries. `PYREAL_NO_RAY_QUERIES=1` turns them off for a run.
- A lost graphics device closes the client with a message saying what happened, and GPU errors are logged.
- An adapter that can't hold the texture array is passed over for one that can.
- `tools/gpu-probe` measures what the graphics layer can do on a machine.
- `tools/engine-view` draws a place with the engine alone, to a picture or in a window, with rain, storms, moons and an overlay folder.
- `tools/screenshots/capture.sh` draws a place, with creatures if asked, from a report line as retail beside Pyreal's defaults at 4K. The pictures in `docs/screenshots/` are made with it.
- Pyreal builds and draws on Windows with Direct3D 12 and Vulkan, ray queries included, checked on an Intel Arc GPU.
- A headless client run can stand where a report line says and capture it, and write a frame every N frames.
- The behaviour scenarios for squelched tells, dropping part of a stack and selling a split stack follow the client as it plays, and the jump timing test quotes the test profile's timings.
- `build.py` packages and signs `Pyreal.app` with the base pack and no MoltenVK.
- Settings are saved atomically, Windows-1252 profiles are read and kept, and unreadable settings, key maps, `worlds.json` and `art-packs.txt` are never saved over.
- One bad world in `worlds.json` no longer resets the rest.
- On a Mac the window is placed in points, Windows clocks are read with their offset, and settings stay beside `-prefs`.
- CI runs the build, the tests and clippy on Mac, Windows and Linux, and the rules and docs once, only for a version tag or when run by hand.
- A version tag builds the release packages for Mac, Windows and Linux and publishes them as a GitHub Release.
- The Mac release is a disk image holding `Pyreal.app` beside a link to Applications, signed with a Developer ID under the hardened runtime, notarized by Apple and stapled. The app's identifier is `com.jacobjohnston.Pyreal`.

### Performance

- The front door re-reads a data file's header only when it changes.
- A packed art pack no longer hashes every file it reads when it opens.
- Navigation, BSP sphere walks, the shadow list and the scene bridge's object checks are faster.
- Far terrain pipelines are made once, and the ray scene gathers nearby statics once a frame.
- Startup makes its pipelines side by side on several threads, warms up the ray-tracing builders in the background, and keeps a Vulkan pipeline cache in the settings folder, so a first launch on Windows reaches the world about twice as fast and later launches sooner still. The front door says when shaders are being prepared.

[Unreleased]: https://github.com/jacobjohnston/pyreal/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.1.2
[0.1.1]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.1.1

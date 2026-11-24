# Changelog

Every release of Pyreal, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the versions follow [Semantic Versioning](https://semver.org/). While the version is below
1.0.0, a minor version (0.2.0) may change settings, files or behaviour, and a patch version
(0.1.1) only fixes things. [RELEASING.md](RELEASING.md) says how a version is cut.

## [Unreleased]

## [0.3.0] - 2026-11-24

### Added

- AC1999 is on the Known Worlds list. It plays the game as it was in November 1999 with the 1999 Retail CD files. Pyreal also checks a world you add by hand the first time you play it, and a world that speaks the November 1999 protocol is played that way. On AC1999, your account name and password can have at most 19 characters together. There, @g speaks in the server's general chat, friends are listed by the server, and commands and options the server does not take are left out.
- The tooltips on the main menu show the key that does the same, such as "Character (C)" or "Map (M)". They follow your key bindings, and an item with no key shows only its name. A button that opens a menu names the key of its first item that has one.
- Hide Interface hides the HUD and every window so you can take a clean picture, and brings them back as they were. Its key is Alt+Z. Escape or any window's key shows the interface again, and so does dying. Questions from the game still appear while the interface is hidden.
- Resting the pointer on an item in your packs, a loot window or a vendor's stock shows its details in the tooltip: its value, burden, workmanship, armor or damage, requirements and spells, as Examine shows them. The tooltip says Assessing until the game answers. When you cannot assess the item or the game does not answer, the tooltip says so. After you write an inscription the tooltip shows the new one. The Assess on Hover setting on the Interface page turns this off, and the tooltip then shows details only for items you have already assessed.
- Holding Shift while the details show puts what you wear in the same place beside the item, with each number that is better in green and each that is worse in red. The Compare With Worn Items setting on the Interface page shows it without Shift. Worn items are compared without the spells cast on them, and armor that covers several body parts is compared part by part with every piece it would replace. A weapon is compared with a wielded weapon of the same kind.
- The inventory has a search box. Type part of a name, or a kind such as armor, weapon, gem or salvage, and items that do not match are dimmed in every pack. The packs that hold a match are marked in the pack column.
- Hold Ctrl and right-click an item to mark it Keep or Junk. The mark shows as a gold or grey corner on the item. Pyreal asks before you sell, salvage, trade, give or drop a Keep item, and a vendor's Sell tab has an Add Junk button that puts your Junk items on the list. Add Junk keeps back anything attuned, retained, inscribed, imbued, tinkered, rare, worn, of no value or refused by that vendor, and its tooltip says why. Marks are kept for each character and stay with the item they were put on.
- When you die and drop items, the place where the game laid your corpse is marked with a headstone on your map and on the edge of your radar. Nothing is marked when you keep all your items. If you died outdoors while playing on another computer, that place is marked when you next log in. Click the headstone, or the Set Waypoint to Corpse button under the radar, to set your waypoint there. The marker goes away when you empty your corpse, when you see it decay, when you come back and the corpse is gone, or when you right click the headstone.
- Places written in chat, such as 12.3N, 45.6E, are links. Click one to set your waypoint there.
- The map has a Share Waypoint button that puts your waypoint's name and place on the chat line as plain text, so players on any client can read it.
- Beneficial effects in their last minute show as their spell icons with a countdown under the effect icons, and a notice at the top of the screen says which one is ending. Harmful effects show there while they last. An effect that a stronger spell of the same kind overrides, such as Armor Self V under Armor Self VI, is left out. The Effects Ending Soon setting on the Interface page turns this off or shows effects from 30 seconds to five minutes before they end. HUD Layout moves the icons.
- The Journal has Pin buttons for notebook pages, contracts and quest timers. Pinned things show in the Quest Tracker under the radar with the time left on each timer. A notice says when a pinned timer is ready, and at login another says when your house's rent is due. Click a row to open it in the Journal, or right-click it to unpin it. The Quest Tracker can be moved in HUD Layout, and pins are kept for each character.
- Damage you deal rises as a number over what you hit, and damage you take and healing rise over your character. The numbers are large and bold with a dark edge. Critical hits are larger and gold, evades and resists show as words, and the rest of your damage takes the colour of its type. A creature you kill shows Killed over it. Damage over time that lands on one body together shows as one smaller number with the ticks added up. The numbers fade with distance and hide behind walls. The Combat Numbers settings on the Interface page turn them off, or turn off the damage you deal, the damage you take or healing on their own. Melee and missile numbers still show when combat messages are squelched in chat, and health lost or gained to another's life transfer spell shows too.
- When your health falls under a quarter, the edges of the screen turn red and the health bar glows. Both pulse like a heartbeat that quickens as your health falls, and they fade when you heal or die. The red edges stay when the interface is hidden. The Low Health Warning setting on the Interface page turns it off or sets it to a fifth, a quarter or a third of your health.
- The Journal has a Deaths tab that names what killed you and lists the hits you took in the last 15 seconds: who dealt each one, how much, its type, whether it was critical, and how long before your death it came. It also shows your health over those seconds, and healing, drains, evades and resisted spells are listed with the hits. Under the list it says which items you lost, or that you kept them all, once the game has said, and your vitae penalty. The blow that killed you is listed with the health you had left and the damage type the death message names. The tab keeps your last 10 deaths until you log out, the newest at the top, and clicking one shows it. The tab's Set Waypoint to Corpse button sets your waypoint to your newest corpse outside dungeons. Bind the Death Journal key to open the Journal at Deaths with one press. Turn on Open Journal on Death on the Interface page to have the Journal open at Deaths when you die.
- Gamepad Mode reads controllers on a Mac, such as a PS5 DualSense connected by USB.
- With a PlayStation controller, Gamepad Mode shows its own buttons. The cross hotbars show the cross, circle, square and triangle in their colours, and the Set to Hotbar notice names L2 and R2. Other controllers show the Xbox buttons as before.
- The Graphics page has a Resolution setting again. In a window it lists the common sizes that fit your screen, from 1024 × 768 to 3840 × 2160, and picking one resizes the window at once. Dragging the window's edges still sets any size, and the setting then shows it as Custom. Pyreal remembers the window's size, and leaving full screen goes back to it. In full screen the setting shows the screen's own size, and Render Scale sets how much detail the game draws there.

### Changed

- The Enhanced Environment pack is 0.5.4. More walls, floors and ceilings take relief that follows their pictures, with brick courses, stones and slabs traced again and top beams standing out. Crystals, gems, pottery, bottles, chests and keys look like what they are made of. The Life Stones, the crystal clusters, the Obsidian Plain, the crystal shards of every colour and the cistern pillars are clear crystal that bends what lies behind it. Bottles and flasks draw their corks, labels, bands and foil solid, and the Bottle of Crystal Champagne has a new Dom Pyrignon label. In End of Retail, the giant elm, the bare dead trees, the red clump trees and the copies of the pack's trees and plants draw with the pack's models. Banyans grow smooth trunks, limbs and roots, and the hay piles are lumpy straw heaps.
- AI fellows, shared waypoints and journal notes need an Empyrean world from this release or later.
- The Display page in Settings is split into an Interface page and a Graphics page. The Interface page holds Interface Scale, UI Blur and the settings for windows and the HUD, such as Status Bar Style, Low Health Warning and Combat Numbers. The Graphics page starts with the screen's settings, Resolution, Full Screen, VSync, HDR Output, Brightness and Field of View, then Quality Preset with Render Scale and Anti-Aliasing, then the lighting, sky and surface settings. Each page's Defaults button puts back only that page's settings, and on the Interface page it also puts back Interface Scale and the HUD settings. Your saved settings keep their values.
- The crystal shards' sparkles are small motes of light in the shard's own colour that twinkle as they drift around it. The haze around a shard and its light on the ground take its colour too, so a green shard lights the grass green. A black shard's light is faint.
- The vents on the cone-shaped mounds in the swamps put out thin wisps of smoke in their own colour, pink, brown or green. The wisps rise from the vent, curl and drift with the wind, and break apart as they climb. They glow faintly as before, so they show at night too. The brown vents no longer draw flames at their mouths, and their orange light on the ground and trees around them is a tenth as bright.

### Removed

- The Check for Updates setting under Settings' Information heading is gone. Pyreal always checks for a newer version when it opens, at most once a day. The check sends only Pyreal's name and version. A saved value for the old setting is ignored.

### Fixed

- On a Mac with MetalFX, the game no longer pauses for a moment each time Render Scale set to Dynamic changes the scale. In a smaller window it also stops using more memory with every change, which could use up all of the computer's memory within minutes. The picture keeps its detail when the render scale changes.
- In character creation, the Undead's burning head and the Empyrean's glowing eyes look as they do in the world, and their light falls on the face. Both had shown as dark squares.
- On the first run, your keys work as soon as you choose your data folder, and the Key Bindings page lists them. Moving and every other key used to wait until Pyreal was started again.
- On a Mac, the settings you change are kept when you quit with Command-Q or from the Dock, as they are when you close the window.
- Turning on Gamepad Mode no longer stops the mouse from clicking the interface. The pad takes the pointer when you press one of its buttons or push a stick, and moving or clicking the mouse gives it back. With no pad connected, the interface works as it does with Gamepad Mode off, and the Controls page says that no controller is connected.
- With UI Blur on, the world behind the shortcut bar and spell bar slots, the stance button and the main menu is blurred as it is behind windows and the chat. A HUD element moved in HUD Layout keeps the blur behind it.
- The dark ground inside each shortcut, spell and inventory slot reaches its frame, so no thin line of the world shows along its top and sides.
- The fountains the game places in towns, such as the one in Ayan Baqur, throw a clear column of water into their basins, as the other fountains do. Their water no longer lights the ground around them blue.
- By day, lamps in rooms that open to the outdoors, such as the huts in the swamps, light the room about as brightly beside the sunlit ground as the original game did. A hut with a strong green lamp had glowed brighter than the sunlit ground around it.
- By day, lights outdoors, such as the flaming arrows Relic Bones carry, lit weapons, magic items and street lamps, show on the sunlit ground about as faintly as in the original game. They still brighten shaded ground nearby, and the glow round them fades in sunlight. At night they glow as before.
- On a Mac, Pyreal no longer quits suddenly when the graphics processor reports an error while the MetalFX upscaler is in use. MetalFX is turned off until Pyreal is restarted, and the world is smoothed by Pyreal's own anti-aliasing.
- A character who logs in or arrives inside a building stands clear of its walls.
- A world you enter while its data files are still being updated is built again from the updated files when the update ends.
- A character who dies on a plugin's journey stays down where it fell, and the journey ends. No key or plugin moves a dead character.
- A plugin's movement and journeys keep going when you click to another window.
- A plugin's journey walks round a large object that comes into sight across its way, such as a fence or a yard wall.
- A plugin's window over a HUD element moved in HUD Layout, such as the party list, takes clicks and can be dragged by its title bar.
- On Windows, plugin messages between the clients you run on one computer keep arriving after the first minute.

## [0.2.1] - 2026-11-22

### Added

- The radar shows how far away your waypoint is, in gold beside its arrow on the rim. It counts down as you walk and goes away when you arrive or clear the waypoint.

### Changed

- The Enhanced Environment pack is 0.5.3. The next hundred most common props keep their pictures to their own size, and glazed pots, glass, crystal, steel and gold look like what they are. Dozens of dungeon walls, floors and ceilings take relief that follows their pictures, and the large square floor paver is a flat tile.
- The built-in trees are drawn with fewer triangles and look the same. Each branch is drawn in fewer segments, pines and spruces in the middle distance draw one needle card for each tuft, and far pines and spruces leave out the branches hidden inside their needles. A forest at Ultra Object Detail draws about a third fewer triangles.
- A place opened on the map from a link in the Journal has a Create Waypoint button beside its marker straight away, so one click sets your waypoint there. It stays beside the marker while you pan or zoom.
- Pyreal checks for a newer version at pyrealclient.app, and asks GitHub when the site does not answer. The Download button on the update notice opens the downloads on pyrealclient.app.

### Removed

- Releases no longer include the plugin tool. The plugin tools and the sample plugins now come from the Dereth project, and Pyreal still loads and runs plugins as before.

### Fixed

- Your character fades smoothly see-through when the camera comes close, and so do ghosts, other see-through creatures and anything fading in or out. They showed a fine pattern of dots, most of all with MetalFX upscaling and a render scale under 100.
- The enhanced portal tunnel flies on smoothly while the world is placed behind it. When placing the world held the game up for more than a quarter of a second, as it often does on first entering the world, the tunnel jumped back to its start and its light went far away again just before the world appeared.
- Fresh snow in the distance lies only on snowy ground. For a quarter of an hour after snow, the land seen past a snowy hill could turn white in wide patches and long straight bands reaching to the horizon.
- Rocks, plants and other small objects in the distance stay drawn out to the edge of the loaded land at every Object Detail level, so changing Object Detail sets how much detail they keep and makes none of them appear or disappear. The flowers and stones among the grass reach as far as Ground Cover sets.
- Stone walkways over a drop in some dungeons no longer flicker with a second, blurred pattern of stones. The top of the shaft under the walkway was drawn in the same place as the walkway's floor.

## [0.2.0] - 2026-11-21

### Added

- UI Blur, on by default under Display, blurs the world behind see-through windows, menus, tooltips and the chat. It costs about 0.2 ms a frame.
- An art pack can let the wind sway one of the original game's models outdoors without replacing it, with a `[[sway]]` entry in its manifest naming the object and how much the wind moves it.
- Pyreal picks the Quality Preset that fits your graphics card the first time it starts, from Low for built-in graphics and older cards up to Ultra for the fastest. It goes by the card's name: Apple's M chips by generation and Pro, Max or Ultra, and NVIDIA, AMD and Intel cards by series and number. A card it does not know gets Medium, or Low when it is built in. Display shows the preset suggested for your card under Quality Preset, with a button that sets it. Existing settings are left as they are, and a new graphics card brings its own preset only while the settings are still the ones chosen for the old card.
- The performance panel shows the build's ID on its last line, the same short hash that names the build.
- The README's and the site's pictures are a library defined in `docs/screenshots/shots.json`, drawn again by `tools/screenshots/capture.py` (in place of `capture.sh`) with the README's pictures rewritten to match. engine-view gives each placed creature or character its own stance, motion, heading, dress, walk, height over the ground and scripts, and takes `--no-face-detail` and `--light-reach`.
- Detailed Faces, on by default under Display, draws the faces of the players near you with finer skin, hair that runs in strands, and wet, rounded eyes. Heads are rounded, and within a few metres brows, lips, beards and moustaches stand out in depth. Caps, hoods and hooded robes keep the face beneath them detailed, and the character creation figure shows it too, without the thin dark lines it had across the forehead and under the eyes. Each face is worked out in the background the first time it comes near, so nothing stutters, and it lets its detail go a while after no one near you wears it. Turned off, faces are drawn as the original game drew them. The Low quality preset turns it off.
- On 1999 and 2005 worlds with the End of Retail files beside them, dungeon and room walls, floors and ceilings that End of Retail painted again at a higher resolution are drawn with End of Retail's sharper picture and its relief, as End of Retail drew them. The 41 that are the same picture only sharper, or that need the older colours to match, are tinted back to the older colours under Classic Ground Coloring and drawn as they are under Throne of Destiny.

### Changed

- Lamps light the room they stand in and the rooms their light reaches through doorways, so a torch behind a wall or under a floor stays out of the rooms beyond. A sealed room with no lamp reaching it keeps the original game's dim light.
- Retail Look lights rooms as the original game did, with its own lamp formulas and the light the player carries, and a wall torch lights the room from the original game's place for it.
- On 1999 and 2005 worlds with the End of Retail files beside them, 48 props that End of Retail remodelled are drawn with End of Retail's model, some tinted to the older colours. The torch pillars keep the older flame.
- Gem and ice golems glint without lighting the ground round them.
- The floor grate's brown haze rises as fume and takes no fire's light.
- Trees from an art pack sway about as widely as they did before gusts were added.
- A hood shades the skin beneath it.
- Travel turns smoothly below 20 frames a second, keeps going down ramps, opens a shut door as it reaches it, and keeps clear of portals it is not taking.
- Frames are about 3 ms faster with the same picture. Each band of the sun's shadows leaves out the objects whose shadow cannot reach what you see, and objects and ground cover draw with a lighter shader unless they show a detailed face or statue stone.
- Smoke from spells, fires and chimneys takes its light at four points of each puff in place of every pixel. A cloud of spell smoke draws in about a seventh of the time, so a spell like Ethangar's Wrath landing on a crowd no longer stalls the frame.
- Storms drive the rain at a steeper slant, about 26 degrees in a full storm, and light rain drifts further with the breeze. Rain streaks are drawn as even lines at any slant, without breaking into dashes. In a storm, trees, bushes, flowers and grass lean steadily the way the rain blows, more in the gusts, with their natural sway still showing. Calm weather is unchanged.
- Trees block you where the art pack draws their trunks and also where the original game's trees did, so a leaning trunk stops you along its lean and the server never moves you back from a tree.
- Desert sand reads as wind-rippled sand rather than dirt. The fine bumps the ground takes near you are left off yellow and grey sand, so its ripples show, and grass no longer grows out onto sand where the sand's picture covers the edge of a meadow. Pebbles still lie on it.
- The Mac download opens in a parchment window with large icons for Pyreal and Applications and an arrow between them, so it is clear where to drag Pyreal to install it.
- Distant ground no longer looks flat. Past about 50 m, hollows are darker and crests lighter, grass shows lusher and drier patches, steep slopes turn earthier, and the light falls on retail's own triangles again under Landscape Smoothing, with small bumps across it. The ground pictures' own grain shows out to the mountains, and hills cast shadows past the shadow distance. The ground near you is unchanged. This costs about 2 ms a frame.
- Portal Style changes the portals standing in the world as well as portal space, and the change shows at once. Original draws them with the swirl from the original game. Enhanced draws each one as a whirl of smoke in its own colour.
- The map is drawn in the background once you enter the world, so it opens straight away the first time. It is kept after you close it and drawn again only when the world's data changes.
- The Enhanced Environment pack is 0.5.2. Every tree and plant is reviewed and regrown to look like its retail tree, and the pines and firs take their needles in retail's own dark green. Clothing, robes, masks and hats show their cloth, silk, hide and fur, and the most common props keep their relief to their own size. Room and dungeon walls take relief made to follow their own pictures, so smooth plaster and stone no longer show rock, caps and mouldings stand proud, and painted signs and cloth lie flat. 2005's pictures take their own relief where they differ from End of Retail's. Armour shows what it is made of across forty-two suits and their helms: quilting puffs between its seams, studs and buckles stand proud, chain is rings, scales overlap, chitin is glossy, fur lies soft, and only the metal shines. The ground's grass, roads, rock and dirt follow their pictures. Desert sand ripples as its picture is painted, deep enough to cast shade, and the large ash trees' trunks narrow into their branches instead of ending in a ledge.
- A Water Golem drawn larger or smaller than usual keeps its water together: its blobs grow and shrink with it.
- Magma Golems no longer carry a large flame round their core. The cracks in their rock glow red instead.
- The Bael'Zharon statues, such as the giant one in the Singularity Caul, are carved from chiselled dark red stone. Their shape is unchanged. Art packs can now give a surface that has no picture a material of its own (`[[solid]]`). On 1999 and 2005 worlds with the End of Retail files beside them, the statues show End of Retail's redrawn face.
- The Quality Presets also set Landscape Smoothing, High-Res Textures, Silk Cobwebs and Portal Style. Low turns the first three off and uses the Original portal style. Terrain Draw Distance goes from High on Low to Horizon on Ultra, and Depth Mapping and Landscape Smoothing are Medium on every preset above Low. A new profile starts at the Medium preset, with Very High terrain and Medium depth mapping.

### Fixed

- Item names show their material again, such as on salvage bags.
- The melted snowman's puddle is drawn as see-through water.
- On a Mac, full screen keeps the right resolution, and a saved resolution fits the screen the window opens on.
- A spellbook school tab lists all of that school's spells whatever the saved filters say, and Launch worlds show no level switches.
- A world's data patch waits for the world's list of records, so it is sent once, and applying it keeps the frame smooth.
- Movement meets walls, steps and slopes at the original game's precision, and Pyreal takes and answers the server's position corrections.
- Pyreal reads a server's newer portal table and leaves closed and temporary portals out of travel routes. Creatures with negative armour show it.
- A world's place list with a line Pyreal does not know still loads.
- A plugin's tooltip goes away when its control is hidden.
- Large buildings and towers far away stay drawn as their simplest shape out to the edge of the land Pyreal shows. A tower on a far hill no longer vanishes at Medium Object Detail where the original game's own distances would drop it.
- Portals in the world whirl in the colours the original game shows them in. The blue portal is blue, the orange one orange, the yellow one yellow and the white one a cool white. The great purple portal is half again as wide as the others, as it is in the original.
- Waterfall mist stays at the top of the falls and at the pool below. It no longer runs down the side of a cliff where the land's water data has a pool hanging out past the edge or where the falls land at different heights.
- Distant tree cards are drawn from the tree's shape just nearer, so far trees keep its gaps and bark. Packs made before this have their cards drawn again when they open.
- Signing in to a 1999 or 2005 world from the world list no longer plays it with the End of Retail files when the world did not answer Pyreal's question about its era, as happens while its server is still starting. The 1999 world turned you away with "Your DAT files are newer than expected". Pyreal now remembers the era each world last named and asks again at the next sign-in.
- A world that moved to other game files since you last played it, such as a 2005 world moved from the End of Retail files to the February 2005 files, is no longer refused with "This world's data was refused: it was made for other game files than yours". Pyreal opens the files the world names when it first asks, and the next sign-in in the same run opens them too.
- On Windows, Pyreal's window no longer opens as a blank white window that stops responding while its shaders are prepared. It shows the Loading picture, Pyreal's logo over the night sky, about a twentieth of a second after it starts, and keeps it until the first screen is ready.
- On Windows, the first start after an update or a new graphics driver no longer leaves the window "Not Responding" for some eleven seconds while the graphics card's ray tracing is prepared. The Loading picture stays up throughout and the window can be moved and closed. A window closed during the Loading picture closes once start-up is done.

## [0.1.4] - 2026-11-18

### Added

- Advocates, sentinels and admins show on the radar as a diamond in their own colours, pink for advocates and cyan for sentinels and admins, so they stand out from PK-lite players and creatures.
- Tab fills in a name in a tell. Type `/t` (or `/tell`, `/w`, `/whisper`) and press Tab to get `/t Name, ` with the selected player's name, and press Tab again to go through players nearby, the people you have recently told or been told by, your fellowship, your allegiance and friends who are online. Letters typed after `/t` narrow the names, and Shift+Tab goes back. `/r` then Tab fills in the last person who told you, and more Tabs go through your recent tell partners; `/rt`, `/mr` and `/pr` fill in the last person you told, the monarch speaker and the patron speaker. Names are filled in without an admin's `+` or an AI Player's `~`, and typing either is the same as leaving it off. Elsewhere in the line Tab still changes the channel.
- `%t` or `<t>` in a line you send becomes the name of what you have selected, as in `/f heal %t please`.
- On an Empyrean world with AI Players, the fellowship window shows each AI Player's mode (Aggressive, Defensive, Assist, Heal or Passive) beside its level. The fellowship's leader gets Mode buttons to change it and Order buttons to tell it to Attack, Follow, Stay or Come, for the AI Player selected in the list or the only one in the fellowship. Out of a fellowship, the window lists the AI Players following you with their modes, and gives you the same buttons for them. In the fellowship bars at the top left, each AI Player's name has small mode icons beside it (a sword, a shield, a target, a cross and a dove) with its own mode lit, and the leader clicks one to change it.
- The tooltip on a member's bar in the fellowship bars at the top left gives their level, as in "Level 126", above their health, stamina and mana.
- When an Advocate offers you a waypoint or a note for your journal, a box says who offers it and what it is, with Accept and Decline. Accept sets the waypoint as `/way` does, or adds the note to the journal as a page of its own. Several offers come up one after another, and an offer you leave unanswered closes when the world withdraws it.
- Horizon's chat windows can be made larger or smaller from any of their four corners. Dragging a top or left corner moves that side while the opposite sides stay where they are, and the window keeps its new place and size. The marks on the top corners show when the pointer is over them.
- Horizon's chat line has a Modern style, now the default. A tell's person sits in a Tell chip before the message and stays there after you send, Tab on the chip goes through your recent tell partners, and the chip's list offers Recent Tells and Tell Someone. Classic, under Chat Input in Settings, keeps the original line. The name in a tell you sent is a link, and a right click on a name offers Tell, Select, Add Friend, Squelch and Copy Name.
- The journal is in every world. Its pages are searched, tagged and sorted as you type, a page's timer counts down and says when it is ready, a Timers tab lists the world's quest timers, and pages show on the map.
- A place written in a journal page's notes or Place line, and your coordinates under the map, are underlined links. Clicking one opens the map on a blue arrow there, and clicking the arrow offers Create Waypoint. A single click on the map offers Create Waypoint at that spot, and a double click still sets the waypoint at once. The hint line above the map is gone.

### Changed

- The five gamepad stance controls move from Horizon's Spellbook to a Gamepad Shortcuts row in the Character window, under Weapon, Shield and Ammo, which now sit right under the figure when no sigil is unlocked. The Spellbook's search runs across the window, and its eight level filters lie in one row along the bottom.
- The 1999 and 2005 worlds lay End of Retail's ground pictures when End of Retail's files are beside them, tinted by Ground Coloring and with the Enhanced Environment pack's relief, so the older ground is as sharp as End of Retail's. An older world with no later files beside it lays its own ground.
- A waterfall's foot is a low bank of mist that billows along it and spills a little over the pool, where the falling water had ended in a hard straight line at the water or the ground. The foam on the pool no longer shows as a bright line when seen from the water's level.
- God Rays are half as strong at every level. The shafts of sunlight, the glow round a low sun and the haze they lay over the land are each half what they were at Low, Medium and High, and the levels still differ from each other as before.
- The fire on Redspire's red spire burns as a ring of fire. Each of its two rings is one band of flame round the spire that licks upward, burns deep orange with a yellow heart, whirls round the spire the way retail's fire puffs go round, and lights the ground and buildings around it with a flickering light. Before, each ring was a few pale clouds on either side of the spire. The same fire round shrines and statues in a few dungeons changes too.
- Fountains, basins, wells and tubs hold water that looks like water when Better Lighting and Water Detail are on. The water in a basin or a tub mirrors the room or the sky above it, even seen from straight above, shows the bottom of the basin through it, and ripples, with rings spreading where the fountain's water lands. Water pouring from a wall fountain's spout runs down in clear streaks and sends rings across its basin. A fountain's jet is a clear column of water that rises from its spout and falls back as drops into the basin, and the streams that spill from one tier to the next run as clear water. With Better Lighting or Water Detail off, fountains keep their blue water and bubbles.
- The green gas that blows out of dungeon vents and gas pits, as in the halls of the Training Academy, is a cloud of gas rather than flat pictures. A vent's gas pours down out of the vent and spreads low over the floor, a pit's gas billows up, and both thin as they spread and are lit by the room, so the vents' green lights make them glow. Every vent and pit that blows this gas looks the same way.
- The brown haze over wooden floor grates, in dungeons and in a few places outdoors, rises from the grate as a soft column of vapour. It billows, thins as it climbs, keeps the original game's brown and glows faintly warm just over the grate. The grates no longer count as fires, so they no longer throw a flickering firelight round them, and in dungeons their haze shows again.
- A smithy's forge indoors, as in the Training Academy, burns with a fire on the coals inside its mouth. Rows of flames lick up from the coals to the roof of its firebox, tallest at the front where the opening is highest, with sparks rising out of them, and they flicker with the fire's light. The coals throw a warm, flickering light out of the mouth onto the floor in front of the forge. Before, the fire was a pale cloud filling the opening. A fire pit indoors, a ring of stones round glowing coals as under the cauldron in the Academy's kitchen, burns the same way: flames lick up from its coals, the middle one tallest, and its coals light the stones and the floor round it. Forges and fire pits outdoors, campfires, and the same fire elsewhere look as they did.
- Iron doors shine as iron does. Their leaves, the iron bands on their wooden posts and lintels, and their hinges and rivets catch the light of nearby lamps, and the wood around them stays wood. This needs the Enhanced Environment pack's iron materials, and an art pack material can now name a metal map to say where it is a metal.
- Blue, green and violet torches burn as flames of their own colour, as the blue pole torches in the Training Academy and the blue and green torches lining many dungeons do. A deep coloured flame with a pale core licks up from the torch, and its coloured light now flickers on the wall and floor round it. Before, these torches showed only a small, pale wisp. A torch whose flame grows larger in the original game, as the academy's pole torches and the tall torches with a long iron head do, burns taller to match. Other orange torches and candles look as they did.
- Torch flames are livelier. Several ragged tongues lick up together over a white-hot heart, and a few sparks rise from the flame and fade. Candles, braziers, fireplaces and creatures of fire look as they did.
- A fireplace's fire, as in the arrival hall of the Training Academy, burns as a fire on its coals. Flames lick up from the coals in two rows, the middle ones tallest, hottest and yellowest low down, with sparks rising out of them, and they flicker with the fireplace's light. The coals throw a warm, flickering light of their own on the inside of the fireplace, its hearthstones and the floor just in front of it. Before, the fire was a pale cloud hanging over the coals. Braziers, campfires, torches and candles look as they did.
- A chat tab's count of new lines is a small round badge. Each tab has an Unread Count box in Settings > Chat that turns its count off.
- Settings > Chat lists the five chat tabs on the left. Choosing one shows its name, Unread Count, Undocked, its opacities while undocked and its channels on the right. The chat input and the docked window's opacities stay across the top.
- Horizon's chat has the game's five tabs and no others. A tell no longer opens a tab of its own, and tells show in whichever tabs have Tells ticked, as in the original game.
- Horizon's chat line stays on screen while you are not typing. It is dimmed and shows where the line will go and any message you left unsent. Click it to start typing, or click the channel or the Tell chip to choose where it goes.
- Horizon's chat uses the colours players of other online games know: say white, tells pink, the fellowship blue, the allegiance green, patron, monarch and vassal lines a darker green, the chat rooms pale rose, emotes orange and the game's notices yellow. Lines you send are a dimmer shade of their channel's colour, so a tell you sent and a tell you got look different and other people's lines stand out. The text you type in the chat line takes the colour of the channel it will go to and changes as you type `/t`, `/f`, `/a` or `/cg`.
- Ethangar's Staff, and anything else held that glows, lights its surroundings softly in its own colour, about a quarter as brightly as before. Creatures, torches, portals and placed lights are unchanged.
- Blades, mace heads, gilt guards and the steel and gold of shields and plate shine as metal, and the books on the Training Academy's shelves stand out in relief, with the Enhanced Environment pack.

### Removed

- The original game's intro movie and its soundtrack no longer play.

### Fixed

- The blue crystal lamp posts at night, as on the hillside by the sea at 82.0S 94.9E, have a faint blue glow round their crystals as in the original game. They had stood in tall brown columns of haze that rose over the slope into the sky and flickered in blocks. The glow round other blue, green and purple lamps is fixed the same way, and an orange lamp's glow now flickers as a whole.
- Trees and plants behind a geyser's steam, or behind smoke or any other cloud, are now covered by it. They had shown through it sharp and clear, as if they stood in front.
- A waterfall seen past a lamp's glow or another see-through thing is now covered by it, like the rock and ground around it. At night it had shown through the glow clear and blue, as if it stood in front of the lamp. The mist at a waterfall's foot is now shaded where a cliff or a tree shades it and is lit by nearby lamps. Before, it glowed in the shade.
- The light on the sea shimmers all the time. Its glitter no longer holds still and then jumps every few seconds, and it no longer lies in pale blotches over the water by day. Along the sun's or the moon's path the light still breaks into glinting streaks, and they twinkle, and a tree standing in front of the path at night is no longer washed over by a glow with a fine grid in it. The sea also no longer changes colour at a straight line several hundred metres out, where the nearby water and the water beyond it were drawn as different kinds of sea.
- The sea no longer changes colour at a hard line partway to the horizon. Near the edge of the world the sea past the last landblock was drawn as dark, bare ground, and the far sea's water stopped there. It is now water all the way to the horizon, and the moon's or the sun's reflection runs across it in one path of glinting streaks from the near water out to the horizon.
- The steam from dungeon vents is a soft veil that rises from the vent. It had been drawn as a thick block of snow that filled the room. The haze in dungeon rooms, purple, blue or warm, is a glowing volume of its colour that drifts and churns slowly.
- Geysers, outdoors and in dungeons, throw up a column of steam that starts at the vent and rises in a thin veil you can see through, spreading and fraying as it climbs, with a low bank of spray round its foot. They had been drawn as a thick lilac cloud hanging over the vent, with a bright white patch below that glowed at night.
- The coloured lights in dungeons and other rooms, such as the green lights of steam vents and the blue and magenta floor lights, light their rooms in their full colour and strength as retail did. They had been dimmed and greyed like a portal's light. Portals and spells are unchanged.
- The stairs at the end of a hallway in the dungeon at 0x8A02 no longer disappear when the camera is a few steps back. They stand in one room and climb into the next, and Pyreal drew them only when the room they stand in was in view. Stairs and other furniture that reach into another room are now drawn when either room is in view, as the original client draws them.
- Square flagstone floors in dungeons show their slabs and joints again, where they were pitted all over. The base pack is now Enhanced Environment 0.3.0.
- Small torch flames no longer float in dungeon rooms where the original client draws a broad glow, as in the rooms at 0x6679. These glows are the soft orange haze many dungeons place in a room, the large swirling golden orbs in dungeons such as 0x0004, and the sheets of light on some floors. Pyreal hid each glow and drew a torch flame in its place. Each glow is now drawn as the original client draws it, with no flame. A glow with no light of its own lights its room steadily, without a torch's flicker.
- A character runs from the moment it enters the world. Before, a character entered without a key pressed first walked until the walk key was pressed.
- In Horizon's character creation, a long name in the Profession, Heritage, Sex and Town lists wraps onto a second line, so "Covenant Warrior, Two Handed" reads in full. The names and the chosen row's highlight stop short of the list's scroll bar. The profession's name over its description wraps beside "Community preset" and no longer runs into it.
- Pyreal logs into GDLE worlds given `-a account:password` with no `-v`. It sends the account alone, as the original client does, and GDLE reads the password from it. The window title and the connect log show the account without its password.
- Seen from below a waterfall, as from the lower pool at 0xF418, the pool at the top no longer shows through the cliff as a sheet of water on its upper half, and its water stops at the edge of the fall.
- The chat line's bar in Horizon is taller, so the Tell chip has as much room above it as below it, and the text you type sits on the same line as the chip's name. Classic's line is centred in the bar too.
- Clicking the see-through part of a door or other object selects it, as in the original game. Before, a click on the green membrane in the middle of an olthoi door went through to the wall behind it, so the door could only be clicked or double-clicked on its frame. A selected door's outline now goes round its membrane too.
- A corpse or an item lying in grass is selected by a click on it, even where blades stand in front of it. Before, the grass, flowers and stones over it took the click, so it was picked only where it showed between them. Its outline goes round all of it too.
- A click that lands on an object's drawn shape but on no drawn pixel of it selects it, as in the original game. This covers a gap in hair, fur or a banner's cut-out picture, a see-through creature's skipped pixels, and an item on the ground a pixel or two across, which the original game's click also found by a sphere round it. A click on a wall or a hill still selects nothing behind it.
- A room's floor or ceiling that is also its way into the room below or above is drawn. Before, the hallway floor at 0x8A02 showed the room under it, and many dungeon rooms had no ceiling.
- The ring under a selected creature or person lies only on the floor or ground round its feet. Before, it ran up a sign, another creature or a door standing in it, and across a table top beside it. It still follows hillsides and stairs. The waypoint ring round your character is laid the same way.
- A wall made of two pictures, one above the other, no longer shows a thin bright line where they meet. The upper picture's pale top edge was bleeding into its bottom edge, as on the hallway walls at 0x8A02.
- The map's journal checkbox and Add Waypoint to Journal sit in a row of their own under the map, so they no longer cover the coordinates or an administrator's Teleport button.
- Find and Spawn says the item list is missing when a search finds nothing, and asks the world for it again after 20 seconds.

## [0.1.3] - 2026-11-15

### Added

- When Pyreal opens, it checks GitHub for a newer release, at most once a day. The world list then shows a line such as "Pyreal 0.1.4 is available." at the foot of the screen between the list and Quit, with a Download button that opens the release page. Check for Updates under Settings' Information heading turns it off. The check sends a plain request naming only Pyreal and its version, and a build made from a checkout never checks.
- The Loot window over a corpse or chest has a Loot All button that moves everything in it to your pack, one item at a time as the server takes each. It passes over anything you may not take, stops with a line in chat when your pack is full or you are carrying too much, and stops when the container closes. Pressing it again while it works stops it. A Loot All key, unbound until chosen in the keys list, does the same.
- Examining another player shows a Tell button at the top right of the Examine window. It opens the chat line with `@tell <name>, ` typed in, ready for your message, and sends nothing until you press Enter.
- Every package's NOTICE.txt names the four typefaces Horizon's text is drawn from (Noto Sans, Saira, Pathway Gothic One and Cormorant SC) and carries each one's SIL Open Font License.

### Changed

- Waterfalls are drawn as sheets of falling water. The water leaves the lip clear and glassy, streams down in ropes that whiten as it falls, runs down the rock to the pool, spreads foam where it lands and sends up soft mist. On a clear sunny day a faint rainbow shows in the mist, and where the pool above stops short of the edge the water runs from it to the edge. They were drawn as a mass of blue flecks over a cloud of snow, and they now cost a small share of what they did to draw, most of all up close.
- The icon's coin carries a pyreal's mark, a C with a stroke above and below, in place of the compass rose.
- Portals in the world have a see-through body in their own colour under their turning arms, shaded darker down the funnel's throat and lighter on its rim. The funnel is twice as deep, it leans a little off your line of sight and slowly goes round so its shape shows, and its arms and dust move about a third faster.
- Enhanced portal space is tinted toward the colour of the portal you stepped through, so a green portal gives a green tunnel and a blue one a blue tunnel. Recalls, the lifestone, houses, logging in and logging off keep their own colours.
- The sign-in and world list screens speak plainly, with short status lines such as "Checking the world...", "Preparing shaders..." and "Loading characters...", and sign-in problems as sentences that say what to do.
- The Explore panel no longer has the Save Timings and Open Folder buttons, and F9 does nothing there. Slow frames are still saved to `logs/hitches`, without a note on screen or in chat. The Save Frame Timings and Open Frame Timings Folder keys, unbound until chosen in the keys list, save the last 30 seconds and open the folder.

### Fixed

- Ponds that the original game paints on dry ground are drawn as water, such as the round pond at 95.9S 91.1W and the pools at the top of the waterfalls near 82S 94E. They were drawn as bare grey silt. You still walk across them, as in the original game.
- At a waterfall with a pool above another, the lower pool no longer shows through the upper pool's beach, and the upper pool stays level at the top of the fall.
- The silt along shores and under water has the land's mud texture, with its grain and shading, in place of one flat brown.
- On a Mac, the arrow, Home, End and forward Delete keys in the chat line and other text boxes move the cursor or delete as they should, rather than typing odd characters. Option with Left, Right or Delete moves or deletes a word at a time, as Control does.

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

[Unreleased]: https://github.com/jacobjohnston/pyreal/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.3.0
[0.2.1]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.2.1
[0.2.0]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.2.0
[0.1.4]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.1.4
[0.1.3]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.1.3
[0.1.2]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.1.2
[0.1.1]: https://github.com/jacobjohnston/pyreal/releases/tag/v0.1.1

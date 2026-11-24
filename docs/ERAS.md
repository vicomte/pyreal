# Eras

How a world from 1999 or 2005 draws its own era's world, models and pictures and keeps every
enhancement the engine and the art pack add.

## The three dat sets

| Era | Files | Iterations |
|---|---|---|
| Launch (October 1999) | `portal.dat`, `cell.dat` | portal 102, cell 67 |
| Infiltration (February 2005) | `portal.dat`, `cell.dat` | portal 2112, cell 1593 |
| End of Retail | `client_portal.dat`, `client_cell_1.dat`, `client_local_English.dat` | portal 2072, cell 982 |

The era table is `core/primitives/src/era.rs`; the front door picks the dat set by era. Files of
other dates are not supported.

## What stays the same across eras

- GfxObj, Setup, picture (0x05), landblock, room, animation, sound, particle, script and
  terrain-type ids are stable across all three eras. Every 1999 portal id exists in 2005.
- Surface (0x08) and DegradeInfo (0x11) were renumbered at Throne of Destiny. Nothing in the
  renderer or the pack keys on them.
- Every record's bytes changed format at Throne of Destiny, so a comparison across that line has
  to be semantic (`docs/formats/40-before-throne-of-destiny.md`).
- Throne of Destiny moved many 2005 pictures to new, larger ids. 1999's art is nearly all 2005's,
  though some identical pictures decode to other colours because their default palette changed.

## Matching pack entries by fingerprint

Each pack entry carries the fingerprint of the record it was made from. An id may have several
entries, and the client draws the one whose fingerprint matches the world's record. The pack
names no eras.

- A picture's fingerprint is its size and a 64-bit average hash: an 8 by 8 box average of
  luminance, one bit for each cell above the mean, the picture seen in its own palette. Two match
  when their sides are in the same proportion and the hashes differ in 10 bits or fewer.
- A model's fingerprint is a hash of its part count, each part's vertex and polygon counts, and
  its bounds to 1 cm. Two match when the hashes are equal.
- `pyreal-art fingerprint <pack folder> --dat-dir <files>` writes each entry's fingerprint into
  `manifest.toml`, on the line after its `id` or `texture`. The client works out a record's with
  the same code (`pyreal_art_pack::fingerprint::of_record`) the first time the pack is asked about
  it. The packer copies the manifest, so `base-pack build` reads no dats.
- `draws_on = "any"` draws an entry on any record, and `draws_on = "none"` on none. An entry with
  no fingerprint draws on any record; one may follow those for the same id that name one, to catch
  every other record. The pack's trees and plants take `draws_on = "any"`, so every era draws them.
- A chosen pack drawn over the base replaces the base's entries made from the same record. Its
  entries that name no fingerprint stand for the base's first entry of their id, so the base's
  entries for older records still draw under it.
- A material may name `palettes`, and is then drawn only where its picture is drawn in one of
  them. The engine passes the palette a swap lays over the picture, else the picture's own. The
  1999 golems share one model and one picture told apart by palette, and the pack gives each kind
  its own material this way.
- A material may also name `laid_palettes`, drawn where an object's look lays one of them over the
  picture, ahead of the picture's other materials. The crystal shards wear their picture's own blue
  palette, which the world's crystals show as their own, and this tells the two apart.
- A record no pack entry matches draws the era's own model or picture with every engine effect.
  The engine's load report counts `pack_unmatched`, the entries left out because none was made
  from the world's record.

The pack repository holds the older eras' own relief maps under `materials/2005/` and
`materials/1999/`, for pictures no End of Retail material matches.

## Effect ids

The engine's tables of hard-coded effect ids stay in its code. Every id in them that an older
era's files hold is the same art there, so an older era needs only its own ids added (the lava
picture in `MOLTEN_PICTURES`, the 2005 corner and small webs in `webs.rs`). A table would move
into the pack only if one of its ids stood for other art in another era.

## The older eras' ground

The 1999 and 2005 files carry two regions. `0x13000000` palette-shifts the ground for the software
renderer. `0x130F0000`, which the clients of the time loaded with a 3D card, texture-merges it as
End of Retail does. `load_region` (`core/world-data/src/landblock.rs`) loads `HARDWARE_REGION`,
`0x130F0000`, wherever the files have it, sky included, so the older eras' ground goes through the
same path as End of Retail's and takes every ground enhancement. End of Retail has no such record.

With End of Retail's files beside a 1999 or 2005 world, each terrain type and the road take End of
Retail's picture and repeat instead (`LaterGround`, `ground_picture`, `engine/src/world.rs`), tinted
by Ground Coloring as End of Retail's own are, with the pack's relief for those pictures. The blend
maps stay the world's. An older world with no later files beside it lays its own pictures.

With End of Retail's files beside an older world, a room End of Retail's cell file holds with the
same piece also draws End of Retail's picture in each slot where the eras' pictures are one of the
pairs the older-era survey found the same layout painted again (`later_rooms::PAIRS`,
`engine/src/later_rooms.rs`), the slot's texture coordinates are the same, and the two still match
when loaded. That check also measures a gain and offset per colour that carries End of Retail's
picture back to the older one's colours. Classic Ground Coloring applies it to the pairs marked
tinted (the same picture, only sharper, plus 0x05001CBF and 0x05000D77); every other pair, and every pair under
Throne of Destiny, draws End of Retail's colours (the developer's review, 2026-11-19). The surface keeps the world's kind and flags and takes End
of Retail's pack material. End of Retail's `client_highres.dat` is opened beside an older world
for this, since most of these pictures are larger only in their high-resolution level.

## Props End of Retail remodelled

End of Retail remodelled some props under their old ids: 101 that the 1999 and 2005 worlds place,
besides the trees and plants the pack draws in every era. The developer reviews them a round at a time and
picks, for each, the older model or End of Retail's (`notes/review/era-props/`). With End of
Retail's files beside an older world, a prop in `later_models::MODELS`
(`engine/src/later_models.rs`) is drawn from those files: its parts, surfaces and pictures, and
the pack's materials for those pictures. Where it stands, how it is turned, its lights and what
the player bumps into stay the world's. A prop marked tinted takes the older model's colours under
Classic Ground Coloring: each of End of Retail's pictures is carried, channel by channel, to the
mean and spread of the older pictures drawn over the same places on the model. Under Throne of
Destiny coloring it keeps End of Retail's own colours.

A prop drawn as End of Retail has it burns with End of Retail's fire, drawn as the engine's
flames (`LaterModels::flames`), even where the older model's fire was a mesh: the torch pillars'
flame is a model in the older eras and an emitter in End of Retail's. End of Retail's torch pillar
on a base has no script of its own, so it burns as the other pillar (`flames::BURNS_AS`). The melted
snowman's puddle, retail's one see-through cyan with no picture, is drawn as water
(`material::is_pool`).

The Bael'Zharon statues are listed too, though their model is the same in 2005: the developer wants every
era to draw End of Retail's, with its redrawn face. A server places them, so the client swaps their
parts one by one (`AssetLoader::later_part`). Their dark red body has no picture; the pack's
`[[solid]]` stone is laid on End of Retail's surface for it and on 2005's, each told apart by its
colour, so the statue keeps the stone even without End of Retail's files beside it.

The art pack lists both, `later_rooms::PAIRS` and `later_models::MODELS`, in its
`tables/drawn-as-eor.tsv`, with the older materials it leaves out because nothing of that era
draws them. `cargo xtask base-pack` refuses a pack whose table differs from the two lists, so a
change to either list goes with a change to the pack's table, and then the pin moves.

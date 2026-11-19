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

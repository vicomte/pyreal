# Map places

The map window's places: right for each world's era, with more kinds for administrators, and
supplied by the world's own server where it can.

## Where they come from

- **Built in.** `pyreal/client/crates/horizon/pieces/` holds one list per era:
  `map-places-launch.tsv`, `map-places-infiltration.tsv` and `map-places-eor.tsv`, written by
  Empyrean's `--write-places` from the three local worlds (their `place` lines left out). A world
  of unknown era takes End of Retail's.
- **From the world.** Pyreal reads the world's status document, at its status address (the
  `world-status` switch the front door passes, or `status_url` in `worlds.json`) or, for a world on
  this machine, at its game port plus 100. The status names the documents the server publishes:
  `"places"` and `"portals"`, each a path on the same address, or `null`. Pyreal asks for the places
  as version 2 (`/places?version=2`), and for the portal table when `--nav-published` names none.
  A document answering `503` is asked again, up to ten times three seconds apart
  (`pyreal_client_runtime::published`). They are read once per session, when the session begins,
  and are not cached on disk. The built-in list for the era shows until the world's own arrives,
  and stays when it never does.

## The document

Both are Empyrean's place store (`pyreal_navigation::PlaceStore`), tab-separated:

```
dereth-places-store	2
world	<world name>
place	<kind>	published	<wcid>	<name>	<at>	0	0	-	<to or ->
mark	<kind>	<name>	<at>	<detail or ->
```

- `place` lines are what the world puts down: its lifestones, portals, vendors and people that
  cannot be attacked, each where it stands, dungeons included. Version 1 has only these.
- `mark` lines are the world's map, as its server works it out: towns, landmarks, houses, dungeon
  entrances with what each dungeon holds, portals, drops, meeting halls, apartments, vendors,
  people, collectors, bindstones, spawn areas and camps. Version 2 adds them.
- A position is `0xCELL x y z`. A detail is sentences joined by an escaped line feed. A line of a
  kind Pyreal does not know is passed over, and so are fields past those it knows.

How the map uses it (`drawn_places` in `pyreal/client/crates/horizon/src/ui/panels/map.rs`):

- With marks, the marks alone.
- With places only, the world's lifestones, vendors, people and portals take the place of the
  built-in ones. What stands in a dungeon is left out (a room in a landblock with nothing
  outdoors). A portal into a dungeon in another landblock is that dungeon's entrance, and a portal
  across the land marks where it leads. A built-in place of a matching kind at the same spot gives
  its kind (a collector, a meeting hall) and its detail. Every other kind comes from the built-in
  list.
- With neither, the built-in list.

## Who sees what

Players see towns, and houses as they zoom in. A character with the administrator flag also sees
every other kind, and picks which with the Places button. The kind decides
(`PlaceKind::players_see`), whichever source the places came from.

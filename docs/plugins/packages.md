# Packages

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

## A package

A package is a folder, or a zip of one, with `plugin.toml` at its root (see
[the manifest](manifest.md)), or a script written for UtilityBelt's scripting, its folder holding
`index.lua` and no manifest ([ubscript.md](ubscript.md)). A zip may instead hold one folder with everything in it, as an
archiver makes of a folder; that folder is then the root. A package holds at most 2,000 files and
64 MiB unpacked; a zip that says it unpacks past that is refused before it is unpacked. Only text,
Lua, TOML, JSON, XML, image, font, VTank profile and route files, a compiled plugin's WebAssembly
([wasm.md](wasm.md)) may be in it: no native code. A package loads by its manifest and its
files alone; no file in it is a signature the host reads.

A host lists packages from its own places: the desktop client its per-user plugins folder (folders
and `.zip` files in it).

## Its hash

A zip file's SHA-256 identifies the download: a plugin source lists it beside the zip's address,
and the Dereth project's plugin tool writes it beside a zip it packs (`<zip>.sha256`, a `sha256sum` line). Packing
the same folder always gives the same bytes (files in path order, Deflate, no times), so a release
can be checked against its source by packing the source again.

## Installing from a source: the launcher

The launcher installs plugins for the Dereth client from plugin sources, on its Plugins page:
lists of plugins at their latest release, each one `plugins.json` at an `https://` address
([sources.md](sources.md)).

- **The sources.** Dereth's own source is there from the start, and nothing is installed. The
  player adds others by their list's address, once they confirm it.
- **The checks.** A zip is kept only when it is the file its source lists (its size and SHA-256)
  and its manifest says what the entry says: the id, version, API version, permissions and web
  domains ([sources.md](sources.md#checking-a-release)). A release needing a newer API version than
  the client's is not installed. An id under `network.dereth.` installs only from Dereth's own
  source.
- **Updates.** An installed plugin updates only from its own source, to a release whose version
  takes precedence over the installed one. The player updates by hand or lets the update setting
  apply them; a release that asks for a permission or a web domain the installed one did not
  always waits for the player.
- **Where.** Under the launcher's data folder, `plugins/sources.json`, `plugins/installed.json`
  and one folder per plugin with its `current.zip`. The launcher offers the client each installed
  plugin's `current.zip` (`--offer-plugins`), which lists it on the first time it finds it and
  as the player leaves it after that; the retail client gets none.

## The author's tool

The tool that makes a plugin repository, checks a plugin as a host checks it, packs, builds,
releases and lists it in a source lives with the Dereth project, at
<https://github.com/jacobjohnston/dereth>. Pyreal does not ship it.

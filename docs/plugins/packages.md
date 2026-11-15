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
and `.zip` files in it), and the headless client `--plugins <folder or zip>` (a package, or a folder
of them).

## Its hash

A zip file's SHA-256 identifies the download: a plugin source lists it beside the zip's address,
and the author's tool writes it beside a zip it packs (`<zip>.sha256`, a `sha256sum` line). Packing
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

`pyreal-plugin` (in the workspace's `tools/plugin`; [publishing.md](guide/publishing.md) walks
through it):

```text
pyreal-plugin new <folder> [--id <id>] [--name <name>] [--github <username>/<repo>]
                                          a new plugin repository, from the template
pyreal-plugin check <folder>              every check a host makes at load, and every script compiled
pyreal-plugin pack <folder> <out.zip>     the folder as a zip, and <out.zip>.sha256 beside it
pyreal-plugin hash <zip>                  a package zip's id, version and SHA-256
pyreal-plugin build <crate> <package>     a Rust plugin built as a component: <package>/plugin.wasm
pyreal-plugin release <folder> <version> [--push]
                                          set the version, close the change log's section, commit, tag
pyreal-plugin notes <folder> <version>    that version's change-log section
pyreal-plugin find <repository> <tag>     the plugin a release tag names
pyreal-plugin add <plugins.json> <zip> <url> [--notes <file>] [--name <name>]
                                          list the zip's release in a source
pyreal-plugin latest <repository> <releases.json>
                                          the zip of each plugin's latest GitHub release, to download
pyreal-plugin site <repository> <releases.json> <zips> <out> [--name <name>]
                                          the repository's source made from its GitHub releases
```

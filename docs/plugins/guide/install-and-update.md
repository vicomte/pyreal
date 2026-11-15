# Installing and updating plugins

**Provenance: design.** A walk-through of how each Dereth host finds plugins; the rules for
packages are in [packages.md](../packages.md), and plugin sources' lists in
[sources.md](../sources.md).

A plugin arrives as a **package**: a folder with `plugin.toml` at its root, or a zip of one. Every
host loads packages the same way once it has them ([lifecycle](lifecycle-and-sandbox.md#loading));
what differs is where each looks. Players get plugins from **plugin sources**: lists of plugins at
their latest release, each one `plugins.json` at an `https://` address. The launcher starts
with Dereth's own source and nothing installed; a player adds others by their
address.

**Every plugin the client finds is listed in the plugin panel** (`/plugin`), after the plugins
that are loaded. A plugin you installed or added on the launcher's Plugins page is on the first time the client finds it, and loads at once. Any other, one
you put in the client's `plugins` folder yourself or one that comes with the client, is listed with
the state `disabled`: choose it and press **Enable**, or type `/plugin enable <id>` (`/plugin list`
names each id). A plugin enabled loads at once, as any plugin does, and the client remembers it:
it loads at every start until you disable it (**Disable**, or `/plugin disable <id>`). One you
disable stays off, though the launcher still offers it. Its windows start closed: open them from
the plugin bar.

| host | where it finds plugins | updates |
|---|---|---|
| the launcher, for the desktop client | installed one by one from plugin sources: Dereth's own, and the ones you add; and the plugins in the folders on this computer you add, as they are on disk | from each plugin's own source: when you press **Update**, or on their own as the update setting says; a folder on this computer is never updated: edit it, and `/plugin reload <id>` |
| the desktop client | its settings folder's `plugins` folder, what the launcher offers (its installs and the folders on this computer), and the plugins beside the program; each `--plugins` folder loads at once | replace the folder or zip; `/plugin reload <id>`, or `--plugin-dev` |
| the retail client, through Decal | the desktop client's `plugins` folder, what the launcher offers (its installs and the folders on this computer), and the plugins beside Dereth Plugins; the launcher's are enabled the first time it finds them, and the others disabled until enabled in its plugin panel | as the desktop client |
| the headless client | each `--plugins` folder or zip, and `plugin load` in a script, loaded as named | the next run reads the files afresh |

## Dereth's plugins

The [sample plugins](samples/README.md) are in the repository, in `pyreal/plugins/`: the trackers,
the hunting macro and the navigator in the manner of the add-ons players used, and the smaller
samples. They reach players through **Dereth's own source**, which lists each of them and which
the launcher starts with: install the ones you want, as from any source. No release carries them
installed.

A client built from the repository finds what any client finds, and not the samples by itself:
name their folder with `--plugins pyreal/plugins`, or add it as a folder on this computer on the
launcher's Plugins page ([running-the-samples.md](running-the-samples.md)).

## The launcher: plugin sources

The launcher's **Plugins** page installs plugins for the Dereth client from plugin sources, and
keeps them updated.

The page has four sections, **Installed**, **Available**, **Sources** and **Make Your Own** (a new
plugin's repository, or a new plugin in one, [publishing.md](publishing.md#a-new-plugin-repository));
a section's heading folds it away and brings it back. Installed is open when the page opens, and
the others are folded. A plugin's row shows its name, version and source and the first
line of what it does; click it and it opens to everything known about it: the whole description,
its id, authors and homepage, what it uses (its permissions and the web domains it reaches), when it
was released, what changed in it and its size. One row is open at a time.

**Sources.** Each source is a line: its name, how many plugins it lists and its address, or why it
could not be read. Dereth's own source is always there. To add another, paste the address of its
`plugins.json` under **Sources** and press **Add source**; a line under it says that Dereth does not
review plugins from other sources, and **Add** adds it. **Remove** takes a source away; the plugins
installed from it stay, with no more updates. The launcher reads every source each time it starts,
and again when you press **Refresh**, which says what is new: plugins a source lists now, updates
applied and waiting, and sources that could not be read.

**Available.** The plugins the sources list that you have not installed, each saying which source
lists it. **Search** narrows the list. Press **Install** on the one you want. The launcher downloads
its zip and keeps it only when it is the file the source lists and its manifest says what the
source says ([the checks](../sources.md#checking-a-release)). A release that needs a newer plugin
API than your client says **Needs a newer Dereth** and cannot be installed. An id under
`network.dereth.` installs only from Dereth's own source. An installed plugin is enabled the first
time the client finds it; disable it in the client's plugin panel, and it stays off.

**Installed.** Each installed plugin updates only from the source it came from, only to a newer
release. When an update waits, its row says so, with what the release asks for that the installed
one did not; **Update** installs it. **Remove** deletes the plugin.

**Folders on this computer.** A plugin you are writing, or one you were given as a folder, runs
from its folder with no build and no release. Under **Installed**, press **Add Plugin Folder** and
choose one plugin's folder (the one holding `plugin.toml`) or a plugin repository (a folder whose
folders include at least one plugin's folder); it is added at once. Any other folder is refused,
one whose plugins are all further down too. Every plugin in the folder is listed under Installed
marked **Local**, found as `pyreal-plugin` finds a repository's plugins (the folders directly in
it, build output left out). The client is offered each one's folder as it is on disk: no zip, no
check against a source, never updated. It is on the first time the client finds it, as an
installed plugin is, and after an edit type `/plugin reload <id>` there (or start the client with
`--plugin-dev`). Open a Local row: its **Location** line has **Open**, which shows the folder, and
**Remove**, which takes the folder off the list and deletes nothing. A folder that no longer holds a
plugin says so on its row. **Make Your Own** adds the folder it wrote in, so a new plugin is there at
once, and keeps a copy of `pyreal-plugin` at its root
([publishing.md](publishing.md#a-new-plugin-repository)).

A plugin on this computer with the id of an installed plugin is the one the client loads, Dereth's
own ids too (the rule that those come only from Dereth's source is about downloads). Both rows say
so; the installed one stays, and keeps updating from its source.

**Automatically update plugins** decides which plugins update on their own when the launcher finds
new releases:

| setting | what happens |
|---|---|
| None | nothing updates on its own; the launcher says that updates are waiting, and you update each one |
| All | every plugin updates on its own |
| Only Dereth | the plugins from Dereth's own source update on their own; the others wait (the setting at first) |

Whatever the setting, a release that asks for a permission or a web domain the installed one did
not waits for you, and one that needs a newer Dereth is never installed.

The launcher offers the Dereth client each plugin on this computer's folder, then each installed
plugin's zip, with `--offer-plugins`; the retail client gets none. A source that cannot be read
when the launcher starts says so on its row under **Sources**, not in the notice. On disk the
plugins are under the launcher's data folder (on Windows `%LOCALAPPDATA%\Dereth\launcher`), in
`plugins`: `sources.json` (the sources, the last list each gave, the settings and the folders on
this computer), `installed.json`, and a folder per plugin with its `current.zip`.

## The desktop client: its plugins folder

The client finds every package in the `plugins` folder of its settings folder (folders holding
`plugin.toml`, and `.zip` files), creating the folder if it is missing, and lists each disabled
until you enable it:

| system | the plugins folder |
|---|---|
| Windows | `%APPDATA%\Pyreal\plugins` |
| macOS | `~/Library/Application Support/Pyreal/plugins` |
| Linux | `$XDG_CONFIG_HOME/pyreal/plugins` (`~/.config/pyreal/plugins`) |

To install a plugin by hand, put its folder or zip there and start the client, then enable it.
To update it, replace the files and `/plugin reload <id>`. With no plugin at all, the plugin panel
(`/plugin`) and `/plugin list` say where this folder is.

| switch | does |
|---|---|
| `--plugins <folder>` | loads the package in the folder (or zip), or every package in it, at once; repeatable |
| `--offer-plugins <folder>` | lists the package in the folder (or zip), or every package in it, beside the plugins folder's, each on the first time the client finds it; repeatable (the launcher passes its installs and the folders on this computer) |
| `--no-plugins` | starts with no plugins at all, as the retail client runs |
| `--plugin-dev` | reloads a plugin whenever one of its files changes (checked once a second) |
| `--plugin-policy <file or https URL>` | the world's plugin policy document ([policy.md](../policy.md)); the launcher passes the world's own |

Stores are kept beside the plugins folder in `plugin-store`, one folder per plugin id and one JSON
file per store scope; which plugins are enabled and which you disabled, and where their windows
stand, are kept there too, under `dereth.plugins`.

## The headless client

`--plugins <folder or zip>` loads a package, or every package one level inside a folder;
repeatable. `plugin load <folder or zip>` in a script loads one more.

| switch | does |
|---|---|
| `--plugin-store <folder>` | keeps stores in the folder; in memory when absent |
| `--plugin-policy <file>` | the world's plugin policy document |
| `--peers <folder>` | the plugins' bus between clients reaches the other headless clients naming the same folder (loopback datagrams; nothing leaves the machine) |

## Publishing a plugin

A plugin is published by listing its release in a plugin source. The author's tool,
`pyreal-plugin`, makes a plugin repository whose GitHub workflows check every push and, on a
release tag, pack the plugin and list it in the repository's own source:
[publishing.md](publishing.md).

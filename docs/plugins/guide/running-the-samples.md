# Running the game with the samples

**Provenance: design.** How to load Dereth's own plugins and what each shows.

The eleven samples are in `pyreal/plugins/` of the repository, one folder per plugin. The client
can load the whole folder at once: a folder of packages loads each package in it.

## In the desktop client

**From source**, a client built in the repository finds what any client finds, and not the
samples by itself. To load them all at once, at the repository's root:

```text
cargo run -p pyreal-client -- --plugins pyreal/plugins
```

with the switches you normally start it with: a `--plugins` folder loads at once. Or add
`pyreal/plugins` as a folder on this computer on the launcher's Plugins page: each sample in it is
enabled the first time the client finds it. **With the launcher**, install the ones you want from
Dereth's own source on its Plugins page
([install-and-update.md](install-and-update.md#the-launcher-plugin-sources)) and play a Dereth
world as usual: each is on the first time the client finds it. Or copy the sample folders into the
client's plugins folder (`%APPDATA%\Dereth\client\plugins` on Windows; the other systems are in
[install-and-update.md](install-and-update.md#the-desktop-client-its-plugins-folder)), and
enable the ones you want in the plugin panel.

A sample enabled loads at once; nothing is asked. Its windows start closed: open each from the
plugin bar. What a sample does without a click of yours is bounded by the world's
[policy](../policy.md): a world that publishes none allows everything.

Type `/plugin` for the plugin panel: every plugin and its state, with **Enable**, **Disable**,
**Reload** and **Reload All**. What a sample prints, and any error it meets, is in the chat. The
desktop client offers every capability the samples use, so all nine load.

## What to look for

| sample | try | you should see |
|---|---|---|
| [XP Meter](samples/xp-meter.md) | kill something; `/xp`; **Reset** | the session's experience and hourly rate in its window, and on `/xp` the total it has ever watched |
| [Chat Filter](samples/chat-filter.md) | in its window add a rule hiding `golem`, then fight one; a rule logging `tell` | lines containing the word hidden and counted; your tells on its Log page |
| [Looter](samples/looter.md) | open a corpse, click **Loot**; pick a profile | the items its profile keeps moved to the pack and counted in its window; the profile's rules in plain words |
| [Trackers](samples/trackers.md) | fight; `/track xp`; `/track mana` | damage dealt and taken by creature and element, crits, evades and kills in its window and its HUD; experience and luminance per hour; each worn item's mana and time left |
| [ID Notices](samples/id-notices.md) | open a corpse or chest; walk near a portal; set the value in its window; `/idn watch drudge`, then walk near one | `[ID]` lines for valuable items, rares, named spells and scrolls you do not know; portals, players and the drudge coming into view, the drudge with a toast |
| [Inventory](samples/inventory.md) | log in two characters in turn; `/inv find ring` on the second; appraise a ring; `/inv stack` | both characters' rings listed with where each is; a line of what the ring is; partial stacks merged |
| [Navigator](samples/navigator.md) | type `holt` in Where to?, pick Holtburg, **Go**; `/nav travel portal Drudge Hideout` | the town found as you type, an arrow and markers toward it, the distance on a HUD, and the character going there; a journey into the dungeon through its portal, drawn on the map and radar |
| [Auto-login](samples/autologin.md) | pick your character in its window, log out | the character entered again as soon as the character list is up |
| [Follow](samples/follow.md) | two clients: **Lead** in one, pick the leader and **Follow** in the other | the second character following the first |

Automation needs the world's leave: the Looter's **Auto**, the Tank, Follow walking to an unseen
leader and the Navigator carrying on along a route all act with no gesture behind them, so each needs its `auto.<domain>` permission and a world that allows the
domain ([permissions](../permissions.md#gestures-and-automation)). A click or a typed command of the
plugin's own gives it 30 seconds of activation in which its actions need only `act.interactive`.

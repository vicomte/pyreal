# The Dereth Network

What a world on the Dereth Network sends this client, and what the client sends it.

**Provenance:** the network's specification, kept with Empyrean in the Dereth fork
(`docs/network/`, chiefly `01-addresses-and-settings.md` and `03-world-behaviour.md`), and
Empyrean's implementation (`empyrean/crates/world/src/federation/`).

**Readers:** no reader of its own. Every message here is a retail message on the pages before this
one. Pinned by the scenarios in `pyreal/testkit/tests/cpu/network.rs` and the channel-line tests in
`core/client-runtime/src/hud/tests.rs`.

## 1. What the network is to a client

The Dereth Network links worlds through a hub: cross-world tells, friends with online status, a
shared Global room and invite-only channels. A world speaks to the hub over a WebSocket of its own.
A client speaks only to its world, in retail messages, so the network needs no sign-in, no handshake
and no opcode of the client's. A world that has not joined the network behaves as it always did.

## 2. Addresses and stand-in ids

- **An address** is `Name@world`, the world named by its slug. A channel's address is
  `name@channel`. No character's name holds an `@`, so a name that does is an address.
- **A stand-in id** is the id a world gives each remote identity (a character's address or a
  channel's), from `0x60000001` to `0x6FFFFFFF`. No object of a world has one. The client makes a
  tell's sender a link and its reply target only for ids from `0x50000001` to `0x6FFFFFFF`, which
  takes in the whole block.
- **The Discord mark.** A line a player said from Discord shows its speaker as
  `Ann@coldeve (Discord)`. The world takes a trailing `(Discord)` off any name it is given, so a
  clicked name works as it is.

## 3. The messages

| what | opcode | direction | as the network uses it |
|---|---|---|---|
| tell by name | `0x005D` | C2S | the target is an address; `@tell Bob@coldeve, text` |
| tell by id | `0x0032` | C2S | `@r` to a remote teller, by its stand-in id |
| spoken line | `0x0015` | C2S | `@dn ...`, `@derethnetwork ...` and `@dng <text>`: not commands of the client, so they go as spoken lines, `/` rewritten to `@` |
| heard tell | `0x02BD` | S2C | a remote tell: the sender's address and stand-in id, the player as the target |
| text box string | `0xF7E0` | S2C | the echo `You tell Bob@coldeve, "..."`, the `@dn` answers, and the friends' online and offline lines, whose name is a `<Tell:IIDString:id:name>` link |
| weenie error | `0x028A` | S2C | `0x052B` *Character not available* for a tell that did not reach anyone; `0x0561` for a full friends list |
| weenie error with string | `0x028B` | S2C | an away recipient's away text |
| chat room event | `0xF7DE` | S2C | a Global room line, in the room that carries it (Roleplay unless the hub moved it), under the speaker's address and stand-in id |
| add friend | `0x0018` | C2S | by name, an address |
| remove friend | `0x0017` | C2S | by the friend's stand-in id |
| friends update | `0x0021` | S2C | rows under stand-in ids: added, removed, and a status change when a remote friend comes online or goes |
| modify character squelch | `0x0058` | C2S | an address by name with id 0; the world keeps it, and it is not in the client's own squelch list |

## 4. Channels

A channel reaches every member as a tell from the channel: its address, its stand-in id, and text
`Speaker@world: line`. The world echoes a line told to a channel as `You tell name@channel, "line"`.
This client draws both as channel lines instead:
`[name] Speaker@world says, "line"` and `[name] You say, "line"`, in the channel's colour, the speaker
a link that tells them by name. Only a sender with a stand-in id whose name is a channel's address
is drawn this way (`pyreal_client_model::chat::composition::network_channel`). A reply goes to the
channel by its id.

## 5. What this client does not do

Pyreal reports nothing to the hub and fetches nothing from it: no player count at login, and no
world list or character directory from the hub. A player reaches everything of the network through
their world, with `@dn help`.

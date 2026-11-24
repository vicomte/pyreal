# The 1999 protocol layer

**Provenance: mixed.** The login layout and the checksum are **wire format**. "The refusal it
answers with" and the servers' answers to the probe were **observed at runtime** against local
AC1999 and Empyrean servers. Where the layer sits, and how Pyreal learns and keeps a world's wire,
is **design opinion**: Pyreal's own.

The AC1999 project's server speaks the protocol of the client released in November 1999 (version
`4.35752`). It is a different wire from the one pages 01 to 04 describe: another packet header and
checksum, a plain-text login of at most 20 characters, its own login and character messages, its
own data patching and older layouts for objects and the player. Pyreal speaks it itself, inside
its own process, and translates it onto the later messages the rest of the client already
handles. There is no helper program, and the player never chooses a protocol.

## Where it sits

- `core/wire1999` (`pyreal-wire1999`) is the wire: packets, checksums, keys, reliability, the 1999
  messages and the translation state. Like `pyreal-transport`, it opens no socket and reads no
  clock; its caller supplies both.
- `Link1999` is a `pyreal_primitives::Transport`, the same seam the later wire sits under. Inbound,
  it reassembles a 1999 message, reads it by the 1999 rules and hands up the later message the
  session dispatches. Outbound, it takes the later message the session sends and passes it,
  rewrites it or drops it. Each area of the game is a module with its own state
  (`link::modules()`).
- The client runtime's 1999 driver (`core/client-runtime/src/net/ac1999.rs`) sits beside the later
  `ClientNetwork` driver: login, grant and acknowledgement on one port, then the same frame pump.
  The runtime picks it when the world is started with `--wire 1999`
  (`Config::wire_1999`).

## How Pyreal knows a world speaks it

A world record carries the wire its server speaks, `World::wire` in `pyreal-launch`: `1999`,
`later`, or nothing yet. The front door passes `--wire 1999` for a 1999 world and nothing for any
other, so the later driver stays the default. A 1999 world plays the Launch era with the 1999
`portal.dat` and `cell.dat`.

The wire is learned in three ways:

1. **Built in.** AC1999 (`ac1999.cobradragon.com`, port 9110) is in
   `pyreal_launch::known::BUILT_IN_WORLDS`. It is always on the Known Worlds list, and a world added
   at that address by hand is the same world.
2. **The probe.** A world added by hand is probed the first time it is played, on a thread of its
   own while the screen says "Checking the world...". The answer is kept in `worlds.json` with the
   world, so it is asked once. Changing the world's address forgets it.
3. **Its status ping.** A world that answers the status ping (06) speaks the later wire, and is
   not probed.

## The probe

Servers of both wires answer one login without an account behind it: the server-tracker login, for
the account `acservertracker`. They refuse it, or start a connection and drop it, and create
nothing. Server trackers have used it for years. Each wire's server reads that login only in its
own form, so Pyreal sends both, the later one first (`pyreal_wire1999::probe`):

1. **The later form**, twice, 600 ms apart: a plain login request (01, the `0x10000` section) for
   `acservertracker:jj9h26hcsggc`, with no password field. A later server answers with a packet of
   its wire whose checksum holds: Empyrean and ACE send the connect request (`0x40000`), GDLE a
   login failure.
2. **The 1999 form**, twice, 600 ms apart: the 1999 login, below. A 1999 server answers with its
   login failure.

| Answer | What it means |
|---|---|
| A later packet with a good checksum, at any point | the later wire |
| No later answer, then a 1999 refusal, grant or grant error with a good 1999 checksum | the 1999 wire |
| Nothing for either form (about 2.4 seconds) | down or unreachable; played as the later wire and asked again next time |

A 1999 packet that answers the later form moves straight on to the 1999 form, since only the 1999
form settles it. The AC1999 server sends nothing for the later form and logs it as a packet with no
session.

The probe is also the world's up or down check: any answer means the world is up. Neither form
brings a count of players.

### The 1999 login it sends

One datagram of 116 bytes: a 20-byte header and a 96-byte body. All numbers are little-endian.

| Offset | Bytes | Value |
|---|---|---|
| 0 | 4 | sequence 1 |
| 4 | 4 | flags `0x2`, the checksum flag |
| 8 | 4 | recipient 0 |
| 12 | 4 | checksum |
| 16 | 2 | body length, 96 |
| 18 | 2 | time and table, 0 |
| 20 | 4 | `0x075BCD15` |
| 24 | 2 | 8, the length of the version with its terminator |
| 26 | 8 | `4.35752` and a zero |
| 34 | 2 | zero, to a four-byte boundary |
| 36 | 4 | `0xBEEFBEEF`, the account marker |
| 40 | 20 | `acservertracker:jj9h`: the account, a colon and the password cut to the field |
| 60 | 4 | 0 |
| 64 | 4 | `0xBEEFBEEF`, the second account field's marker |
| 68 | 24 | zero |
| 92 | 4 | 1 |
| 96 | 4 | 0, no ticket |
| 100 | 4 | 102, the 1999 CD's portal iteration |
| 104 | 4 | 67, the 1999 CD's cell iteration |
| 108 | 4 | 0 |
| 112 | 4 | `0xFFFFFFFF` |

The checksum is the 1999 wire's generic one: the header hashed with `0x03291975` in the checksum
field, plus the body's hash less its length shifted up 16 bits. The server reads the account up to
the colon. The iterations are never 0: the AC1999 server logs a whole login that reports 0.

### The refusal it answers with

The local AC1999 server answered with 28 bytes: sequence 0, flags `0x100000` and no checksum flag,
recipient 11, the generic checksum, a body length of 8, and the body `54 9C DF 04 08 00 00 00`.
The first word is the string id for a failed log-on.

## The login limit

The 1999 login carries `account:password` in 20 lower-case printable characters, so the account
name and the password together can have at most 19. Pyreal refuses a longer pair before it
connects and says so on the login screen, rather than cutting it.

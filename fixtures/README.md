# Fixtures

Two things live here, both built from traffic recorded between the retail client and a local ACE
server. Nothing here is derived from the retail data files, and nothing is hand-edited except the
descriptions in `packet-captures/manifest.json`.

These are the shared recordings, which the client's tests read.

| directory | what it is | how it is made |
|---|---|---|
| [`packet-captures/`](packet-captures/README.md) | the recorded sessions, one `<slug>.jsonl` per recording (the proxy's `{t, dir, pair, len, data}` datagrams, pseudonymised in place), with `index.json` (the per-recording datagram census) and `manifest.json` (what is in each recording) | recorded and pseudonymised in the Dereth fork with its `dereth-corpus record <slug>`; see [`packet-captures/README.md`](packet-captures/README.md#adding-one). The recordings cannot be regenerated. |
| `message-corpus/` | every recording's reassembled messages, `<slug>/blobs.jsonl` (`{idx, dir, t_rel, queue, blob_id, opcode, payload_hex}` per message, both directions, in the order each message's last fragment arrived), and `index.json` (per-recording message counts and the client opcode census) | generated from the recordings with `pyreal-transport`'s reassembly |

`packet-captures/index.json`, the generated fields of `packet-captures/manifest.json` and all of
`message-corpus/` are generated from the recordings by the Dereth fork's `dereth-corpus corpus`
and copied here with them. Pyreal has no tool that regenerates or checks them. The two
unclean-logout recordings are corpus members but deliberately end without a `Disconnect`, so the
capture index lists them under `without_disconnect` and they are left out of everything else.

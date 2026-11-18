# e4mc-for-rust

Native Rust replacement for the [e4mc](https://e4mc.com/) Minecraft mod, built for
[Horizon](https://github.com/horizon-rust/horizon) — Minecraft: Java Edition rewritten
from scratch in Rust. Java mods do not run on Horizon, and e4mc itself has no Horizon
build — this project fills the gap with a small standalone tunnel instead.

Horizon's built-in *Play with Friends* covers most of it (invite codes, encrypted
tunnel, optional relay), but hosting from a home connection with no public IP — CGNAT
and no UPnP — needs an outward-only path. `e4mc-tunnel` is exactly that: the host dials
out to a public relay, friends dial in to the address it prints. No VPN, no VPS, no
router settings, completely free.

## How it works

1. Open your Horizon world to LAN and note the port it prints.
2. Run the tunnel pointed at that port:

   ```
   e4mc-tunnel --port 25565
   ```

3. The tunnel connects outbound to the e4mc broker, which hands back the least loaded
   relay (`https://broker.e4mc.link/getBestRelay`), dials it over QUIC and asks it for
   a public domain; the tunnel prints one:

   ```
   your world is live at meadow-brook.eu.e4mc.link (java edition, TCP only)
   ```

4. Share the domain. Friends join from the multiplayer menu of any Horizon client —
   iPhone, Android or desktop — as they would join any server. Only the host needs the
   tunnel; guests need nothing at all.

Everything except the host runs an unmodified client, since the relay just bridges
their QUIC streams to the LAN port where the world listens.

## Building

```
cargo build --release
```

The single binary is `target/release/e4mc-tunnel` (Rust 2024, no runtime dependencies).

## Options

| flag | meaning |
| --- | --- |
| `--port <PORT>` | TCP port of the Horizon LAN world on this machine |
| `--custom-relay <HOST:PORT>` | use this relay instead of asking the broker |
| `--broker-url <URL>` | broker address returning `{id, host, port}` JSON |
| `--extra-certs <PEM>` | extra root certificates for a private relay |
| `--backoff-seconds <N>` | base of the exponential backoff between reconnect attempts |

The tunnel keeps retrying while it runs: the address survives a relay drop, so the
world opening a new session does not reset the domain.

## iOS and Android

Horizon community forks — [iOS](https://github.com/MonZ05/horizon-ios) and
[Android](https://github.com/tlno25/horizon-android) — speak the same protocol, so
friends on phones join with the tunnel's domain like desktop clients do. Because the
tunnel is plain Rust with no Java, it can later be packed as a Horizon native mod
(`.hmod`) that opens the world to the internet from the in-game menu itself.

## Compatibility notes

* the control protocol matches e4mc's Java mod byte for byte (ALPN `quiclime`,
  varint-prefixed JSON on a bidirectional control stream, raw QUIC streams bridged to
  the LAN port);
* the domains stays assigned for as long as the tunnel runs;
* a private relay needs its own certificate, passed as `--extra-certs`; the broker is
  never contacted in that case;
* when the LAN port comes from Horizon's *Open to LAN* screen it changes every time
  you open the world — pass the printed port each time.

## Privacy

The relay bridges encrypted QUIC streams and the broker only answers with relay
addresses; neither sees the world's traffic. The shareable domain points friends to
the relay, never to your home address, so your IP stays private.

## Personal project

This is a personal tool made for hosting a Horizon world for friends. It is provided
as-is with **no support**: issues may sit unanswered, features land when the author
feels like it. Do not expect help — forks are yours to maintain.

Not affiliated with e4mc or SkyVG. The public relay and broker are used through their
open, documented protocol; both projects are MIT-licensed ([mod](https://github.com/vgskye/e4mc-minecraft-architectury),
[relay](https://github.com/vgskye/e4mc-quiclime)), and this repository ships its own
independent Rust implementation.

## License

Works today with the public e4mc network, live end to end: the broker hands back a
relay, the tunnel completes the control handshake (the relay answers
`dialtone_sidecar` capabilities), a domain is assigned, and the session holds. Open
an issue if your run misbehaves, with `RUST_LOG=debug`.

## License

[MIT](LICENSE)

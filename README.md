# e4mc-for-rust / e4mc-for-rust

[RU](#ru) · [EN](#english)

## RU

Нативная замена мода [e4mc](https://e4mc.com/) для
[Horizon](https://github.com/horizon-rust/horizon) — Minecraft: Java Edition,
переписанного с нуля на Rust. Java-моды на Horizon не запускаются, а у самого e4mc
Horizon-сборки нет — эту нишу закрывает небольшой standalone-туннель.

Встроенный «Играть с друзьями» закрывает большинство сценариев (invite-коды,
шифрованный туннель, опциональный релей), но хостинг с домашнего интернета без
белого IP — CGNAT и без UPnP — требует пути только на исходящие. `e4mc-tunnel`
ровно это и делает: хост цепляется наружу к публичному релею, друзья заходят по
адресу, который он печатает. Без VPN, без VPS, без настроек роутера, бесплатно.

### Запуск

1. Открой мир Horizon по LAN и запомни порт, который он печатает.
2. Подними туннель на этот порт:

   ```
   e4mc-tunnel --port 25565
   ```

3. Туннель идёт наружу к брокеру e4mc, который отдаёт наименее загруженный релей
   (`https://broker.e4mc.link/getBestRelay`), устанавливает соединение по QUIC и
   запрашивает публичный адрес; туннель печатает его:

   ```
   your world is live at meadow-brook.eu.e4mc.link (java edition, TCP only)
   ```

4. Кидаешь адрес друзьям — они заходят из меню мультиплеера любого клиента Horizon:
   iPhone, Android, ПК — как на обычный сервер. Туннель нужен только хосту; друзьям
   не надо ничего.

Все, кроме хоста, сидят на немодифицированном клиенте: релей просто бриджит их
QUIC-стримы к LAN-порту, на котором слушает мир.

### Сборка

```
cargo build --release
```

Один бинарник: `target/release/e4mc-tunnel` (Rust 2024, без runtime-зависимостей).

### Флаги

| флаг | смысл |
| --- | --- |
| `--port <PORT>` | TCP-порт LAN-мира Horizon на этой машине |
| `--custom-relay <HOST:PORT>` | свой релей вместо запроса брокеру |
| `--broker-url <URL>` | адрес брокера, отвечающего `{id, host, port}` JSON |
| `--extra-certs <PEM>` | PEM с корнями для Частного релея |
| `--backoff-seconds <N>` | базовая задержка между попытками реконнекта |

Туннель держит реконнект, пока работает: адрес переживает drop релея, так что новое
открытие мира не сбрасывает домен.

### iPhone и Android

Форки Horizon — [iOS](https://github.com/MonZ05/horizon-ios) и
[Android](https://github.com/tlno25/horizon-android) — говорят тем же протоколом:
и с айфона, и с андроида, и с ПК заходят по домену туннеля, как десктопные. Туннель —
чистый Rust без Java, так что позже он пакуется как нативный мод Horizon (`.hmod`),
открывающий мир в интернет прямо из игрового меню.

### Условия/совместимость

* контрол-протокол совпадает с Java-модом e4mc байт в байт (ALPN `quiclime`,
  JSON с varint-префиксом на bidirectional контрол-стриме, raw QUIC-стримы под
  guest-мост);
* домен живёт, пока живёт туннель;
* частный релей требует свой сертификат (`--extra-certs`), брокер в этом случае
  не запрашивается;
* LAN-порт Horizon печатается заново при каждом открытии мира — передавай каждый раз
  тот, что напечатан.

### Приватность

Релей бриджит шифрованные QUIC-стримы, брокер отвечает только адресами релеев:
мир никто не видит. Домен указывает друзьям на релей, а не на твой домашний адрес —
IP остаётся твоим.

### Личный проект

Личный проект для игры с друзьями на Horizon. «Как есть», **без поддержки**: issue
могут висеть без ответа, фичи падают когда захочу —форки твои, поддерживай сам.

Не связан с e4mc и SkyVG. Публичный релей и брокер используются через их открытый
задокументированный протокол; оба проекта под MIT ([мод](https://github.com/vgskye/e4mc-minecraft-architectury),
[релей](https://github.com/vgskye/e4mc-quiclime)), а этот репозиторий — своя
независимая реализация.

### Благодарности

Идея, протокол и вся relay-инфраструктура — оригинальный
**[e4mc](https://github.com/vgskye)** (SkyVG):

- [e4mc](https://modrinth.com/mod/qANg5Jrr) — Java-мод, с которого всё началось;
- [e4mc-quiclime](https://github.com/vgskye/e4mc-quiclime) — релей;
- брокер, dialtone, домены `*.e4mc.link` — их инфраструктура.

Весь кредит — им. Этот репозиторий — лишь адаптация их подхода под Rust/Horizon.

### Лицензия

[MIT](LICENSE)

---

## English

Native Rust replacement for the [e4mc](https://e4mc.com/) Minecraft mod, built for
[Horizon](https://github.com/horizon-rust/horizon) — Minecraft: Java Edition rewritten
from scratch in Rust. Java mods do not run on Horizon, and e4mc itself has no Horizon
build — this project fills the gap with a small standalone tunnel.

Horizon's built-in *Play with Friends* covers most of it (invite codes, encrypted
tunnel, optional relay), but hosting from a home connection with no public IP — CGNAT
and no UPnP — needs an outward-only path. `e4mc-tunnel` is exactly that: the host dials
out to a public relay, friends dial in to the address it prints. No VPN, no VPS, no
router settings, completely free.

### Usage

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

### Building

```
cargo build --release
```

The single binary is `target/release/e4mc-tunnel` (Rust 2024, no runtime dependencies).

### Options

| flag | meaning |
| --- | --- |
| `--port <PORT>` | TCP port of the Horizon LAN world on this machine |
| `--custom-relay <HOST:PORT>` | use this relay instead of asking the broker |
| `--broker-url <URL>` | broker address returning `{id, host, port}` JSON |
| `--extra-certs <PEM>` | extra root certificates for a private relay |
| `--backoff-seconds <N>` | base of the exponential backoff between reconnect attempts |

The tunnel keeps retrying while it runs: the address survives a relay drop, so the
world opening a new session does not reset the domain.

### iOS and Android

Horizon community forks — [iOS](https://github.com/MonZ05/horizon-ios) and
[Android](https://github.com/tlno25/horizon-android) — speak the same protocol, so
friends on phones join with the tunnel's domain like desktop clients do. Because the
tunnel is plain Rust with no Java, it can later be packed as a Horizon native mod
(`.hmod`) that opens the world to the internet from the in-game menu itself.

### Compatibility notes

* the control protocol matches e4mc's Java mod byte for byte (ALPN `quiclime`,
  varint-prefixed JSON on a bidirectional control stream, raw QUIC streams bridged to
  the LAN port);
* the domain stays assigned for as long as the tunnel runs;
* a private relay needs its own certificate, passed as `--extra-certs`; the broker is
  never contacted in that case;
* when the LAN port comes from Horizon's *Open to LAN* screen it changes every time
  you open the world — pass the printed port each time.

### Privacy

The relay bridges encrypted QUIC streams and the broker only answers with relay
addresses; neither sees the world's traffic. The shareable domain points friends to
the relay, never to your home address, so your IP stays private.

### Personal project

A personal tool made for hosting a Horizon world for friends. Provided as-is with
**no support** — features land when the author feels like it; forks are yours to
maintain.

Not affiliated with e4mc or SkyVG. The public relay and broker are used through their
open, documented protocol; both projects are MIT-licensed
([mod](https://github.com/vgskye/e4mc-minecraft-architectury),
[relay](https://github.com/vgskye/e4mc-quiclime)), and this repository ships its own
independent Rust implementation.

### Credits

The idea, the protocol and the whole relay infrastructure belong to the original
**[e4mc](https://github.com/vgskye)** (SkyVG):

- [e4mc](https://modrinth.com/mod/qANg5Jrr) — the Java mod that started it all;
- [e4mc-quiclime](https://github.com/vgskye/e4mc-quiclime) — the relay;
- the broker, dialtone and the `*.e4mc.link` domains — their infrastructure.

All credit goes to them; this repository only adapts their approach to
Rust/Horizon as an independent Rust implementation of the same open protocol.

### License

[MIT](LICENSE)

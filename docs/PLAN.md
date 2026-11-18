# План: замена e4mc для Rust Minecraft (Horizon) — e4mc-tunnel

Цель: у игроков Horizon (iPhone, Android, ПК) без белого IP, VPN или VPS открыть
мир Horizon в интернет так же просто, как в Java-майнкрафте открывает мод
[e4mc](https://github.com/vgskye/e4mc-minecraft-architectury).

## Почему свой проект, а не порт

У Horizon уже есть *Play with Friends* (crates `horizon-friends` + `horizon-relay`):
invite-коды, SPAKE2+ChaCha20, UPnP/PCP/STUN, опциональный relay. Но:

1. релей не хостится проектом — за шитом CGNAT (нет белого IP) docs/play-with-friends.md
   честно советует «a relay, or a VPN»;
2. Java-моды (сам e4mc) на Horizon не запускаются — моды там нативные Rust (`.hmod`);
3. [issue #6667](https://github.com/horizon-rust/horizon/issues/6667): «публичный адрес
   мира дома» команда признала нужным, но outside-проектом.

Решение: standalone бинарник `e4mc-tunnel` — Rust, `quinn` + `tokio`, ноль runtime
зависимостей. Следующая вишенка — паковка как нативного мода Horizon (`.hmod`).

## Протокол (с e4mc байт в байт)

| элемент | значение |
| --- | --- |
| ALPN | `quiclime` |
| контрол-стрим | bidirectional, JSON с varint-префиксом; relay отвечает с 1-байтовым префиксом |
| guest-трафик | отдельные QUIC-стримы, raw TCP-мост к LAN-порту |

Serverbound: `ProbeCapabilities`, `RequestDomainAssignment` (тег `kind`, snake_case).
Clientbound: `UnknownMessage`, `HasCapabilities {caps}`, `DomainAssignmentComplete
{domain}`, `RequestMessageBroadcast`, `TicketRegistered`.

Брокер: GET `https://broker.e4mc.link/getBestRelay` → `{id, host, port}`;
fallback релей `test.e4mc.link:25575` (как `Config.java` в Java-моде).

## Готово

- [x] `protocol.rs` — varint codec, типы сообщений, юнит-тесты на формат Java-мода
- [x] `broker.rs` — резолв релея через HTTP-брокер, fallback, юнит-тест формы ответа
- [x] `session.rs` — QUIC-сессия: handshake контроля (foreground), фоновый читатель
      broadcasts, accept-мост guest-стримов к TCP
- [x] `main.rs` — CLI: `--port`, `--custom-relay`, `--broker-url`, `--extra-certs`,
      backoff-реконнект, ctrl_c
- [x] интеграционный тест: self-signed fake relay, echo LAN-сервер, домен + раундтрип
- [x] `cargo test` / `cargo clippy --all-targets` / `cargo fmt --check` — чисто

## Проверено live (end-to-end с публичной сетью e4mc)

Брокер выдаёт релей (`uk`), QUIC + ALPN + контрол-handshake идут, релей отвечает
`HasCapabilities {caps: ["dialtone_sidecar"]}`, домен назначается — `triangle-pope.uk.e4mc.link`,
сессия держится, туннель печатает «Your world is open to the internet».
Причина прежнего «connection lost через 1 с» найдена и устранена: Java-мод не делает
`finish()` контрол-стрима после handshake (QuiclimeSession.java:387-392) — стрим
живёт до disconnect.

## Дальше

- `.hmod`: нативный мод Horizon, открывающий туннель из in-game меню (вместе с
  launcher'ом, как «Host a server from the launcher» в ROADMAP.md Horizon)
- P2P: dialtone/iroh (см. E4MC-Offline), домен переживает drop релея
- DNS SRV `_minecraft._tcp` — vanilla-клиенты его ждут

# AR-FA-PNL Private Server

[![Rust 2021](https://img.shields.io/badge/Rust-2021-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tokio runtime](https://img.shields.io/badge/runtime-Tokio-7E57C2?logo=tokio&logoColor=white)](https://tokio.rs/)
[![SQLite storage](https://img.shields.io/badge/storage-SQLite-003B57?logo=sqlite&logoColor=white)](https://www.sqlite.org/)

A private, client-compatible server implementation focused on reliable
protocol handling, deterministic state transitions, and safe local persistence.

> [!NOTE]
> This repository contains the server component only. Private runtime state,
> client assets, captures, credentials, and development databases are kept
> outside version control.

## Overview

The project provides a compact service boundary for encrypted requests,
protobuf messages, account/session state, gameplay reducers, and persistent
player data. Game rules are loaded from generated, source-locked catalogs so
behavior can be inspected and corrected without turning recorded traffic into
runtime input.

## Built with

| Layer | Technology | Role |
| --- | --- | --- |
| Language | Rust 2021 | Memory-safe server and domain logic |
| Runtime | Tokio | Async listeners, task scheduling, and bounded blocking work |
| HTTP | Hyper 1 + hyper-util | Local HTTP/1 transport boundary |
| Wire format | Prost + prost-reflect | Protobuf encoding, decoding, and descriptor-driven messages |
| Transport security | AES/CBC, `sha2` | Encrypted frame handling and request fingerprints |
| Accounts | Argon2 + UUID | Password hashing, grants, and session identity |
| Persistence | SQLite via `rusqlite` | Atomic state, replay protection, migrations, and recovery |
| Configuration | Serde + TOML | Validated local configuration and rule loading |
| Observability | `tracing` + JSON subscriber | Redacted structured operational logs |

## Architecture

```text
encrypted protobuf frame
          │
          ▼
transport and API boundary
          │
          ▼
domain state reducers ─── generated rule catalogs
          │
          ▼
SQLite storage and replay journal
```

The server keeps transport, API, state, and storage responsibilities separate.
Domain folders own their mutations; storage owns transactions and recovery;
the API translates wire concerns without importing domain implementation
details.

## Source layout

```text
src/
  transport.rs       encrypted framing and protobuf helpers
  api/               request routing and protocol responses
  state/
    combat/          battle model, timeline, actions, and effects
    quest/           quest lifecycle and scoring
    synthesis/       synthesis execution and validation
    home/            missions, rewards, profile, and dispatch
    atelier/         research, collection, memoria, and ships
    activities/      story, exploration, expeditions, and housing
    character/       character rules and reducers
    gacha/           draw and banner state
    shop/            catalog, purchases, passes, and challenges
  storage/           accounts, battles, migrations, mutations, and journal
migrations/          versioned SQLite schema changes
config.example.toml  non-secret configuration shape
```

## Current progress

Status is measured against the active server tree as of 2026-09-12.

| Area | Status | Current boundary |
| --- | --- | --- |
| Server module refactor | Complete for the approved scope | Domain ownership is explicit; legacy compatibility wiring is removed |
| Authentication and launcher contract | Functional | Passwords use stdin; stale handoff grants are replaced safely |
| Tutorial and Burst gate | Partial | Burst unlock is observed; timeline placement still needs confirmation |
| Combat timeline and damage | In progress | Opening slot construction follows loaded data; turn ordering and damage composition remain under repair |
| Combat effects | In progress | Effects are split into modules; 554 rules have explicit policy coverage and 8,058 definitions remain unresolved |
| Missions and achievements | In progress | Broad catalogs exist; chapter/milestone mapping still needs behavioral certification |
| Synthesis | In progress | 876 recipes are loaded; bulk quantity and rating edge cases remain open |
| Navigation artifacts | Refreshed | tgrep: 457 indexed files / 26,969 trigrams; Ripwire: 132 files / 4,799 symbols / 3,738 edges |
| Final acceptance | Pending | One integrated fresh-player/RNG pass and final client verification remain |

## Engineering boundaries

- Client call sites, descriptors, decoded rule data, and repeated verified
  observations are preferred in that order when behavior is uncertain.
- Captures and recorded responses are comparison evidence only. They are never
  replayed, imported, or used to hardcode runtime values.
- Existing player state is preserved. The live SQLite database is not a test
  fixture, and private configuration never belongs in source control.
- Checks stay scoped to the behavior changed; the extensive player simulation
  is reserved for the final integrated tree.
- Unknown publisher behavior remains explicit instead of being filled with
  guessed responses or per-entity exceptions.

## Deliberately deferred

Publisher-owned payment, ranking, social, and event surfaces; historical
step-up decks without authoritative source data; and non-core client content
cleanup remain outside the current core-gameplay acceptance boundary.

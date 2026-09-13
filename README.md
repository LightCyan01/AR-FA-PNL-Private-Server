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

Percentages are working implementation estimates, not client test pass rates.

| System | Status | Boundary |
| --- | --- | --- |
| Atelier research, collection, memoria, ships, and dispatch | 90% | Atelier domain state and dispatch routes |
| Story, exploration, expeditions, housing, and activities | 90% | Activity and story reducers |
| Characters, parties, energy, and progression | 90% | Character, party, energy, and progression state |
| Gacha, shops, passes, challenges, and rewards | 85% | Gacha, shop, and reward routes |
| Quests and battle objectives | 80% | Quest state and objective progression |
| Combat skills, actions, Burst, turn order, and timeline | 70% | Skill selection/execution, Burst availability and placement, independent action/turn cursors, timeline construction, and advancement |
| Damage, penetration, and combat effects | 65% | Base-stat composition, damage policy, penetration, hidden enemy defense modifiers, and effect application |
| Missions and achievements | 65% | Mission counter reconciliation; chapter milestone mapping and live awards |
| Synthesis | 65% | Recipe execution, quantity, and rating |
| Integrated acceptance | 35% | Fresh-player and installed-client verification |

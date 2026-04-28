# IEC 60870-5-104 Simulator

[![GitHub Release](https://img.shields.io/github/v/release/kelsoprotein-lab/IEC104Sim)](https://github.com/kelsoprotein-lab/IEC104Sim/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)]()

[中文版](README_CN.md)

A cross-platform IEC 60870-5-104 protocol simulator built with **Rust** + **Tauri 2** + **Vue 3**, featuring both a Slave (server) and Master (client) application.

## Download

Pre-built installers for Windows, macOS, and Linux are available on the [Releases](https://github.com/kelsoprotein-lab/IEC104Sim/releases) page.

## Features

### Slave (IEC104Slave)

- **IEC 104 server** with TCP and TLS support
- **8 data types**: Single Point, Double Point, Step Position, Bitstring, Normalized, Scaled, Short Float, Integrated Totals
- **Data point management**: Add single or batch points with IOA range and ASDU type selection
- **Random mutation**: Simulate value changes at configurable intervals
- **Spontaneous transmission** (COT=3): Automatically sends changed values to connected masters
- **Cyclic transmission**: Periodic data sending with configurable interval
- **General Interrogation** (GI) and **Counter Interrogation** response
- **Control command handling**: Single, Double, Step, and Setpoint commands
- **Communication log** with hex frame display and CSV export
- Auto-start server on creation

### Master (IEC104Master)

- **IEC 104 client** with TCP and TLS support
- **Real-time data display** with incremental polling and virtual scrolling
- **Category tree** with live point counts (SP, DP, ST, BO, ME_NA, ME_NB, ME_NC, IT)
- **Control commands**: Direct Execute and Select-before-Operate (SbO)
- **Right-click context menu** for quick control actions
- **Value panel** showing selected point details
- **General Interrogation**, **Counter Read**, and **Clock Sync** commands
- **Communication log** with frame analysis

## Architecture

```
IEC104Sim/
├── crates/
│   ├── iec104sim-core/     # Core IEC 104 protocol library
│   ├── iec104sim-app/      # Slave Tauri application
│   └── iec104master-app/   # Master Tauri application
├── frontend/               # Slave Vue 3 frontend
└── master-frontend/        # Master Vue 3 frontend
```

## Prerequisites

- [Rust](https://rustup.rs/) (1.77+)
- [Node.js](https://nodejs.org/) (18+)
- [Tauri CLI](https://tauri.app/) (`cargo install tauri-cli`)

## Quick Start

### Install dependencies

```bash
cd frontend && npm install
cd ../master-frontend && npm install
```

### Run Slave

```bash
cd crates/iec104sim-app
cargo tauri dev
```

### Run Master

```bash
cd crates/iec104master-app
cargo tauri dev
```

### Usage

1. **Slave**: Click "New Server" → server auto-starts on port 2404 with default data points
2. **Master**: Click "New Connection" → enter `127.0.0.1:2404` → Connect → Send GI
3. Master's IOA table displays all received data points
4. **Slave**: Click "Random Mutation" to simulate value changes → Master receives spontaneous updates

## IEC 104 Protocol Support

| Feature | Supported Types |
|---------|----------------|
| Monitor (Slave→Master) | M_SP_NA/TB, M_DP_NA/TB, M_ST_NA/TB, M_BO_NA/TB, M_ME_NA/TD, M_ME_NB/TE, M_ME_NC/TF, M_IT_NA/TB |
| Control (Master→Slave) | C_SC_NA, C_DC_NA, C_RC_NA, C_SE_NA/NB/NC |
| System | C_IC_NA (GI), C_CI_NA (Counter), C_CS_NA (Clock Sync) |
| COT | Spontaneous(3), Activation(6), ActivationCon(7), ActivationTerm(10), Interrogated(20), CounterInterrogated(37) |
| Transport | TCP, TLS (mutual TLS supported) |

## Tech Stack

- **Backend**: Rust, Tokio (async runtime), native-tls
- **Frontend**: Vue 3, TypeScript, Vite
- **Desktop**: Tauri 2

## Changelog

See [CHANGELOG.md](CHANGELOG.md) or the [Releases page](https://github.com/kelsoprotein-lab/IEC104Sim/releases).

### Auto-update

Starting from v1.0.9, both apps check GitHub Releases on startup and prompt the user to install
new versions. Users on v1.0.8 or earlier need to upgrade manually one time.

### macOS install note

The bundles are **not Apple-notarized** (no paid Developer Program). From v1.1.2 the `.app`
inside the dmg is ad-hoc signed, so on first launch macOS shows the standard "unidentified
developer" warning — right-click → **Open** to bypass.

If you downloaded a v1.1.1 or earlier dmg and see **"is damaged, can't be opened, move to
Trash"**, that's the unsigned-app behaviour newer macOS enforces. Run:

```bash
xattr -dr com.apple.quarantine "/Applications/IEC104Master.app"
xattr -dr com.apple.quarantine "/Applications/IEC104Slave.app"
```

…or upgrade to v1.1.2+ (the in-app updater will push it).

## License

MIT

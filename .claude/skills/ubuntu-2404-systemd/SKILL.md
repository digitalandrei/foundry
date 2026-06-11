---
name: ubuntu-2404-systemd
description: >
  For the Foundry project at /opt/foundry. Ubuntu 24.04 + systemd service
  packaging for foundry-controller and foundry-agent: unit files,
  hardening, journald logging, and install scripts. Use when writing or
  changing anything under deployment/ or debugging service behavior.
---

# Ubuntu 24.04 & systemd

Ops context: `docs/DEPLOYMENT.md`. Unit files and install scripts live in
`deployment/` and are the deployable truth — keep them and the docs in
sync.

## Service Layout

| Service | Binary | Config |
|---|---|---|
| `foundry-controller.service` | `/srv/foundry/foundry-controller` | `/srv/foundry/.env` (EnvironmentFile) |
| `foundry-agent.service` | `/usr/local/bin/foundry-agent` | `/etc/foundry-agent/config.toml` (root-only, 0600) |

## Unit Conventions

- `Type=simple`, `Restart=on-failure`, `RestartSec=5`,
  `WantedBy=multi-user.target`.
- Both binaries handle SIGTERM gracefully (finish in-flight task, flush) —
  `TimeoutStopSec` sized to the agent's stop grace period.
- Dedicated system users (`foundry`, `foundry-agent`); the agent user needs
  the `docker` group (and NVML device access) — that is its privilege
  boundary, don't run as root.
- Hardening baseline on both units: `NoNewPrivileges=yes`,
  `ProtectSystem=strict` with explicit `ReadWritePaths`, `ProtectHome=yes`,
  `PrivateTmp=yes`. Relax only with a comment in the unit explaining why
  (agent needs `/var/run/docker.sock` and `/dev/nvidia*`).

## Logging

- Both services log structured JSON to stdout → journald.
- Inspect: `journalctl -u foundry-controller -f` /
  `journalctl -u foundry-agent -f`; no log files of our own.

## Install / Upgrade Flow

- Controller: `cargo build --release -p foundry-controller` →
  `sudo install -m 755 target/release/foundry-controller /srv/foundry/` →
  `sudo systemctl restart foundry-controller` → verify
  `systemctl is-active` + `curl -fsS http://127.0.0.1:<port>/health`.
- Agent: install script in `deployment/agent/` (binary + unit + enroll
  command); upgrades replace the binary and restart — identity in
  `config.toml` survives.
- This host aliases `cp`/`rm` to `-i`: use `\cp -f`, `\rm -f`, or
  `install`.

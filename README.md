<p align="center">
  <img src="logo-new.png" alt="Draco" width="200">
</p>

<p align="center">
  <sub>Logo inspired by a reference image generated with <a href="https://www.craiyon.com/pt/image/Hem-omdSQoWd0VBfMnBbHg">Craiyon</a>.</sub>
</p>

<p align="center">
  <a href="https://github.com/britors/Draco/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/britors/Draco" alt="License">
  </a>
  <a href="https://github.com/britors/Draco/issues">
    <img src="https://img.shields.io/github/issues/britors/Draco" alt="Issues">
  </a>
</p>

<p align="center">
  <strong>English</strong> · <a href="README.pt-BR.md">Português</a>
</p>

<p align="center">
  <strong><a href="https://dracodb.com.br/en/">dracodb.com.br</a></strong> — official Draco Postgres website, with downloads and installation
</p>

**Draco** is a free, open-source PostgreSQL client for Linux and Windows and the
database client of the **Lyra OS** ecosystem: a schema explorer, SQL editor and
administration tool for PostgreSQL. It runs on any modern Linux distribution,
with first-class visual integration with Lyra (GNOME/Wayland).

![Draco SQL editor with syntax highlighting and the result grid](site/assets/screenshots/01-sql-editor.png)

<p align="center">
  <img src="site/assets/screenshots/02-table-detail.png" alt="Table detail" width="49%">
  <img src="site/assets/screenshots/03-erd.png" alt="Entity-relationship diagram" width="49%">
  <img src="site/assets/screenshots/04-dashboard.png" alt="Dashboard" width="49%">
  <img src="site/assets/screenshots/05-programming.png" alt="Programming workspace editing a function" width="49%">
</p>

The screenshots use a fictional database and are produced by
`scripts/capture-screenshots.sh` (see [`docs/development/tauri.md`](docs/development/tauri.md)).

- Asynchronous Postgres driver (`tokio-postgres`) for queries — no external CLI
  (`psql`); backup and restore deliberately use the official PostgreSQL tools.
- SSH tunnels (including jump hosts) handled in process (`russh`), without
  relying on the `ssh` binary.
- Official Tauri 2 interface, with a local frontend and no CDN.
- Programming workbench with a dedicated screen and SQL editor for
  views/functions/procedures/triggers, plus validated editors for sequences and
  common indexes.
- Native GitHub integration: configurable repository, branches, diff against the
  deployed database, branch comparison, commits and pull requests.
- No password or passphrase is handled in plain text — storage is delegated to
  the system Secret Service (GNOME Keyring/KWallet, via `keyring`).

> **Status**: Tauri 2 is the only official frontend and artifact. The interface
> is available in English and Brazilian Portuguese.

---

## Repository layout

- `draco-core`: Postgres pool, SSH tunnel, introspection/DDL/stats queries,
  local storage (TOML/XDG) and secrets — with no dependency on any GUI toolkit.
- `draco-app`: use cases and serializable DTOs consumed by the Tauri shell.
- `src-tauri`: Tauri 2 shell, minimal capabilities and typed IPC bridge.
- `frontend/dist`: local web shell bundled by Tauri, with no network
  dependencies at runtime.
- `data`: `.desktop` file and AppStream metadata.
- `site`: static website (Portuguese at the root, English under `site/en/`)
  published to <https://dracodb.com.br> by the `Site` workflow (GitHub Pages) on
  every change to `main`.
- `packaging/obs`: artifacts for the RPM package on OBS
  (`home:rodrigosbrito:lyra/postgres-draco`).

## Building

System dependencies for the official app (Fedora/openSUSE names): WebKitGTK
4.1, GTK3, OpenSSL, librsvg and `xdg-desktop-portal` (native file pickers),
plus a recent stable Rust toolchain (`cargo`, `rustc` ≥ 1.85).

```sh
cargo build --locked --release -p draco-tauri
./target/release/draco
```

For diagnostics, use `RUST_BACKTRACE=1 cargo run -p draco-tauri`. Passwords and
query contents are never written to the logs.

### Tests

```sh
cargo test -p draco-core
cargo test -p draco-app
cargo test -p draco-tauri
(cd frontend && npm run check && npm test)
```

## Installation

Every `vX.Y.Z` tag builds four native packages and attaches them to the GitHub
Release:

- Windows x64 NSIS installer (`.exe`), with no console window and installed for
  the current user by default;
- `.deb` package built on Ubuntu 24.04;
- `.rpm` package built on Fedora 43;
- `.rpm` package built on openSUSE Leap 16.0.

The official RPM on OBS (`home:rodrigosbrito:lyra/postgres-draco`) remains
available for openSUSE. The OBS package is not called plain "draco" because that
name is already used by openSUSE's "graphics" project; the application is still
called Draco.

> **Tauri release:** version `2.3.0` ships native packages on GitHub for Windows,
> Ubuntu, Fedora and openSUSE and keeps the official RPM on OBS. It adds an
> experimental OpenAI-compatible AI provider with a configurable URL (Ollama,
> LM Studio, vLLM) and an optional key. `2.2.1` refreshed the AppStream listing, and
> `2.2.0` added connections from `postgres://` URLs and
> `.pgpass`/`pg_service.conf` import, per-connection environments and read-only
> mode, CSV/JSON import, a replication monitor, schema diff and a notification
> when long operations finish.

## License

[GPL-3.0-or-later](LICENSE) © Rodrigo Brito

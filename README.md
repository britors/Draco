<p align="center">
  <img src="logo-banner.svg" alt="Prisma4Postgres" width="480">
</p>

<p align="center">
  <a href="https://github.com/britors/Prisma4Postgres/releases">
    <img src="https://img.shields.io/github/v/release/britors/Prisma4Postgres?label=release&color=b44fff" alt="Release">
  </a>
  <a href="https://github.com/britors/Prisma4Postgres/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/britors/Prisma4Postgres" alt="License">
  </a>
  <a href="https://github.com/britors/Prisma4Postgres/issues">
    <img src="https://img.shields.io/github/issues/britors/Prisma4Postgres" alt="Issues">
  </a>
  <a href="https://aur.archlinux.org/packages/prisma4postgres-bin">
    <img src="https://img.shields.io/aur/version/prisma4postgres-bin?label=AUR&color=b44fff" alt="AUR">
  </a>
</p>

**Prisma4Postgres** is a standalone Electron desktop app for exploring, querying, and **administering** PostgreSQL databases. No VS Code required, no CLI wrappers, no config files — just connect and work.

> Built for developers and DBAs who want a fast, native PostgreSQL client that goes beyond simple query execution.

---

<p align="center">
  <a href="https://github.com/britors/Prisma4Postgres/releases/latest">
    <img src="https://img.shields.io/badge/⬇%20Download%20latest%20release-b44fff?style=for-the-badge&logoColor=white" alt="Download latest release">
  </a>
</p>

## Download

> **[👉 GitHub Releases — download the latest version](https://github.com/britors/Prisma4Postgres/releases/latest)**

| Platform | File |
|---|---|
| 🪟 Windows | `.exe` (NSIS installer) |
| 🐧 Linux (Debian/Ubuntu) | `.deb` |
| 🐧 Linux (Fedora/RHEL) | `.rpm` |
| 🐧 Linux (Arch/Manjaro) | [AUR: prisma4postgres-bin](https://aur.archlinux.org/packages/prisma4postgres-bin) |

---

## Features

### Connection Dashboard

- **Auto-opens on connect** as a dedicated tab — instant overview of your server
- **Arc gauge charts** (SVG) for Buffer Cache Hit ratio, Connection Usage, and Rollback Rate
- **Stacked connection bar** — Active / Idle-in-TX / Idle at a glance
- **KPI cards** — Deadlocks, Temp Files, Transactions, Max Connections (color-coded green/yellow/red)
- **Top 10 largest tables** with proportional size bars
- Server info: PostgreSQL version, uptime, host, port
- Database info: name, size, encoding, collation
- Accessible from the tree icon or **Connection → Dashboard** (`Ctrl+Shift+D`)

### Explorer (left sidebar)

- Tree view of all schemas, tables, views, functions, sequences, and extensions
- Folder icons matching **OpenBase.Icons** style — open/closed per node state
- Column details with PK / FK badges and data types
- Estimated row count badges (optional, configurable)
- Real-time filter/search across tables and columns
- Drag handle to resize the sidebar
- **Double-click a connection** to connect; **double-click a table** to open its detail tab
- **Create Table icon** on each connected connection
- **Star (☆)** icon — favourites appear at the top of the list
- **VACUUM ANALYZE** wrench button on each table row
- **Edit Table** pencil button on each table row — opens the Table Editor directly
- **Drop Table** trash button on each table row (with confirmation)
- **Edit** and **New Function** buttons on function nodes and the Functions group

### Global Search

- **`Ctrl+P`** opens a quick-search overlay
- Searches tables, views, columns, and functions across connected databases
- Results navigate to the matching object in the Explorer

### SQL Query Editor

- Monaco editor with SQL syntax highlighting and live autocomplete
- Multi-tab queries — each tab shows an SQL file icon + name
- **`Ctrl+Enter`** or **`F8`** to run (runs selection if text is selected)
- **`Ctrl+T`** to open a new query tab from anywhere
- **`Shift+Alt+F`** to format SQL (keywords uppercase, clause newlines)
- Connection selector per tab
- Resizable split between editor and results (drag handle)
- Cancel button stops a running query via `pg_cancel_backend`
- Auto-reconnect on connection drop

### SQL Command Bar

- Compact `sql>` input bar — toggle with the `⌘` button in the toolbar
- **Enter** executes immediately; **↑ / ↓** navigates a 20-entry history

### Results

- Result grid with NULL highlighting
- Click any row to open a **key:value detail panel** on the right
- Copy as **CSV**, **JSON**, or **Markdown** to clipboard
- Export as **CSV** or **JSON** via save dialog

### EXPLAIN Plan Viewer

- One-click `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)`
- Expandable node tree with cost, actual time, and row counts
- Expensive nodes highlighted (Seq Scan, high relative cost)
- Toggle between tree view and raw JSON

### Schema Inspection

- Full DDL viewer with Monaco read-only
- Indexes & constraints panel (size, type badges, definitions)
- FK Map — outgoing and incoming foreign keys, click to navigate

### Table Detail Tab

- Per-table tabs from the Explorer
- Columns, constraints, indexes, FK map
- **Run SELECT**, **Edit Table**, **Import CSV/JSON**
- Column statistics (histogram, most common values)

### Visual Table Creator

- GUI form — no SQL required
- 20 PostgreSQL column types, nullable, default, PK, UNIQUE
- **Foreign Key** constraints with referenced schema/table/column and ON DELETE / ON UPDATE actions
- Live `CREATE TABLE` SQL preview
- **New Schema** button inline
- Tab closes automatically after the table is created

### Table Editor

- Opens from the **Edit** button on any table row in the Explorer
- **Existing columns**: rename, change type, change nullability/default, or drop (with undo)
- **Add columns**: same column editor as the Table Creator
- **Existing constraints** (FK, UQ, CHECK): drop with one click, undo before applying
- **Add constraints**: new UNIQUE or FK constraints with full reference and action options
- Live `ALTER TABLE` SQL preview
- Applies all changes in one execution; reloads the table detail on success

### Function Editor

- **Monaco-based editor** for creating and editing stored functions
- Open via **Edit** button on any function node or **+** on the Functions group
- **Save** (`Ctrl+S`) — executes `CREATE OR REPLACE FUNCTION`; reloads the Explorer
- **Validate** — runs in `BEGIN`/`ROLLBACK` to detect syntax errors without persisting anything
- **Test** — enter any SQL expression (`SELECT schema.func(args)`), result displayed inline as a table
- Loads existing DDL via `pg_get_functiondef()` when editing
- Generates a plpgsql template for new functions

### Activity & Lock Viewer

- **Sessions** sub-tab: live `pg_stat_activity` with auto-refresh every 5 s
- Long-running queries highlighted (> 5 s amber, > 30 s red)
- Cancel individual queries via `pg_cancel_backend`
- **Locks** sub-tab: active locks with blocked / blocking mapping, `pg_locks` + `pg_stat_activity`

### Database Stats Dashboard

- Cache hit ratio, commits, rollbacks, deadlocks, temp files
- Table bloat analysis — dead tuples, wasted space, autovacuum status
- Index health — unused indexes, duplicate indexes, missing index suggestions

### Sequence Viewer

- Per-schema sequence list with current value and increment
- **Next Value** — advance the sequence and show the result
- **Reset Value** — set to any value via prompt

### Extension Manager

- Installed extensions with version
- Available extensions (up to 200) with install/drop buttons
- Integrated in the Explorer tree per connection

### VACUUM / ANALYZE Runner

- Quick **VACUUM ANALYZE** button on each table in the Explorer tree
- Full Maintain menu in table detail: ANALYZE, VACUUM, VACUUM ANALYZE, VACUUM FULL
- Duration reported in the status bar

### User & Role Manager

- **Roles** tab: list all roles with attributes (superuser, createdb, login, etc.)
- Create role: name, password, LOGIN / CREATEDB / CREATEROLE / SUPERUSER flags
- Drop role with confirmation

### Scheduled Jobs (pg_cron)

- **Jobs** tab for managing scheduled SQL jobs via the [pg_cron](https://github.com/citusdata/pg_cron) extension
- Detects if pg_cron is installed; shows step-by-step installation instructions if not
- Job list: active/paused indicator, cron schedule, SQL command, database, last run timestamp and status
- **Create** job: optional name, cron expression, SQL command (Monaco Editor)
- **Edit** job: update schedule and command
- **Pause / Resume** toggle per job
- **Delete** job via `cron.unschedule()`
- **Run History**: last 50 executions with start time, duration, return message

### ERD (Entity-Relationship Diagram)

- Auto-generated ERD from the current schema
- Visual foreign key relationships
- Zoom, pan, and node dragging

### Snippets

- Save any query as a named snippet (`Ctrl+Shift+S`)
- Searchable; click to load into editor

### Query History

- Last 50 queries, searchable, click to restore

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Enter` / `F8` | Run query (or selection) |
| `Ctrl+T` | New query tab |
| `Ctrl+S` | Save function (in Function Editor) |
| `Ctrl+Shift+S` | Save query as snippet |
| `Ctrl+Shift+E` | Export results |
| `Ctrl+Shift+D` | Open Connection Dashboard |
| `Ctrl+P` | Global search |
| `Ctrl+1` | Query tab |
| `Ctrl+2` | History tab |
| `Ctrl+3` | Snippets tab |
| `Ctrl+4` | Activity tab |
| `Ctrl+5` | ERD tab |
| `Ctrl+B` | Toggle sidebar |
| `Shift+Alt+F` | Format SQL |

---

## Getting Started

### Add a connection

1. Click **+** in the Explorer sidebar header
2. Fill in host, port, database, user, and optional password
3. Click **Test** to verify, then **Save**

### Connect and explore

- **Double-click** a connection to connect
- The **Connection Dashboard** opens automatically — server stats, top tables, cache hit ratio
- Expand the tree: schema → Tables → columns

### Run a query

1. Switch to the **Query** tab
2. Select your connection from the dropdown
3. Write SQL and press `F8`
4. Drag the divider to resize editor / results

### Create or edit a function

1. Expand a schema's **Functions** group in the Explorer
2. Click **+** (New Function) or the **Edit** pencil on an existing function
3. Write or edit the body in the Monaco editor
4. Click **Validate** to check syntax, **Test** to run it, **Save** to persist

### Manage scheduled jobs

1. Open the **Jobs** tab
2. Select a connection — the app detects whether pg_cron is installed
3. Click **New Job**, enter a cron expression and SQL command, save

### Administer roles

Open the **Roles** tab, select a connection, and use **New Role** or the **Drop** button per row.

---

## Settings

| Setting | Default | Description |
|---|---|---|
| `queryTimeout` | `30000` | Query timeout (ms) |
| `defaultPort` | `5432` | Pre-filled port for new connections |
| `defaultSsl` | `false` | SSL enabled by default |
| `showRowCount` | `false` | Estimated row count badges in Explorer |

Passwords are encrypted with `electron.safeStorage` and stored in `userData/passwords.json`.

---

## Layout

![Prisma4Postgres — Explorer sidebar and SQL query editor](image.png)

- **Left sidebar** always visible, drag handle to resize (160–600 px, persisted)
- **Right area** holds Query, History, Activity, Stats, Roles, Jobs, ERD tabs + dynamic table / dashboard tabs
- **Query/Results split** resizable (persisted per session)
- **Row detail panel** slides in on the right when you click a result row

### Title Bar

- Custom frameless title bar — drag anywhere on the bar to move the window
- **☰** hamburger button opens the full application menu (File, Edit, View, Connection, Help)
- **─ □ ✕** window controls (minimize, maximize/restore, close) with comfortable click targets
- Maximize icon switches to restore when the window is maximized

---

## Development

### Prerequisites

- Node.js 24+ (via [nvm](https://github.com/nvm-sh/nvm) recommended)
- npm

### Setup

```bash
git clone https://github.com/britors/Prisma4Postgres.git
cd Prisma4Postgres
npm install
```

### Run in development

```bash
npm run dev
```

### Run tests

```bash
npm test
```

46 unit tests covering `PgConnection` validation, `PrismaParser`, and `ConnectionManager`.

### Build for distribution

```bash
npm run package
```

Produces a `.deb` / `.rpm` (Linux) or `.exe` (Windows) in `dist/`.

---

## Tech Stack

| Layer | Technology |
|---|---|
| Shell | Electron 42 |
| Language | TypeScript 6 |
| DB driver | node-postgres (`pg`) |
| SQL editor | Monaco Editor 0.45 (CDN) |
| Charts | Inline SVG (no external libraries) |
| Icons | [OpenBase.Icons](https://github.com/britors/OpenBase.Icons) style (inline SVG) |
| Theme | CSS variables — syncs with system light/dark theme |
| Build | esbuild |
| Tests | Node built-in `node:test` + `tsx` |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions, architecture overview, and coding guidelines.

Found a bug or have a feature request? [Open an issue](https://github.com/britors/Prisma4Postgres/issues).

---

## License

[GPL-3.0-or-later](LICENSE) © Rodrigo Brito

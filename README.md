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
</p>

**Prisma4Postgres** is a standalone Electron desktop app for exploring PostgreSQL databases — all without leaving a dedicated workspace. No VS Code required, no CLI wrappers, no config files: just connect and explore.

---

## Layout

```
┌──────────────────┬──────────────────────────────────────────────┐
│                  │  [Query] [History] [Activity]                │
│  Explorer        ├──────────────────────────────────────────────┤
│  (left sidebar)  │  Monaco SQL editor                           │
│                  ├╌╌╌ drag to resize ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┤
│  connections     │  Result grid / EXPLAIN plan                  │
│  └ schemas       └──────────────────────────────────────────────┘
│    └ tables
│      └ columns
└──────────────────
```

- **Left sidebar** always visible, drag handle to resize (160 – 600 px, persisted)
- **Right area** holds Query, History, Activity tabs + dynamic table detail tabs
- **Query/Results split** resizable (default 50/50, persisted per session)

---

## Features

### Explorer (left sidebar)
- Tree view of all schemas, tables, views, and functions
- Folder icons matching **OpenBase.Icons** style — open/closed per node state
- Column details with PK / FK badges and data types
- Filter / search nodes in real time
- Drag handle to resize the sidebar
- Double-click a table to open its detail tab

### SQL Query Editor
- Monaco editor with SQL syntax highlighting and live autocomplete
- Multi-tab queries — each tab shows an SQL file icon + name
- **`Ctrl+Enter`** or **`F8`** to run (runs selection if text is selected)
- **`Ctrl+T`** to open a new query tab
- **`Shift+Alt+F`** to format SQL (keywords uppercase, clause newlines)
- Connection selector per tab
- Resizable split between editor and results (drag the divider)
- Result grid with NULL highlighting and clickable row detail panel
- Copy results as **CSV**, **JSON**, or **Markdown** to clipboard
- Export results as **CSV** or **JSON** via save dialog
- Query history (last 50 runs, searchable, click to restore)
- Cancel button stops a running query via `pg_cancel_backend`
- Auto-reconnect on connection drop (retries once silently)

### EXPLAIN Plan Viewer
- One-click `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` via **Explain** button
- Expandable node tree with cost, actual time, and row counts
- Expensive nodes highlighted (Seq Scan, high relative cost)
- Toggle between tree view and raw JSON

### Schema Inspection
- Full DDL viewer with Monaco read-only (`CREATE TABLE` reconstructed from `pg_catalog`)
- Indexes & constraints panel (size, type badges, definitions)
- FK Map — outgoing and incoming foreign keys, click to navigate tree

### Table Detail Tab
- Open per-table detail tabs from the Explorer (click icon or double-click row)
- Columns with type, nullable, default, PK/FK badges
- Constraints, indexes, and FK map sections
- **Run SELECT** button opens the table in the query editor
- **Copy model** generates a Prisma `model {}` block for the table

### Activity Viewer
- **Activity** tab shows live `pg_stat_activity` for the selected connection
- Auto-refreshes every 5 seconds (toggle)
- Highlights long-running queries (> 5s amber, > 30s red)
- Cancel individual queries per row

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Enter` | Run query (or selection) |
| `F8` | Run query (or selection) |
| `Ctrl+T` | New query tab |
| `Shift+Alt+F` | Format SQL |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo in editor |

---

## Getting Started

### Add a connection

1. Click **+** in the Explorer sidebar header
2. Fill in host, port, database, user, and optional password
3. Click **Test** to verify connectivity, then **Save**

### Connect and explore

- Click the **plug icon** next to a connection to connect
- Expand the tree: schema → Tables & Views → columns
- Double-click a table to open its detail tab

### Run a query

1. Switch to the **Query** tab
2. Select your connection from the dropdown
3. Write SQL and press `F8` (or `Ctrl+Enter`)
4. Results appear in the bottom pane; drag the divider to resize
5. Click a row to see its full key:value breakdown on the right
6. Click **Export** to save as CSV or JSON, or use the clipboard copy buttons

### View EXPLAIN

With a SELECT query in the editor, click **Explain** to see the full query plan tree with cost and timing for each node.

---

## Settings

Settings are stored in the app's user-data directory (`userData/settings.json`).

| Setting | Default | Description |
|---|---|---|
| `queryTimeout` | `30000` | Query timeout (ms) |
| `defaultPort` | `5432` | Pre-filled port for new connections |
| `defaultSsl` | `false` | SSL enabled by default for new connections |
| `showRowCount` | `false` | Show estimated row count badges in the Explorer |

Passwords are encrypted with `electron.safeStorage` and stored separately in `userData/passwords.json`.

---

## Development

### Prerequisites

- Node.js 20+ (via [nvm](https://github.com/nvm-sh/nvm) recommended)
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

Produces an **AppImage** (Linux), **dmg** (macOS), or **NSIS installer** (Windows) in `dist/`.

---

## Tech Stack

| Layer | Technology |
|---|---|
| Shell | Electron 29 |
| Language | TypeScript 5 |
| DB driver | node-postgres (`pg`) |
| SQL editor | Monaco Editor 0.45 (CDN) |
| Icons | [OpenBase.Icons](https://github.com/britors/OpenBase.Icons) style (inline SVG) |
| Theme | [OpenBase.Theme](https://github.com/britors/OpenBase.Theme) colors |
| Build | esbuild |
| Tests | Node built-in `node:test` + `tsx` |

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions, architecture overview, and coding guidelines.

Found a bug or have a feature request? [Open an issue](https://github.com/britors/Prisma4Postgres/issues).

---

## License

[MIT](LICENSE) © Rodrigo Brito

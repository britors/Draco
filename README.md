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

**Prisma4Postgres** is a standalone Electron desktop app for exploring and managing PostgreSQL databases. No VS Code required, no CLI wrappers, no config files — just connect and explore.

---

<p align="center">
  <a href="https://github.com/britors/Prisma4Postgres/releases/latest">
    <img src="https://img.shields.io/badge/⬇%20Download%20latest%20release-b44fff?style=for-the-badge&logoColor=white" alt="Download latest release">
  </a>
</p>

## ⬇ Download

> **[👉 GitHub Releases — download the latest version](https://github.com/britors/Prisma4Postgres/releases/latest)**

| Platform | File |
|---|---|
| 🪟 Windows | `.exe` (NSIS installer) |
| 🐧 Linux (Debian/Ubuntu) | `.deb` |
| 🐧 Linux (Fedora/RHEL) | `.rpm` |
| 🐧 Linux (Arch/Manjaro) | [AUR: prisma4postgres-bin](https://aur.archlinux.org/packages/prisma4postgres-bin) |

---

## Features

### Application Menu

- **File**: New Query Tab, Save as Snippet, Export Results, New Connection, Quit
- **Edit**: standard text operations (undo, redo, cut, copy, paste, select all)
- **View**: switch to Query / History / Snippets / Activity / ERD tabs, toggle sidebar, Global Search
- **Connection**: New Connection, Create Table
- **Help**: GitHub, Report Issue, About

### Explorer (left sidebar)
- Tree view of all schemas, tables, views, and functions
- Folder icons matching **OpenBase.Icons** style — open/closed per node state
- Column details with PK / FK badges and data types
- Estimated row count badges (optional, configurable)
- Real-time filter/search across tables and columns
- Drag handle to resize the sidebar
- **Double-click a connection** to connect to the database
- **Double-click a table** to open its detail tab
- **Create Table icon** on each connected connection — opens the visual table creator pre-wired to that database
- **Star (☆)** icon on each connection to mark it as a favourite — favourites appear at the top of the list

### Global Search

- **`Ctrl+P`** (or View → Global Search) opens a quick-search overlay
- Searches tables, views, columns, and functions across connected databases
- Results navigate to the matching object in the Explorer

### SQL Query Editor
- Monaco editor with SQL syntax highlighting and live autocomplete
- Multi-tab queries — each tab shows an SQL file icon + name
- **`Ctrl+Enter`** or **`F8`** to run (runs selection if text is selected)
- **`Ctrl+T`** to open a new query tab from anywhere in the app
- **`Shift+Alt+F`** to format SQL (keywords uppercase, clause newlines)
- Connection selector per tab
- Resizable split between editor and results (drag handle)
- Cancel button stops a running query via `pg_cancel_backend`
- Auto-reconnect on connection drop (retries once silently)

### SQL Command Bar (line editor)
- Compact `sql>` input bar — toggle with the `⌘` button in the toolbar
- **Enter** executes immediately against the active connection
- **↑ / ↓** navigates a 20-entry command history
- **Escape** clears the field
- Visibility persisted across sessions

### Results
- Result grid with NULL highlighting
- Click any row to open a **key:value detail panel** on the right
- Copy results as **CSV**, **JSON**, or **Markdown** to clipboard (no dialog)
- Export results as **CSV** or **JSON** via save dialog

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
- Per-table tabs opened from the Explorer (icon or double-click)
- Columns with full type, nullable, default, PK / FK badges
- Constraints, indexes with size, and FK map sections
- **Run SELECT** — opens a pre-filled query in the editor
- **Copy model** — generates a Prisma `model {}` block for the table and copies to clipboard

### Visual Table Creator
- GUI form to define a new table — no SQL required
- Column editor: name, type (20 PostgreSQL types), length, nullable, default, PK
- Live `CREATE TABLE` SQL preview as you type
- **Copy SQL** copies the DDL to clipboard
- **Execute** runs against the selected database and refreshes the Explorer on success
- Accessible from the Create Table icon on any connected connection
- **New Schema** button — create a PostgreSQL schema directly from the table creator without leaving the form

### Snippets

- Save any query as a named snippet with the bookmark (🔖) button or **`Ctrl+Shift+S`**
- Snippets tab lists all saved queries, searchable by name or content
- Click a snippet to load it into the editor; delete with the trash icon

### Activity Viewer
- **Activity** tab shows live `pg_stat_activity` for the selected connection
- Auto-refreshes every 5 seconds (toggle checkbox)
- Long-running queries highlighted (> 5s amber, > 30s red)
- Cancel individual queries per row via `pg_cancel_backend`

### Query History
- Last 50 queries, searchable, click to restore in editor

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Enter` | Run query (or selection) |
| `F8` | Run query (or selection) |
| `Ctrl+T` | New query tab |
| `Ctrl+Shift+S` | Save current query as snippet |
| `Ctrl+Shift+E` | Export results to CSV |
| `Ctrl+P` | Global search (tables, columns, functions) |
| `Ctrl+1` | Switch to Query tab |
| `Ctrl+2` | Switch to History tab |
| `Ctrl+3` | Switch to Snippets tab |
| `Ctrl+4` | Switch to Activity tab |
| `Ctrl+5` | Switch to ERD tab |
| `Ctrl+B` | Toggle sidebar |
| `Shift+Alt+F` | Format SQL |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo in editor |

---

## Getting Started

### Add a connection

1. Click **+** in the Explorer sidebar header
2. Fill in host, port, database, user, and optional password
3. Click **Test** to verify connectivity, then **Save**

### Connect and explore

- **Double-click** a connection to connect, or click the plug icon
- Expand the tree: schema → Tables & Views → columns
- **Double-click** a table to open its detail tab

### Run a query

1. Switch to the **Query** tab
2. Select your connection from the dropdown
3. Write SQL and press `F8` (or `Ctrl+Enter`)
4. Results appear in the bottom pane; drag the divider to resize
5. Click any row to see its full key:value breakdown on the right
6. Use the clipboard copy buttons (CSV / JSON / MD) or **Export** to save to file

### Quick queries — line editor

Toggle the `sql>` bar with the `⌘` button in the query toolbar. Type a statement, press **Enter** to run. Use **↑ / ↓** to browse history.

### Create a table visually

Click the table icon (`⊞`) on any connected connection in the Explorer. Fill in the column form, check the live SQL preview, and click **Execute**.

### View EXPLAIN

With a SELECT query in the editor, click **Explain** to see the full query plan tree with cost and timing for each node.

### Monitor active queries

Switch to the **Activity** tab, select a connection, and click **Refresh** (or enable auto-refresh). Long-running queries are highlighted; click **Cancel** to stop them.

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

## Layout

![Prisma4Postgres — Explorer sidebar and SQL query editor](image.png)

- **Left sidebar** always visible, drag handle to resize (160–600 px, persisted)
- **Right area** holds Query, History, Activity tabs + dynamic table detail tabs
- **Query/Results split** resizable (default 50/50, persisted per session)
- **Row detail panel** slides in on the right when you click a result row

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

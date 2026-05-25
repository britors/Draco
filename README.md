<p align="center">
  <img src="logo-banner.svg" alt="Prisma4Postgres" width="480">
</p>

<p align="center">
  <a href="https://marketplace.visualstudio.com/items?itemName=britors.prisma4postgres">
    <img src="https://img.shields.io/visual-studio-marketplace/v/britors.prisma4postgres?label=VS%20Code%20Marketplace&color=007acc" alt="Marketplace">
  </a>
  <a href="https://github.com/britors/Prisma4Postgres/blob/main/LICENSE">
    <img src="https://img.shields.io/github/license/britors/Prisma4Postgres" alt="License">
  </a>
  <a href="https://github.com/britors/Prisma4Postgres/issues">
    <img src="https://img.shields.io/github/issues/britors/Prisma4Postgres" alt="Issues">
  </a>
</p>

**Prisma4Postgres** is a VS Code extension that turns the sidebar into a full PostgreSQL workspace — schema explorer, SQL query editor, Prisma ORM integration, and more. No CLI wrappers, no configuration files: just connect and explore.

---

## Features

### Database Explorer
- Tree view of all schemas, tables, views, and functions
- Column details with PK / FK badges and data types
- Prisma model indicator on mapped tables
- Filter / search nodes in real time

### SQL Query Editor
- Monaco-based editor with SQL syntax highlighting
- Multi-tab queries (`Ctrl+Enter` to run)
- Connection selector per tab
- Result grid with NULL highlighting
- Export results as **CSV** or **JSON** via a save dialog
- Query history (last 50 runs, searchable, click to restore)
- SQL autocomplete from live schema (tables, views, functions)

### EXPLAIN Plan Viewer
- One-click `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` on any query
- Expandable node tree with cost, actual time, and row counts
- Expensive nodes highlighted (Seq Scan, high relative cost)
- Toggle between tree view and raw JSON

### Schema Inspection
- Full DDL viewer with Monaco read-only (`CREATE TABLE` reconstructed from `pg_catalog`)
- Indexes & constraints panel (size, type badges, definitions)
- FK Map — outgoing and incoming foreign keys, click to navigate tree
- Table preview (first N rows, configurable)

### Prisma Integration
- Auto-detects `schema.prisma` in the workspace (file watcher)
- Maps models to database tables — ✓ found / ✗ missing indicator
- **Drift panel**: side-by-side Prisma model fields vs actual DB columns with type compatibility check
- Run **`prisma db pull`** with real-time streaming log
- Run **`prisma migrate status`** with real-time streaming log
- Migration history from `_prisma_migrations` with status indicators

---

## Installation

1. Open VS Code
2. Go to **Extensions** (`Ctrl+Shift+X`)
3. Search for `Prisma4Postgres`
4. Click **Install**

Or install from the command line:
```bash
code --install-extension britors.prisma4postgres
```

---

## Getting Started

### Add a connection

1. Click the **Prisma4Postgres** icon in the Activity Bar
2. Click **Add Connection** (or the `+` button in the toolbar)
3. Fill in host, port, database, user, and password
4. Click **Test** to verify, then **Save**

### Connect and explore

- Click the **plug icon** next to a connection to connect
- Expand the tree to browse schemas → Tables & Views → columns
- Hover over a table for quick actions: Preview, DDL, copy name

### Run a query

1. Switch to the **Query** tab
2. Select your connection from the dropdown
3. Write SQL and press `Ctrl+Enter` (or click **Run**)
4. Results appear in the grid below; use **Export** to save

### View EXPLAIN

With a SELECT query in the editor, click **Explain** to see the full query plan tree with cost and timing for each node.

### Prisma integration

Open a workspace that contains a `schema.prisma` file. Switch to the **Prisma** tab to see:
- Schema file status and datasource info
- All models with their mapped table names and existence status
- Run `db pull` or `migrate status` from the action buttons
- Click **diff icon** next to any model to open the Drift panel

---

## Configuration

All settings are available under **File → Preferences → Settings** → search `prisma4postgres`.

| Setting | Default | Description |
|---|---|---|
| `prisma4postgres.queryTimeout` | `30000` | Query timeout in milliseconds |
| `prisma4postgres.previewRowLimit` | `100` | Max rows in table preview |
| `prisma4postgres.defaultPort` | `5432` | Pre-filled port for new connections |
| `prisma4postgres.defaultSsl` | `false` | SSL enabled by default for new connections |
| `prisma4postgres.showRowCount` | `false` | Show estimated row count in the Explorer tree |

---

## Diagnostics

The extension writes detailed logs to the **Prisma4Postgres** output channel (`View → Output → Prisma4Postgres`). Connection events, query timing, and errors are all captured there.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions, architecture overview, and coding guidelines.

Found a bug or have a feature request? [Open an issue](https://github.com/britors/Prisma4Postgres/issues).

---

## License

[MIT](LICENSE) © Rodrigo Brito

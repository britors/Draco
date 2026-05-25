# Contributing to Prisma4Postgres

## Prerequisites

- Node.js ≥ 18 (tested with v24)
- VS Code 1.80+
- A running PostgreSQL instance for manual testing

## Setup

```bash
git clone https://github.com/britors/Prisma4Postgres.git
cd Prisma4Postgres
npm install
```

## Development workflow

**Build once:**
```bash
node esbuild.js
```

**Watch mode:**
```bash
node esbuild.js --watch
```

**Type-check (no emit):**
```bash
npx tsc --noEmit
```

Open the repo in VS Code and press **F5** to launch the Extension Development Host. Changes in watch mode are picked up after reloading the host window (`Ctrl+R`).

## Project structure

```
src/
  extension.ts              # Entry point — registers providers and commands
  types/PgConnection.ts     # Core connection interface + validation
  storage/
    ConnectionStorage.ts    # Metadata in globalState, passwords in SecretStorage
    HistoryStorage.ts       # Query history in globalState (max 50 entries)
  db/
    PgDriver.ts             # node-postgres pool wrapper + testConnection()
    ConnectionManager.ts    # Lifecycle manager for per-connection PgDriver instances
    queries.ts              # All information_schema queries
  providers/
    SidebarProvider.ts      # WebviewViewProvider — main IPC hub
    PreviewPanel.ts         # Static WebviewPanel for table preview
  webview/
    getSidebarHtml.ts       # Full sidebar HTML/CSS/JS (Monaco, tree, history)
    getPreviewHtml.ts       # Static preview panel HTML
```

## Architecture

The extension uses a **WebviewView sidebar** communicating with the extension host via `postMessage({ command, data })` in both directions.

- Webview → host: `vscode.postMessage({ command, data })`
- Host → webview: `webview.postMessage({ command, data })`

All database access happens on the extension host side through `PgDriver` (a `node-postgres` pool). The webview never touches the database directly.

## UI rules

The sidebar must look native to VS Code:

- **Colors**: only `--vscode-*` CSS variables — never hardcoded hex values
- **Icons**: codicons only (`<i class="codicon codicon-NAME">`)
- **No external UI libraries** (Bootstrap, Tailwind, Material, etc.)
- **Fonts**: `var(--vscode-font-family)` / `var(--vscode-font-size)`

Monaco Editor is loaded from the jsDelivr CDN (`monaco-editor@0.45.0`). The CSP allows `https://cdn.jsdelivr.net` for scripts and fonts, and `blob:` for Monaco's web workers.

## Webview template literal rules

When generating HTML with inline `<script>` blocks, follow these escaping rules to avoid `SyntaxError` at runtime:

- Use `\\n` (not `\n`) inside JS string literals embedded in template literals
- Use `/\x3c/g` (not `/</g`) in regex literals
- Use Unicode escapes for any `<` or `>` in JS string values inside `<script>` blocks

## Milestones

| Milestone | Issues | Status |
|-----------|--------|--------|
| v0.1 Foundation | #2–#7 | Done |
| v0.2 Database Explorer | #8–#14 | Done |
| v0.3 Query Editor | #15–#19 | Done |
| v0.4 Schema Inspection | #20–#23 | Open |
| v0.5 Prisma Integration | #24–#28 | Open |
| v0.6 Polish | #29–#32 | Open |

Pick up any open issue in the next milestone. Reference the issue number in your commit message (`Closes #N`).

## Pull requests

- One PR per issue or small group of tightly related issues
- Run `npx tsc --noEmit` and `node esbuild.js` before opening a PR — both must succeed with no errors
- Keep the sidebar HTML/JS in `getSidebarHtml.ts` self-contained; avoid importing runtime dependencies into the webview bundle

## Commit style

```
feat: short description (#N)

Optional body explaining the why.

Closes #N
```

Types: `feat`, `fix`, `chore`, `refactor`, `docs`.

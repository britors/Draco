import { BrowserWindow, ipcMain, dialog } from 'electron';
import * as fs from 'fs';
import * as path from 'path';
import { spawn } from 'child_process';
import { randomUUID } from 'crypto';

import { ConnectionManager } from '../db/ConnectionManager';
import { testConnection } from '../db/PgDriver';
import {
  getSchemas, getTables, getColumns, getFunctions, getFunctionParams,
  previewTable, getCompletionData, getTableDDL, getIndexes, getConstraints,
  getFKMap, checkMigrationsTable, getMigrations, getTableEstimates,
} from '../db/queries';
import { validateConnection, PgConnection } from '../types/PgConnection';
import { parsePrismaSchema, ParsedPrismaSchema } from '../prisma/PrismaParser';
import {
  listConnections, getConnection, saveConnection, deleteConnection, getPassword,
  listHistory, addHistory, getSettings, patchSettings,
} from './store';
import { createPanelWindow } from './window';

// ── State ────────────────────────────────────────────────────────────────────

let mainWin: BrowserWindow;
const connManager = new ConnectionManager();
let prismaSchema: ParsedPrismaSchema | null = null;

// Temporary store for panel data (DDL, Preview, Drift windows)
const panelStore = new Map<string, unknown>();

// ── Helpers ──────────────────────────────────────────────────────────────────

function send(command: string, data?: unknown): void {
  if (mainWin && !mainWin.isDestroyed()) {
    mainWin.webContents.send('to-renderer', { command, data });
  }
}

function pushConnections(): void {
  const connections = listConnections();
  connManager.syncConnections(connections);
  send('updateConnections', connections);
  send('updateStatuses', connManager.getStatuses());
}

function buildDatabaseUrl(conn: PgConnection, password: string): string {
  const user = encodeURIComponent(conn.user);
  const pass = encodeURIComponent(password);
  const db   = encodeURIComponent(conn.database);
  const ssl  = conn.ssl ? '?sslmode=require' : '';
  return `postgresql://${user}:${pass}@${conn.host}:${conn.port}/${db}${ssl}`;
}

// ── IPC registration ─────────────────────────────────────────────────────────

export function registerIpc(win: BrowserWindow): void {
  mainWin = win;

  // ── Fire-and-forget messages from renderer ────────────────────────────────
  ipcMain.on('from-renderer', async (event, { command, data }: { command: string; data: unknown }) => {
    // Forward navigate-to-table from child panels to main window
    if (command === 'navigate-to-table' && event.sender !== mainWin.webContents) {
      send('navigateToTable', data);
      mainWin.focus();
      return;
    }

    switch (command) {

      // ── Init ──────────────────────────────────────────────────────────────

      case 'ready': {
        pushConnections();
        if (prismaSchema) send('prismaSchema', prismaSchema);
        const s = getSettings();
        send('settings', {
          defaultPort: s.defaultPort,
          defaultSsl: s.defaultSsl,
          showRowCount: s.showRowCount,
        });
        // Auto-load saved Prisma schema path
        if (s.prismaSchemaPath) {
          try {
            const content = fs.readFileSync(s.prismaSchemaPath, 'utf-8');
            prismaSchema = parsePrismaSchema(s.prismaSchemaPath, content);
            send('prismaSchema', prismaSchema);
          } catch { /* file may have been moved */ }
        }
        break;
      }

      case 'refreshConnections':
        pushConnections();
        break;

      // ── Connection CRUD ───────────────────────────────────────────────────

      case 'saveConnection': {
        const { conn, password } = data as { conn: Omit<PgConnection, 'id'> & { id?: string }; password: string };
        const errors = validateConnection(conn);
        if (errors.length > 0) { send('formError', errors.join(', ')); return; }

        let finalPassword = password;
        if (!finalPassword && conn.id) finalPassword = getPassword(conn.id);

        try {
          saveConnection(conn, finalPassword);
          pushConnections();
        } catch (err) {
          send('formError', `Failed to save connection: ${String(err)}`);
        }
        break;
      }

      case 'deleteConnection': {
        const { id } = data as { id: string };
        const conn = getConnection(id);
        const { response } = await dialog.showMessageBox(mainWin, {
          type: 'warning',
          buttons: ['Delete', 'Cancel'],
          defaultId: 1,
          cancelId: 1,
          message: `Delete connection "${conn?.label ?? id}"?`,
          detail: 'This action cannot be undone.',
        });
        if (response !== 0) return;
        try {
          await connManager.disconnect(id);
          deleteConnection(id);
          pushConnections();
        } catch (err) {
          send('formError', `Failed to delete connection: ${String(err)}`);
        }
        break;
      }

      case 'testConnection': {
        const { conn, password } = data as { conn: PgConnection; password: string };
        try {
          await testConnection(conn as never, password);
          send('testResult', { success: true });
        } catch (err) {
          send('testResult', { success: false, message: String(err) });
        }
        break;
      }

      // ── Connect / Disconnect ──────────────────────────────────────────────

      case 'connect': {
        const { connId } = data as { connId: string };
        const password = getPassword(connId);
        const timeout = getSettings().queryTimeout;
        try {
          await connManager.connect(connId, password, timeout);
          send('connectionStatus', { id: connId, status: 'connected' });
        } catch (err) {
          const msg = String(err);
          send('connectionStatus', { id: connId, status: 'error', message: msg });
          const { response } = await dialog.showMessageBox(mainWin, {
            type: 'error',
            buttons: ['Reconnect', 'OK'],
            defaultId: 1,
            message: `Connection failed (${getConnection(connId)?.label ?? connId})`,
            detail: msg,
          });
          if (response === 0) send('reconnect', { connId });
        }
        break;
      }

      case 'disconnect': {
        const { connId } = data as { connId: string };
        await connManager.disconnect(connId);
        send('connectionStatus', { id: connId, status: 'disconnected' });
        break;
      }

      // ── Schema / Table / Column / Function loading ────────────────────────

      case 'loadSchemas': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          send('schemasLoaded', { connId, schemas: await getSchemas(driver) });
        } catch (err) {
          console.error('[loadSchemas]', err);
        }
        break;
      }

      case 'loadTables': {
        const { connId, schema } = data as { connId: string; schema: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          send('tablesLoaded', { connId, schema, tables: await getTables(driver, schema) });
        } catch (err) {
          console.error('[loadTables]', err);
        }
        break;
      }

      case 'loadColumns': {
        const { connId, schema, table } = data as { connId: string; schema: string; table: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          send('columnsLoaded', { connId, schema, table, columns: await getColumns(driver, schema, table), isView: false });
        } catch (err) {
          console.error('[loadColumns]', err);
        }
        break;
      }

      case 'loadFunctions': {
        const { connId, schema } = data as { connId: string; schema: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          send('functionsLoaded', { connId, schema, functions: await getFunctions(driver, schema) });
        } catch (err) {
          console.error('[loadFunctions]', err);
        }
        break;
      }

      case 'loadFuncParams': {
        const { connId, schema, specificName } = data as { connId: string; schema: string; specificName: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          send('funcParamsLoaded', { connId, schema, specificName, params: await getFunctionParams(driver, schema, specificName) });
        } catch (err) {
          console.error('[loadFuncParams]', err);
        }
        break;
      }

      case 'loadTableEstimates': {
        const { connId, schema } = data as { connId: string; schema: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          send('tableEstimatesLoaded', { connId, schema, estimates: await getTableEstimates(driver, schema) });
        } catch { /* best-effort */ }
        break;
      }

      case 'loadCompletions': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          send('completionsLoaded', await getCompletionData(driver));
        } catch { /* best-effort */ }
        break;
      }

      // ── Query execution ───────────────────────────────────────────────────

      case 'executeQuery': {
        const { connId, sql, tabId } = data as { connId: string; sql: string; tabId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) {
          send('queryResult', { tabId, error: 'Not connected. Please connect first.' });
          return;
        }
        const conn = getConnection(connId);
        const start = Date.now();
        try {
          const rows = await driver.query(sql);
          const durationMs = Date.now() - start;
          const columns = rows.length > 0 ? Object.keys(rows[0]) : [];
          send('queryResult', { tabId, columns, rows, durationMs });
          addHistory({
            sql, connId, connLabel: conn?.label ?? connId,
            timestamp: Date.now(), durationMs, rowCount: rows.length,
          });
        } catch (err) {
          const durationMs = Date.now() - start;
          const msg = String(err);
          const isTimeout = /canceling statement due to statement timeout/i.test(msg);
          send('queryResult', { tabId, error: msg, durationMs, isTimeout });
        }
        break;
      }

      case 'executeExplain': {
        const { connId, sql, tabId } = data as { connId: string; sql: string; tabId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) {
          send('explainResult', { tabId, error: 'Not connected. Please connect first.' });
          return;
        }
        try {
          const rows = await driver.query<Record<string, unknown>>(`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ${sql}`);
          const raw = rows[0]['QUERY PLAN'];
          const plan = typeof raw === 'string' ? JSON.parse(raw) : raw;
          send('explainResult', { tabId, plan });
        } catch (err) {
          send('explainResult', { tabId, error: String(err) });
        }
        break;
      }

      case 'loadHistory':
        send('historyLoaded', listHistory());
        break;

      // ── DDL viewer ────────────────────────────────────────────────────────

      case 'openDDL': {
        const { connId, schema, table } = data as { connId: string; schema: string; table: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('formError', 'Not connected.'); return; }
        const conn = getConnection(connId);
        try {
          const [ddl, indexes, constraints, fkMap] = await Promise.all([
            getTableDDL(driver, schema, table),
            getIndexes(driver, schema, table),
            getConstraints(driver, schema, table),
            getFKMap(driver, schema, table),
          ]);
          const key = randomUUID();
          panelStore.set(key, { connId, connLabel: conn?.label ?? connId, schema, table, ddl, indexes, constraints, fkMap });
          const panelWin = createPanelWindow('ddl.html', `DDL: ${schema}.${table}`);
          panelWin.webContents.once('did-finish-load', () => {
            panelWin.webContents.send('to-renderer', { command: 'panel-key', data: key });
          });
        } catch (err) {
          send('formError', `Failed to load DDL: ${String(err)}`);
        }
        break;
      }

      // ── Preview ───────────────────────────────────────────────────────────

      case 'previewTable': {
        const { connId, schema, table } = data as { connId: string; schema: string; table: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('formError', 'Not connected.'); return; }
        const conn = getConnection(connId);
        const rowLimit = getSettings().previewRowLimit;
        try {
          const result = await previewTable(driver, schema, table, rowLimit);
          // Send inline to main window (no separate panel for preview in v1.0)
          send('previewResult', {
            connLabel: conn?.label ?? connId,
            schema, table,
            columns: result.columns,
            rows: result.rows,
            estimate: result.estimate,
          });
        } catch (err) {
          send('formError', `Preview failed: ${String(err)}`);
        }
        break;
      }

      // ── Export ────────────────────────────────────────────────────────────

      case 'exportResult': {
        const { format, columns, rows } = data as {
          format: 'csv' | 'json';
          columns: string[];
          rows: Record<string, unknown>[];
        };
        const ext = format === 'csv' ? 'csv' : 'json';
        const result = await dialog.showSaveDialog(mainWin, {
          defaultPath: `query-result.${ext}`,
          filters: format === 'csv'
            ? [{ name: 'CSV files', extensions: ['csv'] }]
            : [{ name: 'JSON files', extensions: ['json'] }],
        });
        if (result.canceled || !result.filePath) break;

        let content: string;
        if (format === 'csv') {
          const esc = (v: unknown) => {
            if (v === null || v === undefined) return '';
            const s = typeof v === 'object' ? JSON.stringify(v) : String(v);
            return s.includes(',') || s.includes('"') || s.includes('\n')
              ? '"' + s.replace(/"/g, '""') + '"' : s;
          };
          content = [
            columns.map(esc).join(','),
            ...rows.map(row => columns.map(c => esc(row[c])).join(',')),
          ].join('\n');
        } else {
          content = JSON.stringify(rows, null, 2);
        }
        fs.writeFileSync(result.filePath, content, 'utf-8');
        break;
      }

      // ── Prisma integration ────────────────────────────────────────────────

      case 'loadMigrations': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) return;
        try {
          const hasTable = await checkMigrationsTable(driver);
          if (!hasTable) { send('migrationsLoaded', { hasTable: false, entries: [] }); return; }
          send('migrationsLoaded', { hasTable: true, entries: await getMigrations(driver) });
        } catch (err) {
          send('migrationsLoaded', { hasTable: false, error: String(err) });
        }
        break;
      }

      case 'runPrismaCommand': {
        const { command: prismaCmd, connId } = data as { command: 'db-pull' | 'migrate-status'; connId: string };
        if (!prismaSchema) {
          send('prismaLog', { text: 'No schema.prisma selected. Use the Browse button.\n', done: true });
          return;
        }
        const conn = getConnection(connId);
        if (!conn) {
          send('prismaLog', { text: 'No connection selected.\n', done: true });
          return;
        }
        const password = getPassword(connId);
        const databaseUrl = buildDatabaseUrl(conn, password);
        const schemaPath = prismaSchema.filePath;
        const args = prismaCmd === 'db-pull'
          ? ['prisma', 'db', 'pull', '--schema', schemaPath]
          : ['prisma', 'migrate', 'status', '--schema', schemaPath];

        const proc = spawn('npx', args, {
          cwd: path.dirname(schemaPath),
          env: { ...process.env, DATABASE_URL: databaseUrl },
          shell: process.platform === 'win32',
        });

        send('prismaLog', { text: `> npx ${args.join(' ')}\n`, done: false });
        proc.stdout.on('data', (d: Buffer) => send('prismaLog', { text: d.toString(), done: false }));
        proc.stderr.on('data', (d: Buffer) => send('prismaLog', { text: d.toString(), done: false }));
        proc.on('close', (code: number | null) => {
          send('prismaLog', { text: `\nFinished (exit ${code ?? '?'}).\n`, done: true });
          if (prismaCmd === 'db-pull' && code === 0 && prismaSchema) {
            try {
              const updated = fs.readFileSync(schemaPath, 'utf-8');
              prismaSchema = parsePrismaSchema(schemaPath, updated);
              send('prismaSchema', prismaSchema);
            } catch { /* best-effort */ }
          }
        });
        proc.on('error', (err: Error) => {
          send('prismaLog', { text: `Error: ${err.message}\n`, done: true });
        });
        break;
      }

      case 'openDrift': {
        const { connId, schema, tableName, modelName } = data as {
          connId: string; schema: string; tableName: string; modelName: string;
        };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('formError', 'Not connected.'); return; }
        const model = prismaSchema?.models.find(m => m.name === modelName);
        if (!model) { send('formError', `Model "${modelName}" not found.`); return; }
        try {
          const columns = await getColumns(driver, schema, tableName);
          send('driftResult', { modelName, tableName, schema, model, columns });
        } catch (err) {
          send('formError', `Failed to load columns: ${String(err)}`);
        }
        break;
      }

      case 'saveSettings': {
        patchSettings(data as Record<string, unknown>);
        break;
      }
    }
  });

  // ── Invoke handlers (request-response) ───────────────────────────────────
  ipcMain.handle('ipc-invoke', async (_event, { command, data }: { command: string; data: unknown }) => {
    switch (command) {

      case 'open-schema-dialog': {
        const result = await dialog.showOpenDialog(mainWin, {
          title: 'Select schema.prisma',
          properties: ['openFile'],
          filters: [{ name: 'Prisma Schema', extensions: ['prisma'] }],
        });
        if (result.canceled || result.filePaths.length === 0) return null;
        const filePath = result.filePaths[0];
        try {
          const content = fs.readFileSync(filePath, 'utf-8');
          prismaSchema = parsePrismaSchema(filePath, content);
          patchSettings({ prismaSchemaPath: filePath });
          send('prismaSchema', prismaSchema);
          return filePath;
        } catch (err) {
          return { error: String(err) };
        }
      }

      case 'get-panel-data': {
        const key = data as string;
        const payload = panelStore.get(key);
        panelStore.delete(key);
        return payload ?? null;
      }

      default:
        return null;
    }
  });
}

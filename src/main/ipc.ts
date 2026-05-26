import { BrowserWindow, ipcMain, dialog, Notification } from 'electron';
import * as fs from 'fs';
import * as path from 'path';
import { randomUUID } from 'crypto';

import { ConnectionManager } from '../db/ConnectionManager';
import { testConnection } from '../db/PgDriver';
import {
  getSchemas, getTables, getColumns, getFunctions, getFunctionParams,
  getCompletionData, getTableDDL, getIndexes, getConstraints,
  getFKMap, getTableEstimates,
  getTableDetail, getERDData,
  browseTableData, updateTableRow, getColumnStats, generatePrismaSchema, importTableRows,
} from '../db/queries';
import { validateConnection, PgConnection } from '../types/PgConnection';
import {
  listConnections, getConnection, saveConnection, deleteConnection, getPassword,
  listHistory, addHistory, clearHistory, getSettings, patchSettings,
  listSnippets, saveSnippet, deleteSnippet,
  getSshPassword, storeSshPassword,
} from './store';
import { createPanelWindow } from './window';

// ── State ────────────────────────────────────────────────────────────────────

let mainWin: BrowserWindow;
const connManager = new ConnectionManager();

// Temporary store for panel data (DDL windows)
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
        const s = getSettings();
        send('settings', {
          defaultPort: s.defaultPort,
          defaultSsl: s.defaultSsl,
          showRowCount: s.showRowCount,
        });
        break;
      }

      case 'refreshConnections':
        pushConnections();
        break;

      // ── Connection CRUD ───────────────────────────────────────────────────

      case 'saveConnection': {
        const { conn, password, sshPassword } = data as {
          conn: Omit<PgConnection, 'id'> & { id?: string }; password: string; sshPassword?: string;
        };
        const errors = validateConnection(conn);
        if (errors.length > 0) { send('formError', errors.join(', ')); return; }

        let finalPassword = password;
        if (!finalPassword && conn.id) finalPassword = getPassword(conn.id);

        try {
          const saved = saveConnection(conn, finalPassword);
          if (sshPassword !== undefined) storeSshPassword(saved.id, sshPassword);
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
        const password    = getPassword(connId);
        const sshPassword = getSshPassword(connId);
        const timeout = getSettings().queryTimeout;
        try {
          await connManager.connect(connId, password, timeout, sshPassword || undefined);
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
        let driver = connManager.getDriver(connId);
        if (!driver) {
          send('queryResult', { tabId, error: 'Not connected. Please connect first.' });
          return;
        }
        const conn = getConnection(connId);
        const start = Date.now();
        const runQuery = async () => driver!.query(sql);
        try {
          const rows = await runQuery();
          const durationMs = Date.now() - start;
          const columns = rows.length > 0 ? Object.keys(rows[0]) : [];
          send('queryResult', { tabId, columns, rows, durationMs });
          addHistory({
            sql, connId, connLabel: conn?.label ?? connId,
            timestamp: Date.now(), durationMs, rowCount: rows.length,
          });
          // Desktop notification for long queries (#68)
          if (durationMs >= 5_000 && Notification.isSupported()) {
            new Notification({
              title: 'Query finished',
              body: `Completed in ${(durationMs / 1000).toFixed(1)}s — ${rows.length} rows`,
            }).show();
          }
        } catch (err) {
          const msg = String(err);
          const isConnErr = /connection.*terminated|connection.*closed|connection.*reset|ECONNRESET|ECONNREFUSED|pool.*destroyed/i.test(msg);
          if (isConnErr) {
            // auto-reconnect once (#54)
            try {
              send('reconnect', { connId });
              const password = getPassword(connId) ?? '';
              await connManager.connect(connId, password, getSettings().queryTimeout);
              send('connectionStatus', { id: connId, status: 'connected' });
              driver = connManager.getDriver(connId);
              const rows = await runQuery();
              const durationMs = Date.now() - start;
              const columns = rows.length > 0 ? Object.keys(rows[0]) : [];
              send('queryResult', { tabId, columns, rows, durationMs });
              addHistory({ sql, connId, connLabel: conn?.label ?? connId, timestamp: Date.now(), durationMs, rowCount: rows.length });
              if (durationMs >= 5_000 && Notification.isSupported()) {
                new Notification({ title: 'Query finished', body: `Completed in ${(durationMs / 1000).toFixed(1)}s — ${rows.length} rows` }).show();
              }
            } catch (err2) {
              send('connectionStatus', { id: connId, status: 'error' });
              send('queryResult', { tabId, error: String(err2), durationMs: Date.now() - start, isTimeout: false });
            }
          } else {
            const durationMs = Date.now() - start;
            const isTimeout = /canceling statement due to statement timeout/i.test(msg);
            send('queryResult', { tabId, error: msg, durationMs, isTimeout });
          }
        }
        break;
      }

      case 'cancelQuery': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (driver) await driver.cancelActive().catch(() => {});
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

      case 'clearHistory':
        clearHistory();
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

      // ── Table detail tab ─────────────────────────────────────────────────

      case 'loadTableDetail': {
        const { connId, schema, tableName } = data as { connId: string; schema: string; tableName: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('tableDetailLoaded', { connId, schema, tableName, error: 'Not connected.' }); return; }
        try {
          const detail = await getTableDetail(driver, schema, tableName);
          send('tableDetailLoaded', { connId, schema, tableName, detail });
        } catch (err) {
          send('tableDetailLoaded', { connId, schema, tableName, error: String(err) });
        }
        break;
      }



      // ── Browse table (#62) ───────────────────────────────────────────────

      case 'browseTable': {
        const { connId, schema, tableName, offset, limit } = data as { connId: string; schema: string; tableName: string; offset: number; limit: number };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('browseTableResult', { connId, schema, tableName, offset, error: 'Not connected.' }); break; }
        try {
          const result = await browseTableData(driver, schema, tableName, offset, limit);
          send('browseTableResult', { connId, schema, tableName, offset, limit, ...result });
        } catch (err) {
          send('browseTableResult', { connId, schema, tableName, offset, error: String(err) });
        }
        break;
      }

      // ── Update row (#63) ─────────────────────────────────────────────────

      case 'updateTableRow': {
        const { connId, schema, tableName, pkCols, pkVals, column, newValue } = data as {
          connId: string; schema: string; tableName: string;
          pkCols: string[]; pkVals: unknown[]; column: string; newValue: string | null;
        };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('updateRowResult', { error: 'Not connected.' }); break; }
        try {
          await updateTableRow(driver, schema, tableName, pkCols, pkVals, column, newValue);
          send('updateRowResult', { ok: true, connId, schema, tableName });
        } catch (err) {
          send('updateRowResult', { error: String(err) });
        }
        break;
      }

      // ── Column stats (#67) ───────────────────────────────────────────────

      case 'loadColumnStats': {
        const { connId, schema, tableName } = data as { connId: string; schema: string; tableName: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('columnStatsLoaded', { connId, schema, tableName, error: 'Not connected.' }); break; }
        try {
          const stats = await getColumnStats(driver, schema, tableName);
          send('columnStatsLoaded', { connId, schema, tableName, stats });
        } catch (err) {
          send('columnStatsLoaded', { connId, schema, tableName, error: String(err) });
        }
        break;
      }

      // ── Snippets (#65) ───────────────────────────────────────────────────

      case 'loadSnippets':
        send('snippetsLoaded', listSnippets());
        break;

      case 'saveSnippet': {
        const { name, sql } = data as { name: string; sql: string };
        const entry = saveSnippet({ name, sql });
        send('snippetsLoaded', listSnippets());
        send('snippetSaved', entry);
        break;
      }

      case 'deleteSnippet': {
        const { id } = data as { id: string };
        deleteSnippet(id);
        send('snippetsLoaded', listSnippets());
        break;
      }

      // ── Import data (#66) ────────────────────────────────────────────────

      case 'openImportDialog': {
        const { connId, schema, tableName } = data as { connId: string; schema: string; tableName: string };
        const result = await dialog.showOpenDialog(mainWin, {
          title: `Import data into ${schema}.${tableName}`,
          filters: [
            { name: 'CSV files', extensions: ['csv'] },
            { name: 'JSON files', extensions: ['json'] },
          ],
          properties: ['openFile'],
        });
        if (result.canceled || !result.filePaths.length) break;
        const filePath = result.filePaths[0];
        const ext = path.extname(filePath).toLowerCase();
        try {
          const raw = fs.readFileSync(filePath, 'utf-8');
          let columns: string[];
          let rows: (string | null)[][];
          if (ext === '.json') {
            const arr: Record<string, unknown>[] = JSON.parse(raw);
            if (!Array.isArray(arr) || !arr.length) { send('importPreview', { error: 'JSON must be an array of objects.' }); break; }
            columns = Object.keys(arr[0]);
            rows = arr.map(r => columns.map(c => r[c] == null ? null : String(r[c])));
          } else {
            const lines = raw.split(/\r?\n/).filter(l => l.trim());
            const parseCsv = (line: string) => {
              const result: string[] = []; let cur = ''; let inQ = false;
              for (let i = 0; i < line.length; i++) {
                const ch = line[i];
                if (ch === '"') { if (inQ && line[i + 1] === '"') { cur += '"'; i++; } else inQ = !inQ; }
                else if (ch === ',' && !inQ) { result.push(cur); cur = ''; }
                else cur += ch;
              }
              result.push(cur); return result;
            };
            columns = parseCsv(lines[0]);
            rows = lines.slice(1).map(l => parseCsv(l).map(v => v === '' ? null : v));
          }
          send('importPreview', { connId, schema, tableName, columns, rows: rows.slice(0, 5), totalRows: rows.length, allRows: rows });
        } catch (err) {
          send('importPreview', { error: String(err) });
        }
        break;
      }

      case 'importTableData': {
        const { connId, schema, tableName, columns, rows } = data as {
          connId: string; schema: string; tableName: string;
          columns: string[]; rows: (string | null)[][];
        };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('importResult', { error: 'Not connected.' }); break; }
        try {
          const inserted = await importTableRows(driver, schema, tableName, columns, rows);
          send('importResult', { ok: true, inserted });
        } catch (err) {
          send('importResult', { error: String(err) });
        }
        break;
      }

      // ── Prisma schema gen (#60) ──────────────────────────────────────────

      case 'generatePrismaSchema': {
        const { connId, schema } = data as { connId: string; schema: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('prismaSchemaGenerated', { error: 'Not connected.' }); break; }
        try {
          const prismaSchema = await generatePrismaSchema(driver, schema);
          send('prismaSchemaGenerated', { schema: prismaSchema });
        } catch (err) {
          send('prismaSchemaGenerated', { error: String(err) });
        }
        break;
      }

      case 'savePrismaSchema': {
        const { content } = data as { content: string };
        const result = await dialog.showSaveDialog(mainWin, {
          defaultPath: 'schema.prisma',
          filters: [{ name: 'Prisma Schema', extensions: ['prisma'] }],
        });
        if (!result.canceled && result.filePath) {
          fs.writeFileSync(result.filePath, content, 'utf-8');
          send('prismaSchemasSaved', { ok: true, filePath: result.filePath });
        }
        break;
      }

      // ── Activity viewer (#53) ─────────────────────────────────────────────

      case 'loadActivity': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('activityLoaded', { connId, error: 'Not connected.' }); break; }
        try {
          const rows = await driver.query<Record<string, unknown>>(`
            SELECT pid, usename, application_name, state, wait_event_type, wait_event,
              EXTRACT(EPOCH FROM (now() - query_start))::numeric(10,1) AS duration,
              LEFT(query, 200) AS query
            FROM pg_stat_activity
            WHERE datname = current_database()
              AND pid <> pg_backend_pid()
            ORDER BY duration DESC NULLS LAST
          `);
          send('activityLoaded', { connId, rows });
        } catch (err) {
          send('activityLoaded', { connId, error: String(err) });
        }
        break;
      }

      case 'cancelActivity': {
        const { connId, pid } = data as { connId: string; pid: number };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try { await driver.query('SELECT pg_cancel_backend($1)', [pid]); } catch { /* best-effort */ }
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

      case 'saveSettings': {
        patchSettings(data as Record<string, unknown>);
        break;
      }

      case 'loadERDData': {
        const { connId, schema } = data as { connId: string; schema: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('erdDataLoaded', { connId, schema, error: 'Not connected.' }); return; }
        try {
          const erd = await getERDData(driver, schema);
          send('erdDataLoaded', { connId, schema, ...erd });
        } catch (err) {
          send('erdDataLoaded', { connId, schema, error: String(err) });
        }
        break;
      }
    }
  });

  // ── Invoke handlers (request-response) ───────────────────────────────────
  ipcMain.handle('ipc-invoke', async (_event, { command, data }: { command: string; data: unknown }) => {
    switch (command) {

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

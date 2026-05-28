import { BrowserWindow, ipcMain, dialog, Notification, shell, Menu } from 'electron';
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
  browseTableData, updateTableRow, getColumnStats, importTableRows,
  globalSearch, createSchema,
} from '../db/queries';
import { validateConnection, PgConnection } from '../types/PgConnection';
import {
  listConnections, getConnection, saveConnection, deleteConnection, getPassword,
  listHistory, addHistory, clearHistory, getSettings, patchSettings,
  getSshPassword, storeSshPassword,
  listSnippets, saveSnippet, deleteSnippet, renameSnippet,
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


// ── IPC registration ─────────────────────────────────────────────────────────

export function registerIpc(win: BrowserWindow): void {
  mainWin = win;

  // Remove stale listeners before re-registering (macOS activate calls this again)
  ipcMain.removeAllListeners('win-ctrl');
  ipcMain.removeAllListeners('from-renderer');
  ipcMain.removeHandler('ipc-invoke');

  // ── Window controls ───────────────────────────────────────────────────────
  ipcMain.on('win-ctrl', (_event, action: string) => {
    if (win.isDestroyed()) return;
    if (action === 'min') win.minimize();
    else if (action === 'max') { if (win.isMaximized()) win.unmaximize(); else win.maximize(); }
    else if (action === 'close') win.close();
  });
  const notifyMax = (v: boolean) => { if (!win.isDestroyed()) win.webContents.send('win-maximized', v); };
  win.on('maximize', () => notifyMax(true));
  win.on('unmaximize', () => notifyMax(false));

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

      case 'showAppMenu': {
        if (win.isDestroyed()) break;
        const menu = Menu.getApplicationMenu();
        if (menu) menu.popup({ window: win });
        break;
      }

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

      case 'createSchema': {
        const { connId, schemaName } = data as { connId: string; schemaName: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('createSchemaResult', { ok: false, error: 'Not connected.' }); break; }
        try {
          await createSchema(driver, schemaName);
          const schemas = await getSchemas(driver);
          send('schemasLoaded', { connId, schemas });
          send('createSchemaResult', { ok: true, schemaName });
        } catch (err: unknown) {
          send('createSchemaResult', { ok: false, error: (err as Error).message });
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
          send('queryResult', { tabId, columns, rows, rowCount: rows.length, durationMs });
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
              send('queryResult', { tabId, columns, rows, rowCount: rows.length, durationMs });
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

      // ── Lock viewer (#79) ─────────────────────────────────────────────────

      case 'loadLocks': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('locksLoaded', { connId, error: 'Not connected.' }); break; }
        try {
          const rows = await driver.query<Record<string, unknown>>(`
            SELECT
              blocked.pid AS blocked_pid,
              blocked_act.usename AS blocked_user,
              LEFT(blocked_act.query, 120) AS blocked_query,
              blocking.pid AS blocking_pid,
              blocking_act.usename AS blocking_user,
              LEFT(blocking_act.query, 120) AS blocking_query,
              blocked.locktype,
              EXTRACT(EPOCH FROM (now() - blocked_act.query_start))::numeric(10,1) AS wait_sec
            FROM pg_locks blocked
            JOIN pg_stat_activity blocked_act  ON blocked_act.pid  = blocked.pid
            JOIN pg_locks blocking             ON blocking.transactionid = blocked.transactionid
                                              AND blocking.pid != blocked.pid
                                              AND blocking.granted
            JOIN pg_stat_activity blocking_act ON blocking_act.pid = blocking.pid
            WHERE NOT blocked.granted
            ORDER BY wait_sec DESC NULLS LAST
          `);
          send('locksLoaded', { connId, rows });
        } catch (err) {
          send('locksLoaded', { connId, error: String(err) });
        }
        break;
      }

      // ── Sequence viewer (#85) ─────────────────────────────────────────────

      case 'loadSequences': {
        const { connId, schema } = data as { connId: string; schema: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('seqsLoaded', { connId, schema, sequences: [] }); break; }
        try {
          const sequences = await driver.query<Record<string, unknown>>(`
            SELECT sequence_name AS name,
              start_value, increment_by, min_value, max_value,
              cycle, cache_size,
              pg_get_serial_sequence(quote_ident(table_schema) || '.' || quote_ident(table_name), column_name) IS NOT NULL AS owned
            FROM information_schema.sequences
            LEFT JOIN information_schema.columns
              ON column_default LIKE 'nextval(%' || quote_ident(sequence_name) || '%'
            WHERE sequence_schema = $1
            ORDER BY sequence_name
          `, [schema]);
          send('seqsLoaded', { connId, schema, sequences });
        } catch {
          // fallback: simpler query
          try {
            const sequences = await driver.query<Record<string, unknown>>(`
              SELECT relname AS name, 0 AS increment_by
              FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE c.relkind = 'S' AND n.nspname = $1 ORDER BY relname
            `, [schema]);
            send('seqsLoaded', { connId, schema, sequences });
          } catch (err2) {
            send('seqsLoaded', { connId, schema, sequences: [], error: String(err2) });
          }
        }
        break;
      }

      case 'seqNextVal': {
        const { connId, schema, name } = data as { connId: string; schema: string; name: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          const rows = await driver.query<{ nextval: unknown }>(`SELECT nextval($1)`, [`"${schema}"."${name}"`]);
          send('seqNextValResult', { value: String(rows[0]?.nextval ?? '') });
        } catch (err) {
          send('seqNextValResult', { error: String(err) });
        }
        break;
      }

      case 'seqSetVal': {
        const { connId, schema, name, value } = data as { connId: string; schema: string; name: string; value: number };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(`SELECT setval($1, $2)`, [`"${schema}"."${name}"`, value]);
          send('seqNextValResult', { value: 'Reset to ' + value });
          // refresh the list
          const sequences = await driver.query<Record<string, unknown>>(`
            SELECT relname AS name, 0 AS increment_by
            FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE c.relkind = 'S' AND n.nspname = $1 ORDER BY relname
          `, [schema]);
          send('seqsLoaded', { connId, schema, sequences });
        } catch (err) {
          send('seqNextValResult', { error: String(err) });
        }
        break;
      }

      // ── User & Role manager (#80) ─────────────────────────────────────────

      case 'loadRoles': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('rolesLoaded', { connId, error: 'Not connected.' }); break; }
        try {
          const roles = await driver.query<Record<string, unknown>>(`
            SELECT rolname, rolsuper, rolcreatedb, rolcreaterole, rolcanlogin,
              rolconnlimit,
              to_char(rolvaliduntil, 'YYYY-MM-DD') AS rolvaliduntil
            FROM pg_roles
            ORDER BY rolname
          `);
          send('rolesLoaded', { connId, roles });
        } catch (err) {
          send('rolesLoaded', { connId, error: String(err) });
        }
        break;
      }

      case 'createRole': {
        const { connId, name, password, login, createdb, createrole, superuser } =
          data as { connId: string; name: string; password: string; login: boolean; createdb: boolean; createrole: boolean; superuser: boolean };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        const safeName = name.replace(/"/g, '""');
        const parts: string[] = [];
        if (login)      parts.push('LOGIN');
        if (password)   parts.push(`PASSWORD '${password.replace(/'/g, "''")}'`);
        if (createdb)   parts.push('CREATEDB');
        if (createrole) parts.push('CREATEROLE');
        if (superuser)  parts.push('SUPERUSER');
        try {
          await driver.query(`CREATE ROLE "${safeName}" ${parts.join(' ')}`);
          send('roleChanged', { connId });
        } catch (err) {
          send('showError', { message: String(err) });
        }
        break;
      }

      case 'dropRole': {
        const { connId, name } = data as { connId: string; name: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        const safeName = name.replace(/"/g, '""');
        try {
          await driver.query(`DROP ROLE IF EXISTS "${safeName}"`);
          send('roleChanged', { connId });
        } catch (err) {
          send('showError', { message: String(err) });
        }
        break;
      }

      // ── Dashboard ─────────────────────────────────────────────────────────

      case 'loadDashboard': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('dashboardLoaded', { connId, error: 'Not connected.' }); break; }
        try {
          let dashTimer: ReturnType<typeof setTimeout>;
          const timeout = new Promise<never>((_, reject) => {
            dashTimer = setTimeout(() => reject(new Error('Dashboard queries timed out after 15s')), 15_000);
          });
          const [serverRows, dbRows, connRows, perfRows, tableRows] = await Promise.race([
            Promise.all([
            driver.query<Record<string, unknown>>(`
              SELECT
                split_part(version(), ' ', 2) AS pg_version,
                pg_postmaster_start_time() AS start_time,
                EXTRACT(EPOCH FROM (now() - pg_postmaster_start_time()))::int AS uptime_sec,
                inet_server_addr()::text AS host,
                inet_server_port()::text AS port
            `),
            driver.query<Record<string, unknown>>(`
              SELECT
                current_database() AS db_name,
                pg_size_pretty(pg_database_size(current_database())) AS db_size,
                pg_encoding_to_char(encoding) AS encoding,
                datcollate AS collation
              FROM pg_database WHERE datname = current_database()
            `),
            driver.query<Record<string, unknown>>(`
              SELECT
                count(*)                                                              AS total,
                count(*) FILTER (WHERE state = 'active')                             AS active,
                count(*) FILTER (WHERE state = 'idle')                               AS idle,
                count(*) FILTER (WHERE state = 'idle in transaction')                AS idle_in_tx,
                current_setting('max_connections')::int                              AS max_conn
              FROM pg_stat_activity WHERE datname = current_database()
            `),
            driver.query<Record<string, unknown>>(`
              SELECT
                xact_commit AS commits, xact_rollback AS rollbacks, deadlocks, temp_files,
                CASE WHEN blks_hit + blks_read = 0 THEN 100
                     ELSE round(blks_hit::numeric / (blks_hit + blks_read) * 100, 2)
                END AS cache_hit
              FROM pg_stat_database WHERE datname = current_database()
            `),
            driver.query<Record<string, unknown>>(`
              SELECT n.nspname AS schema, c.relname AS table,
                pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size,
                pg_total_relation_size(c.oid) AS raw_bytes,
                s.n_live_tup
              FROM pg_class c
              JOIN pg_namespace n ON n.oid = c.relnamespace
              LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid
              WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema')
              ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 10
            `),
            ]),
            timeout,
          ]) as [Record<string, unknown>[], Record<string, unknown>[], Record<string, unknown>[], Record<string, unknown>[], Record<string, unknown>[]];
          clearTimeout(dashTimer!);
          const sv = serverRows[0] ?? {};
          const db = dbRows[0] ?? {};
          const cn = connRows[0] ?? {};
          const pf = perfRows[0] ?? {};
          const uptimeSec = Number(sv.uptime_sec) || 0;
          const days = Math.floor(uptimeSec / 86400);
          const hours = Math.floor((uptimeSec % 86400) / 3600);
          const mins  = Math.floor((uptimeSec % 3600) / 60);
          const uptime = days > 0 ? `${days}d ${hours}h ${mins}m` : hours > 0 ? `${hours}h ${mins}m` : `${mins}m`;
          send('dashboardLoaded', {
            connId,
            pgVersion: sv.pg_version, uptime, host: sv.host, port: sv.port,
            dbName: db.db_name, dbSize: db.db_size, encoding: db.encoding, collation: db.collation,
            totalConn: cn.total, activeConn: cn.active, idleConn: cn.idle, idleInTxConn: cn.idle_in_tx, maxConn: cn.max_conn,
            commits: pf.commits, rollbacks: pf.rollbacks, deadlocks: pf.deadlocks, tempFiles: pf.temp_files, cacheHit: pf.cache_hit,
            topTables: tableRows,
          });
        } catch (err) {
          clearTimeout(dashTimer!);
          send('dashboardLoaded', { connId, error: String(err) });
        }
        break;
      }

      // ── Drop table ────────────────────────────────────────────────────────

      case 'dropTable': {
        const { connId, schema, table } = data as { connId: string; schema: string; table: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(`DROP TABLE "${schema.replace(/"/g, '""')}"."${table.replace(/"/g, '""')}"`);
          send('dropTableResult', { connId, schema, table });
        } catch (err) {
          send('dropTableResult', { connId, schema, table, error: String(err) });
        }
        break;
      }

      // ── VACUUM / ANALYZE runner (#82) ────────────────────────────────────

      case 'runVacuum': {
        const { connId, schema, tableName, op } = data as { connId: string; schema: string; tableName: string; op: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        const allowedOps = ['VACUUM', 'ANALYZE', 'VACUUM ANALYZE', 'VACUUM FULL'];
        if (!allowedOps.includes(op)) break;
        const start = Date.now();
        try {
          await driver.query(`${op} "${schema}"."${tableName}"`);
          send('vacuumResult', { op, durationMs: Date.now() - start });
        } catch (err) {
          send('vacuumResult', { op, error: String(err), durationMs: Date.now() - start });
        }
        break;
      }

      // ── Extension manager (#84) ───────────────────────────────────────────

      case 'loadExtensions': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('extsLoaded', { connId, installed: [], available: [] }); break; }
        try {
          const [installed, available] = await Promise.all([
            driver.query<Record<string, unknown>>(`
              SELECT name, default_version, installed_version, comment
              FROM pg_available_extensions
              WHERE installed_version IS NOT NULL
              ORDER BY name
            `),
            driver.query<Record<string, unknown>>(`
              SELECT name, default_version, comment
              FROM pg_available_extensions
              ORDER BY name
              LIMIT 200
            `),
          ]);
          send('extsLoaded', { connId, installed, available });
        } catch (err) {
          send('extsLoaded', { connId, installed: [], available: [], error: String(err) });
        }
        break;
      }

      case 'extInstall': {
        const { connId, name } = data as { connId: string; name: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(`CREATE EXTENSION IF NOT EXISTS "${name}"`);
          // reload list
          const installed = await driver.query<Record<string, unknown>>(`
            SELECT name, default_version, installed_version, comment
            FROM pg_available_extensions WHERE installed_version IS NOT NULL ORDER BY name
          `);
          const available = await driver.query<Record<string, unknown>>(`
            SELECT name, default_version, comment FROM pg_available_extensions ORDER BY name LIMIT 200
          `);
          send('extsLoaded', { connId, installed, available });
        } catch (err) {
          send('showError', { message: String(err) });
        }
        break;
      }

      case 'extDrop': {
        const { connId, name } = data as { connId: string; name: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(`DROP EXTENSION IF EXISTS "${name}"`);
          const installed = await driver.query<Record<string, unknown>>(`
            SELECT name, default_version, installed_version, comment
            FROM pg_available_extensions WHERE installed_version IS NOT NULL ORDER BY name
          `);
          const available = await driver.query<Record<string, unknown>>(`
            SELECT name, default_version, comment FROM pg_available_extensions ORDER BY name LIMIT 200
          `);
          send('extsLoaded', { connId, installed, available });
        } catch (err) {
          send('showError', { message: String(err) });
        }
        break;
      }

      // ── Database stats dashboard (#86) ───────────────────────────────────

      case 'loadDbStats': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('dbStatsLoaded', { connId, error: 'Not connected.' }); break; }
        try {
          const [dbRows, tableRows, bloatRows, unusedIdxRows, seqScanRows] = await Promise.all([
            driver.query<Record<string, unknown>>(`
              SELECT
                xact_commit, xact_rollback, deadlocks, temp_files,
                blks_hit, blks_read,
                CASE WHEN blks_hit + blks_read = 0 THEN 100
                     ELSE round(blks_hit::numeric / (blks_hit + blks_read) * 100, 2)
                END AS cache_hit_pct,
                pg_size_pretty(pg_database_size(current_database())) AS size
              FROM pg_stat_database
              WHERE datname = current_database()
            `),
            driver.query<Record<string, unknown>>(`
              SELECT
                n.nspname AS schema,
                c.relname AS table,
                pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size,
                pg_size_pretty(pg_relation_size(c.oid))       AS table_size,
                pg_size_pretty(pg_indexes_size(c.oid))        AS index_size,
                s.n_live_tup, s.n_dead_tup
              FROM pg_class c
              JOIN pg_namespace n ON n.oid = c.relnamespace
              LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid
              WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema')
              ORDER BY pg_total_relation_size(c.oid) DESC
              LIMIT 30
            `),
            driver.query<Record<string, unknown>>(`
              SELECT
                schemaname AS schema, relname AS table,
                n_live_tup, n_dead_tup,
                CASE WHEN n_live_tup + n_dead_tup = 0 THEN 0
                     ELSE round(n_dead_tup::numeric / (n_live_tup + n_dead_tup) * 100, 1)
                END AS bloat_pct,
                to_char(last_autovacuum, 'YYYY-MM-DD HH24:MI')  AS last_autovacuum,
                to_char(last_vacuum,     'YYYY-MM-DD HH24:MI')  AS last_vacuum
              FROM pg_stat_user_tables
              WHERE n_dead_tup > 0
              ORDER BY bloat_pct DESC NULLS LAST
              LIMIT 30
            `),
            driver.query<Record<string, unknown>>(`
              SELECT schemaname AS schema, relname AS table, indexrelname AS index,
                pg_size_pretty(pg_relation_size(indexrelid)) AS size, idx_scan
              FROM pg_stat_user_indexes
              WHERE idx_scan = 0
              ORDER BY pg_relation_size(indexrelid) DESC
              LIMIT 30
            `),
            driver.query<Record<string, unknown>>(`
              SELECT schemaname AS schema, relname AS table,
                seq_scan, n_live_tup,
                pg_size_pretty(pg_relation_size(relid)) AS size
              FROM pg_stat_user_tables
              WHERE seq_scan > 50 AND n_live_tup > 1000
              ORDER BY seq_scan DESC
              LIMIT 20
            `),
          ]);
          send('dbStatsLoaded', { connId, db: dbRows[0] ?? {}, tables: tableRows, bloat: bloatRows, unusedIdx: unusedIdxRows, seqScans: seqScanRows });
        } catch (err) {
          send('dbStatsLoaded', { connId, error: String(err) });
        }
        break;
      }

      // ── Function editor ───────────────────────────────────────────────────

      case 'getFunctionDDL': {
        const { connId, schema, funcName } = data as { connId: string; schema: string; funcName: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('functionDDLLoaded', { connId, error: 'Not connected.' }); break; }
        try {
          const rows = await driver.query<{ ddl: string; args: string; oid: string }>(`
            SELECT pg_get_functiondef(p.oid) AS ddl,
                   pg_get_function_identity_arguments(p.oid) AS args,
                   p.oid::text AS oid
            FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = $1 AND p.proname = $2
            ORDER BY p.oid
          `, [schema, funcName]);
          send('functionDDLLoaded', { connId, schema, funcName, overloads: rows });
        } catch (err) {
          send('functionDDLLoaded', { connId, error: String(err) });
        }
        break;
      }

      case 'saveFunction': {
        const { connId, ddl } = data as { connId: string; ddl: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(ddl);
          send('functionSaved', { connId });
        } catch (err) {
          send('functionSaved', { connId, error: String(err) });
        }
        break;
      }

      case 'validateFunction': {
        const { connId, ddl } = data as { connId: string; ddl: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query('BEGIN');
          await driver.query(ddl);
          await driver.query('ROLLBACK');
          send('functionValidated', { connId, valid: true });
        } catch (err) {
          try { await driver.query('ROLLBACK'); } catch (_) { /* ignore */ }
          send('functionValidated', { connId, valid: false, error: String(err) });
        }
        break;
      }

      case 'callFunction': {
        const { connId, sql } = data as { connId: string; sql: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          const rows = await driver.query<Record<string, unknown>>(sql);
          const columns = rows.length > 0 ? Object.keys(rows[0]) : [];
          send('funcTestResult', { connId, rows, columns });
        } catch (err) {
          send('funcTestResult', { connId, error: String(err) });
        }
        break;
      }

      // ── pg_cron Job manager ───────────────────────────────────────────────

      case 'loadJobs': {
        const { connId } = data as { connId: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('jobsLoaded', { connId, error: 'Not connected.' }); break; }
        try {
          const extRows = await driver.query<{ extname: string }>(
            `SELECT extname FROM pg_extension WHERE extname = 'pg_cron'`
          );
          if (!extRows.length) { send('jobsLoaded', { connId, installed: false, jobs: [] }); break; }
          const jobRows = await driver.query<Record<string, unknown>>(`
            SELECT j.jobid, j.jobname, j.schedule, j.command, j.database, j.username, j.active,
              r.status AS last_status,
              to_char(r.start_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS last_run,
              r.return_message AS last_msg
            FROM cron.job j
            LEFT JOIN LATERAL (
              SELECT status, start_time, return_message
              FROM cron.job_run_details
              WHERE jobid = j.jobid
              ORDER BY start_time DESC LIMIT 1
            ) r ON true
            ORDER BY j.jobid
          `);
          send('jobsLoaded', { connId, installed: true, jobs: jobRows });
        } catch (err) {
          send('jobsLoaded', { connId, error: String(err) });
        }
        break;
      }

      case 'createJob': {
        const { connId, name, schedule, command } = data as { connId: string; name: string; schedule: string; command: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          if (name && name.trim()) {
            await driver.query(`SELECT cron.schedule($1, $2, $3)`, [name.trim(), schedule, command]);
          } else {
            await driver.query(`SELECT cron.schedule($1, $2)`, [schedule, command]);
          }
          send('jobChanged', { connId });
        } catch (err) {
          send('jobChanged', { connId, error: String(err) });
        }
        break;
      }

      case 'updateJob': {
        const { connId, jobId, schedule, command } = data as { connId: string; jobId: number; schedule: string; command: string };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(`UPDATE cron.job SET schedule = $1, command = $2 WHERE jobid = $3`, [schedule, command, jobId]);
          send('jobChanged', { connId });
        } catch (err) {
          send('jobChanged', { connId, error: String(err) });
        }
        break;
      }

      case 'toggleJob': {
        const { connId, jobId, active } = data as { connId: string; jobId: number; active: boolean };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(`UPDATE cron.job SET active = $1 WHERE jobid = $2`, [active, jobId]);
          send('jobChanged', { connId });
        } catch (err) {
          send('jobChanged', { connId, error: String(err) });
        }
        break;
      }

      case 'deleteJob': {
        const { connId, jobId } = data as { connId: string; jobId: number };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          await driver.query(`SELECT cron.unschedule($1)`, [jobId]);
          send('jobChanged', { connId });
        } catch (err) {
          send('jobChanged', { connId, error: String(err) });
        }
        break;
      }

      case 'loadJobRuns': {
        const { connId, jobId } = data as { connId: string; jobId: number };
        const driver = connManager.getDriver(connId);
        if (!driver) break;
        try {
          const rows = await driver.query<Record<string, unknown>>(`
            SELECT runid, status, return_message,
              to_char(start_time AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS start_time,
              to_char(end_time   AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS') AS end_time,
              CASE WHEN end_time IS NOT NULL AND start_time IS NOT NULL
                THEN round(EXTRACT(EPOCH FROM (end_time - start_time))::numeric, 2)
                ELSE NULL END AS duration_sec
            FROM cron.job_run_details
            WHERE jobid = $1
            ORDER BY start_time DESC LIMIT 50
          `, [jobId]);
          send('jobRunsLoaded', { connId, jobId, runs: rows });
        } catch (err) {
          send('jobRunsLoaded', { connId, jobId, error: String(err) });
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

      // ── Global search (#74) ──────────────────────────────────────────────

      case 'globalSearch': {
        const { connId, term } = data as { connId: string; term: string };
        const driver = connManager.getDriver(connId);
        if (!driver) { send('searchResults', { results: [] }); break; }
        try {
          const results = await globalSearch(driver, term);
          send('searchResults', { results });
        } catch {
          send('searchResults', { results: [] });
        }
        break;
      }

      // ── Snippets (#65) ────────────────────────────────────────────────────

      case 'loadSnippets':
        send('snippetsLoaded', listSnippets());
        break;

      case 'saveSnippet': {
        const { name, sql } = data as { name: string; sql: string };
        const s = saveSnippet({ name, sql });
        send('snippetsLoaded', listSnippets());
        send('snippetSaved', s);
        break;
      }

      case 'deleteSnippet': {
        const { id } = data as { id: string };
        deleteSnippet(id);
        send('snippetsLoaded', listSnippets());
        break;
      }

      case 'renameSnippet': {
        const { id, name } = data as { id: string; name: string };
        renameSnippet(id, name);
        send('snippetsLoaded', listSnippets());
        break;
      }

      // ── Favorites (#69) ───────────────────────────────────────────────────

      case 'toggleFavorite': {
        const { id } = data as { id: string };
        const conn = getConnection(id);
        if (!conn) break;
        const password = getPassword(id);
        saveConnection({ ...conn, favorite: !conn.favorite }, password);
        pushConnections();
        break;
      }

      case 'open-external': {
        const url = data as string;
        if (typeof url === 'string' && (url.startsWith('https://') || url.startsWith('http://'))) {
          shell.openExternal(url);
        }
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

      case 'get-about-info': {
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const pkg = require('../../package.json') as { version: string };
        return {
          version:  pkg.version,
          electron: process.versions.electron,
          node:     process.versions.node,
          chrome:   process.versions.chrome,
        };
      }

      default:
        return null;
    }
  });
}

import * as vscode from 'vscode';
import * as path from 'path';
import { spawn } from 'child_process';
import { ConnectionStorage } from '../storage/ConnectionStorage';
import { ConnectionManager } from '../db/ConnectionManager';
import { HistoryStorage } from '../storage/HistoryStorage';
import { testConnection } from '../db/PgDriver';
import { getSchemas, getTables, getColumns, getFunctions, getFunctionParams, previewTable, getCompletionData, getTableDDL, getIndexes, getConstraints, getFKMap, checkMigrationsTable, getMigrations } from '../db/queries';
import { validateConnection } from '../types/PgConnection';
import { PgConnection } from '../types/PgConnection';
import { getSidebarHtml } from '../webview/getSidebarHtml';
import { PreviewPanel } from './PreviewPanel';
import { DDLPanel } from './DDLPanel';
import { DriftPanel } from './DriftPanel';
import { ParsedPrismaSchema, parsePrismaSchema } from '../prisma/PrismaParser';

interface IpcMessage {
  command: string;
  data?: unknown;
}

interface SaveConnectionData {
  conn: {
    id?: string;
    label: string;
    host: string;
    port: number;
    database: string;
    user: string;
    ssl: boolean;
  };
  password: string;
}

export class SidebarProvider implements vscode.WebviewViewProvider {
  static readonly viewId = 'prisma4postgres.sidebar';

  private _view?: vscode.WebviewView;
  private _prismaSchema: ParsedPrismaSchema | null = null;

  constructor(
    private readonly _context: vscode.ExtensionContext,
    private readonly _storage: ConnectionStorage,
    private readonly _connManager: ConnectionManager,
    private readonly _history: HistoryStorage
  ) {}

  setPrismaSchema(schema: ParsedPrismaSchema | null): void {
    this._prismaSchema = schema;
    this.postMessage('prismaSchema', schema);
  }

  resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ) {
    this._view = webviewView;

    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this._context.extensionUri],
    };

    webviewView.webview.html = this._getHtml(webviewView.webview);

    webviewView.webview.onDidReceiveMessage(
      (message: IpcMessage) => this._handleMessage(message),
      undefined,
      this._context.subscriptions
    );
  }

  postMessage(command: string, data?: unknown) {
    this._view?.webview.postMessage({ command, data });
  }

  private _pushConnections() {
    const connections = this._storage.listConnections();
    this._connManager.syncConnections(connections);
    this.postMessage('updateConnections', connections);
    this.postMessage('updateStatuses', this._connManager.getStatuses());
  }

  private async _handleMessage(message: IpcMessage) {
    switch (message.command) {

      case 'ready':
        this._pushConnections();
        if (this._prismaSchema) this.postMessage('prismaSchema', this._prismaSchema);
        break;

      case 'addConnection':
        this.postMessage('switchTab', 'explorer');
        break;

      case 'refreshConnections':
        this._pushConnections();
        break;

      // ── Connection CRUD ───────────────────────────────────────────

      case 'saveConnection': {
        const { conn, password } = message.data as SaveConnectionData;
        const errors = validateConnection(conn);
        if (errors.length > 0) { vscode.window.showErrorMessage(errors.join(', ')); return; }

        let finalPassword = password;
        if (!finalPassword && conn.id) finalPassword = await this._storage.getPassword(conn.id);

        try {
          await this._storage.saveConnection(conn, finalPassword);
          this._pushConnections();
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to save connection: ${String(err)}`);
        }
        break;
      }

      case 'deleteConnection': {
        const { id } = message.data as { id: string };
        const conn = this._storage.getConnection(id);
        const answer = await vscode.window.showWarningMessage(
          `Delete connection "${conn?.label ?? id}"?`,
          { modal: true },
          'Delete'
        );
        if (answer !== 'Delete') return;
        try {
          await this._connManager.disconnect(id);
          await this._storage.deleteConnection(id);
          this._pushConnections();
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to delete connection: ${String(err)}`);
        }
        break;
      }

      case 'testConnection': {
        const { conn, password } = message.data as SaveConnectionData;
        try {
          await testConnection(conn as never, password);
          this.postMessage('testResult', { success: true });
        } catch (err) {
          this.postMessage('testResult', { success: false, message: String(err) });
        }
        break;
      }

      // ── Connect / Disconnect (#8) ──────────────────────────────────

      case 'connect': {
        const { connId } = message.data as { connId: string };
        const password = await this._storage.getPassword(connId);
        try {
          await this._connManager.connect(connId, password);
          this.postMessage('connectionStatus', { id: connId, status: 'connected' });
        } catch (err) {
          this.postMessage('connectionStatus', { id: connId, status: 'error', message: String(err) });
          vscode.window.showErrorMessage(`Connection failed: ${String(err)}`);
        }
        break;
      }

      case 'disconnect': {
        const { connId } = message.data as { connId: string };
        await this._connManager.disconnect(connId);
        this.postMessage('connectionStatus', { id: connId, status: 'disconnected' });
        break;
      }

      // ── Schema / Table / Column / Function loading (#9–#12) ───────

      case 'loadSchemas': {
        const { connId } = message.data as { connId: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) return;
        try {
          const schemas = await getSchemas(driver);
          this.postMessage('schemasLoaded', { connId, schemas });
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to load schemas: ${String(err)}`);
        }
        break;
      }

      case 'loadTables': {
        const { connId, schema } = message.data as { connId: string; schema: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) return;
        try {
          const tables = await getTables(driver, schema);
          this.postMessage('tablesLoaded', { connId, schema, tables });
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to load tables: ${String(err)}`);
        }
        break;
      }

      case 'loadColumns': {
        const { connId, schema, table } = message.data as { connId: string; schema: string; table: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) return;
        try {
          const tableInfo = this._connManager.get(connId);
          const columns = await getColumns(driver, schema, table);
          const tList = tableInfo ? [] : [];
          const isView = false; // resolved from loaded tables state on webview side
          this.postMessage('columnsLoaded', { connId, schema, table, columns, isView });
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to load columns: ${String(err)}`);
        }
        break;
      }

      case 'loadFunctions': {
        const { connId, schema } = message.data as { connId: string; schema: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) return;
        try {
          const functions = await getFunctions(driver, schema);
          this.postMessage('functionsLoaded', { connId, schema, functions });
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to load functions: ${String(err)}`);
        }
        break;
      }

      case 'loadFuncParams': {
        const { connId, schema, specificName } = message.data as { connId: string; schema: string; specificName: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) return;
        try {
          const params = await getFunctionParams(driver, schema, specificName);
          this.postMessage('funcParamsLoaded', { connId, schema, specificName, params });
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to load function params: ${String(err)}`);
        }
        break;
      }

      // ── Query execution (#15–#18) ────────────────────────────────

      case 'executeQuery': {
        const { connId, sql, tabId } = message.data as { connId: string; sql: string; tabId: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) {
          this.postMessage('queryResult', { tabId, error: 'Not connected. Please connect first.' });
          return;
        }
        const conn = this._storage.getConnection(connId);
        const start = Date.now();
        try {
          const rows = await driver.query(sql);
          const durationMs = Date.now() - start;
          const columns = rows.length > 0 ? Object.keys(rows[0]) : [];
          this.postMessage('queryResult', { tabId, columns, rows, durationMs });
          await this._history.add({
            sql,
            connId,
            connLabel: conn?.label ?? connId,
            timestamp: Date.now(),
            durationMs,
            rowCount: rows.length,
          });
        } catch (err) {
          const durationMs = Date.now() - start;
          this.postMessage('queryResult', { tabId, error: String(err), durationMs });
        }
        break;
      }

      case 'loadHistory': {
        this.postMessage('historyLoaded', this._history.list());
        break;
      }

      case 'loadCompletions': {
        const { connId } = message.data as { connId: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) return;
        try {
          const data = await getCompletionData(driver);
          this.postMessage('completionsLoaded', data);
        } catch {
          // completions are best-effort
        }
        break;
      }

      // ── DDL Viewer (#20, #21, #22) ────────────────────────────────

      case 'openDDL': {
        const { connId, schema, table } = message.data as { connId: string; schema: string; table: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) { vscode.window.showErrorMessage('Not connected. Please connect first.'); return; }
        const conn = this._storage.getConnection(connId);
        try {
          const [ddl, indexes, constraints, fkMap] = await Promise.all([
            getTableDDL(driver, schema, table),
            getIndexes(driver, schema, table),
            getConstraints(driver, schema, table),
            getFKMap(driver, schema, table),
          ]);
          DDLPanel.show({
            extensionUri: this._context.extensionUri,
            connId,
            connLabel: conn?.label ?? connId,
            schema,
            table,
            ddl,
            indexes,
            constraints,
            fkMap,
            onNavigate: (cid, s, t) => {
              this.postMessage('navigateToTable', { connId: cid, schema: s, table: t });
            },
          });
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to load DDL: ${String(err)}`);
        }
        break;
      }

      // ── Export result (#23) ───────────────────────────────────────

      case 'exportResult': {
        const { format, columns, rows } = message.data as {
          format: 'csv' | 'json';
          columns: string[];
          rows: Record<string, unknown>[];
        };
        const ext = format === 'csv' ? 'csv' : 'json';
        const uri = await vscode.window.showSaveDialog({
          filters: format === 'csv' ? { 'CSV files': ['csv'] } : { 'JSON files': ['json'] },
          defaultUri: vscode.Uri.file(`query-result.${ext}`),
        });
        if (!uri) break;
        let content: string;
        if (format === 'csv') {
          const escape = (v: unknown) => {
            if (v === null || v === undefined) return '';
            const s = typeof v === 'object' ? JSON.stringify(v) : String(v);
            return s.includes(',') || s.includes('"') || s.includes('\n')
              ? '"' + s.replace(/"/g, '""') + '"'
              : s;
          };
          content = [
            columns.map(escape).join(','),
            ...rows.map(row => columns.map(c => escape(row[c])).join(',')),
          ].join('\n');
        } else {
          content = JSON.stringify(rows, null, 2);
        }
        await vscode.workspace.fs.writeFile(uri, Buffer.from(content, 'utf-8'));
        vscode.window.showInformationMessage(`Exported ${rows.length} rows to ${uri.fsPath}`);
        break;
      }

      // ── Prisma Integration (#24–#28) ──────────────────────────────

      case 'loadMigrations': {
        const { connId } = message.data as { connId: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) return;
        try {
          const hasTable = await checkMigrationsTable(driver);
          if (!hasTable) { this.postMessage('migrationsLoaded', { hasTable: false, entries: [] }); return; }
          const entries = await getMigrations(driver);
          this.postMessage('migrationsLoaded', { hasTable: true, entries });
        } catch (err) {
          this.postMessage('migrationsLoaded', { hasTable: false, error: String(err) });
        }
        break;
      }

      case 'runPrismaCommand': {
        const { command, connId } = message.data as { command: 'db-pull' | 'migrate-status'; connId: string };
        if (!this._prismaSchema) {
          this.postMessage('prismaLog', { text: 'No schema.prisma found in workspace.\n', done: true }); return;
        }
        const conn = this._storage.getConnection(connId);
        if (!conn) {
          this.postMessage('prismaLog', { text: 'No connection selected.\n', done: true }); return;
        }
        const password = await this._storage.getPassword(connId);
        const databaseUrl = buildDatabaseUrl(conn, password);
        const schemaPath = this._prismaSchema.filePath;
        const args = command === 'db-pull'
          ? ['prisma', 'db', 'pull', '--schema', schemaPath]
          : ['prisma', 'migrate', 'status', '--schema', schemaPath];

        const proc = spawn('npx', args, {
          cwd: path.dirname(schemaPath),
          env: { ...process.env, DATABASE_URL: databaseUrl },
          shell: process.platform === 'win32',
        });

        this.postMessage('prismaLog', { text: `> npx ${args.join(' ')}\n`, done: false });
        proc.stdout.on('data', (data: Buffer) => this.postMessage('prismaLog', { text: data.toString(), done: false }));
        proc.stderr.on('data', (data: Buffer) => this.postMessage('prismaLog', { text: data.toString(), done: false }));
        proc.on('close', (code: number | null) => {
          this.postMessage('prismaLog', { text: `\nFinished (exit ${code ?? '?'}).\n`, done: true });
          if (command === 'db-pull' && code === 0) {
            vscode.workspace.fs.readFile(vscode.Uri.file(schemaPath)).then(bytes => {
              const parsed = parsePrismaSchema(schemaPath, Buffer.from(bytes).toString('utf-8'));
              this._prismaSchema = parsed;
              this.postMessage('prismaSchema', parsed);
            });
          }
        });
        proc.on('error', (err: Error) => {
          this.postMessage('prismaLog', { text: `Error: ${err.message}\n`, done: true });
        });
        break;
      }

      case 'openDrift': {
        const { connId, schema, tableName, modelName } = message.data as {
          connId: string; schema: string; tableName: string; modelName: string;
        };
        const driver = this._connManager.getDriver(connId);
        if (!driver) { vscode.window.showErrorMessage('Not connected.'); return; }
        const model = this._prismaSchema?.models.find(m => m.name === modelName);
        if (!model) { vscode.window.showErrorMessage(`Model "${modelName}" not found.`); return; }
        try {
          const columns = await getColumns(driver, schema, tableName);
          DriftPanel.show({ modelName, tableName, schema, model, columns });
        } catch (err) {
          vscode.window.showErrorMessage(`Failed to load columns: ${String(err)}`);
        }
        break;
      }

      // ── Preview (#13) ─────────────────────────────────────────────

      case 'previewTable': {
        const { connId, schema, table } = message.data as { connId: string; schema: string; table: string };
        const driver = this._connManager.getDriver(connId);
        if (!driver) { vscode.window.showErrorMessage('Not connected. Please connect first.'); return; }
        const conn = this._storage.getConnection(connId);
        try {
          const { columns, rows, estimate } = await previewTable(driver, schema, table);
          PreviewPanel.show({
            extensionUri: this._context.extensionUri,
            connLabel: conn?.label ?? connId,
            schema,
            table,
            columns,
            rows,
            estimate,
          });
        } catch (err) {
          vscode.window.showErrorMessage(`Preview failed: ${String(err)}`);
        }
        break;
      }
    }
  }

  private _getHtml(webview: vscode.Webview): string {
    const codiconsUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this._context.extensionUri, 'node_modules', '@vscode/codicons', 'dist', 'codicon.css')
    );
    return getSidebarHtml({
      nonce: getNonce(),
      cspSource: webview.cspSource,
      codiconsUri: codiconsUri.toString(),
    });
  }
}

function getNonce(): string {
  let text = '';
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 32; i++) text += chars.charAt(Math.floor(Math.random() * chars.length));
  return text;
}

function buildDatabaseUrl(conn: PgConnection, password: string): string {
  const user = encodeURIComponent(conn.user);
  const pass = encodeURIComponent(password);
  const db   = encodeURIComponent(conn.database);
  const ssl  = conn.ssl ? '?sslmode=require' : '';
  return `postgresql://${user}:${pass}@${conn.host}:${conn.port}/${db}${ssl}`;
}

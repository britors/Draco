import * as vscode from 'vscode';
import { ConnectionStorage } from '../storage/ConnectionStorage';
import { ConnectionManager } from '../db/ConnectionManager';
import { testConnection } from '../db/PgDriver';
import { getSchemas, getTables, getColumns, getFunctions, getFunctionParams, previewTable } from '../db/queries';
import { validateConnection } from '../types/PgConnection';
import { getSidebarHtml } from '../webview/getSidebarHtml';
import { PreviewPanel } from './PreviewPanel';

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

  constructor(
    private readonly _context: vscode.ExtensionContext,
    private readonly _storage: ConnectionStorage,
    private readonly _connManager: ConnectionManager
  ) {}

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

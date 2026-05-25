import * as vscode from 'vscode';
import { ConnectionStorage } from '../storage/ConnectionStorage';
import { testConnection } from '../db/PgDriver';
import { validateConnection } from '../types/PgConnection';
import { getSidebarHtml } from '../webview/getSidebarHtml';

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
    private readonly _storage: ConnectionStorage
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
    this.postMessage('updateConnections', this._storage.listConnections());
  }

  private async _handleMessage(message: IpcMessage) {
    switch (message.command) {
      case 'ready':
        this._pushConnections();
        break;

      case 'addConnection':
        // triggered by the activity-bar button — webview already shows the form via its own state,
        // but if the sidebar is on another tab we switch to explorer
        this.postMessage('switchTab', 'explorer');
        break;

      case 'refreshConnections':
        this._pushConnections();
        break;

      case 'saveConnection': {
        const { conn, password } = message.data as SaveConnectionData;

        const errors = validateConnection(conn);
        if (errors.length > 0) {
          vscode.window.showErrorMessage(errors.join(', '));
          return;
        }

        // Keep existing password when the field is left blank on edit
        let finalPassword = password;
        if (!finalPassword && conn.id) {
          finalPassword = await this._storage.getPassword(conn.id);
        }

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
        const label = conn?.label ?? id;

        const answer = await vscode.window.showWarningMessage(
          `Delete connection "${label}"?`,
          { modal: true },
          'Delete'
        );
        if (answer !== 'Delete') return;

        try {
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
    }
  }

  private _getHtml(webview: vscode.Webview): string {
    const codiconsUri = webview.asWebviewUri(
      vscode.Uri.joinPath(
        this._context.extensionUri,
        'node_modules',
        '@vscode/codicons',
        'dist',
        'codicon.css'
      )
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
  for (let i = 0; i < 32; i++) {
    text += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return text;
}

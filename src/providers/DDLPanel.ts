import * as vscode from 'vscode';
import { IndexInfo, ConstraintInfo, FKMapEntry } from '../db/queries';
import { getDDLHtml } from '../webview/getDDLHtml';

interface DDLPanelOptions {
  extensionUri: vscode.Uri;
  connId: string;
  connLabel: string;
  schema: string;
  table: string;
  ddl: string;
  indexes: IndexInfo[];
  constraints: ConstraintInfo[];
  fkMap: FKMapEntry[];
  onNavigate: (connId: string, schema: string, table: string) => void;
}

export class DDLPanel {
  private static readonly _panels = new Map<string, DDLPanel>();

  private readonly _panel: vscode.WebviewPanel;
  private readonly _connId: string;
  private readonly _onNavigate: (connId: string, schema: string, table: string) => void;

  static show(opts: DDLPanelOptions): void {
    const key = `${opts.connId}:${opts.schema}.${opts.table}`;
    const existing = DDLPanel._panels.get(key);
    if (existing) { existing._panel.reveal(vscode.ViewColumn.One); return; }
    new DDLPanel(opts, key);
  }

  private constructor(opts: DDLPanelOptions, key: string) {
    this._connId = opts.connId;
    this._onNavigate = opts.onNavigate;

    this._panel = vscode.window.createWebviewPanel(
      'draco.ddl',
      `DDL: ${opts.schema}.${opts.table}`,
      vscode.ViewColumn.One,
      { enableScripts: true, retainContextWhenHidden: true }
    );

    const nonce = getNonce();
    this._panel.webview.html = getDDLHtml({
      nonce,
      cspSource: this._panel.webview.cspSource,
      schema: opts.schema,
      table: opts.table,
      ddl: opts.ddl,
      indexes: opts.indexes,
      constraints: opts.constraints,
      fkMap: opts.fkMap,
    });

    this._panel.webview.onDidReceiveMessage(msg => {
      if (msg.command === 'navigateToTable') {
        const { schema, table } = msg.data as { schema: string; table: string };
        this._onNavigate(this._connId, schema, table);
      }
    });

    this._panel.onDidDispose(() => DDLPanel._panels.delete(key));
    DDLPanel._panels.set(key, this);
  }
}

function getNonce(): string {
  let text = '';
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 32; i++) text += chars.charAt(Math.floor(Math.random() * chars.length));
  return text;
}

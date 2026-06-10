import * as vscode from 'vscode';
import { getPreviewHtml } from '../webview/getPreviewHtml';

interface PreviewParams {
  extensionUri: vscode.Uri;
  connLabel: string;
  schema: string;
  table: string;
  columns: string[];
  rows: Record<string, unknown>[];
  estimate: number;
}

export class PreviewPanel {
  private static readonly _panels = new Map<string, vscode.WebviewPanel>();

  static show(params: PreviewParams): void {
    const key = `${params.schema}.${params.table}`;
    const title = `${params.schema}.${params.table}`;

    let panel = this._panels.get(key);

    if (panel) {
      panel.reveal(vscode.ViewColumn.One);
    } else {
      panel = vscode.window.createWebviewPanel(
        'draco.preview',
        title,
        vscode.ViewColumn.One,
        { enableScripts: false, retainContextWhenHidden: true }
      );

      panel.onDidDispose(() => this._panels.delete(key));
      this._panels.set(key, panel);
    }

    panel.title = title;
    panel.webview.html = getPreviewHtml({
      cspSource: panel.webview.cspSource,
      connLabel: params.connLabel,
      schema: params.schema,
      table: params.table,
      columns: params.columns,
      rows: params.rows,
      estimate: params.estimate,
    });
  }
}

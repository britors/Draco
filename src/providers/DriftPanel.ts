import * as vscode from 'vscode';
import { PrismaModel } from '../parser/DracoParser';
import { ColumnInfo } from '../db/queries';
import { getDriftHtml } from '../webview/getDriftHtml';

interface DriftPanelOptions {
  modelName: string;
  tableName: string;
  schema: string;
  model: PrismaModel;
  columns: ColumnInfo[];
}

export class DriftPanel {
  private static readonly _panels = new Map<string, DriftPanel>();

  private readonly _panel: vscode.WebviewPanel;

  static show(opts: DriftPanelOptions): void {
    const key = `drift:${opts.schema}.${opts.tableName}`;
    const existing = DriftPanel._panels.get(key);
    if (existing) { existing._panel.reveal(vscode.ViewColumn.One); return; }
    new DriftPanel(opts, key);
  }

  private constructor(opts: DriftPanelOptions, key: string) {
    this._panel = vscode.window.createWebviewPanel(
      'draco.drift',
      `Drift: ${opts.modelName} ↔ ${opts.schema}.${opts.tableName}`,
      vscode.ViewColumn.One,
      { enableScripts: false }
    );
    this._panel.webview.html = getDriftHtml(opts);
    this._panel.onDidDispose(() => DriftPanel._panels.delete(key));
    DriftPanel._panels.set(key, this);
  }
}

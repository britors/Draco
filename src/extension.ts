import * as vscode from 'vscode';
import { SidebarProvider } from './providers/SidebarProvider';
import { ConnectionStorage } from './storage/ConnectionStorage';
import { ConnectionManager } from './db/ConnectionManager';
import { HistoryStorage } from './storage/HistoryStorage';
import { parseDracoSchema } from './parser/DracoParser';

export function activate(context: vscode.ExtensionContext) {
  const storage     = new ConnectionStorage(context.globalState, context.secrets);
  const connManager = new ConnectionManager();
  const history     = new HistoryStorage(context.globalState);
  const out         = vscode.window.createOutputChannel('Draco');
  const provider    = new SidebarProvider(context, storage, connManager, history, out);
  context.subscriptions.push(out);

  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(SidebarProvider.viewId, provider)
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('draco.addConnection', () => {
      provider.postMessage('addConnection', {});
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('draco.refreshConnections', () => {
      provider.postMessage('refreshConnections', {});
    })
  );

  // ── Prisma schema.prisma watcher (#24) ─────────────────────────────
  const scanPrismaSchema = async () => {
    const files = await vscode.workspace.findFiles('**/schema.prisma', '**/node_modules/**', 1);
    if (files.length === 0) { provider.setPrismaSchema(null); return; }
    try {
      const bytes = await vscode.workspace.fs.readFile(files[0]);
      const parsed = parseDracoSchema(files[0].fsPath, Buffer.from(bytes).toString('utf-8'));
      provider.setPrismaSchema(parsed);
    } catch {
      provider.setPrismaSchema(null);
    }
  };

  scanPrismaSchema();

  const watcher = vscode.workspace.createFileSystemWatcher('**/schema.prisma');
  watcher.onDidCreate(scanPrismaSchema);
  watcher.onDidChange(scanPrismaSchema);
  watcher.onDidDelete(() => provider.setPrismaSchema(null));
  context.subscriptions.push(watcher);
}

export function deactivate() {}

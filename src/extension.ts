import * as vscode from 'vscode';
import { SidebarProvider } from './providers/SidebarProvider';
import { ConnectionStorage } from './storage/ConnectionStorage';
import { ConnectionManager } from './db/ConnectionManager';
import { HistoryStorage } from './storage/HistoryStorage';

export function activate(context: vscode.ExtensionContext) {
  const storage     = new ConnectionStorage(context.globalState, context.secrets);
  const connManager = new ConnectionManager();
  const history     = new HistoryStorage(context.globalState);
  const provider    = new SidebarProvider(context, storage, connManager, history);

  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(SidebarProvider.viewId, provider)
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('prisma4postgres.addConnection', () => {
      provider.postMessage('addConnection', {});
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('prisma4postgres.refreshConnections', () => {
      provider.postMessage('refreshConnections', {});
    })
  );
}

export function deactivate() {}

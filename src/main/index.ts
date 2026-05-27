import { app, BrowserWindow } from 'electron';
import { createMainWindow } from './window';
import { registerIpc } from './ipc';
import { createMenu } from './menu';

const gotLock = app.requestSingleInstanceLock();

if (!gotLock) {
  app.quit();
} else {
  app.on('second-instance', () => {
    const wins = BrowserWindow.getAllWindows();
    if (wins.length > 0) {
      const win = wins[0];
      if (win.isMinimized()) win.restore();
      win.focus();
    }
  });

  app.whenReady().then(() => {
    const win = createMainWindow();
    registerIpc(win);
    createMenu(win);

    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) {
        const w = createMainWindow();
        registerIpc(w);
      }
    });
  });

  app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') app.quit();
  });
}

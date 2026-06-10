import { app, BrowserWindow } from 'electron';
import { createMainWindow, createSplashWindow } from './window';
import { registerIpc } from './ipc';

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
    const splash = createSplashWindow();
    const win = createMainWindow();
    registerIpc(win);

    const splashStart = Date.now();
    win.once('ready-to-show', () => {
      const elapsed = Date.now() - splashStart;
      const remaining = Math.max(0, 1500 - elapsed);
      setTimeout(() => {
        splash.destroy();
        win.show();
      }, remaining);
    });

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

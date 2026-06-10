import { app, BrowserWindow, Menu, MenuItem, shell } from 'electron';
import { createAboutWindow } from './window';

function send(win: BrowserWindow, command: string, data?: unknown): void {
  if (!win.isDestroyed()) win.webContents.send('to-renderer', { command, data });
}

export function createMenu(win: BrowserWindow): void {
  const isMac = process.platform === 'darwin';
  const isDev = !app.isPackaged;

  const template: (Electron.MenuItemConstructorOptions | MenuItem)[] = [
    // ── File ──────────────────────────────────────────────────────────────────
    {
      label: 'File',
      submenu: [
        {
          label: 'New Query Tab',
          accelerator: 'CmdOrCtrl+T',
          click: () => send(win, 'newQueryTab'),
        },
        {
          label: 'Save as Snippet',
          accelerator: 'CmdOrCtrl+Shift+S',
          click: () => send(win, 'saveAsSnippet'),
        },
        {
          label: 'Export Results…',
          accelerator: 'CmdOrCtrl+Shift+E',
          click: () => send(win, 'exportResultsFromMenu'),
        },
        { type: 'separator' },
        {
          label: 'New Connection…',
          click: () => send(win, 'newConnection'),
        },
        { type: 'separator' },
        isMac
          ? { role: 'close' }
          : {
              label: 'Quit',
              accelerator: 'CmdOrCtrl+Q',
              click: () => app.quit(),
            },
      ],
    },

    // ── Edit ─────────────────────────────────────────────────────────────────
    {
      label: 'Edit',
      submenu: [
        { role: 'undo' },
        { role: 'redo' },
        { type: 'separator' },
        { role: 'cut' },
        { role: 'copy' },
        { role: 'paste' },
        { role: 'selectAll' },
      ],
    },

    // ── View ──────────────────────────────────────────────────────────────────
    {
      label: 'View',
      submenu: [
        {
          label: 'Global Search',
          accelerator: 'CmdOrCtrl+P',
          click: () => send(win, 'openGlobalSearch'),
        },
        { type: 'separator' },
        {
          label: 'Query',
          accelerator: 'CmdOrCtrl+1',
          click: () => send(win, 'switchTab', 'query'),
        },
        {
          label: 'History',
          accelerator: 'CmdOrCtrl+2',
          click: () => send(win, 'switchTab', 'history'),
        },
        {
          label: 'Snippets',
          accelerator: 'CmdOrCtrl+3',
          click: () => send(win, 'switchTab', 'snippets'),
        },
        {
          label: 'Activity',
          accelerator: 'CmdOrCtrl+4',
          click: () => send(win, 'switchTab', 'activity'),
        },
        {
          label: 'ERD',
          accelerator: 'CmdOrCtrl+5',
          click: () => send(win, 'switchTab', 'erd'),
        },
        { type: 'separator' },
        {
          label: 'Toggle Sidebar',
          accelerator: 'CmdOrCtrl+B',
          click: () => send(win, 'toggleSidebar'),
        },
        ...(isDev
          ? [
              { type: 'separator' as const },
              { role: 'reload' as const },
              { role: 'toggleDevTools' as const },
            ]
          : []),
      ],
    },

    // ── Connection ────────────────────────────────────────────────────────────
    {
      label: 'Connection',
      submenu: [
        {
          label: 'New Connection…',
          click: () => send(win, 'newConnection'),
        },
        { type: 'separator' },
        {
          label: 'Dashboard',
          accelerator: 'CmdOrCtrl+Shift+D',
          click: () => send(win, 'openDashboardFromMenu'),
        },
        { type: 'separator' },
        {
          label: 'Create Table…',
          click: () => send(win, 'createTableFromMenu'),
        },
      ],
    },

    // ── Help ──────────────────────────────────────────────────────────────────
    {
      label: 'Help',
      submenu: [
        {
          label: 'GitHub Repository',
          click: () => shell.openExternal('https://github.com/britors/Draco'),
        },
        {
          label: 'Report Issue',
          click: () => shell.openExternal('https://github.com/britors/Draco/issues/new'),
        },
        { type: 'separator' },
        {
          label: 'About',
          click: () => createAboutWindow(),
        },
      ],
    },
  ];

  // On macOS prepend the app menu
  if (isMac) {
    template.unshift({
      label: app.name,
      submenu: [
        { role: 'about' },
        { type: 'separator' },
        { role: 'services' },
        { type: 'separator' },
        { role: 'hide' },
        { role: 'hideOthers' },
        { role: 'unhide' },
        { type: 'separator' },
        { role: 'quit' },
      ],
    });
  }

  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

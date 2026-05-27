import { BrowserWindow, app } from 'electron';
import * as path from 'path';
import * as fs from 'fs';

interface WindowBounds {
  x?: number;
  y?: number;
  width: number;
  height: number;
}

function loadBounds(): WindowBounds {
  const file = path.join(app.getPath('userData'), 'window-bounds.json');
  try {
    return JSON.parse(fs.readFileSync(file, 'utf-8'));
  } catch {
    return { width: 1200, height: 800 };
  }
}

function saveBounds(win: BrowserWindow): void {
  const file = path.join(app.getPath('userData'), 'window-bounds.json');
  try {
    fs.writeFileSync(file, JSON.stringify(win.getBounds()));
  } catch { /* best-effort */ }
}

export function createMainWindow(): BrowserWindow {
  const bounds = loadBounds();

  const win = new BrowserWindow({
    ...bounds,
    minWidth: 900,
    minHeight: 600,
    title: 'Prisma4Postgres',
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  win.loadFile(path.join(__dirname, '../renderer/index.html'));
  win.on('close', () => saveBounds(win));

  if (process.env.DEV === '1') win.webContents.openDevTools();

  return win;
}

export function createAboutWindow(): void {
  const win = new BrowserWindow({
    width: 400,
    height: 320,
    resizable: false,
    minimizable: false,
    maximizable: false,
    title: 'About Prisma4Postgres',
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  win.setMenu(null);
  win.loadFile(path.join(__dirname, '../renderer/about.html'));
}

export function createPanelWindow(
  htmlFile: string,
  title: string,
  width = 900,
  height = 700
): BrowserWindow {
  const win = new BrowserWindow({
    width,
    height,
    minWidth: 600,
    minHeight: 400,
    title,
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  win.loadFile(path.join(__dirname, '../renderer', htmlFile));
  return win;
}

import { app, safeStorage } from 'electron';
import * as fs from 'fs';
import * as path from 'path';
import { randomUUID } from 'crypto';
import { PgConnection } from '../types/PgConnection';

// ── Types ────────────────────────────────────────────────────────────────────

export interface HistoryEntry {
  id: string;
  sql: string;
  connId: string;
  connLabel: string;
  timestamp: number;
  durationMs: number;
  rowCount: number;
}

export interface AppSettings {
  queryTimeout: number;
  previewRowLimit: number;
  defaultSsl: boolean;
  defaultPort: number;
  showRowCount: boolean;
}

const DEFAULT_SETTINGS: AppSettings = {
  queryTimeout: 30_000,
  previewRowLimit: 100,
  defaultSsl: false,
  defaultPort: 5432,
  showRowCount: false,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

function dataFile(name: string): string {
  return path.join(app.getPath('userData'), name);
}

function readJson<T>(name: string, fallback: T): T {
  try {
    return JSON.parse(fs.readFileSync(dataFile(name), 'utf-8')) as T;
  } catch {
    return fallback;
  }
}

function writeJson(name: string, data: unknown): void {
  fs.writeFileSync(dataFile(name), JSON.stringify(data, null, 2), 'utf-8');
}

// ── Connections ──────────────────────────────────────────────────────────────

export function listConnections(): PgConnection[] {
  return readJson<PgConnection[]>('connections.json', []);
}

export function getConnection(id: string): PgConnection | undefined {
  return listConnections().find(c => c.id === id);
}

export function saveConnection(
  conn: Omit<PgConnection, 'id'> & { id?: string },
  password: string
): PgConnection {
  const connections = listConnections();
  const id = conn.id ?? randomUUID();
  const full: PgConnection = { ...conn, id };

  const idx = connections.findIndex(c => c.id === id);
  if (idx >= 0) connections[idx] = full;
  else connections.push(full);

  writeJson('connections.json', connections);
  storePassword(id, password);
  return full;
}

export function deleteConnection(id: string): void {
  writeJson('connections.json', listConnections().filter(c => c.id !== id));
  removePassword(id);
}

// ── Passwords ────────────────────────────────────────────────────────────────

function getPasswordMap(): Record<string, string> {
  return readJson<Record<string, string>>('passwords.json', {});
}

function storePassword(id: string, password: string): void {
  const map = getPasswordMap();
  if (safeStorage.isEncryptionAvailable()) {
    map[id] = safeStorage.encryptString(password).toString('base64');
  } else {
    map[id] = Buffer.from(password, 'utf-8').toString('base64');
  }
  writeJson('passwords.json', map);
}

function removePassword(id: string): void {
  const map = getPasswordMap();
  delete map[id];
  writeJson('passwords.json', map);
}

export function getSshPassword(id: string): string { return getPassword('ssh:' + id); }
export function storeSshPassword(id: string, password: string): void { storePassword('ssh:' + id, password); }

export function getPassword(id: string): string {
  const map = getPasswordMap();
  const encoded = map[id];
  if (!encoded) return '';
  const buf = Buffer.from(encoded, 'base64');
  if (safeStorage.isEncryptionAvailable()) {
    try { return safeStorage.decryptString(buf); } catch { return ''; }
  }
  return buf.toString('utf-8');
}

// ── Query history ────────────────────────────────────────────────────────────

const MAX_HISTORY = 50;

export function listHistory(): HistoryEntry[] {
  return readJson<HistoryEntry[]>('history.json', []);
}

export function addHistory(entry: Omit<HistoryEntry, 'id'>): void {
  const entries = listHistory();
  entries.unshift({ ...entry, id: randomUUID() });
  if (entries.length > MAX_HISTORY) entries.length = MAX_HISTORY;
  writeJson('history.json', entries);
}

export function clearHistory(): void {
  writeJson('history.json', []);
}

// ── Settings ─────────────────────────────────────────────────────────────────

export function getSettings(): AppSettings {
  return { ...DEFAULT_SETTINGS, ...readJson<Partial<AppSettings>>('settings.json', {}) };
}

export function patchSettings(partial: Partial<AppSettings>): void {
  writeJson('settings.json', { ...getSettings(), ...partial });
}

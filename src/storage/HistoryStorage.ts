import * as vscode from 'vscode';
import { randomUUID } from 'crypto';

const HISTORY_KEY = 'draco.queryHistory';
const MAX_ENTRIES = 50;

export interface HistoryEntry {
  id: string;
  sql: string;
  connId: string;
  connLabel: string;
  timestamp: number;
  durationMs: number;
  rowCount: number;
}

export class HistoryStorage {
  constructor(private readonly _globalState: vscode.Memento) {}

  list(): HistoryEntry[] {
    return this._globalState.get<HistoryEntry[]>(HISTORY_KEY, []);
  }

  async add(entry: Omit<HistoryEntry, 'id'>): Promise<void> {
    const entries = this.list();
    entries.unshift({ ...entry, id: randomUUID() });
    if (entries.length > MAX_ENTRIES) entries.length = MAX_ENTRIES;
    await this._globalState.update(HISTORY_KEY, entries);
  }
}

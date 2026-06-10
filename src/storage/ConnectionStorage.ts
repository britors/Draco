import * as vscode from 'vscode';
import { randomUUID } from 'crypto';
import { DbConnection } from '../types/DbConnection';

const CONNECTIONS_KEY = 'draco.connections';
const SECRET_PREFIX = 'draco.password.';

export class ConnectionStorage {
  constructor(
    private readonly _globalState: vscode.Memento,
    private readonly _secrets: vscode.SecretStorage
  ) {}

  listConnections(): DbConnection[] {
    return this._globalState.get<DbConnection[]>(CONNECTIONS_KEY, []);
  }

  getConnection(id: string): DbConnection | undefined {
    return this.listConnections().find(c => c.id === id);
  }

  async getPassword(id: string): Promise<string> {
    return (await this._secrets.get(`${SECRET_PREFIX}${id}`)) ?? '';
  }

  async saveConnection(
    conn: Omit<DbConnection, 'id'> & { id?: string },
    password: string
  ): Promise<DbConnection> {
    const connections = this.listConnections();
    const id = conn.id ?? randomUUID();
    const full: DbConnection = { ...conn, id };

    const idx = connections.findIndex(c => c.id === id);
    if (idx >= 0) {
      connections[idx] = full;
    } else {
      connections.push(full);
    }

    await this._globalState.update(CONNECTIONS_KEY, connections);
    await this._secrets.store(`${SECRET_PREFIX}${id}`, password);
    return full;
  }

  async deleteConnection(id: string): Promise<void> {
    const connections = this.listConnections().filter(c => c.id !== id);
    await this._globalState.update(CONNECTIONS_KEY, connections);
    await this._secrets.delete(`${SECRET_PREFIX}${id}`);
  }
}

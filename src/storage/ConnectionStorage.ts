import * as vscode from 'vscode';
import { randomUUID } from 'crypto';
import { PgConnection } from '../types/PgConnection';

const CONNECTIONS_KEY = 'prisma4postgres.connections';
const SECRET_PREFIX = 'prisma4postgres.password.';

export class ConnectionStorage {
  constructor(
    private readonly _globalState: vscode.Memento,
    private readonly _secrets: vscode.SecretStorage
  ) {}

  listConnections(): PgConnection[] {
    return this._globalState.get<PgConnection[]>(CONNECTIONS_KEY, []);
  }

  getConnection(id: string): PgConnection | undefined {
    return this.listConnections().find(c => c.id === id);
  }

  async getPassword(id: string): Promise<string> {
    return (await this._secrets.get(`${SECRET_PREFIX}${id}`)) ?? '';
  }

  async saveConnection(
    conn: Omit<PgConnection, 'id'> & { id?: string },
    password: string
  ): Promise<PgConnection> {
    const connections = this.listConnections();
    const id = conn.id ?? randomUUID();
    const full: PgConnection = { ...conn, id };

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

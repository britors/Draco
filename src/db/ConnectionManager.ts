import { DbConnection } from '../types/DbConnection';
import { DbDriver } from './DbDriver';
import { PostgresDriver } from './PostgresDriver';

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface ManagedConnection {
  conn: DbConnection;
  status: ConnectionStatus;
  driver?: DbDriver;
  error?: string;
}

export class ConnectionManager {
  private readonly _map = new Map<string, ManagedConnection>();

  syncConnections(connections: DbConnection[]): void {
    const ids = new Set(connections.map(c => c.id));
    for (const id of this._map.keys()) {
      if (!ids.has(id)) this._remove(id);
    }
    for (const conn of connections) {
      if (this._map.has(conn.id)) {
        this._map.get(conn.id)!.conn = conn;
      } else {
        this._map.set(conn.id, { conn, status: 'disconnected' });
      }
    }
  }

  get(id: string): ManagedConnection | undefined {
    return this._map.get(id);
  }

  getDriver(id: string): DbDriver | undefined {
    return this._map.get(id)?.driver;
  }

  async connect(id: string, password: string, statementTimeout = 30_000, sshPassword?: string, jumpPassword?: string): Promise<void> {
    const managed = this._map.get(id);
    if (!managed) throw new Error('Connection not registered');

    managed.status = 'connecting';
    // TODO: Factory for different engines
    const driver = new PostgresDriver(managed.conn, password, statementTimeout, `draco-${id}`, sshPassword, jumpPassword);
    try {
      await driver.connect();
      managed.driver = driver;
      managed.status = 'connected';
      managed.error = undefined;
    } catch (err) {
      managed.status = 'error';
      managed.error = String(err);
      throw err;
    }
  }

  async disconnect(id: string): Promise<void> {
    const managed = this._map.get(id);
    if (!managed) return;
    await managed.driver?.disconnect();
    managed.driver = undefined;
    managed.status = 'disconnected';
    managed.error = undefined;
  }

  getStatuses(): Record<string, ConnectionStatus> {
    const out: Record<string, ConnectionStatus> = {};
    for (const [id, m] of this._map) out[id] = m.status;
    return out;
  }

  private _remove(id: string): void {
    const m = this._map.get(id);
    if (m?.driver) m.driver.disconnect().catch(() => {});
    this._map.delete(id);
  }
}

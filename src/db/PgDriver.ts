import { Pool, QueryResult } from 'pg';
import * as net from 'net';
import { Client as SshClient } from 'ssh2';
import * as fs from 'fs';
import { PgConnection } from '../types/PgConnection';

function buildSshCfg(
  host: string, port: number, username: string,
  keyPath: string | undefined, password: string | undefined
): Record<string, unknown> {
  const cfg: Record<string, unknown> = { host, port, username };
  if (keyPath) {
    cfg.privateKey = fs.readFileSync(keyPath);
    if (password) cfg.passphrase = password;
  } else {
    cfg.password = password ?? '';
  }
  return cfg;
}

function createSshTunnel(
  conn: PgConnection,
  sshPassword?: string,
  jumpPassword?: string,
): Promise<{ localPort: number; close: () => void }> {
  return new Promise((resolve, reject) => {
    const sshHost = conn.sshHost ?? conn.host;
    const sshPort = conn.sshPort ?? 22;
    const sshUser = conn.sshUser ?? process.env.USER ?? 'root';

    function connectMain(sock?: import('stream').Duplex) {
      const ssh = new SshClient();
      const server = net.createServer();
      server.listen(0, '127.0.0.1', () => {
        const localPort = (server.address() as net.AddressInfo).port;
        ssh.on('ready', () => {
          server.on('connection', socket => {
            ssh.forwardOut('127.0.0.1', localPort, conn.host, conn.port, (err, stream) => {
              if (err) { socket.destroy(); return; }
              socket.pipe(stream).pipe(socket);
              stream.on('close', () => socket.destroy());
              socket.on('close', () => stream.destroy());
            });
          });
          resolve({
            localPort,
            close: () => {
              try { ssh.end(); } catch { /* */ }
              server.close();
              if (sock) try { sock.destroy(); } catch { /* */ }
            }
          });
        });
        ssh.on('error', err => { server.close(); reject(err); });
        const cfg = buildSshCfg(sshHost, sshPort, sshUser, conn.sshKeyPath, sshPassword);
        if (sock) cfg.sock = sock;
        ssh.connect(cfg as Parameters<SshClient['connect']>[0]);
      });
      server.on('error', reject);
    }

    if (conn.sshJumpHost) {
      const jumpHost = conn.sshJumpHost;
      const jumpPort = conn.sshJumpPort ?? 22;
      const jumpUser = conn.sshJumpUser ?? process.env.USER ?? 'root';
      const jump = new SshClient();
      jump.on('ready', () => {
        jump.forwardOut('127.0.0.1', 0, sshHost, sshPort, (err, stream) => {
          if (err) { jump.end(); reject(err); return; }
          connectMain(stream as unknown as import('stream').Duplex);
          stream.on('error', (e: Error) => reject(e));
        });
      });
      jump.on('error', err => reject(err));
      const jumpCfg = buildSshCfg(jumpHost, jumpPort, jumpUser, conn.sshJumpKeyPath, jumpPassword);
      jump.connect(jumpCfg as Parameters<SshClient['connect']>[0]);
    } else {
      connectMain();
    }
  });
}

export class PgDriver {
  private _pool: Pool | null = null;
  private _appName: string = 'prisma4postgres';
  private _tunnel: { localPort: number; close: () => void } | null = null;

  constructor(
    private readonly _conn: PgConnection,
    private readonly _password: string,
    private readonly _statementTimeout: number = 30_000,
    appName?: string,
    private readonly _sshPassword?: string,
    private readonly _jumpPassword?: string,
  ) {
    if (appName) this._appName = appName;
  }

  async connect(): Promise<void> {
    let host = this._conn.host;
    let port = this._conn.port;
    if (this._conn.sshEnabled) {
      this._tunnel = await createSshTunnel(this._conn, this._sshPassword, this._jumpPassword);
      host = '127.0.0.1';
      port = this._tunnel.localPort;
    }
    this._pool = new Pool({
      host,
      port,
      database: this._conn.database,
      user: this._conn.user,
      password: this._password,
      ssl: this._conn.ssl ? { rejectUnauthorized: false } : false,
      application_name: this._appName,
      max: 5,
      idleTimeoutMillis: 30_000,
      connectionTimeoutMillis: 10_000,
      statement_timeout: this._statementTimeout,
    });
    const client = await this._pool.connect();
    client.release();
  }

  async disconnect(): Promise<void> {
    await this._pool?.end();
    this._pool = null;
    this._tunnel?.close();
    this._tunnel = null;
  }

  async query<T extends Record<string, unknown> = Record<string, unknown>>(
    sql: string,
    params: unknown[] = []
  ): Promise<T[]> {
    if (!this._pool) throw new Error('Not connected');
    const result: QueryResult<T> = await this._pool.query<T>(sql, params as never[]);
    return result.rows;
  }

  async cancelActive(): Promise<void> {
    if (!this._pool) return;
    await this._pool.query(
      `SELECT pg_cancel_backend(pid) FROM pg_stat_activity WHERE application_name = $1 AND state = 'active' AND pid <> pg_backend_pid()`,
      [this._appName]
    );
  }

  get isConnected(): boolean {
    return this._pool !== null;
  }
}

export async function testConnection(conn: PgConnection, password: string): Promise<void> {
  const driver = new PgDriver(conn, password);
  try {
    await driver.connect();
  } finally {
    await driver.disconnect();
  }
}

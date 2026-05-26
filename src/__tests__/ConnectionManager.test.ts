import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { ConnectionManager } from '../db/ConnectionManager';
import type { PgConnection } from '../types/PgConnection';

function makeConn(id: string, label = id): PgConnection {
  return { id, label, host: 'localhost', port: 5432, database: 'db', user: 'u', ssl: false };
}

// ── initial state ─────────────────────────────────────────────────────────────
describe('ConnectionManager — initial state', () => {
  it('getStatuses returns empty object when no connections registered', () => {
    const mgr = new ConnectionManager();
    assert.deepEqual(mgr.getStatuses(), {});
  });

  it('get returns undefined for unknown id', () => {
    const mgr = new ConnectionManager();
    assert.equal(mgr.get('unknown'), undefined);
  });

  it('getDriver returns undefined for unknown id', () => {
    const mgr = new ConnectionManager();
    assert.equal(mgr.getDriver('unknown'), undefined);
  });
});

// ── syncConnections ────────────────────────────────────────────────────────────
describe('ConnectionManager — syncConnections', () => {
  let mgr: ConnectionManager;
  beforeEach(() => { mgr = new ConnectionManager(); });

  it('registers new connections with disconnected status', () => {
    mgr.syncConnections([makeConn('a'), makeConn('b')]);
    const s = mgr.getStatuses();
    assert.equal(s['a'], 'disconnected');
    assert.equal(s['b'], 'disconnected');
  });

  it('removes connections no longer in the list', () => {
    mgr.syncConnections([makeConn('a'), makeConn('b')]);
    mgr.syncConnections([makeConn('a')]);
    const s = mgr.getStatuses();
    assert.equal(s['a'], 'disconnected');
    assert.equal(s['b'], undefined);
  });

  it('updates connection metadata for an existing id', () => {
    mgr.syncConnections([makeConn('a', 'Old Label')]);
    mgr.syncConnections([makeConn('a', 'New Label')]);
    assert.equal(mgr.get('a')?.conn.label, 'New Label');
  });

  it('preserves status of existing connections across sync', () => {
    mgr.syncConnections([makeConn('a')]);
    // Simulate status change (internal state hack via get)
    const mc = mgr.get('a')!;
    mc.status = 'error';
    mgr.syncConnections([makeConn('a')]);
    assert.equal(mgr.get('a')?.status, 'error');
  });

  it('handles empty list — removes all connections', () => {
    mgr.syncConnections([makeConn('a'), makeConn('b')]);
    mgr.syncConnections([]);
    assert.deepEqual(mgr.getStatuses(), {});
  });

  it('get returns the managed connection object after sync', () => {
    const conn = makeConn('x');
    mgr.syncConnections([conn]);
    const mc = mgr.get('x');
    assert.ok(mc);
    assert.equal(mc.conn.id, 'x');
    assert.equal(mc.status, 'disconnected');
  });

  it('getDriver returns undefined for disconnected connections', () => {
    mgr.syncConnections([makeConn('a')]);
    assert.equal(mgr.getDriver('a'), undefined);
  });
});

// ── getStatuses ───────────────────────────────────────────────────────────────
describe('ConnectionManager — getStatuses', () => {
  it('returns correct statuses for multiple connections', () => {
    const mgr = new ConnectionManager();
    mgr.syncConnections([makeConn('a'), makeConn('b'), makeConn('c')]);
    const s = mgr.getStatuses();
    assert.equal(Object.keys(s).length, 3);
    for (const v of Object.values(s)) {
      assert.equal(v, 'disconnected');
    }
  });
});

// ── disconnect on unregistered ─────────────────────────────────────────────────
describe('ConnectionManager — disconnect', () => {
  it('disconnect on unknown id resolves without error', async () => {
    const mgr = new ConnectionManager();
    await assert.doesNotReject(() => mgr.disconnect('never-registered'));
  });
});

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { formatLagSeconds, formatWalBytes, replicationMode, slotNeedsAttention } from '../dist/replication-view.js';

test('WAL distances use binary units and hide missing values', () => {
  assert.equal(formatWalBytes(null), '—');
  assert.equal(formatWalBytes(undefined), '—');
  assert.equal(formatWalBytes(0), '0 B');
  assert.equal(formatWalBytes(1023), '1,023 B');
  assert.equal(formatWalBytes(1536), '1.5 KiB');
  assert.equal(formatWalBytes(16 * 1024 * 1024), '16.0 MiB');
  assert.equal(formatWalBytes(300 * 1024 ** 3), '300 GiB');
  assert.equal(formatWalBytes(-5), '0 B', 'a standby slightly ahead never shows negative lag');
  assert.equal(formatWalBytes(1536, 'pt-BR'), '1,5 KiB');
});

test('lag seconds pick a readable unit', () => {
  assert.equal(formatLagSeconds(null), '—');
  assert.equal(formatLagSeconds(''), '—');
  assert.equal(formatLagSeconds('0.002'), '2 ms');
  assert.equal(formatLagSeconds('12.34'), '12.3 s');
  assert.equal(formatLagSeconds('600'), '10 min');
  assert.equal(formatLagSeconds('36000.0'), '10 h');
  assert.equal(formatLagSeconds('nope'), '—');
});

test('slots holding WAL while inactive or lost need attention', () => {
  assert.equal(slotNeedsAttention({ active: true, retained_wal_bytes: 10 ** 9, wal_status: 'reserved' }), false);
  assert.equal(slotNeedsAttention({ active: false, retained_wal_bytes: 4096, wal_status: 'reserved' }), true);
  assert.equal(slotNeedsAttention({ active: false, retained_wal_bytes: null, wal_status: null }), false);
  assert.equal(slotNeedsAttention({ active: true, retained_wal_bytes: 0, wal_status: 'lost' }), true);
});

test('the panel mode follows recovery, senders and slots', () => {
  assert.equal(replicationMode({ in_recovery: true, replicas: [], slots: [] }), 'standby');
  assert.equal(replicationMode({ in_recovery: false, replicas: [], slots: [] }), 'none');
  assert.equal(replicationMode({ in_recovery: false, replicas: [{}], slots: [] }), 'primary');
  assert.equal(replicationMode({ in_recovery: false, replicas: [], slots: [{}] }), 'primary');
});

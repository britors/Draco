import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { validateConnection, defaultConnection } from '../types/DbConnection';

describe('validateConnection', () => {
  it('returns no errors for a fully valid connection', () => {
    const errors = validateConnection({
      label: 'Local', host: 'localhost', port: 5432,
      database: 'mydb', user: 'postgres', ssl: false,
    });
    assert.deepEqual(errors, []);
  });

  it('requires label', () => {
    const errors = validateConnection({ host: 'localhost', port: 5432, database: 'db', user: 'u' });
    assert.ok(errors.some(e => e.includes('Label')));
  });

  it('treats whitespace-only label as missing', () => {
    const errors = validateConnection({ label: '   ', host: 'h', port: 5432, database: 'db', user: 'u' });
    assert.ok(errors.some(e => e.includes('Label')));
  });

  it('requires host', () => {
    const errors = validateConnection({ label: 'L', port: 5432, database: 'db', user: 'u' });
    assert.ok(errors.some(e => e.includes('Host')));
  });

  it('requires database', () => {
    const errors = validateConnection({ label: 'L', host: 'h', port: 5432, user: 'u' });
    assert.ok(errors.some(e => e.includes('Database')));
  });

  it('requires user', () => {
    const errors = validateConnection({ label: 'L', host: 'h', port: 5432, database: 'db' });
    assert.ok(errors.some(e => e.includes('User')));
  });

  it('rejects port 0', () => {
    const errors = validateConnection({ label: 'L', host: 'h', port: 0, database: 'db', user: 'u' });
    assert.ok(errors.some(e => e.includes('Port')));
  });

  it('rejects port > 65535', () => {
    const errors = validateConnection({ label: 'L', host: 'h', port: 65536, database: 'db', user: 'u' });
    assert.ok(errors.some(e => e.includes('Port')));
  });

  it('accepts port boundary 1', () => {
    const errors = validateConnection({ label: 'L', host: 'h', port: 1, database: 'db', user: 'u' });
    assert.ok(!errors.some(e => e.includes('Port')));
  });

  it('accepts port boundary 65535', () => {
    const errors = validateConnection({ label: 'L', host: 'h', port: 65535, database: 'db', user: 'u' });
    assert.ok(!errors.some(e => e.includes('Port')));
  });

  it('accumulates multiple errors', () => {
    const errors = validateConnection({});
    assert.ok(errors.length >= 4, `expected ≥4 errors, got ${errors.length}`);
  });
});

describe('defaultConnection', () => {
  it('returns default port 5432', () => {
    assert.equal(defaultConnection().port, 5432);
  });

  it('returns localhost as default host', () => {
    assert.equal(defaultConnection().host, 'localhost');
  });

  it('returns ssl false by default', () => {
    assert.equal(defaultConnection().ssl, false);
  });

  it('returns empty string for label, database, user', () => {
    const d = defaultConnection();
    assert.equal(d.label, '');
    assert.equal(d.database, '');
    assert.equal(d.user, '');
  });
});

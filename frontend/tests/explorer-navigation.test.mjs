import assert from 'node:assert/strict';
import { test } from 'node:test';

import { groupSchemaObjects, quoteSqlIdentifier, schemaObjectSql } from '../dist/explorer-navigation.js';

test('PostgreSQL identifiers are quoted before being placed in navigation SQL', () => {
  assert.equal(quoteSqlIdentifier('Mixed Case'), '"Mixed Case"');
  assert.equal(quoteSqlIdentifier('a"b'), '"a""b"');
});

test('routines and sequences open safe SQL templates', () => {
  assert.equal(
    schemaObjectSql('app', { kind: 'function', name: 'calculate' }),
    '-- Add any required arguments before running\nSELECT "app"."calculate"();',
  );
  assert.equal(
    schemaObjectSql('app', { kind: 'procedure', name: 'refresh' }),
    '-- Add any required arguments before running\nCALL "app"."refresh"();',
  );
  assert.equal(
    schemaObjectSql('app', { kind: 'sequence', name: 'item_id_seq' }),
    'SELECT * FROM "app"."item_id_seq";',
  );
  assert.equal(schemaObjectSql('app', { kind: 'trigger', name: 'audit' }), null);
});

test('programming objects are grouped separately from sequences', () => {
  const objects = [
    { kind: 'function', name: 'calculate' },
    { kind: 'sequence', name: 'items_id_seq' },
    { kind: 'procedure', name: 'refresh' },
    { kind: 'trigger', name: 'audit' },
  ];
  const grouped = groupSchemaObjects(objects);
  assert.deepEqual(grouped.programming.map((object) => object.kind), ['function', 'procedure', 'trigger']);
  assert.deepEqual(grouped.sequences.map((object) => object.kind), ['sequence']);
});

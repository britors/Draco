import assert from 'node:assert/strict';
import { test } from 'node:test';
import { alignLines, diffSummary } from '../dist/schema-diff-view.js';

test('identical definitions align as same rows', () => {
  assert.deepEqual(alignLines('a\nb', 'a\nb'), [
    { left: 'a', right: 'a', type: 'same' },
    { left: 'b', right: 'b', type: 'same' },
  ]);
});

test('changed lines pair up and extra lines stay on their side', () => {
  const rows = alignLines('CREATE TABLE t (\n    id integer,\n    name text\n);', 'CREATE TABLE t (\n    id bigint,\n    legacy text,\n    name text\n);');
  assert.deepEqual(rows.map((row) => row.type), ['same', 'changed', 'removed', 'same', 'same']);
  assert.equal(rows[1].left, '    id integer,');
  assert.equal(rows[1].right, '    id bigint,');
  assert.equal(rows[2].left, null);
});

test('a missing side shows only the other one', () => {
  assert.deepEqual(alignLines('x', null), [{ left: 'x', right: null, type: 'added' }]);
  assert.deepEqual(alignLines(null, 'y'), [{ left: null, right: 'y', type: 'removed' }]);
  assert.deepEqual(alignLines(null, null), []);
});

test('very large inputs fall back to positional pairing', () => {
  const left = Array.from({ length: 1000 }, (_, index) => `l${index}`).join('\n');
  const right = Array.from({ length: 1000 }, (_, index) => (index === 3 ? 'x' : `l${index}`)).join('\n');
  const rows = alignLines(left, right);
  assert.equal(rows.length, 1000);
  assert.equal(rows[3].type, 'changed');
});

test('summary counts objects by status', () => {
  assert.deepEqual(diffSummary([{ status: 'added' }, { status: 'changed' }, { status: 'added' }]), { added: 2, removed: 0, changed: 1 });
});

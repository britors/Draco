import assert from 'node:assert/strict';
import { test } from 'node:test';
import { applySuggestion, buildCompletionIndex, currentWordRange, suggest } from '../dist/sql-autocomplete.js';

const data = {
  schemas: ['public', 'billing'],
  tables: [
    { schema: 'public', name: 'users', kind: 'table' },
    { schema: 'public', name: 'user_roles', kind: 'view' },
  ],
  columns: [
    { schema: 'public', table: 'users', name: 'id' },
    { schema: 'public', table: 'users', name: 'email' },
    { schema: 'public', table: 'orders', name: 'user_id' },
  ],
  functions: [{ schema: 'public', name: 'now' }],
};

test('currentWordRange finds the identifier and an optional dotted qualifier', () => {
  assert.deepEqual(currentWordRange('SELECT u.na FROM users u', 10), { start: 9, end: 11, word: 'na', qualifier: 'u' });
  assert.deepEqual(currentWordRange('SELECT ', 7), { start: 7, end: 7, word: '', qualifier: null });
  assert.equal(currentWordRange('SELECT users.email', 15).qualifier, 'users');
});

test('suggest ranks prefix matches before substring matches and includes keywords, tables, columns and functions', () => {
  const index = buildCompletionIndex(data);
  const labels = suggest(index, 'SELECT use', 10).map((item) => item.label);
  assert.ok(labels.includes('users'));
  assert.ok(labels.includes('user_roles'));
  assert.ok(labels.includes('user_id'));
  assert.ok(labels.includes('CURRENT_USER'));
  assert.equal(labels[0], 'user_id', 'prefix matches for "use" sort before substring-only matches');
});

test('a dotted qualifier that matches a known table scopes suggestions to that table\'s columns', () => {
  const index = buildCompletionIndex(data);
  const text = 'SELECT users.e FROM users';
  const results = suggest(index, text, 14);
  assert.deepEqual(results.map((item) => item.label), ['email']);
  assert.equal(results[0].kind, 'column');
});

test('an unknown qualifier falls back to the whole index instead of returning nothing', () => {
  const index = buildCompletionIndex(data);
  const results = suggest(index, 'SELECT alias.use', 16);
  assert.ok(results.some((item) => item.label === 'users'));
});

test('below the minimum length no suggestions are offered', () => {
  const index = buildCompletionIndex(data);
  assert.deepEqual(suggest(index, 'SELECT ', 7), []);
});

test('applySuggestion replaces only the identifier under the caret and moves the caret past it', () => {
  const result = applySuggestion('SELECT use FROM t', 10, { label: 'users', insertText: 'users' });
  assert.equal(result.text, 'SELECT users FROM t');
  assert.equal(result.caret, 12);
});

test('function suggestions insert an opening parenthesis', () => {
  const index = buildCompletionIndex(data);
  const now = suggest(index, 'SELECT no', 9).find((item) => item.label === 'now');
  assert.equal(now.insertText, 'now(');
});

test('the index never emits duplicate entries for the same table appearing as a column reference', () => {
  const index = buildCompletionIndex(data);
  const userEntries = index.items.filter((item) => item.label === 'users');
  assert.equal(userEntries.length, 1);
});

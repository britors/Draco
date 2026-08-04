import assert from 'node:assert/strict';
import { test } from 'node:test';
import { highlightSql, highlightSqlIncremental, splitSqlStatements, tokenizeSql } from '../dist/sql-highlight.js';

test('keywords, types, strings, comments and numbers are classified', () => {
  const tokens = tokenizeSql("SELECT id::bigint FROM users WHERE age > 10 -- note\n");
  const types = tokens.map((token) => token.type);
  assert.ok(types.includes('keyword'));
  assert.ok(types.includes('type'));
  assert.ok(types.includes('number'));
  assert.ok(types.includes('comment'));
});

test('function calls are distinguished from plain identifiers', () => {
  const tokens = tokenizeSql('SELECT count(*), users.name FROM users');
  const count = tokens.find((token) => token.value === 'count');
  const users = tokens.find((token) => token.value === 'users' && token.type !== 'keyword');
  assert.equal(count.type, 'function');
  assert.equal(users.type, 'plain');
});

test('quoted identifiers, parameters and dollar-quoted bodies are preserved verbatim', () => {
  const tokens = tokenizeSql('SELECT "My Col" FROM t WHERE id = $1');
  assert.ok(tokens.some((token) => token.type === 'ident' && token.value === '"My Col"'));
  assert.ok(tokens.some((token) => token.type === 'param' && token.value === '$1'));

  const body = "CREATE FUNCTION f() RETURNS void AS $$ BEGIN RAISE NOTICE 'it''s ok'; END; $$ LANGUAGE plpgsql;";
  const dollarToken = tokenizeSql(body).find((token) => token.type === 'string');
  assert.ok(dollarToken.value.startsWith('$$') && dollarToken.value.endsWith('$$'));
  assert.ok(dollarToken.value.includes("it''s ok"));
});

test('an unterminated quote does not throw or drop trailing text', () => {
  assert.doesNotThrow(() => tokenizeSql("SELECT 'oops"));
  const tokens = tokenizeSql("SELECT 'oops");
  const text = tokens.map((token) => token.value).join('');
  assert.equal(text, "SELECT 'oops");
});

test('highlightSql escapes HTML and reassembles to the original text', () => {
  const sql = "SELECT '<script>' AS x, 1 & 2";
  const html = highlightSql(sql);
  assert.ok(!html.includes('<script>'));
  assert.ok(html.includes('&lt;script&gt;'));
  const stripped = html.replace(/<span class="sql-tok-[a-z]+">|<\/span>/g, '').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
  assert.equal(stripped, sql);
});

test('a trailing newline is preserved so the overlay keeps the same line count as the textarea', () => {
  assert.ok(highlightSql('SELECT 1;\n').endsWith('\n'));
  assert.ok(!highlightSql('SELECT 1;').endsWith('\n'));
});

test('scripts split only on statement semicolons, not semicolons inside PostgreSQL syntax', () => {
  const sql = "SELECT ';'; -- comment ;\nSELECT $$body;still body$$; /* outer ; /* nested ; */ done */ SELECT 3;";
  const statements = splitSqlStatements(sql);
  assert.equal(statements.length, 3);
  assert.equal(statements.join(''), sql);
  assert.match(statements[1], /body;still body/);
});

test('statement splitting distinguishes standard strings from E strings', () => {
  const escaped = String.raw`SELECT E'it\'s;inside'; SELECT 2;`;
  assert.equal(splitSqlStatements(escaped).length, 2);
  const standard = String.raw`SELECT '\'; SELECT 2;`;
  assert.equal(splitSqlStatements(standard).length, 2);
});

test('incremental highlighting reuses unchanged statements', () => {
  const first = highlightSqlIncremental('SELECT 1;\nSELECT 2;');
  assert.equal(first.reused, 0);
  const unchanged = highlightSqlIncremental('SELECT 1;\nSELECT 2;', first.cache);
  assert.equal(unchanged.reused, 2);
  const changed = highlightSqlIncremental('SELECT 1;\nSELECT 3;', unchanged.cache);
  assert.equal(changed.reused, 1);
  assert.equal(changed.cache.map((entry) => entry.sql).join(''), 'SELECT 1;\nSELECT 3;');
});

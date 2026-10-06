import assert from 'node:assert/strict';
import { test } from 'node:test';

import { sqlMayWrite } from '../dist/sql-write-detect.js';

test('plain reads are not treated as writes', () => {
  assert.equal(sqlMayWrite('SELECT id, name FROM public.users WHERE id = 1'), false);
  assert.equal(sqlMayWrite('with t as (select 1) select * from t'), false);
  assert.equal(sqlMayWrite('SHOW search_path'), false);
  assert.equal(sqlMayWrite(''), false);
  assert.equal(sqlMayWrite(null), false);
});

test('data and schema changes are detected regardless of case', () => {
  for (const sql of [
    'INSERT INTO t VALUES (1)',
    'update t set a = 1',
    'Delete From t',
    'TRUNCATE t',
    'drop table t',
    'ALTER TABLE t ADD COLUMN b int',
    'create index on t (a)',
    'GRANT SELECT ON t TO r',
    'select * into new_table from t',
    'select setval(\'s\', 1)',
    'DO $$ BEGIN END $$',
    'CALL p()',
  ]) {
    assert.equal(sqlMayWrite(sql), true, sql);
  }
});

test('keywords inside literals, comments and quoted identifiers are ignored', () => {
  assert.equal(sqlMayWrite("SELECT 'delete from t' AS text"), false);
  assert.equal(sqlMayWrite("SELECT 'it''s an update'"), false);
  assert.equal(sqlMayWrite('SELECT "drop" FROM t'), false);
  assert.equal(sqlMayWrite('-- drop table t\nSELECT 1'), false);
  assert.equal(sqlMayWrite('/* truncate t */ SELECT 1'), false);
  assert.equal(sqlMayWrite('SELECT $body$ insert into t $body$'), false);
  assert.equal(sqlMayWrite('SELECT $$ insert $$'), false);
});

test('a write after a literal or comment is still detected', () => {
  assert.equal(sqlMayWrite("SELECT 'x'; DELETE FROM t"), true);
  assert.equal(sqlMayWrite('/* note */ UPDATE t SET a = 1'), true);
  assert.equal(sqlMayWrite('-- note\nDROP TABLE t'), true);
  assert.equal(sqlMayWrite('SELECT $a$ x $a$; TRUNCATE t'), true);
});

test('unterminated literals and comments swallow the rest of the input', () => {
  assert.equal(sqlMayWrite("SELECT 'unterminated delete"), false);
  assert.equal(sqlMayWrite('SELECT 1 /* drop'), false);
});

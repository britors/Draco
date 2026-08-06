import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  assembleFunctionDdl,
  formatFunctionParameters,
  parseFunctionParameters,
  sliceFunctionDdl,
} from '../dist/function-ddl.js';

test('slices a simple function into header fields and body', () => {
  const ddl = [
    'CREATE OR REPLACE FUNCTION public.add_numbers(a integer, b integer DEFAULT 1)',
    ' RETURNS integer',
    ' LANGUAGE plpgsql',
    'AS $function$',
    'BEGIN',
    '  RETURN a + b;',
    'END;',
    '$function$',
  ].join('\n');

  const sliced = sliceFunctionDdl(ddl, 'function');
  assert.ok(sliced, 'expected the DDL to be sliceable');
  assert.equal(sliced.header.schema, 'public');
  assert.equal(sliced.header.name, 'add_numbers');
  assert.equal(sliced.header.returns, 'integer');
  assert.equal(sliced.header.language, 'plpgsql');
  assert.equal(sliced.header.params.length, 2);
  assert.deepEqual(sliced.header.params[0], { mode: null, name: 'a', type: 'integer', default: null });
  assert.deepEqual(sliced.header.params[1], { mode: null, name: 'b', type: 'integer', default: '1' });
  assert.match(sliced.body, /RETURN a \+ b;/);
});

test('reassembling a sliced function reproduces an equivalent script', () => {
  const ddl = [
    'CREATE OR REPLACE FUNCTION public.add_numbers(a integer, b integer DEFAULT 1)',
    ' RETURNS integer',
    ' LANGUAGE plpgsql',
    'AS $function$',
    'BEGIN',
    '  RETURN a + b;',
    'END;',
    '$function$',
  ].join('\n');

  const sliced = sliceFunctionDdl(ddl, 'function');
  const rebuilt = assembleFunctionDdl(sliced.header, sliced.body);
  assert.match(rebuilt, /CREATE OR REPLACE FUNCTION "public"\."add_numbers"\("a" integer, "b" integer DEFAULT 1\)/);
  assert.match(rebuilt, /RETURNS integer/);
  assert.match(rebuilt, /LANGUAGE plpgsql/);
});

test('handles OUT/INOUT/VARIADIC parameters and preserves explicit modes', () => {
  const ddl = [
    'CREATE OR REPLACE FUNCTION app.calc_stats(p_id integer, OUT total numeric, OUT item_count integer)',
    ' RETURNS record',
    ' LANGUAGE sql',
    'AS $$',
    'SELECT sum(amount), count(*) FROM items WHERE id = p_id;',
    '$$',
  ].join('\n');

  const sliced = sliceFunctionDdl(ddl, 'function');
  assert.ok(sliced);
  assert.deepEqual(sliced.header.params.map((p) => p.mode), [null, 'OUT', 'OUT']);
  assert.deepEqual(sliced.header.params.map((p) => p.name), ['p_id', 'total', 'item_count']);
});

test('handles nested parens in parameter types and defaults without mis-splitting', () => {
  const ddl = [
    'CREATE OR REPLACE FUNCTION public.price_calc(p_amount numeric(10,2) DEFAULT 0.0, p_rate numeric(5,4) DEFAULT (0.05)::numeric)',
    ' RETURNS numeric',
    ' LANGUAGE sql',
    'AS $$',
    'SELECT p_amount * (1 + p_rate);',
    '$$',
  ].join('\n');

  const sliced = sliceFunctionDdl(ddl, 'function');
  assert.ok(sliced, 'expected nested-paren parameter list to slice cleanly');
  assert.equal(sliced.header.params.length, 2);
  assert.equal(sliced.header.params[0].type, 'numeric(10,2)');
  assert.equal(sliced.header.params[0].default, '0.0');
  assert.equal(sliced.header.params[1].type, 'numeric(5,4)');
  assert.equal(sliced.header.params[1].default, '(0.05)::numeric');
});

test('slices a procedure, which has no RETURNS clause', () => {
  const ddl = [
    'CREATE OR REPLACE PROCEDURE public.bump_counter(p_id integer, p_amount integer DEFAULT 1)',
    ' LANGUAGE plpgsql',
    'AS $$',
    'BEGIN',
    '  UPDATE counters SET value = value + p_amount WHERE id = p_id;',
    'END;',
    '$$',
  ].join('\n');

  const sliced = sliceFunctionDdl(ddl, 'procedure');
  assert.ok(sliced);
  assert.equal(sliced.header.kind, 'procedure');
  assert.equal(sliced.header.returns, null);
  const rebuilt = assembleFunctionDdl(sliced.header, sliced.body);
  assert.doesNotMatch(rebuilt, /RETURNS/);
  assert.match(rebuilt, /CREATE OR REPLACE PROCEDURE/);
});

test('preserves extra clauses (STRICT, SECURITY DEFINER, SET) verbatim', () => {
  const ddl = [
    'CREATE OR REPLACE FUNCTION app.safe_div(a numeric, b numeric)',
    ' RETURNS numeric',
    ' LANGUAGE plpgsql',
    ' STRICT',
    ' SECURITY DEFINER',
    " SET search_path TO 'public'",
    'AS $$',
    'BEGIN',
    '  RETURN a / b;',
    'END;',
    '$$',
  ].join('\n');

  const sliced = sliceFunctionDdl(ddl, 'function');
  assert.ok(sliced);
  assert.match(sliced.header.extra, /STRICT/);
  assert.match(sliced.header.extra, /SECURITY DEFINER/);
  assert.match(sliced.header.extra, /SET search_path/);
  const rebuilt = assembleFunctionDdl(sliced.header, sliced.body);
  assert.match(rebuilt, /STRICT/);
  assert.match(rebuilt, /SECURITY DEFINER/);
});

test('returns null (falls back to raw DDL editing) for a LANGUAGE C function with no dollar-quoted body', () => {
  const ddl = [
    'CREATE OR REPLACE FUNCTION public.native_fn(a integer)',
    ' RETURNS integer',
    ' LANGUAGE c',
    ' STRICT',
    "AS '$libdir/myext', 'native_fn'",
  ].join('\n');

  assert.equal(sliceFunctionDdl(ddl, 'function'), null);
});

test('returns null for DDL that does not even look like the requested kind', () => {
  assert.equal(sliceFunctionDdl('CREATE TRIGGER foo BEFORE INSERT ON bar EXECUTE FUNCTION baz();', 'function'), null);
  assert.equal(sliceFunctionDdl('CREATE OR REPLACE PROCEDURE p() LANGUAGE sql AS $$ SELECT 1; $$', 'function'), null);
});

test('parseFunctionParameters / formatFunctionParameters round-trip for a fresh (no-name) form build', () => {
  const params = [
    { mode: null, name: 'p_id', type: 'integer', default: null },
    { mode: null, name: 'p_label', type: 'text', default: "'unnamed'" },
  ];
  const raw = formatFunctionParameters(params);
  assert.equal(raw, '"p_id" integer, "p_label" text DEFAULT \'unnamed\'');
  assert.deepEqual(parseFunctionParameters(raw), params);
});

test('assembleFunctionDdl builds a fresh function script from a blank structured form', () => {
  const header = { kind: 'function', schema: 'public', name: 'new_fn', params: [], returns: 'void', language: 'plpgsql', extra: '' };
  const ddl = assembleFunctionDdl(header, '  -- function body\n');
  assert.match(ddl, /^CREATE OR REPLACE FUNCTION "public"\."new_fn"\(\)/);
  assert.match(ddl, /RETURNS void/);
  assert.match(ddl, /LANGUAGE plpgsql/);
  assert.match(ddl, /AS \$\$/);
  assert.match(ddl, /-- function body/);
});

test('assembleFunctionDdl picks a different dollar-quote tag when the body already contains $$', () => {
  const header = { kind: 'function', schema: 'public', name: 'weird', params: [], returns: 'text', language: 'sql' };
  const ddl = assembleFunctionDdl(header, "SELECT '$$literal$$';");
  assert.doesNotMatch(ddl, /AS \$\$\nSELECT/);
  assert.match(ddl, /AS \$function\$/);
});

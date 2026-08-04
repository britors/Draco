import assert from 'node:assert/strict';
import { test } from 'node:test';

import { resultRowToText, resultToCsv, resultToJson, resultToTsv, serializeResult } from '../dist/result-export.js';

const result = {
  columns: ['plain', 'comma', 'quote', 'newline', 'nullable', 'number', 'boolean', 'object'],
  rows: [
    {
      plain: 'value',
      comma: 'one,two',
      quote: 'say "hello"',
      newline: 'first\nsecond',
      nullable: null,
      number: 42,
      boolean: false,
      object: { nested: true },
    },
  ],
};

test('CSV export quotes delimiters and preserves scalar and structured values', () => {
  assert.equal(
    resultToCsv(result),
    'plain,comma,quote,newline,nullable,number,boolean,object\r\n' +
      'value,"one,two","say ""hello""","first\nsecond",,42,false,"{""nested"":true}"',
  );
});

test('JSON export preserves null, booleans, numbers and nested objects', () => {
  assert.deepEqual(JSON.parse(resultToJson(result)), result.rows);
});

test('clipboard TSV uses tabs and quotes tabs, newlines and structured values', () => {
  const tsv = resultToTsv({ columns: ['name', 'note', 'nullable'], rows: [{ name: 'one\ttwo', note: 'line\nbreak', nullable: null }] });
  assert.equal(tsv, 'name\tnote\tnullable\n"one\ttwo"\t"line\nbreak"\t');
});

test('row detail labels every value and makes NULL explicit', () => {
  const detail = resultRowToText(result.rows[0], result.columns);
  assert.match(detail, /^plain: value/);
  assert.match(detail, /nullable: NULL/);
  assert.match(detail, /boolean: false/);
  assert.match(detail, /object: \{\n  "nested": true\n\}$/);
});

test('serializer returns download metadata and rejects unknown formats', () => {
  assert.deepEqual(serializeResult(result, 'json'), {
    content: resultToJson(result),
    extension: 'json',
    type: 'application/json',
  });
  assert.equal(serializeResult(result, 'csv').type, 'text/csv;charset=utf-8');
  assert.throws(() => serializeResult(result, 'xml'), /Unsupported result export format/);
});

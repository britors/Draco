import assert from 'node:assert/strict';
import { test } from 'node:test';

import {
  MAX_RESULT_COLUMN_WIDTH,
  MIN_RESULT_COLUMN_WIDTH,
  clampResultColumnWidth,
} from '../dist/result-columns.js';

test('result column widths are rounded and kept inside usable bounds', () => {
  assert.equal(clampResultColumnWidth(199.6), 200);
  assert.equal(clampResultColumnWidth(1), MIN_RESULT_COLUMN_WIDTH);
  assert.equal(clampResultColumnWidth(10_000), MAX_RESULT_COLUMN_WIDTH);
});

test('invalid widths fall back to the minimum instead of producing invalid CSS', () => {
  assert.equal(clampResultColumnWidth(Number.NaN), MIN_RESULT_COLUMN_WIDTH);
  assert.equal(clampResultColumnWidth(Number.POSITIVE_INFINITY), MIN_RESULT_COLUMN_WIDTH);
});

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { visibleRange } from '../dist/virtual-list.js';

test('at the top of the list the range starts at 0, not a negative overscan offset', () => {
  const { start, end } = visibleRange({ rowCount: 100000, rowHeight: 35, scrollTop: 0, viewportHeight: 350, overscan: 10 });
  assert.equal(start, 0);
  assert.equal(end, 20); // 10 visible rows (350 / 35) + 10 overscan
});

test('scrolled into the middle, the range is centered on the viewport with overscan on both sides', () => {
  const { start, end } = visibleRange({ rowCount: 100000, rowHeight: 35, scrollTop: 3500, viewportHeight: 350, overscan: 10 });
  assert.equal(start, 90); // firstVisible=100, minus 10 overscan
  assert.equal(end, 120); // 100 + 10 visible + 10 overscan
});

test('near the bottom the range is clamped to rowCount, never past the last row', () => {
  const { start, end } = visibleRange({ rowCount: 50, rowHeight: 35, scrollTop: 100000, viewportHeight: 350, overscan: 10 });
  assert.equal(end, 50);
  assert.ok(start <= end);
  assert.ok(start >= 0);
});

test('a tiny result set never produces an out-of-bounds or inverted range', () => {
  const { start, end } = visibleRange({ rowCount: 3, rowHeight: 35, scrollTop: 0, viewportHeight: 350, overscan: 10 });
  assert.equal(start, 0);
  assert.equal(end, 3);
});

test('an empty result set renders nothing', () => {
  assert.deepEqual(visibleRange({ rowCount: 0, rowHeight: 35, scrollTop: 0, viewportHeight: 350 }), { start: 0, end: 0 });
});

test('a zero or unmeasured row height does not divide by zero or hang', () => {
  assert.deepEqual(visibleRange({ rowCount: 10, rowHeight: 0, scrollTop: 0, viewportHeight: 350 }), { start: 0, end: 0 });
});

test('a zero-height viewport (not yet laid out) still returns at least one row so the grid is never stuck empty', () => {
  const { start, end } = visibleRange({ rowCount: 10, rowHeight: 35, scrollTop: 0, viewportHeight: 0, overscan: 0 });
  assert.ok(end - start >= 1);
});

test('negative scrollTop (rubber-band overscroll on some platforms) is clamped instead of going negative', () => {
  const { start } = visibleRange({ rowCount: 100, rowHeight: 35, scrollTop: -50, viewportHeight: 350, overscan: 5 });
  assert.equal(start, 0);
});

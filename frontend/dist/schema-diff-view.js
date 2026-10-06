// Side-by-side line alignment for the schema diff, kept apart from app.js so it can be tested
// without a DOM. Rows pair the source (left) and target (right) definitions line by line.

// Longest common subsequence over lines; inputs are small DDL texts, so O(n·m) is fine. Very long
// definitions fall back to a positional pairing to keep the UI responsive.
const MAX_CELLS = 400000;

export function alignLines(left = '', right = '') {
  const a = left === null || left === undefined ? [] : String(left).split('\n');
  const b = right === null || right === undefined ? [] : String(right).split('\n');
  if (!a.length && !b.length) return [];
  if (a.length * b.length > MAX_CELLS) {
    return Array.from({ length: Math.max(a.length, b.length) }, (_, index) => pair(a[index], b[index]));
  }
  const table = Array.from({ length: a.length + 1 }, () => new Uint32Array(b.length + 1));
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  const rows = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      rows.push({ left: a[i], right: b[j], type: 'same' });
      i += 1; j += 1;
    } else if (j < b.length && (i === a.length || table[i][j + 1] >= table[i + 1][j])) {
      rows.push({ left: null, right: b[j], type: 'removed' });
      j += 1;
    } else {
      rows.push({ left: a[i], right: null, type: 'added' });
      i += 1;
    }
  }
  return mergeChanges(rows);
}

function pair(left, right) {
  if (left === undefined) return { left: null, right, type: 'removed' };
  if (right === undefined) return { left, right: null, type: 'added' };
  return { left, right, type: left === right ? 'same' : 'changed' };
}

// A run of target-only lines next to a run of source-only lines reads better as changed rows.
function mergeChanges(rows) {
  const merged = [];
  for (let index = 0; index < rows.length;) {
    if (rows[index].type === 'same') { merged.push(rows[index]); index += 1; continue; }
    const removed = [];
    const added = [];
    while (index < rows.length && rows[index].type !== 'same') {
      if (rows[index].type === 'removed') removed.push(rows[index].right); else added.push(rows[index].left);
      index += 1;
    }
    for (let k = 0; k < Math.max(removed.length, added.length); k += 1) merged.push(pair(added[k], removed[k]));
  }
  return merged;
}

// Counts objects per status for the summary line.
export function diffSummary(objects) {
  const summary = { added: 0, removed: 0, changed: 0 };
  for (const object of objects) summary[object.status] += 1;
  return summary;
}

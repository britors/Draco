// Schema-aware autocomplete for the local SQL editor. Pure functions only: building the searchable
// index from CompletionDataView, finding the word under the caret, and ranking suggestions. DOM
// wiring (popup, keyboard, insertion) lives in app.js so this stays testable without a browser.

import { KEYWORDS } from './sql-highlight.js';

const KEYWORD_ITEMS = [...KEYWORDS].map((word) => ({
  label: word.toUpperCase(),
  detail: 'keyword',
  kind: 'keyword',
  insertText: word.toUpperCase(),
}));

/** Turns a CompletionDataView ({ schemas, tables, columns, functions }) into a flat, searchable item list. */
export function buildCompletionIndex(data) {
  const items = [...KEYWORD_ITEMS];
  const seen = new Set(items.map((item) => `keyword:${item.label}`));
  const push = (kind, label, detail, insertText = label) => {
    const key = `${kind}:${label.toLowerCase()}:${detail}`;
    if (seen.has(key)) return;
    seen.add(key);
    items.push({ label, detail, kind, insertText });
  };
  for (const schema of data?.schemas || []) push('schema', schema, 'schema');
  for (const table of data?.tables || []) push(table.kind === 'view' ? 'view' : 'table', table.name, `${table.schema}.${table.name}`);
  for (const fn of data?.functions || []) push('function', fn.name, `${fn.schema} function`, `${fn.name}(`);
  const columnsByTable = new Map();
  for (const column of data?.columns || []) {
    push('column', column.name, `${column.table} column`);
    const list = columnsByTable.get(column.table.toLowerCase()) || [];
    list.push(column);
    columnsByTable.set(column.table.toLowerCase(), list);
  }
  return { items, columnsByTable };
}

const WORD_CHAR = /[A-Za-z0-9_]/;

/** Finds the identifier under the caret, plus an optional `qualifier.` immediately before it (e.g. "u." in "u.na|"). */
export function currentWordRange(text, caret) {
  let start = caret;
  while (start > 0 && WORD_CHAR.test(text[start - 1])) start -= 1;
  let end = caret;
  while (end < text.length && WORD_CHAR.test(text[end])) end += 1;
  let qualifier = null;
  if (start > 0 && text[start - 1] === '.') {
    let qualifierStart = start - 1;
    while (qualifierStart > 0 && WORD_CHAR.test(text[qualifierStart - 1])) qualifierStart -= 1;
    if (qualifierStart < start - 1) qualifier = text.slice(qualifierStart, start - 1);
  }
  return { start, end, word: text.slice(start, end), qualifier };
}

function rank(items, prefix) {
  const lower = prefix.toLowerCase();
  const startsWith = [];
  const contains = [];
  for (const item of items) {
    const label = item.label.toLowerCase();
    if (label.startsWith(lower)) startsWith.push(item);
    else if (label.includes(lower)) contains.push(item);
  }
  const byLabel = (a, b) => a.label.localeCompare(b.label);
  return [...startsWith.sort(byLabel), ...contains.sort(byLabel)];
}

/**
 * Suggestions for the identifier under the caret. When it is qualified (e.g. "orders.") and the
 * qualifier matches a known table, only that table's columns are offered — otherwise falls back to
 * the whole index (keywords, schemas, tables, views, functions and every column).
 */
export function suggest(index, text, caret, { limit = 20, minLength = 1 } = {}) {
  const { word, qualifier } = currentWordRange(text, caret);
  if (word.length < minLength) return [];
  if (qualifier) {
    const scoped = index.columnsByTable.get(qualifier.toLowerCase());
    if (scoped?.length) {
      const items = scoped.map((column) => ({ label: column.name, detail: `${column.table} column`, kind: 'column', insertText: column.name }));
      return rank(items, word).slice(0, limit);
    }
  }
  return rank(index.items, word).slice(0, limit);
}

/** Replaces the identifier under the caret with the chosen suggestion; returns the new text and caret position. */
export function applySuggestion(text, caret, suggestion) {
  const { start, end } = currentWordRange(text, caret);
  const insert = suggestion.insertText ?? suggestion.label;
  return { text: text.slice(0, start) + insert + text.slice(end), caret: start + insert.length };
}

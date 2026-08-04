// Lightweight SQL tokenizer + highlighter for the local editor overlay. No parser, no CDN: a
// single scanning regex classifies tokens well enough for readability, not for validating SQL.

export const KEYWORDS = new Set([
  'select', 'from', 'where', 'join', 'inner', 'left', 'right', 'full', 'outer', 'cross', 'lateral',
  'on', 'using', 'group', 'by', 'order', 'having', 'limit', 'offset', 'fetch', 'first', 'next', 'rows', 'only',
  'insert', 'into', 'values', 'update', 'set', 'delete', 'merge', 'returning',
  'create', 'alter', 'drop', 'table', 'index', 'view', 'materialized', 'schema', 'extension', 'sequence',
  'function', 'procedure', 'trigger', 'language', 'returns', 'as', 'replace', 'if', 'exists',
  'and', 'or', 'not', 'null', 'is', 'in', 'any', 'all', 'some', 'between', 'like', 'ilike', 'similar',
  'distinct', 'union', 'intersect', 'except', 'case', 'when', 'then', 'else', 'end', 'with', 'recursive',
  'conflict', 'do', 'nothing', 'cascade', 'restrict', 'primary', 'key', 'foreign', 'references',
  'unique', 'check', 'default', 'constraint', 'begin', 'commit', 'rollback', 'transaction', 'savepoint',
  'explain', 'analyze', 'vacuum', 'grant', 'revoke', 'to', 'over', 'partition', 'window', 'filter',
  'true', 'false', 'unknown', 'asc', 'desc', 'nulls', 'last', 'array', 'cast', 'collate', 'concurrently',
  'temporary', 'temp', 'unlogged', 'owner', 'current_user', 'current_date', 'current_timestamp',
]);

const TYPES = new Set([
  'int', 'integer', 'bigint', 'smallint', 'serial', 'bigserial', 'smallserial', 'numeric', 'decimal',
  'real', 'double', 'precision', 'money', 'text', 'varchar', 'character', 'char', 'boolean', 'bool',
  'date', 'time', 'timestamp', 'timestamptz', 'interval', 'json', 'jsonb', 'uuid', 'bytea', 'point',
  'inet', 'cidr', 'macaddr', 'xml', 'tsvector', 'tsquery', 'varying', 'zone',
]);

const TOKEN_PATTERN = new RegExp(
  [
    '(?<comment>--[^\\n]*|/\\*[\\s\\S]*?\\*/)',
    '(?<dollar>\\$(\\w*)\\$[\\s\\S]*?\\$\\2\\$)',
    "(?<string>'(?:[^']|'')*')",
    '(?<qident>"(?:[^"]|"")*")',
    '(?<param>\\$\\d+)',
    '(?<number>\\b\\d+\\.?\\d*(?:[eE][+-]?\\d+)?\\b)',
    '(?<word>\\b[A-Za-z_][A-Za-z0-9_]*\\b)',
  ].join('|'),
  'g',
);

const ESCAPE_MAP = { '&': '&amp;', '<': '&lt;', '>': '&gt;' };
const escapeHtml = (text) => text.replace(/[&<>]/g, (char) => ESCAPE_MAP[char]);

export function tokenizeSql(sql) {
  const tokens = [];
  let lastIndex = 0;
  TOKEN_PATTERN.lastIndex = 0;
  let match;
  while ((match = TOKEN_PATTERN.exec(sql))) {
    if (match.index > lastIndex) tokens.push({ type: 'plain', value: sql.slice(lastIndex, match.index) });
    const groups = match.groups;
    if (groups.comment !== undefined) tokens.push({ type: 'comment', value: groups.comment });
    else if (groups.dollar !== undefined) tokens.push({ type: 'string', value: groups.dollar });
    else if (groups.string !== undefined) tokens.push({ type: 'string', value: groups.string });
    else if (groups.qident !== undefined) tokens.push({ type: 'ident', value: groups.qident });
    else if (groups.param !== undefined) tokens.push({ type: 'param', value: groups.param });
    else if (groups.number !== undefined) tokens.push({ type: 'number', value: groups.number });
    else if (groups.word !== undefined) {
      const lower = groups.word.toLowerCase();
      if (KEYWORDS.has(lower)) tokens.push({ type: 'keyword', value: groups.word });
      else if (TYPES.has(lower)) tokens.push({ type: 'type', value: groups.word });
      else {
        const followedByParen = /^\s*\(/.test(sql.slice(TOKEN_PATTERN.lastIndex));
        tokens.push({ type: followedByParen ? 'function' : 'plain', value: groups.word });
      }
    }
    lastIndex = TOKEN_PATTERN.lastIndex;
  }
  if (lastIndex < sql.length) tokens.push({ type: 'plain', value: sql.slice(lastIndex) });
  return tokens;
}

export function highlightSql(sql) {
  let html = '';
  for (const token of tokenizeSql(sql)) {
    const text = escapeHtml(token.value);
    html += token.type === 'plain' ? text : `<span class="sql-tok-${token.type}">${text}</span>`;
  }
  // A trailing newline is invisible in a <pre>, so the overlay would be one line shorter than the
  // textarea and scroll out of sync on the last line. Pad it so both share the same line count.
  return sql.endsWith('\n') ? `${html}\n` : html;
}

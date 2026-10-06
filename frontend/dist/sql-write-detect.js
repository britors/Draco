// Heuristic used only to ask for confirmation before SQL runs on a production connection. It
// never allows anything: PostgreSQL permissions and read-only sessions are the real guarantee.
// Comments, string literals, dollar-quoted bodies and quoted identifiers are skipped so that a
// word like 'delete' inside a value does not count, while a keyword anywhere else does.
const WRITE_KEYWORDS = new Set([
  'insert', 'update', 'delete', 'merge', 'truncate', 'drop', 'alter', 'create', 'grant', 'revoke',
  'comment', 'copy', 'vacuum', 'analyze', 'reindex', 'cluster', 'refresh', 'call', 'do', 'reassign',
  'security', 'import', 'nextval', 'setval', 'into',
]);

function stripLiterals(sql) {
  let output = '';
  let index = 0;
  while (index < sql.length) {
    const rest = sql.slice(index);
    if (rest.startsWith('--')) {
      const end = sql.indexOf('\n', index);
      index = end === -1 ? sql.length : end;
    } else if (rest.startsWith('/*')) {
      const end = sql.indexOf('*/', index + 2);
      index = end === -1 ? sql.length : end + 2;
      output += ' ';
    } else if (sql[index] === "'" || sql[index] === '"') {
      const quote = sql[index];
      index += 1;
      while (index < sql.length) {
        if (sql[index] === quote && sql[index + 1] === quote) index += 2;
        else if (sql[index] === quote) { index += 1; break; }
        else index += 1;
      }
      output += ' ';
    } else {
      const dollar = rest.match(/^\$([A-Za-z_][A-Za-z0-9_]*)?\$/);
      if (dollar) {
        const end = sql.indexOf(dollar[0], index + dollar[0].length);
        index = end === -1 ? sql.length : end + dollar[0].length;
        output += ' ';
      } else {
        output += sql[index];
        index += 1;
      }
    }
  }
  return output;
}

export function sqlMayWrite(sql) {
  const words = stripLiterals(String(sql || '')).toLowerCase().match(/[a-z_][a-z0-9_]*/g) || [];
  return words.some((word) => WRITE_KEYWORDS.has(word));
}

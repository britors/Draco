import { dracoModel, dracoField } from '../parser/dracoParser';
import { ColumnInfo } from '../db/queries';

const draco_SCALARS = new Set([
  'String','Int','BigInt','Float','Decimal','Boolean','DateTime','Json','Bytes',
]);

function isRelationField(f: dracoField): boolean {
  return f.isList && !draco_SCALARS.has(f.dracoType);
}

function typeCompatible(dracoType: string, dbType: string): boolean {
  const db = dbType.toLowerCase();
  switch (dracoType) {
    case 'String':   return /text|varchar|char|citext|uuid|name|enum/.test(db);
    case 'Int':      return /int|integer|int2|int4|serial|smallint/.test(db);
    case 'BigInt':   return /bigint|int8|bigserial/.test(db);
    case 'Float':    return /real|float|double/.test(db);
    case 'Decimal':  return /numeric|decimal/.test(db);
    case 'Boolean':  return /bool/.test(db);
    case 'DateTime': return /timestamp|date|time/.test(db);
    case 'Json':     return /json/.test(db);
    case 'Bytes':    return /bytea/.test(db);
    default:         return true;
  }
}

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

type RowStatus = 'ok' | 'type-mismatch' | 'missing-in-db' | 'missing-in-model';

interface DriftRow {
  status: RowStatus;
  fieldName: string | null;
  dracoType: string | null;
  columnName: string | null;
  dbType: string | null;
}

export function getDriftHtml(params: {
  modelName: string;
  tableName: string;
  schema: string;
  model: dracoModel;
  columns: ColumnInfo[];
}): string {
  const { modelName, tableName, schema, model, columns } = params;

  const dbColMap = new Map(columns.map(c => [c.name, c]));
  const modelFields = model.fields.filter(f => !isRelationField(f));
  const usedCols = new Set<string>();

  const rows: DriftRow[] = [];

  for (const field of modelFields) {
    const col = dbColMap.get(field.columnName);
    usedCols.add(field.columnName);
    if (col) {
      const ok = typeCompatible(field.dracoType, col.dataType);
      rows.push({
        status: ok ? 'ok' : 'type-mismatch',
        fieldName: field.name,
        dracoType: field.dracoType + (field.isList ? '[]' : '') + (field.isOptional ? '?' : ''),
        columnName: col.name,
        dbType: col.dataType,
      });
    } else {
      rows.push({
        status: 'missing-in-db',
        fieldName: field.name,
        dracoType: field.dracoType,
        columnName: null,
        dbType: null,
      });
    }
  }

  for (const col of columns) {
    if (!usedCols.has(col.name)) {
      rows.push({ status: 'missing-in-model', fieldName: null, dracoType: null, columnName: col.name, dbType: col.dataType });
    }
  }

  const statusIcon = (s: RowStatus) => {
    switch (s) {
      case 'ok':               return '<span style="color:var(--vscode-testing-iconPassed,#73c991)">✓</span>';
      case 'type-mismatch':    return '<span style="color:var(--vscode-editorWarning-foreground,#cca700)">⚠</span>';
      case 'missing-in-db':    return '<span style="color:var(--vscode-errorForeground,#f48771)">✗ not in DB</span>';
      case 'missing-in-model': return '<span style="color:var(--vscode-descriptionForeground)">— not in model</span>';
    }
  };

  const okCount      = rows.filter(r => r.status === 'ok').length;
  const warnCount    = rows.filter(r => r.status === 'type-mismatch').length;
  const missingCount = rows.filter(r => r.status === 'missing-in-db' || r.status === 'missing-in-model').length;

  const tableRows = rows.map(r => `
    <tr class="${r.status}">
      <td>${r.fieldName ? esc(r.fieldName) : '<em style="color:var(--vscode-descriptionForeground)">—</em>'}</td>
      <td>${r.dracoType ? `<code>${esc(r.dracoType)}</code>` : '<em style="color:var(--vscode-descriptionForeground)">—</em>'}</td>
      <td>${r.columnName ? esc(r.columnName) : '<em style="color:var(--vscode-descriptionForeground)">—</em>'}</td>
      <td>${r.dbType ? `<code>${esc(r.dbType)}</code>` : '<em style="color:var(--vscode-descriptionForeground)">—</em>'}</td>
      <td>${statusIcon(r.status)}</td>
    </tr>`).join('');

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline';">
  <title>Drift: ${esc(modelName)}</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
      padding: 16px; overflow-y: auto;
    }
    h1 { font-size: 15px; font-weight: 600; margin-bottom: 4px; }
    .subtitle { font-size: 12px; color: var(--vscode-descriptionForeground); margin-bottom: 16px; }
    .summary { display: flex; gap: 16px; margin-bottom: 16px; font-size: 12px; }
    .badge { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: 10px; font-size: 11px; font-weight: 600; }
    .badge-ok    { background: rgba(115,201,145,.15); color: var(--vscode-testing-iconPassed,#73c991); }
    .badge-warn  { background: rgba(204,167,0,.15);   color: var(--vscode-editorWarning-foreground,#cca700); }
    .badge-err   { background: rgba(244,135,113,.15); color: var(--vscode-errorForeground,#f48771); }
    table { width: 100%; border-collapse: collapse; font-size: 12px; }
    th {
      text-align: left; font-size: 10px; font-weight: 700; text-transform: uppercase;
      letter-spacing: 0.5px; color: var(--vscode-descriptionForeground);
      padding: 6px 10px; border-bottom: 2px solid var(--vscode-panel-border);
    }
    td { padding: 5px 10px; border-bottom: 1px solid rgba(128,128,128,0.1); vertical-align: middle; }
    tr:hover { background: var(--vscode-list-hoverBackground); }
    code { font-family: var(--vscode-editor-font-family, monospace); font-size: 11px;
      background: rgba(128,128,128,0.1); padding: 1px 4px; border-radius: 2px; }
    tr.type-mismatch td:nth-child(2),
    tr.type-mismatch td:nth-child(4) { color: var(--vscode-editorWarning-foreground,#cca700); }
    tr.missing-in-db { opacity: 0.8; }
    tr.missing-in-model { opacity: 0.8; }
  </style>
</head>
<body>
  <h1>Drift: <code>${esc(modelName)}</code> ↔ <code>${esc(schema)}.${esc(tableName)}</code></h1>
  <div class="subtitle">draco model vs database table comparison</div>
  <div class="summary">
    <span class="badge badge-ok">✓ ${okCount} matched</span>
    ${warnCount ? `<span class="badge badge-warn">⚠ ${warnCount} type mismatch</span>` : ''}
    ${missingCount ? `<span class="badge badge-err">✗ ${missingCount} missing</span>` : ''}
  </div>
  <table>
    <thead>
      <tr>
        <th>draco field</th>
        <th>draco type</th>
        <th>DB column</th>
        <th>DB type</th>
        <th>Status</th>
      </tr>
    </thead>
    <tbody>${tableRows}</tbody>
  </table>
</body>
</html>`;
}

export function getPreviewHtml(params: {
  cspSource: string;
  connLabel: string;
  schema: string;
  table: string;
  columns: string[];
  rows: Record<string, unknown>[];
  estimate: number;
}): string {
  const { cspSource, connLabel, schema, table, columns, rows, estimate } = params;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy"
    content="default-src 'none'; style-src ${cspSource} 'unsafe-inline';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${escHtml(schema)}.${escHtml(table)}</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
      display: flex;
      flex-direction: column;
      height: 100vh;
      overflow: hidden;
    }
    .header {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 6px 16px;
      border-bottom: 1px solid var(--vscode-panel-border);
      background: var(--vscode-editorGroupHeader-tabsBackground);
      flex-shrink: 0;
      flex-wrap: wrap;
    }
    .header-title {
      font-weight: 600;
      font-size: 13px;
    }
    .header-sub {
      font-size: 11px;
      color: var(--vscode-descriptionForeground);
    }
    .badge {
      background: var(--vscode-badge-background);
      color: var(--vscode-badge-foreground);
      border-radius: 10px;
      padding: 1px 8px;
      font-size: 11px;
      white-space: nowrap;
    }
    .table-wrapper {
      flex: 1;
      overflow: auto;
    }
    table {
      border-collapse: collapse;
      width: max-content;
      min-width: 100%;
    }
    thead th {
      position: sticky;
      top: 0;
      z-index: 1;
      background: var(--vscode-editorGroupHeader-tabsBackground);
      padding: 4px 12px;
      text-align: left;
      font-weight: 600;
      border-bottom: 2px solid var(--vscode-panel-border);
      white-space: nowrap;
    }
    td {
      padding: 3px 12px;
      border-bottom: 1px solid var(--vscode-list-inactiveSelectionBackground, rgba(128,128,128,0.1));
      white-space: nowrap;
      max-width: 400px;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    tbody tr:hover {
      background: var(--vscode-list-hoverBackground);
    }
    .null {
      color: var(--vscode-descriptionForeground);
      font-style: italic;
    }
    .empty {
      padding: 32px;
      text-align: center;
      color: var(--vscode-descriptionForeground);
    }
  </style>
</head>
<body>
  <div class="header">
    <span class="header-title">${escHtml(schema)}.${escHtml(table)}</span>
    <span class="header-sub">${escHtml(connLabel)}</span>
    <span class="badge">~${estimate.toLocaleString()} rows</span>
    <span class="badge">LIMIT 100</span>
  </div>
  <div class="table-wrapper">
    ${buildTable(columns, rows)}
  </div>
</body>
</html>`;
}

function buildTable(columns: string[], rows: Record<string, unknown>[]): string {
  if (rows.length === 0) return '<div class="empty">No rows returned.</div>';

  let html = '<table><thead><tr>';
  for (const col of columns) html += `<th>${escHtml(col)}</th>`;
  html += '</tr></thead><tbody>';

  for (const row of rows) {
    html += '<tr>';
    for (const col of columns) {
      const val = row[col];
      if (val === null || val === undefined) {
        html += '<td><span class="null">NULL</span></td>';
      } else {
        html += `<td>${escHtml(String(val))}</td>`;
      }
    }
    html += '</tr>';
  }

  html += '</tbody></table>';
  return html;
}

function escHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

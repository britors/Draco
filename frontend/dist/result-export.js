function delimitedCell(value, delimiter) {
  if (value === null || value === undefined) return '';
  const text = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return text.includes(delimiter) || /["\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function resultToDelimited(result, delimiter, lineEnding) {
  const rows = [
    result.columns,
    ...result.rows.map((row) => result.columns.map((column) => row[column])),
  ];
  return rows.map((row) => row.map((value) => delimitedCell(value, delimiter)).join(delimiter)).join(lineEnding);
}

export function resultToCsv(result) {
  return resultToDelimited(result, ',', '\r\n');
}

export function resultToTsv(result) {
  return resultToDelimited(result, '\t', '\n');
}

export function resultToJson(result) {
  return JSON.stringify(result.rows, null, 2);
}

export function resultRowToText(row, columns) {
  return columns
    .map((column) => {
      const value = row[column];
      if (value === null || value === undefined) return `${column}: NULL`;
      if (typeof value === 'object') return `${column}: ${JSON.stringify(value, null, 2)}`;
      return `${column}: ${String(value)}`;
    })
    .join('\n\n');
}

export function serializeResult(result, format) {
  if (format === 'json') {
    return {
      content: resultToJson(result),
      extension: 'json',
      type: 'application/json',
    };
  }
  if (format === 'csv') {
    return {
      content: resultToCsv(result),
      extension: 'csv',
      type: 'text/csv;charset=utf-8',
    };
  }
  throw new TypeError(`Unsupported result export format: ${format}`);
}

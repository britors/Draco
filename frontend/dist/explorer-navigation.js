export function quoteSqlIdentifier(value) {
  return `"${String(value).replaceAll('"', '""')}"`;
}

export function schemaObjectSql(schema, object) {
  const qualified = `${quoteSqlIdentifier(schema)}.${quoteSqlIdentifier(object.name)}`;
  if (object.kind === 'function') {
    return `-- Add any required arguments before running\nSELECT ${qualified}();`;
  }
  if (object.kind === 'procedure') {
    return `-- Add any required arguments before running\nCALL ${qualified}();`;
  }
  if (object.kind === 'sequence') {
    return `SELECT * FROM ${qualified};`;
  }
  return null;
}

export function groupSchemaObjects(objects = []) {
  return {
    programming: objects.filter((object) => ['function', 'procedure', 'trigger'].includes(object.kind)),
    sequences: objects.filter((object) => object.kind === 'sequence'),
  };
}

import { PgDriver } from './PgDriver';

export interface SchemaInfo {
  name: string;
}

export interface IndexInfo {
  name: string;
  definition: string;
  size: string;
  isUnique: boolean;
  isPrimary: boolean;
}

export interface ConstraintInfo {
  name: string;
  type: string;
  definition: string;
}

export interface FKMapEntry {
  direction: 'outgoing' | 'incoming';
  constraintName: string;
  column: string;
  foreignSchema: string;
  foreignTable: string;
  foreignColumn: string;
}

export interface MigrationEntry {
  id: string;
  migrationName: string;
  startedAt: string;
  finishedAt: string | null;
  rolledBackAt: string | null;
  logs: string | null;
  appliedStepsCount: number;
}

export interface TableInfo {
  name: string;
  type: 'table' | 'view';
}

export interface ColumnInfo {
  name: string;
  dataType: string;
  isNullable: boolean;
  hasDefault: boolean;
  isPrimaryKey: boolean;
  isForeignKey: boolean;
}

export interface TableDetailColumn {
  name: string;
  dataType: string;       // e.g. "character varying", "integer"
  fullType: string;       // formatted with size, e.g. "varchar(255)", "numeric(10,2)"
  isNullable: boolean;
  columnDefault: string | null;
  isPrimaryKey: boolean;
  isForeignKey: boolean;
  ordinalPosition: number;
}

export interface TableDetail {
  columns: TableDetailColumn[];
  constraints: ConstraintInfo[];
  indexes: IndexInfo[];
  fkMap: FKMapEntry[];
  rowEstimate: number;
}

export interface FunctionInfo {
  name: string;
  type: 'FUNCTION' | 'PROCEDURE';
  returnType: string;
  specificName: string;
}

export interface FunctionParam {
  name: string;
  dataType: string;
  mode: string;
}

export async function getSchemas(driver: PgDriver): Promise<SchemaInfo[]> {
  const rows = await driver.query<{ schema_name: string }>(
    `SELECT schema_name FROM information_schema.schemata
     WHERE schema_name NOT IN ('pg_catalog','information_schema','pg_toast')
     ORDER BY schema_name`
  );
  return rows.map(r => ({ name: r.schema_name }));
}

export async function getTables(driver: PgDriver, schema: string): Promise<TableInfo[]> {
  const rows = await driver.query<{ table_name: string; table_type: string }>(
    `SELECT table_name, table_type FROM information_schema.tables
     WHERE table_schema = $1 ORDER BY table_name`,
    [schema]
  );
  return rows.map(r => ({ name: r.table_name, type: r.table_type === 'VIEW' ? 'view' : 'table' }));
}

export async function getColumns(driver: PgDriver, schema: string, table: string): Promise<ColumnInfo[]> {
  const rows = await driver.query<{
    column_name: string;
    data_type: string;
    is_nullable: string;
    column_default: string | null;
    is_primary_key: boolean;
    is_foreign_key: boolean;
  }>(
    `SELECT
       c.column_name,
       c.data_type,
       c.is_nullable,
       c.column_default,
       COALESCE(pk.is_pk, false) AS is_primary_key,
       COALESCE(fk.is_fk, false) AS is_foreign_key
     FROM information_schema.columns c
     LEFT JOIN (
       SELECT kcu.column_name, true AS is_pk
       FROM information_schema.table_constraints tc
       JOIN information_schema.key_column_usage kcu
         ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
       WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = $1 AND tc.table_name = $2
     ) pk ON c.column_name = pk.column_name
     LEFT JOIN (
       SELECT kcu.column_name, true AS is_fk
       FROM information_schema.table_constraints tc
       JOIN information_schema.key_column_usage kcu
         ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
       WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2
     ) fk ON c.column_name = fk.column_name
     WHERE c.table_schema = $1 AND c.table_name = $2
     ORDER BY c.ordinal_position`,
    [schema, table]
  );
  return rows.map(r => ({
    name: r.column_name,
    dataType: r.data_type,
    isNullable: r.is_nullable === 'YES',
    hasDefault: r.column_default !== null,
    isPrimaryKey: Boolean(r.is_primary_key),
    isForeignKey: Boolean(r.is_foreign_key),
  }));
}

export async function getFunctions(driver: PgDriver, schema: string): Promise<FunctionInfo[]> {
  const rows = await driver.query<{
    routine_name: string;
    routine_type: string;
    data_type: string;
    specific_name: string;
  }>(
    `SELECT routine_name, routine_type, data_type, specific_name
     FROM information_schema.routines
     WHERE routine_schema = $1 AND routine_type IN ('FUNCTION','PROCEDURE')
     ORDER BY routine_name`,
    [schema]
  );
  return rows.map(r => ({
    name: r.routine_name,
    type: r.routine_type as 'FUNCTION' | 'PROCEDURE',
    returnType: r.data_type,
    specificName: r.specific_name,
  }));
}

export async function getFunctionParams(
  driver: PgDriver,
  schema: string,
  specificName: string
): Promise<FunctionParam[]> {
  const rows = await driver.query<{
    parameter_name: string;
    data_type: string;
    parameter_mode: string;
  }>(
    `SELECT parameter_name, data_type, parameter_mode
     FROM information_schema.parameters
     WHERE specific_schema = $1 AND specific_name = $2 AND parameter_name IS NOT NULL
     ORDER BY ordinal_position`,
    [schema, specificName]
  );
  return rows.map(r => ({ name: r.parameter_name, dataType: r.data_type, mode: r.parameter_mode }));
}

export async function getCompletionData(driver: PgDriver): Promise<{
  tables: { schema: string; name: string; type: 'table' | 'view' }[];
  functions: { schema: string; name: string }[];
}> {
  const [tables, functions] = await Promise.all([
    driver.query<{ schema: string; name: string; table_type: string }>(
      `SELECT table_schema AS schema, table_name AS name, table_type
       FROM information_schema.tables
       WHERE table_schema NOT IN ('pg_catalog','information_schema','pg_toast')
       ORDER BY table_schema, table_name`
    ),
    driver.query<{ schema: string; name: string }>(
      `SELECT routine_schema AS schema, routine_name AS name
       FROM information_schema.routines
       WHERE routine_schema NOT IN ('pg_catalog','information_schema','pg_toast')
         AND routine_type IN ('FUNCTION','PROCEDURE')
       ORDER BY routine_schema, routine_name`
    ),
  ]);
  return {
    tables: tables.map(t => ({ schema: t.schema, name: t.name, type: (t.table_type === 'VIEW' ? 'view' : 'table') as 'view' | 'table' })),
    functions: functions.map(f => ({ schema: f.schema, name: f.name })),
  };
}

export async function getTableEstimates(
  driver: PgDriver,
  schema: string
): Promise<Record<string, number>> {
  const rows = await driver.query<{ relname: string; estimate: number }>(
    `SELECT c.relname, c.reltuples::bigint AS estimate
     FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE n.nspname = $1 AND c.relkind IN ('r','v','m')`,
    [schema]
  );
  return Object.fromEntries(rows.map(r => [r.relname, Number(r.estimate)]));
}

export async function getTableDDL(driver: PgDriver, schema: string, table: string): Promise<string> {
  const oidRows = await driver.query<{ oid: number }>(
    `SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relname = $2 AND n.nspname = $1`,
    [schema, table]
  );
  if (!oidRows.length) return `-- Table "${schema}"."${table}" not found`;
  const oid = oidRows[0].oid;

  const columns = await driver.query<{
    attname: string;
    type: string;
    notnull: boolean;
    defval: string | null;
  }>(
    `SELECT a.attname, pg_catalog.format_type(a.atttypid, a.atttypmod) AS type,
            a.attnotnull AS notnull, pg_get_expr(d.adbin, d.adrelid) AS defval
     FROM pg_attribute a
     LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
     WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
     ORDER BY a.attnum`,
    [oid]
  );

  const constraints = await driver.query<{ conname: string; condef: string }>(
    `SELECT conname, pg_get_constraintdef(oid) AS condef
     FROM pg_constraint WHERE conrelid = $1 AND contype IN ('p','f','u','c')
     ORDER BY contype, conname`,
    [oid]
  );

  const qSchema = schema.replace(/"/g, '""');
  const qTable = table.replace(/"/g, '""');

  const colLines = columns.map(c => {
    let line = `  "${c.attname}" ${c.type}`;
    if (c.defval !== null) line += ` DEFAULT ${c.defval}`;
    if (c.notnull) line += ' NOT NULL';
    return line;
  });
  const conLines = constraints.map(c => `  CONSTRAINT "${c.conname}" ${c.condef}`);
  const body = [...colLines, ...conLines].join(',\n');
  return `CREATE TABLE "${qSchema}"."${qTable}" (\n${body}\n);`;
}

export async function getIndexes(driver: PgDriver, schema: string, table: string): Promise<IndexInfo[]> {
  const rows = await driver.query<{
    index_name: string;
    index_def: string;
    index_size: string;
    is_unique: boolean;
    is_primary: boolean;
  }>(
    `SELECT i.relname AS index_name, pg_get_indexdef(ix.indexrelid) AS index_def,
            pg_size_pretty(pg_relation_size(ix.indexrelid)) AS index_size,
            ix.indisunique AS is_unique, ix.indisprimary AS is_primary
     FROM pg_index ix
     JOIN pg_class i ON i.oid = ix.indexrelid
     JOIN pg_class t ON t.oid = ix.indrelid
     JOIN pg_namespace n ON n.oid = t.relnamespace
     WHERE n.nspname = $1 AND t.relname = $2
     ORDER BY ix.indisprimary DESC, ix.indisunique DESC, i.relname`,
    [schema, table]
  );
  return rows.map(r => ({
    name: r.index_name,
    definition: r.index_def,
    size: r.index_size,
    isUnique: Boolean(r.is_unique),
    isPrimary: Boolean(r.is_primary),
  }));
}

export async function getConstraints(driver: PgDriver, schema: string, table: string): Promise<ConstraintInfo[]> {
  const oidRows = await driver.query<{ oid: number }>(
    `SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relname = $2 AND n.nspname = $1`,
    [schema, table]
  );
  if (!oidRows.length) return [];
  const rows = await driver.query<{ name: string; type: string; definition: string }>(
    `SELECT conname AS name,
            CASE contype WHEN 'p' THEN 'PRIMARY KEY' WHEN 'f' THEN 'FOREIGN KEY' WHEN 'u' THEN 'UNIQUE' WHEN 'c' THEN 'CHECK' END AS type,
            pg_get_constraintdef(oid) AS definition
     FROM pg_constraint WHERE conrelid = $1 AND contype IN ('p','f','u','c')
     ORDER BY contype, conname`,
    [oidRows[0].oid]
  );
  return rows.map(r => ({ name: r.name, type: r.type, definition: r.definition }));
}

export async function getFKMap(driver: PgDriver, schema: string, table: string): Promise<FKMapEntry[]> {
  const [out, inc] = await Promise.all([
    driver.query<{
      constraint_name: string;
      column_name: string;
      foreign_schema: string;
      foreign_table: string;
      foreign_column: string;
    }>(
      `SELECT kcu.constraint_name, kcu.column_name,
              ccu.table_schema AS foreign_schema, ccu.table_name AS foreign_table, ccu.column_name AS foreign_column
       FROM information_schema.key_column_usage kcu
       JOIN information_schema.table_constraints tc USING (constraint_name, table_schema, table_name)
       JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_name = tc.constraint_name
       WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2
       ORDER BY kcu.constraint_name, kcu.ordinal_position`,
      [schema, table]
    ),
    driver.query<{
      constraint_name: string;
      referencing_schema: string;
      referencing_table: string;
      referencing_column: string;
      referenced_column: string;
    }>(
      `SELECT tc.constraint_name, tc.table_schema AS referencing_schema, tc.table_name AS referencing_table,
              kcu.column_name AS referencing_column, ccu.column_name AS referenced_column
       FROM information_schema.table_constraints tc
       JOIN information_schema.key_column_usage kcu USING (constraint_name, table_schema, table_name)
       JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_name = tc.constraint_name
       WHERE tc.constraint_type = 'FOREIGN KEY' AND ccu.table_schema = $1 AND ccu.table_name = $2
       ORDER BY tc.table_schema, tc.table_name, tc.constraint_name`,
      [schema, table]
    ),
  ]);

  return [
    ...out.map(r => ({
      direction: 'outgoing' as const,
      constraintName: r.constraint_name,
      column: r.column_name,
      foreignSchema: r.foreign_schema,
      foreignTable: r.foreign_table,
      foreignColumn: r.foreign_column,
    })),
    ...inc.map(r => ({
      direction: 'incoming' as const,
      constraintName: r.constraint_name,
      column: r.referenced_column,
      foreignSchema: r.referencing_schema,
      foreignTable: r.referencing_table,
      foreignColumn: r.referencing_column,
    })),
  ];
}

export async function getTableDetail(
  driver: PgDriver,
  schema: string,
  table: string
): Promise<TableDetail> {
  const [rawCols, constraints, indexes, fkMap, estRows] = await Promise.all([
    driver.query<{
      column_name: string;
      data_type: string;
      udt_name: string;
      character_maximum_length: number | null;
      numeric_precision: number | null;
      numeric_scale: number | null;
      is_nullable: string;
      column_default: string | null;
      ordinal_position: number;
      is_pk: boolean;
      is_fk: boolean;
    }>(
      `SELECT
         c.column_name, c.data_type, c.udt_name,
         c.character_maximum_length, c.numeric_precision, c.numeric_scale,
         c.is_nullable, c.column_default, c.ordinal_position,
         COALESCE(pk.is_pk, false) AS is_pk,
         COALESCE(fk.is_fk, false) AS is_fk
       FROM information_schema.columns c
       LEFT JOIN (
         SELECT kcu.column_name, true AS is_pk
         FROM information_schema.table_constraints tc
         JOIN information_schema.key_column_usage kcu
           ON tc.constraint_name = kcu.constraint_name
           AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
         WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = $1 AND tc.table_name = $2
       ) pk ON c.column_name = pk.column_name
       LEFT JOIN (
         SELECT kcu.column_name, true AS is_fk
         FROM information_schema.table_constraints tc
         JOIN information_schema.key_column_usage kcu
           ON tc.constraint_name = kcu.constraint_name
           AND tc.table_schema = kcu.table_schema AND tc.table_name = kcu.table_name
         WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2
       ) fk ON c.column_name = fk.column_name
       WHERE c.table_schema = $1 AND c.table_name = $2
       ORDER BY c.ordinal_position`,
      [schema, table]
    ),
    getConstraints(driver, schema, table),
    getIndexes(driver, schema, table),
    getFKMap(driver, schema, table),
    driver.query<{ estimate: number }>(
      `SELECT reltuples::bigint AS estimate
       FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
       WHERE n.nspname = $1 AND c.relname = $2`,
      [schema, table]
    ),
  ]);

  const columns: TableDetailColumn[] = rawCols.map(r => {
    let fullType = r.udt_name;
    if (r.character_maximum_length !== null) fullType += `(${r.character_maximum_length})`;
    else if (r.numeric_precision !== null && r.numeric_scale !== null) fullType += `(${r.numeric_precision},${r.numeric_scale})`;
    else if (r.numeric_precision !== null) fullType += `(${r.numeric_precision})`;
    return {
      name: r.column_name,
      dataType: r.data_type,
      fullType,
      isNullable: r.is_nullable === 'YES',
      columnDefault: r.column_default,
      isPrimaryKey: Boolean(r.is_pk),
      isForeignKey: Boolean(r.is_fk),
      ordinalPosition: r.ordinal_position,
    };
  });

  return {
    columns,
    constraints,
    indexes,
    fkMap,
    rowEstimate: Number(estRows[0]?.estimate ?? 0),
  };
}

export interface ERDColumn {
  name: string;
  dataType: string;
  isPk: boolean;
  isFk: boolean;
  isNullable: boolean;
}

export interface ERDTable {
  name: string;
  type: 'table' | 'view';
  columns: ERDColumn[];
}

export interface ERDRelation {
  fromTable: string;
  fromColumn: string;
  toTable: string;
  toColumn: string;
  constraintName: string;
}

export async function getERDData(
  driver: PgDriver,
  schema: string
): Promise<{ tables: ERDTable[]; relations: ERDRelation[] }> {
  const [tableRows, colRows, pkRows, fkRows] = await Promise.all([
    driver.query<{ table_name: string; table_type: string }>(
      `SELECT table_name, table_type
       FROM information_schema.tables
       WHERE table_schema = $1 AND table_type IN ('BASE TABLE','VIEW')
       ORDER BY table_name`,
      [schema]
    ),
    driver.query<{ table_name: string; column_name: string; data_type: string; udt_name: string; is_nullable: string }>(
      `SELECT table_name, column_name, data_type, udt_name, is_nullable
       FROM information_schema.columns
       WHERE table_schema = $1
       ORDER BY table_name, ordinal_position`,
      [schema]
    ),
    driver.query<{ table_name: string; column_name: string }>(
      `SELECT tc.table_name, kcu.column_name
       FROM information_schema.table_constraints tc
       JOIN information_schema.key_column_usage kcu
         USING (constraint_name, table_schema, table_name)
       WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = $1`,
      [schema]
    ),
    driver.query<{ from_table: string; from_column: string; to_table: string; to_column: string; constraint_name: string }>(
      `SELECT kcu.table_name AS from_table, kcu.column_name AS from_column,
              ccu.table_name AS to_table, ccu.column_name AS to_column,
              tc.constraint_name
       FROM information_schema.table_constraints tc
       JOIN information_schema.key_column_usage kcu
         USING (constraint_name, table_schema, table_name)
       JOIN information_schema.constraint_column_usage ccu
         ON ccu.constraint_name = tc.constraint_name AND ccu.table_schema = tc.table_schema
       WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1
       ORDER BY kcu.table_name, tc.constraint_name, kcu.ordinal_position`,
      [schema]
    ),
  ]);

  const pkSet = new Set(pkRows.map(r => r.table_name + '.' + r.column_name));
  const fkSet = new Set(fkRows.map(r => r.from_table + '.' + r.from_column));

  const colsByTable = new Map<string, ERDColumn[]>();
  for (const r of colRows) {
    if (!colsByTable.has(r.table_name)) colsByTable.set(r.table_name, []);
    const typeLabel = r.data_type === 'USER-DEFINED' ? r.udt_name : r.data_type;
    colsByTable.get(r.table_name)!.push({
      name: r.column_name,
      dataType: typeLabel,
      isPk: pkSet.has(r.table_name + '.' + r.column_name),
      isFk: fkSet.has(r.table_name + '.' + r.column_name),
      isNullable: r.is_nullable === 'YES',
    });
  }

  const tables: ERDTable[] = tableRows.map(r => ({
    name: r.table_name,
    type: r.table_type === 'VIEW' ? 'view' : 'table',
    columns: colsByTable.get(r.table_name) ?? [],
  }));

  const relations: ERDRelation[] = fkRows.map(r => ({
    fromTable: r.from_table,
    fromColumn: r.from_column,
    toTable: r.to_table,
    toColumn: r.to_column,
    constraintName: r.constraint_name,
  }));

  return { tables, relations };
}

// ── Browse table data (#62) ──────────────────────────────────────────────────

export async function browseTableData(
  driver: PgDriver,
  schema: string,
  table: string,
  offset: number,
  limit: number
): Promise<{ columns: string[]; rows: Record<string, unknown>[]; total: number }> {
  const qSchema = schema.replace(/"/g, '""');
  const qTable  = table.replace(/"/g, '""');
  const [countRows, rows] = await Promise.all([
    driver.query<{ count: string }>(`SELECT COUNT(*)::text AS count FROM "${qSchema}"."${qTable}"`),
    driver.query<Record<string, unknown>>(`SELECT * FROM "${qSchema}"."${qTable}" LIMIT $1 OFFSET $2`, [limit, offset]),
  ]);
  return { columns: rows.length ? Object.keys(rows[0]) : [], rows, total: Number(countRows[0]?.count ?? 0) };
}

// ── Update table row (#63) ────────────────────────────────────────────────────

export async function updateTableRow(
  driver: PgDriver,
  schema: string,
  table: string,
  pkCols: string[],
  pkVals: unknown[],
  column: string,
  newValue: string | null
): Promise<void> {
  const qSchema = schema.replace(/"/g, '""');
  const qTable  = table.replace(/"/g, '""');
  const qCol    = column.replace(/"/g, '""');
  const where   = pkCols.map((c, i) => `"${c.replace(/"/g, '""')}" = $${i + 2}`).join(' AND ');
  await driver.query(
    `UPDATE "${qSchema}"."${qTable}" SET "${qCol}" = $1 WHERE ${where}`,
    [newValue === '' ? null : newValue, ...pkVals]
  );
}

// ── Column statistics (#67) ──────────────────────────────────────────────────

export interface ColumnStat {
  column: string;
  nullFrac: number;
  nDistinct: number;
  mostCommonVals: string | null;
  histogramBounds: string | null;
}

export async function getColumnStats(driver: PgDriver, schema: string, table: string): Promise<ColumnStat[]> {
  return driver.query<ColumnStat>(
    `SELECT attname          AS "column",
            null_frac        AS "nullFrac",
            n_distinct       AS "nDistinct",
            most_common_vals::text AS "mostCommonVals",
            histogram_bounds::text AS "histogramBounds"
     FROM pg_stats
     WHERE schemaname = $1 AND tablename = $2
     ORDER BY (
       SELECT ordinal_position FROM information_schema.columns c
       WHERE c.table_schema = $1 AND c.table_name = $2 AND c.column_name = pg_stats.attname
     )`,
    [schema, table]
  );
}

// ── Prisma schema generator (#60) ────────────────────────────────────────────

const PG_TO_PRISMA: Record<string, string> = {
  integer: 'Int', int4: 'Int', int2: 'Int', smallint: 'Int', int: 'Int',
  bigint: 'BigInt', int8: 'BigInt',
  boolean: 'Boolean', bool: 'Boolean',
  text: 'String', varchar: 'String', 'character varying': 'String', bpchar: 'String', char: 'String',
  uuid: 'String', json: 'Json', jsonb: 'Json',
  numeric: 'Decimal', decimal: 'Decimal', money: 'Decimal',
  real: 'Float', float4: 'Float', float8: 'Float', 'double precision': 'Float',
  timestamp: 'DateTime', 'timestamp without time zone': 'DateTime',
  timestamptz: 'DateTime', 'timestamp with time zone': 'DateTime',
  date: 'DateTime', time: 'String', 'time without time zone': 'String',
  bytea: 'Bytes',
  serial: 'Int', bigserial: 'BigInt', smallserial: 'Int',
};

function _toPascal(s: string): string { return s.replace(/(?:^|_)(\w)/g, (_, c: string) => c.toUpperCase()); }
function _toCamel(s: string): string  { const p = _toPascal(s); return p.charAt(0).toLowerCase() + p.slice(1); }

export async function generatePrismaSchema(driver: PgDriver, schema: string): Promise<string> {
  const [tables, columns, pks, fks, uniques] = await Promise.all([
    driver.query<{ name: string }>(
      `SELECT table_name AS name FROM information_schema.tables WHERE table_schema=$1 AND table_type='BASE TABLE' ORDER BY table_name`, [schema]),
    driver.query<{ table_name: string; column_name: string; data_type: string; udt_name: string; is_nullable: string; column_default: string | null }>(
      `SELECT table_name, column_name, data_type, udt_name, is_nullable, column_default FROM information_schema.columns WHERE table_schema=$1 ORDER BY table_name, ordinal_position`, [schema]),
    driver.query<{ table_name: string; column_name: string }>(
      `SELECT tc.table_name, kcu.column_name FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage kcu USING(constraint_name,table_schema,table_name) WHERE tc.constraint_type='PRIMARY KEY' AND tc.table_schema=$1`, [schema]),
    driver.query<{ table_name: string; column_name: string; foreign_table: string; foreign_column: string }>(
      `SELECT kcu.table_name, kcu.column_name, ccu.table_name AS foreign_table, ccu.column_name AS foreign_column FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage kcu ON kcu.constraint_name=tc.constraint_name AND kcu.table_schema=tc.table_schema AND kcu.table_name=tc.table_name JOIN information_schema.referential_constraints rc ON rc.constraint_name=tc.constraint_name AND rc.constraint_schema=tc.constraint_schema JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_name=rc.unique_constraint_name AND ccu.constraint_schema=$1 WHERE tc.constraint_type='FOREIGN KEY' AND tc.table_schema=$1`, [schema]),
    driver.query<{ table_name: string; column_name: string }>(
      `SELECT tc.table_name, kcu.column_name FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage kcu USING(constraint_name,table_schema,table_name) WHERE tc.constraint_type='UNIQUE' AND tc.table_schema=$1`, [schema]),
  ]);

  const pkSet     = new Set(pks.map(p => `${p.table_name}.${p.column_name}`));
  const uniqueSet = new Set(uniques.map(u => `${u.table_name}.${u.column_name}`));
  const fkMap     = new Map(fks.map(f => [`${f.table_name}.${f.column_name}`, f]));
  const colsByTable = new Map<string, typeof columns[0][]>();
  for (const c of columns) {
    if (!colsByTable.has(c.table_name)) colsByTable.set(c.table_name, []);
    colsByTable.get(c.table_name)!.push(c);
  }

  let out = `generator client {\n  provider = "prisma-client-js"\n}\n\ndatasource db {\n  provider = "postgresql"\n  url      = env("DATABASE_URL")\n}\n\n`;

  for (const table of tables) {
    const cols      = colsByTable.get(table.name) ?? [];
    const modelName = _toPascal(table.name);
    out += `model ${modelName} {\n`;
    for (const col of cols) {
      const isPk  = pkSet.has(`${table.name}.${col.column_name}`);
      const isNull = col.is_nullable === 'YES' && !isPk;
      const isUniq = uniqueSet.has(`${table.name}.${col.column_name}`) && !isPk;
      const prismaType = (PG_TO_PRISMA[col.data_type] ?? PG_TO_PRISMA[col.udt_name] ?? 'String') + (isNull ? '?' : '');
      const fieldName  = _toCamel(col.column_name);
      const attrs: string[] = [];
      if (isPk) attrs.push('@id');
      if (col.column_default?.includes('nextval(')) attrs.push('@default(autoincrement())');
      else if (col.column_default === 'gen_random_uuid()' || col.column_default?.includes('uuid_generate')) attrs.push('@default(uuid())');
      else if (col.column_default?.includes('now()') || col.column_default?.includes('CURRENT_TIMESTAMP')) attrs.push('@default(now())');
      if (isUniq) attrs.push('@unique');
      if (fieldName !== col.column_name) attrs.push(`@map("${col.column_name}")`);
      out += `  ${fieldName.padEnd(24)}${prismaType.padEnd(12)}${attrs.length ? attrs.join(' ') : ''}\n`;
    }
    out += `\n  @@map("${table.name}")\n}\n\n`;
  }
  return out.trimEnd();
}

// ── Import table data (#66) ───────────────────────────────────────────────────

export async function importTableRows(
  driver: PgDriver,
  schema: string,
  table: string,
  columns: string[],
  rows: (string | null)[][]
): Promise<number> {
  if (!rows.length) return 0;
  const qSchema = schema.replace(/"/g, '""');
  const qTable  = table.replace(/"/g, '""');
  const colList = columns.map(c => `"${c.replace(/"/g, '""')}"`).join(', ');
  let inserted = 0;
  const CHUNK = 500;
  for (let i = 0; i < rows.length; i += CHUNK) {
    const chunk = rows.slice(i, i + CHUNK);
    const placeholders = chunk.map((_, ri) =>
      '(' + columns.map((_, ci) => `$${ri * columns.length + ci + 1}`).join(', ') + ')'
    ).join(', ');
    await driver.query(
      `INSERT INTO "${qSchema}"."${qTable}" (${colList}) VALUES ${placeholders}`,
      chunk.flat()
    );
    inserted += chunk.length;
  }
  return inserted;
}

export async function checkMigrationsTable(driver: PgDriver, schema = 'public'): Promise<boolean> {
  const rows = await driver.query<{ exists: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM information_schema.tables
       WHERE table_schema = $1 AND table_name = '_prisma_migrations'
     ) AS exists`,
    [schema]
  );
  return Boolean(rows[0]?.exists);
}

export async function getMigrations(driver: PgDriver, schema = 'public'): Promise<MigrationEntry[]> {
  const rows = await driver.query<{
    id: string;
    migration_name: string;
    started_at: Date;
    finished_at: Date | null;
    rolled_back_at: Date | null;
    logs: string | null;
    applied_steps_count: number;
  }>(
    `SELECT id, migration_name, started_at, finished_at, rolled_back_at, logs, applied_steps_count
     FROM "${schema}"._prisma_migrations
     ORDER BY started_at DESC`
  );
  return rows.map(r => ({
    id: r.id,
    migrationName: r.migration_name,
    startedAt: r.started_at instanceof Date ? r.started_at.toISOString() : String(r.started_at),
    finishedAt: r.finished_at instanceof Date ? r.finished_at.toISOString() : (r.finished_at ? String(r.finished_at) : null),
    rolledBackAt: r.rolled_back_at instanceof Date ? r.rolled_back_at.toISOString() : (r.rolled_back_at ? String(r.rolled_back_at) : null),
    logs: r.logs,
    appliedStepsCount: Number(r.applied_steps_count),
  }));
}

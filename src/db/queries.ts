import { PgDriver } from './PgDriver';

export interface SchemaInfo {
  name: string;
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

export async function previewTable(
  driver: PgDriver,
  schema: string,
  table: string
): Promise<{ columns: string[]; rows: Record<string, unknown>[]; estimate: number }> {
  const qSchema = schema.replace(/"/g, '""');
  const qTable  = table.replace(/"/g, '""');

  const rows = await driver.query(`SELECT * FROM "${qSchema}"."${qTable}" LIMIT 100`);

  const est = await driver.query<{ estimate: number }>(
    `SELECT reltuples::bigint AS estimate
     FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE n.nspname = $1 AND c.relname = $2`,
    [schema, table]
  );

  return {
    columns: rows.length > 0 ? Object.keys(rows[0]) : [],
    rows,
    estimate: Number(est[0]?.estimate ?? 0),
  };
}

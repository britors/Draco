export interface PgConnection {
  id: string;
  label: string;
  host: string;
  port: number;
  database: string;
  user: string;
  ssl: boolean;
}

export function validateConnection(conn: Partial<PgConnection>): string[] {
  const errors: string[] = [];
  if (!conn.label?.trim()) errors.push('Label is required');
  if (!conn.host?.trim()) errors.push('Host is required');
  if (!conn.database?.trim()) errors.push('Database is required');
  if (!conn.user?.trim()) errors.push('User is required');
  const port = Number(conn.port);
  if (!port || port < 1 || port > 65535) errors.push('Port must be between 1 and 65535');
  return errors;
}

export function defaultConnection(): Omit<PgConnection, 'id'> {
  return { label: '', host: 'localhost', port: 5432, database: '', user: '', ssl: false };
}

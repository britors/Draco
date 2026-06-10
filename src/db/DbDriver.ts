export interface DbDriver {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  query<T extends Record<string, unknown> = Record<string, unknown>>(
    sql: string,
    params?: unknown[]
  ): Promise<T[]>;
  cancelActive?(): Promise<void>;
  readonly isConnected: boolean;
}

import pg from 'pg';

const num = (v: string | undefined, d: number) => (v ? parseInt(v, 10) : d);

export const dbConfig = {
  user: process.env.POSTGRES_USER || 'contract',
  password: process.env.POSTGRES_PASSWORD || 'contractpw',
  host: process.env.POSTGRES_HOST || 'localhost',
  database: process.env.POSTGRES_DB || 'folio_db',
  port: num(process.env.POSTGRES_PORT, 5432),
};

export const pool = new pg.Pool({
  ...dbConfig,
  max: 20,
  idleTimeoutMillis: 30_000,
});

export const query = <T extends pg.QueryResultRow = pg.QueryResultRow>(
  text: string,
  params?: unknown[],
) => pool.query<T>(text, params as never);

let controlReadPool: pg.Pool | null = null;
let controlWritePool: pg.Pool | null = null;

function configuredControlWriteConnection(): string | null {
  const connectionString = process.env.ERPX_CONTROL_WRITE_DATABASE_URL?.trim();
  return connectionString || null;
}

function assertControlDatabaseTransport(connectionString: string, label: string): void {
  if (process.env.NODE_ENV === 'production' && !/[?&]sslmode=verify-full(?:&|$)/i.test(connectionString)) {
    throw new Error(`${label} must use PostgreSQL TLS verify-full`);
  }
}

/**
 * Read-side access for ERPX control-plane projections. The primary Folio pool
 * is never an implicit ERPX read fallback. A developer may explicitly enable a
 * local, non-production break-glass read with ERPX_LOCAL_BREAK_GLASS_READ; a
 * production process must always point this at the isolated control read
 * store.
 */
export function controlReadQuery<T extends pg.QueryResultRow = pg.QueryResultRow>(
  text: string,
  params?: unknown[],
) {
  const connectionString = process.env.ERPX_CONTROL_READ_DATABASE_URL;
  if (!connectionString) {
    if (process.env.NODE_ENV !== 'production' && process.env.ERPX_LOCAL_BREAK_GLASS_READ === 'true') {
      return pool.query<T>(text, params as never);
    }
    return Promise.reject(new Error('ERPX_CONTROL_READ_DATABASE_URL is required'));
  }
  try {
    assertControlDatabaseTransport(connectionString, 'ERPX_CONTROL_READ_DATABASE_URL');
  } catch (error) {
    return Promise.reject(error);
  }
  controlReadPool ??= new pg.Pool({ connectionString, max: 8, idleTimeoutMillis: 30_000 });
  return controlReadPool.query<T>(text, params as never);
}

/**
 * Explicit ERPX control-plane write access. This is never a fallback to the
 * Folio pool: Folio remains the command owner until an explicitly approved
 * cutover, while the bridge can publish durable shadow events when configured.
 */
export function controlWriteQuery<T extends pg.QueryResultRow = pg.QueryResultRow>(
  text: string,
  params?: unknown[],
) {
  const connectionString = configuredControlWriteConnection();
  if (!connectionString) return Promise.reject(new Error('ERPX_CONTROL_WRITE_DATABASE_URL is required'));
  try {
    assertControlDatabaseTransport(connectionString, 'ERPX_CONTROL_WRITE_DATABASE_URL');
  } catch (error) {
    return Promise.reject(error);
  }
  controlWritePool ??= new pg.Pool({ connectionString, max: 8, idleTimeoutMillis: 30_000 });
  return controlWritePool.query<T>(text, params as never);
}

export async function withControlWriteTransaction<T>(
  fn: (q: <R extends pg.QueryResultRow = pg.QueryResultRow>(text: string, params?: unknown[]) => Promise<pg.QueryResult<R>>) => Promise<T>,
): Promise<T> {
  const connectionString = configuredControlWriteConnection();
  if (!connectionString) throw new Error('ERPX_CONTROL_WRITE_DATABASE_URL is required');
  assertControlDatabaseTransport(connectionString, 'ERPX_CONTROL_WRITE_DATABASE_URL');
  controlWritePool ??= new pg.Pool({ connectionString, max: 8, idleTimeoutMillis: 30_000 });
  const client = await controlWritePool.connect();
  try {
    await client.query('BEGIN');
    const wrapped = (<R extends pg.QueryResultRow = pg.QueryResultRow>(text: string, params?: unknown[]): Promise<pg.QueryResult<R>> =>
      client.query<R>(text, params as never)) as <R extends pg.QueryResultRow = pg.QueryResultRow>(text: string, params?: unknown[]) => Promise<pg.QueryResult<R>>;
    const result = await fn(wrapped);
    await client.query('COMMIT');
    return result;
  } catch (err) {
    await client.query('ROLLBACK').catch(() => {});
    throw err;
  } finally {
    client.release();
  }
}

export async function withTransaction<T>(
  fn: (q: <R extends pg.QueryResultRow = pg.QueryResultRow>(text: string, params?: unknown[]) => Promise<pg.QueryResult<R>>) => Promise<T>,
): Promise<T> {
  const client = await pool.connect();
  try {
    await client.query('BEGIN');
    const wrapped = (<R extends pg.QueryResultRow = pg.QueryResultRow>(text: string, params?: unknown[]): Promise<pg.QueryResult<R>> =>
      client.query<R>(text, params as never)) as <R extends pg.QueryResultRow = pg.QueryResultRow>(text: string, params?: unknown[]) => Promise<pg.QueryResult<R>>;
    const result = await fn(wrapped);
    await client.query('COMMIT');
    return result;
  } catch (err) {
    await client.query('ROLLBACK').catch(() => {});
    throw err;
  } finally {
    client.release();
  }
}

let readonlyPool: pg.Pool | null = null;

export function getReadOnlyPool(): pg.Pool {
  if (readonlyPool) return readonlyPool;
  readonlyPool = new pg.Pool({
    host: dbConfig.host,
    port: dbConfig.port,
    database: dbConfig.database,
    user: 'folio_readonly_agent',
    password: process.env.READONLY_AGENT_PW || 'agent_readonly_pw_change_me',
    max: 4,
    idleTimeoutMillis: 30_000,
    connectionTimeoutMillis: 5_000,
  });
  return readonlyPool;
}

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
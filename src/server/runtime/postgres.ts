import { Pool, types, type PoolClient, type PoolConfig, type QueryResult } from 'pg';
import type { SqlDatabase, SqlResult, SqlStatement } from './types';
import { currentReadMetrics } from './read-metrics';

// The business API uses exact safe integers and serialized JSON. Reject overflow,
// rather than silently rounding PostgreSQL bigint/numeric values.
export function safeNumber(value: string): number {
  const number = Number(value);
  if (!Number.isFinite(number) || (Number.isInteger(number) && !Number.isSafeInteger(number)))
    throw new RangeError('DATABASE_NUMBER_OUT_OF_RANGE');
  return number;
}
types.setTypeParser(types.builtins.INT8, safeNumber);
types.setTypeParser(types.builtins.NUMERIC, safeNumber);
types.setTypeParser(types.builtins.JSON, value => value);
types.setTypeParser(types.builtins.JSONB, value => value);
types.setTypeParser(types.builtins.DATE, value => value);

/** Number positional parameters without touching quoted text, comments or identifiers. */
export function parameters(sql: string, count: number): string {
  let result = '', index = 0;
  for (let i = 0; i < sql.length;) {
    const rest = sql.slice(i);
    const token = rest.match(/^(?:'(?:''|[^'])*'|"(?:""|[^"])*"|--[^\n]*(?:\n|$)|\/\*[\s\S]*?\*\/|\$([a-zA-Z_][\w]*)?\$[\s\S]*?\$\1\$)/);
    if (token) { result += token[0]; i += token[0].length; continue; }
    const identifier = rest.match(/^[a-zA-Z_][a-zA-Z_0-9]*/)?.[0];
    if (identifier) {
      result += /^[a-z_].*[A-Z]/.test(identifier) || identifier === 'user' ? `"${identifier}"` : identifier;
      i += identifier.length; continue;
    }
    result += sql[i] === '?' ? `$${++index}` : sql[i]; i++;
  }
  if (index !== count) throw new Error(`SQL_PARAMETER_COUNT:${index}:${count}`);
  return result;
}
function values(input: unknown[]) {
  return input.map(value => {
    if (typeof value === 'number' && (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))))
      throw new RangeError('DATABASE_NUMBER_OUT_OF_RANGE');
    if (value === undefined) return null;
    if (value !== null && !['string','number','boolean'].includes(typeof value) && !(value instanceof Date) && !(value instanceof Uint8Array)) throw new TypeError('INVALID_SQL_VALUE');
    return value instanceof Uint8Array ? Buffer.from(value) : value;
  });
}
function result<T>(query: QueryResult, start: number): SqlResult<T> {
  return { results: query.rows as T[], success: true, meta: { changes: query.rowCount ?? 0, duration: performance.now() - start } };
}
class Statement implements SqlStatement {
  constructor(readonly database: { pool: Pool | PoolClient }, readonly sql: string, readonly args: unknown[] = []) {}
  bind(...args: unknown[]) { return new Statement(this.database, this.sql, args); }
  async execute(client: Pool | PoolClient): Promise<QueryResult> {
    const sql = parameters(this.sql, this.args.length), args = values(this.args);
    const metrics = currentReadMetrics();
    if (!metrics) return client.query(sql, args);
    const started = performance.now();
    const leased = client instanceof Pool ? await client.connect() : null;
    metrics.poolWaitMs += performance.now() - started;
    const execution = performance.now();
    metrics.queries++;
    try { return await (leased ?? client).query(sql, args); }
    finally { metrics.queryMs += performance.now() - execution; leased?.release(); }
  }
  async all<T = Record<string, unknown>>() { const start = performance.now(); return result<T>(await this.execute(this.database.pool), start); }
  run<T = Record<string, unknown>>() { return this.all<T>(); }
  async first<T = Record<string, unknown>>(column?: string): Promise<T | null> {
    const row = (await this.execute(this.database.pool)).rows[0];
    return row === undefined ? null : column === undefined ? row as T : row[column] as T;
  }
  async raw<T = unknown[]>(options?: { columnNames?: boolean }): Promise<T[]> {
    const query = await this.execute(this.database.pool);
    const names = query.fields.map(field => field.name);
    const rows = query.rows.map(row => names.map(name => row[name]));
    return (options?.columnNames ? [names, ...rows] : rows) as T[];
  }
}
export class PostgresDatabase implements SqlDatabase {
  readonly pool: Pool;
  constructor(config: PoolConfig) {
    this.pool = new Pool({ max: 1, idleTimeoutMillis: 30_000, connectionTimeoutMillis: 5_000,
      options: '-c timezone=UTC -c statement_timeout=30000 -c lock_timeout=10000', ...config });
    this.pool.on('error', () => console.error(JSON.stringify({ event: 'database_connection_error' })));
  }
  prepare(sql: string) { return new Statement(this, sql); }
  /** Pin a connection for a session advisory lock without holding a transaction
   * across external work. Each batch still commits independently on this client. */
  async withSession<T>(action: (session: SqlDatabase) => Promise<T>): Promise<T> {
    const client = await this.pool.connect();
    try { return await action(new SessionDatabase(client)); }
    finally { client.release(); }
  }
  /** Reads and derived writes share the same snapshot; retry the entire calculation. */
  async transaction<T>(action: (tx: SqlDatabase) => Promise<T>): Promise<T> {
    for (let attempt = 0; ; attempt++) {
      const waiting = performance.now();
      const client = await this.pool.connect();
      const metrics = currentReadMetrics();
      if (metrics) metrics.poolWaitMs += performance.now() - waiting;
      try {
        await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
        const tx = new TransactionDatabase(client);
        const value = await action(tx);
        await client.query('COMMIT');
        return value;
      } catch (error) {
        await client.query('ROLLBACK').catch(() => {});
        if (attempt >= 3 || !['40001', '40P01'].includes((error as { code?: string }).code ?? '')) throw error;
      } finally { client.release(); }
    }
  }
  async batch<T = Record<string, unknown>>(statements: SqlStatement[]): Promise<SqlResult<T>[]> {
    if (statements.some(s => !(s instanceof Statement) || s.database !== this)) throw new Error('CROSS_DATABASE_TRANSACTION');
    for (let attempt = 0; ; attempt++) {
      const waiting = performance.now();
      const client = await this.pool.connect();
      const metrics = currentReadMetrics();
      if (metrics) metrics.poolWaitMs += performance.now() - waiting;
      try {
        await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
        const results: SqlResult<T>[] = [];
        for (const statement of statements as Statement[]) {
          const start = performance.now();
          results.push(result<T>(await statement.execute(client), start));
        }
        await client.query('COMMIT'); return results;
      } catch (error) {
        await client.query('ROLLBACK').catch(() => {});
        if (attempt >= 3 || !['40001', '40P01'].includes((error as { code?: string }).code ?? '')) throw error;
      } finally { client.release(); }
      await new Promise(resolve => setTimeout(resolve, 10 * 2 ** attempt));
    }
  }
  async exec(sql: string) {
    const start = performance.now(), results = await this.pool.query(sql);
    return { count: Array.isArray(results) ? results.length : 1, duration: performance.now() - start };
  }
  close() { return this.pool.end(); }
}

class TransactionDatabase implements SqlDatabase {
  constructor(readonly pool: PoolClient) {}
  prepare(sql: string) { return new Statement(this, sql); }
  async batch<T = Record<string, unknown>>(statements: SqlStatement[]): Promise<SqlResult<T>[]> {
    const results: SqlResult<T>[] = [];
    for (const statement of statements) {
      if (!(statement instanceof Statement) || statement.database !== this) throw new Error('CROSS_DATABASE_TRANSACTION');
      const start = performance.now();
      results.push(result<T>(await statement.execute(this.pool), start));
    }
    return results;
  }
  async exec(sql: string) {
    const start = performance.now();
    const queries = await this.pool.query(sql);
    return { count: Array.isArray(queries) ? queries.length : 1, duration: performance.now() - start };
  }
}

class SessionDatabase extends TransactionDatabase {
  override async batch<T = Record<string, unknown>>(statements: SqlStatement[]): Promise<SqlResult<T>[]> {
    for (let attempt = 0; ; attempt++) {
      try {
        await this.pool.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
        const results = await super.batch<T>(statements);
        await this.pool.query('COMMIT');
        return results;
      } catch (error) {
        await this.pool.query('ROLLBACK').catch(() => {});
        if (attempt >= 3 || !['40001', '40P01'].includes((error as { code?: string }).code ?? '')) throw error;
      }
    }
  }
}

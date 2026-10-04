import { Pool, types, type PoolClient, type PoolConfig, type QueryResult } from 'pg';
import { errorMonitor, type EventEmitter } from 'node:events';
import type { SqlDatabase, SqlResult, SqlStatement } from './types';
import { currentReadMetrics, recordReadPoolWait, recordReadQuery, runWithReadMetrics } from './read-metrics';

const instrumented = Symbol.for('erpx.postgres.readMetrics');
const transactionOwnership = Symbol.for('erpx.postgres.transactionOwnership');
type TransactionOwnership = { pool: Pool; active: boolean };
const runtimeGlobal = globalThis as typeof globalThis & { [transactionOwnership]?: WeakMap<SqlDatabase, TransactionOwnership> };
// A retained pool and an active callback keep their identity across dev reloads.
// Raw client adapters and pinned sessions never receive this capability.
const transactionOwners = runtimeGlobal[transactionOwnership] ??= new WeakMap<SqlDatabase, TransactionOwnership>();
type Marked = { [instrumented]?: boolean };
type Callback = (this: unknown, ...args: unknown[]) => unknown;

/** Instrument at the driver boundary so raw Drizzle/auth queries and SQL control
 * statements are measured exactly once, alongside the application's statements.
 * Queries keep their arguments, return value, callback and release behavior. */
function instrumentClient(client: PoolClient) {
  const marked = client as PoolClient & Marked;
  if (marked[instrumented]) return;
  Object.defineProperty(marked, instrumented, { value: true });
  const original = client.query;
  client.query = function(this: PoolClient, ...input: unknown[]) {
    const scope = currentReadMetrics();
    if (!scope) return Reflect.apply(original, this, input);
    const started = performance.now();
    let settled = false;
    let cleanup = () => {};
    const finish = (failed: boolean) => {
      if (settled) return;
      settled = true;
      cleanup();
      recordReadQuery(scope, performance.now() - started, failed);
    };
    const callback = (source: Callback): Callback => function(this: unknown, ...args: unknown[]) {
      finish(!!args[0]);
      return runWithReadMetrics(scope, () => Reflect.apply(source, this, args));
    };
    const args = [...input];
    const callbackIndex = args.length - 1;
    if (callbackIndex > 0 && typeof args[callbackIndex] === 'function') {
      args[callbackIndex] = callback(args[callbackIndex] as Callback);
    } else if (args[0] && typeof args[0] === 'object' && 'callback' in args[0] && typeof args[0].callback === 'function') {
      const config = args[0];
      if ('submit' in config) {
        // pg mutates Query callbacks too (e.g. query_timeout). Keep Query identity.
        const source = config.callback as Callback;
        const wrapped = callback(source);
        config.callback = wrapped;
        cleanup = () => { if (config.callback === wrapped) config.callback = source; };
      } else {
        args[0] = Object.assign(Object.create(config), { callback: callback(config.callback as Callback) });
      }
    }
    try {
      const returned: unknown = Reflect.apply(original, this, args);
      if (returned && typeof returned === 'object' && 'then' in returned && typeof returned.then === 'function') {
        // Observe the original promise; keep the driver's Promise implementation.
        void (returned as Promise<unknown>).then(() => finish(false), () => finish(true));
      } else if (returned && typeof returned === 'object' && 'once' in returned && typeof returned.once === 'function') {
        const emitter = returned as EventEmitter;
        const priorCleanup = cleanup;
        const onEnd = () => finish(false), onError = () => finish(true);
        cleanup = () => {
          priorCleanup();
          emitter.removeListener('end', onEnd);
          emitter.removeListener(errorMonitor, onError);
        };
        emitter.once('end', onEnd);
        // Observing errors must not turn an unhandled stream error into success.
        emitter.once(errorMonitor, onError);
      }
      return returned;
    } catch (error) {
      finish(true);
      throw error;
    }
  } as PoolClient['query'];
}

function instrumentPool(pool: Pool) {
  const marked = pool as Pool & Marked;
  if (marked[instrumented]) return;
  Object.defineProperty(marked, instrumented, { value: true });
  pool.on('acquire', instrumentClient);
  const original = pool.connect;
  pool.connect = function(this: Pool, callback?: Callback) {
    const scope = currentReadMetrics();
    if (!scope && !callback) return Reflect.apply(original, this, []);
    const started = scope ? performance.now() : 0;
    let settled = false;
    const finish = (failed: boolean) => {
      if (settled) return;
      settled = true;
      if (scope) recordReadPoolWait(scope, performance.now() - started, failed);
    };
    try {
      if (callback) return Reflect.apply(original, this, [function(this: unknown, ...args: unknown[]) {
        finish(!!args[0]);
        return runWithReadMetrics(scope, () => Reflect.apply(callback, this, args));
      }]);
      const returned = Reflect.apply(original, this, []) as Promise<PoolClient>;
      void returned.then(() => finish(false), () => finish(true));
      return returned;
    } catch (error) {
      finish(true);
      throw error;
    }
  } as Pool['connect'];
}

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
    return client.query(sql, args);
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
    instrumentPool(this.pool);
    this.pool.on('error', () => console.error(JSON.stringify({ event: 'database_connection_error' })));
  }
  prepare(sql: string) { return new Statement(this, sql); }
  assertActiveTransaction(database: SqlDatabase) {
    const ownership = transactionOwners.get(database);
    if (!ownership?.active || ownership.pool !== this.pool) throw new Error('TRANSACTION_NOT_ACTIVE_OR_OWNED');
  }
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
      const client = await this.pool.connect();
      try {
        await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
        const tx = new TransactionDatabase(client);
        const ownership = { pool: this.pool, active: true };
        transactionOwners.set(tx, ownership);
        let value: T;
        try { value = await action(tx); }
        finally { ownership.active = false; }
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
      const client = await this.pool.connect();
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

/** Adapt an owner's already-open PostgreSQL transaction; this never begins,
 * commits, rolls back or releases the client on the owner's behalf. */
export function transactionDatabase(client: PoolClient): SqlDatabase {
  return new TransactionDatabase(client);
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

/** HMR keeps runtime database instances and pools. Refresh the methods as well as
 * the pool: pre-instrumentation Statement methods counted queries themselves. */
export function refreshPostgresReadInstrumentation(database: PostgresDatabase) {
  instrumentPool(database.pool);
  Object.setPrototypeOf(database, PostgresDatabase.prototype);
}
const retained = (globalThis as typeof globalThis & { erpxDevRuntime?: { env: Record<string, unknown> } }).erpxDevRuntime;
if (process.env.NODE_ENV === 'development' && retained) {
  for (const database of Object.values(retained.env)) {
    if (database && typeof database === 'object' && 'pool' in database && 'prepare' in database && 'close' in database &&
      typeof database.prepare === 'function' && typeof database.close === 'function') {
      refreshPostgresReadInstrumentation(database as PostgresDatabase);
    }
  }
}

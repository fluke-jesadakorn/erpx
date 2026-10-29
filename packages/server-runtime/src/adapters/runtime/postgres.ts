import { Pool, types, type PoolClient, type PoolConfig, type QueryResult } from 'pg';
import { errorMonitor, type EventEmitter } from 'node:events';
import type { SqlDatabase, SqlResult, SqlStatement } from "@erpx/server-runtime/adapters/runtime/types";
import { currentReadMetrics, recordReadPoolWait, recordReadQuery, recordReadLockQuery, recordReadTransactionRetry, runWithReadMetrics } from "@erpx/server-runtime/adapters/runtime/read-metrics";

const instrumented = Symbol.for('erpx.postgres.readMetrics');
const transactionOwnership = Symbol.for('erpx.postgres.transactionOwnership');
type TransactionOwnership = { pool: Pool; active: boolean };
const runtimeGlobal = globalThis as typeof globalThis & { [transactionOwnership]?: WeakMap<SqlDatabase, TransactionOwnership> };
// A retained pool and an active callback keep their identity across dev reloads.
// Raw client adapters and pinned sessions never receive this capability.
const transactionOwners = runtimeGlobal[transactionOwnership] ??= new WeakMap<SqlDatabase, TransactionOwnership>();
// Waiting for a local aggregate must not occupy every pool connection.
// These empty promises schedule capacity only: the PostgreSQL session lock,
// scoped owner row lock and SERIALIZABLE snapshot still authorize each write.
const poolLockWaiters = new WeakMap<Pool, Map<string, Promise<void>>>();
async function acquirePoolLockTurn(pool: Pool, lock: {namespace:string;key:string}): Promise<() => void> {
  let waiters=poolLockWaiters.get(pool);
  if(!waiters){waiters=new Map();poolLockWaiters.set(pool,waiters);}
  const key=JSON.stringify([lock.namespace,lock.key]),previous=waiters.get(key);
  let resolve!:()=>void;
  const current=new Promise<void>(done=>{resolve=done;});
  waiters.set(key,current);
  const leave=()=>{if(waiters.get(key)===current)waiters.delete(key);resolve();};
  let timer:ReturnType<typeof setTimeout>|undefined;
  try {
    if(previous)await Promise.race([previous,new Promise<never>((_,reject)=>{
      // Bound local capacity waiting like the standard runtime's 10s lock wait.
      timer=setTimeout(()=>reject(Object.assign(new Error('Local transaction lock wait timed out'),{code:'55P03'})),10_000);
    })]);
    return leave;
  } catch(error) {
    // An expired middle waiter must neither bypass nor retain its predecessor.
    void previous!.then(leave);
    throw error;
  } finally {if(timer!==undefined)clearTimeout(timer);}
}
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
    const sqlText=typeof input[0]==='string'?input[0]:input[0]&&typeof input[0]==='object'&&'text' in input[0]?String(input[0].text):'';
    const locking=/\bFOR\s+(?:(?:NO\s+KEY|KEY)\s+)?(?:UPDATE|SHARE)\b|\bpg_(?:try_)?advisory_(?:xact_)?lock\s*\(/i.test(sqlText);
    let settled = false;
    let cleanup = () => {};
    const finish = (failed: boolean) => {
      if (settled) return;
      settled = true;
      cleanup();
      const duration=performance.now()-started;
      recordReadQuery(scope, duration, failed);
      if(locking)recordReadLockQuery(scope,duration,failed);
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
  compiled() { return { sql: parameters(this.sql, this.args.length), args: values(this.args) }; }
  async execute(client: Pool | PoolClient): Promise<QueryResult> {
    const { sql, args } = this.compiled();
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
    this.pool = new Pool({ pipeline: true, max: 1, idleTimeoutMillis: 30_000, connectionTimeoutMillis: 5_000,
      // These pools serve bounded OLTP reads/writes. Complex authorization predicates
      // can trigger expensive LLVM compilation before returning a handful of rows.
      // Keep SQL/authority/timeouts unchanged; explicit operator options still override.
      options: '-c timezone=UTC -c statement_timeout=30000 -c lock_timeout=10000 -c jit=off', ...config });
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
  async transaction<T>(action: (tx: SqlDatabase) => Promise<T>,options?:{sessionLock:{namespace:string;key:string};preferIndexedReads?:boolean;pipelineBatches?:boolean}): Promise<T> {
    const lock=options?.sessionLock;
    if(lock&&(!lock.namespace||lock.namespace.length>100||!lock.key||lock.key.length>200))throw new Error('INVALID_TRANSACTION_SESSION_LOCK');
    for (let attempt = 0; ; attempt++) {
      const leave=lock?await acquirePoolLockTurn(this.pool,lock):undefined;
      try {
        const client = await this.pool.connect();
        let locked=false,discard=false;
        try {
          // Coordinate a busy aggregate before establishing the serializable
          // snapshot. Its owner still takes the authoritative row lock inside.
          if(lock){await client.query('SELECT pg_advisory_lock(hashtext($1),hashtext($2))',[lock.namespace,lock.key]);locked=true;}
          // Prefer fine-grained indexed predicate locks for an owner's selected
          // transaction. PostgreSQL can still scan when no indexed path exists;
          // SET LOCAL cannot leak this preference through the pooled connection.
          await client.query(options?.preferIndexedReads
            ? 'BEGIN ISOLATION LEVEL SERIALIZABLE; SET LOCAL enable_seqscan=off'
            : 'BEGIN ISOLATION LEVEL SERIALIZABLE');
          const tx = new TransactionDatabase(client, options?.pipelineBatches);
          const ownership = { pool: this.pool, active: true };
          transactionOwners.set(tx, ownership);
          let value: T;
          try { value = await action(tx); }
          finally { ownership.active = false; }
          await client.query('COMMIT');
          return value;
        } catch (error) {
          await client.query('ROLLBACK').catch(() => {});
          recordReadTransactionRetry(error,attempt < 3);
          if (attempt >= 3 || !['40001', '40P01'].includes((error as { code?: string }).code ?? '')) throw error;
        } finally {
          if(locked)try{
            const released=await client.query('SELECT pg_advisory_unlock(hashtext($1),hashtext($2)) released',[lock!.namespace,lock!.key]);
            discard=released.rows[0]?.released!==true;
          }catch{discard=true;}
          // Never return a connection whose session lock could remain held.
          client.release(discard);
        }
      } finally {leave?.();}
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
        recordReadTransactionRetry(error,attempt < 3);
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
  constructor(readonly pool: PoolClient, readonly pipelineBatches = false) {}
  prepare(sql: string) { return new Statement(this, sql); }
  async batch<T = Record<string, unknown>>(statements: SqlStatement[]): Promise<SqlResult<T>[]> {
    // Validate every statement before dispatch. A caller may catch an input
    // error inside its owner transaction; that must not commit a partial batch.
    if (statements.some(statement => !(statement instanceof Statement) || statement.database !== this))
      throw new Error('CROSS_DATABASE_TRANSACTION');
    const prepared = (statements as Statement[]).map(statement => statement.compiled());
    if (this.pipelineBatches && this.pool.pipeline) {
      // pg processes this checked-out client's queries in submission order.
      // Settle all responses before returning an error, rollback or release;
      // preserve the first SQL error rather than a later aborted-tx error.
      const settled = await Promise.allSettled(prepared.map(async ({sql,args}) => {
        const start = performance.now();
        return result<T>(await this.pool.query(sql,args),start);
      }));
      const failure = settled.find(value => value.status === 'rejected');
      if (failure?.status === 'rejected') throw failure.reason;
      return settled.map(value => (value as PromiseFulfilledResult<SqlResult<T>>).value);
    }
    const results: SqlResult<T>[] = [];
    for (const {sql,args} of prepared) {
      const start = performance.now();
      results.push(result<T>(await this.pool.query(sql,args),start));
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
        recordReadTransactionRetry(error,attempt < 3);
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

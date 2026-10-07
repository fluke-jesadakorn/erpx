import { afterEach, describe, expect, it, vi } from 'vitest';
import { Query, type PoolClient } from 'pg';
import { sql } from 'drizzle-orm';
import { drizzle } from 'drizzle-orm/node-postgres';
import { env } from './runtime-env';
import { PostgresDatabase, refreshPostgresReadInstrumentation } from '../src/server/runtime/postgres';
import { currentReadMetrics, measureReadPhase, measureReads, type ReadMeasurement } from '../src/server/runtime/read-metrics';

const temporary: PostgresDatabase[] = [];
afterEach(async () => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
  await Promise.all(temporary.splice(0).map(database => database.close()));
});
function singleConnection(timeout = 5_000) {
  const database = new PostgresDatabase({ ...env.H001_DB.pool.options, max: 1, connectionTimeoutMillis: timeout });
  temporary.push(database);
  return database;
}

describe('request read metrics at the PostgreSQL driver boundary', () => {
  it('counts raw pool/Drizzle and Statement queries once, preserving configured rows and parameters', async () => {
    const database = singleConnection();
    const measured = await measureReads(async () => {
      expect(await database.prepare('SELECT ?::int value').bind(7).first('value')).toBe(7);
      expect((await database.pool.query({ text: 'SELECT $1::int value', values: [8], rowMode: 'array' })).rows).toEqual([[8]]);
      expect((await database.pool.query('SELECT $1::text value', ['private-parameter'])).rows[0].value).toBe('private-parameter');
      expect((await database.exec('SELECT 1; SELECT 2')).count).toBe(2);
      expect((await drizzle(database.pool).execute(sql`SELECT 9::int value`)).rows[0].value).toBe(9);
    });
    expect(measured.metrics.queries).toBe(5);
    expect(measured.metrics.queryMs).toBeGreaterThan(0);
    expect(measured.metrics.poolWaitMs).toBeGreaterThan(0);
    expect(measured.metrics.queryErrors).toBe(0);
    expect(JSON.stringify(measured.metrics)).not.toContain('private-parameter');
    expect(database.pool.idleCount).toBe(database.pool.totalCount);
  });

  it('preserves callbacks, callback configs and event Query identity', async () => {
    const database = singleConnection();
    const measured = await measureReads(async () => {
      await new Promise<void>((resolve, reject) => database.pool.query('SELECT 1 value', (error, result) => {
        if (error) return reject(error);
        expect(result.rows[0].value).toBe(1);
        resolve();
      }));
      await new Promise<void>((resolve, reject) => database.pool.query('SELECT $1::int value', [2], (error, result) => {
        if (error) return reject(error);
        expect(result.rows[0].value).toBe(2);
        resolve();
      }));
      const client = await database.pool.connect();
      try {
        await new Promise<void>((resolve, reject) => {
          const config = { text: 'SELECT 3 value', callback: (error: Error | null, result: { rows: { value: number }[] }) => {
            if (error) return reject(error);
            expect(result.rows[0].value).toBe(3);
            resolve();
          } };
          const source = config.callback;
          client.query(config);
          expect(config.callback).toBe(source);
        });
        const stream = new Query('SELECT 4 value');
        await new Promise<void>((resolve, reject) => {
          stream.once('error', reject);
          stream.once('end', resolve);
          expect(client.query(stream)).toBe(stream);
        });
        const callbackQuery = Object.assign(new Query('SELECT 5 value'), { callback: (_error: Error | null, _result: { rows: {value:number}[] }) => {void _error;void _result;} });
        await new Promise<void>((resolve, reject) => {
          callbackQuery.callback = (error, result) => {
            if (error) return reject(error);
            expect(result.rows[0].value).toBe(5);
            resolve();
          };
          expect(client.query(callbackQuery)).toBe(callbackQuery);
        });
      } finally { client.release(); }
    });
    expect(measured.metrics.queries).toBe(5);
    expect(measured.metrics.queryErrors).toBe(0);
  });

  it('aggregates nested and concurrent phases into their ancestor without mixing sibling queries', async () => {
    const database = singleConnection();
    const outer = await measureReads(async () => {
      await database.pool.query('SELECT 1');
      return Promise.all([
        measureReads(async () => {
          await database.pool.query('SELECT pg_sleep(0.02)');
          return measureReads(() => database.prepare('SELECT 2').all());
        }),
        measureReads(() => database.pool.query('SELECT 3')),
      ]);
    });
    expect(outer.metrics.queries).toBe(4);
    expect(outer.result[0].metrics.queries).toBe(2);
    expect(outer.result[0].result.metrics.queries).toBe(1);
    expect(outer.result[1].metrics.queries).toBe(1);
    expect(outer.metrics.queryMs).toBeGreaterThanOrEqual(outer.result[0].metrics.queryMs + outer.result[1].metrics.queryMs);
    expect(currentReadMetrics()).toBeUndefined();

    // pg pool dispatches queued callbacks while the other scope releases a client.
    const independent = await Promise.all([
      measureReads(() => database.pool.query('SELECT pg_sleep(0.02)')),
      measureReads(() => database.pool.query('SELECT 4')),
      measureReads(() => database.pool.query('SELECT 5')),
    ]);
    expect(independent.map(value => value.metrics.queries)).toEqual([1, 1, 1]);
    expect(independent[1].metrics.poolWaitMs).toBeGreaterThan(5);
  });

  it('counts session and transaction control statements and keeps commit/rollback/release behavior', async () => {
    const database = singleConnection();
    const success = await measureReads(() => database.transaction(async tx => {
      expect(await tx.prepare('SELECT 6 value').first('value')).toBe(6);
      await tx.exec('SELECT 7');
    }));
    expect(success.metrics.queries).toBe(4); // BEGIN, SELECT, SELECT, COMMIT.
    const session = await measureReads(() => database.withSession(async connection => {
      await connection.prepare('SELECT 8').all();
      await connection.batch([connection.prepare('SELECT 9')]);
    }));
    expect(session.metrics.queries).toBe(4); // Session SELECT plus BEGIN/SELECT/COMMIT.
    let failure: ReadMeasurement | undefined;
    await expect(measureReads(() => database.transaction(tx => tx.prepare('SELECT 1 / 0').all()), values => { failure = values; })).rejects.toThrow();
    expect(failure).toMatchObject({ queries: 3, queryErrors: 1, poolErrors: 0, failed: true });
    expect(database.pool.idleCount).toBe(1);
    expect((await database.pool.query('SELECT 10 value')).rows[0].value).toBe(10);
  });

  it('retains failed nested measurements and safely reports phase failures without logging private errors', async () => {
    const database = singleConnection();
    vi.stubEnv('ERPX_PERF_LOG', '1');
    const log = vi.spyOn(console, 'info').mockImplementation(() => {});
    const error = new Error('private actor and credentials');
    let rootFailure: ReadMeasurement | undefined;
    await expect(measureReads(async () => {
      await measureReadPhase('test.phase', async () => {
        await database.pool.query('SELECT 1');
        throw error;
      });
    }, values => { rootFailure = values; })).rejects.toBe(error);
    expect(rootFailure).toMatchObject({ queries: 1, queryErrors: 0, failed: true });
    const logged = JSON.parse(String(log.mock.calls[0][0]));
    expect(logged).toMatchObject({ event: 'page_read', phase: 'test.phase', queries: 1, failed: true });
    expect(JSON.stringify(log.mock.calls)).not.toContain(error.message);
  });

  it('measures acquisition failures and returns the sole connection for the following request', async () => {
    const database = singleConnection(35);
    let leased: PoolClient | undefined = await database.pool.connect();
    let failure: ReadMeasurement | undefined;
    try {
      await expect(measureReads(() => database.pool.query('SELECT 1'), values => { failure = values; })).rejects.toThrow();
      expect(failure).toMatchObject({ queries: 0, queryErrors: 0, poolErrors: 1, failed: true });
      expect(failure!.poolWaitMs).toBeGreaterThanOrEqual(25);
    } finally { leased.release(); leased = undefined; }
    const recovered = await measureReads(() => database.prepare('SELECT 11 value').first('value'));
    expect(recovered.result).toBe(11);
    expect(recovered.metrics).toMatchObject({ queries: 1, queryErrors: 0, poolErrors: 0 });
    expect(database.pool.idleCount).toBe(1);
  });

  it('refreshes a retained HMR database without replacing or repeatedly wrapping its pool', async () => {
    const database = singleConnection();
    const pool = database.pool;
    await pool.query('SELECT 1'); // A client already exists before the refresh.
    Object.setPrototypeOf(database, { pool, prepare: () => { throw new Error('obsolete methods'); }, close: () => pool.end() });
    refreshPostgresReadInstrumentation(database);
    refreshPostgresReadInstrumentation(database);
    expect(database.pool).toBe(pool);
    expect(database.pool.options.max).toBe(1);
    expect(database.pool.listenerCount('acquire')).toBe(1);
    const refreshed = await measureReads(async () => {
      await database.prepare('SELECT 12').all();
      await database.pool.query('SELECT 13');
    });
    expect(refreshed.metrics.queries).toBe(2);
  });
});

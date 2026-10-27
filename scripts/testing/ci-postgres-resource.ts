import { spawn } from 'node:child_process';
import { appendFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { Pool } from 'pg';
import { ciVerificationArgv } from './ci-postgres-contract';

// This wrapper is only for the ephemeral GitHub service, never the shared Mac cluster.
if (process.env.CI !== 'true' || process.env.GITHUB_ACTIONS !== 'true') throw new Error('ISOLATED_GITHUB_POSTGRES_REQUIRED');
const argv = ciVerificationArgv(process.argv.slice(2));
const url = new URL(process.env.ERPX_CI_POSTGRES_MONITOR_URL ?? '');
if (!['postgres:', 'postgresql:'].includes(url.protocol) || url.hostname !== '127.0.0.1' || url.port !== '5432'
  || url.username !== 'postgres' || !url.password || url.pathname !== '/postgres' || url.search || url.hash)
  throw new Error('EXACT_ISOLATED_CI_POSTGRES_REQUIRED');
const directory = resolve('.scratch/dev/test-suites/ci-postgres');
await mkdir(directory, { recursive: true, mode: 0o700 });
const pool = new Pool({ connectionString: url.href, max: 1, application_name: 'erpx-ci-predicate-monitor',
  connectionTimeoutMillis: 5_000, query_timeout: 2_000,
  options: '-c default_transaction_read_only=on -c statement_timeout=1500 -c lock_timeout=500' });
const stop = new AbortController();
let attempts = 0, samples = 0, errors = 0, bytes = 0, peakSiread = 0, peakTargets = 0, peakConnections = 0, peakTransactions = 0;
let truncated = false;
const errorCodes: Record<string, number> = {};
const maximumSamples = 21_600, maximumBytes = 4 * 1024 * 1024;
try {
  const identity = (await pool.query<{ actor: string; database: string; version: string }>(
    "SELECT current_user actor,current_database() database,current_setting('server_version_num') version")).rows[0];
  if (identity?.actor !== 'postgres' || identity.database !== 'postgres' || identity.version !== '180006')
    throw new Error('POSTGRES_18_6_CI_IDENTITY_REQUIRED');
  const settings = (await pool.query<{ name: string; setting: string; source: string; pending_restart: boolean }>(
    "SELECT name,setting,source,pending_restart FROM pg_settings WHERE name IN ('max_connections','max_prepared_transactions','autovacuum_worker_slots','max_worker_processes','max_wal_senders','max_pred_locks_per_transaction','max_pred_locks_per_relation','max_pred_locks_per_page') ORDER BY name")).rows;
  for (const [name, value] of [['max_pred_locks_per_transaction', '256'], ['max_pred_locks_per_relation', '32'], ['max_pred_locks_per_page', '2']]) {
    const actual = settings.find(setting => setting.name === name);
    if (!actual || actual.setting !== value || actual.pending_restart || actual.source !== 'configuration file')
      throw new Error('CI_PREDICATE_STARTUP_BUDGET_NOT_APPLIED');
  }
  const allocations = (await pool.query<{ name: string; allocated_size: string }>(
    "SELECT name,allocated_size::text FROM pg_shmem_allocations WHERE name ILIKE '%predicate%' OR name ILIKE '%serializ%' ORDER BY name LIMIT 32")).rows;
  await writeFile(resolve(directory, 'configuration.json'), JSON.stringify({ version: 1, identity, settings, allocations }, null, 2) + '\n', { mode: 0o600 });
  await writeFile(resolve(directory, 'samples.jsonl'), '', { mode: 0o600 });
  const sample = async () => {
    attempts++;
    // Aggregate only: no statement text, query arguments, cookies, URLs or credentials.
    const result = (await pool.query<{ siread: number; targets: number; connections: number; transactions: number; lock_waiters: number; idle_transactions: number; oldest_transaction_ms: number }>(`
      WITH predicate AS (
        SELECT count(*)::int siread,count(DISTINCT (database,relation,page,tuple))::int targets
        FROM pg_locks WHERE mode='SIReadLock'
      ), activity AS (
        SELECT count(*)::int connections,count(*) FILTER(WHERE xact_start IS NOT NULL)::int transactions,
          count(*) FILTER(WHERE wait_event_type='Lock')::int lock_waiters,
          count(*) FILTER(WHERE state='idle in transaction')::int idle_transactions,
          coalesce(max(extract(epoch FROM clock_timestamp()-xact_start)*1000),0)::bigint::float8 oldest_transaction_ms
        FROM pg_stat_activity WHERE backend_type='client backend' AND pid<>pg_backend_pid()
      ) SELECT * FROM predicate CROSS JOIN activity`)).rows[0];
    peakSiread = Math.max(peakSiread, result.siread); peakTargets = Math.max(peakTargets, result.targets);
    peakConnections = Math.max(peakConnections, result.connections); peakTransactions = Math.max(peakTransactions, result.transactions);
    const line = JSON.stringify({ at: new Date().toISOString(), ...result }) + '\n';
    if (bytes + Buffer.byteLength(line) > maximumBytes) { truncated = true; return false; }
    await appendFile(resolve(directory, 'samples.jsonl'), line); bytes += Buffer.byteLength(line); samples++;
    return true;
  };
  await sample();
  const monitoring = (async () => {
    while (!stop.signal.aborted && attempts < maximumSamples) {
      try { await delay(1_000, undefined, { signal: stop.signal }); }
      catch { break; }
      if (stop.signal.aborted) break;
      try { if (!await sample()) break; } catch (error) {
        errors++;
        const code = error && typeof error === 'object' && 'code' in error && typeof error.code === 'string'
          && /^[A-Z0-9]{5}$/.test(error.code) ? error.code : 'unknown';
        const key = code in errorCodes || Object.keys(errorCodes).length < 16 ? code : 'other';
        errorCodes[key] = (errorCodes[key] ?? 0) + 1;
      }
    }
    if (attempts >= maximumSamples) truncated = true;
  })();
  const environment = { ...process.env }; delete environment.ERPX_CI_POSTGRES_MONITOR_URL;
  const child = spawn(process.execPath, argv, { stdio: 'inherit', env: environment, detached: process.platform !== 'win32' });
  let receivedSignal: NodeJS.Signals | undefined;
  const onSignal = (signal: NodeJS.Signals) => {
    receivedSignal = signal;
    if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
    try { process.kill(process.platform === 'win32' ? child.pid : -child.pid, signal); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ESRCH') process.exitCode = 1; }
  };
  const interrupt = () => onSignal('SIGINT'), terminate = () => onSignal('SIGTERM');
  process.on('SIGINT', interrupt); process.on('SIGTERM', terminate);
  try {
    const result = await new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((accept, reject) => {
      child.once('error', reject); child.once('exit', (code, signal) => accept({ code, signal }));
    });
    process.exitCode = receivedSignal === 'SIGINT' || result.signal === 'SIGINT' ? 130
      : receivedSignal || result.signal ? 143 : result.code ?? 1;
  } finally {
    process.off('SIGINT', interrupt); process.off('SIGTERM', terminate); stop.abort(); await monitoring;
    await writeFile(resolve(directory, 'summary.json'), JSON.stringify({ version: 1, sampleIntervalMs: 1_000, sampledPeaksOnly: true,
      maximumSamples, maximumBytes, attempts, samples, errors, errorCodes, bytes, truncated,
      peakSiread, peakTargets, peakConnections, peakTransactions, childExitCode: process.exitCode }, null, 2) + '\n', { mode: 0o600 });
  }
} finally { stop.abort(); await pool.end(); }

/** Bootstrap only the keyed, disposable contributor container; never an operator workspace. */
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmdirSync, statSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { randomBytes } from 'node:crypto';

const key = process.env.ERPX_DEV_WORKSPACE_KEY ?? '';
const root = '/workspace/erpx';
const data = `${root}/.runtime/local`;
const config = `${data}/config`;
const lease = '/run/erpx-secrets/workspace-key';
if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(key) || process.platform !== 'linux' || !existsSync('/.dockerenv') || resolve(process.cwd()) !== root || !existsSync(`${root}/.git`)) throw new Error('ISOLATED_KEYED_WORKSPACE_REQUIRED');
for (const name of readdirSync(root)) if ((name === '.env' || name.startsWith('.env.')) && name !== '.env.example') throw new Error('WORKSPACE_ENV_FILES_FORBIDDEN');
for (const name of Object.keys(process.env)) if ((name.startsWith('ERPX_') && name !== 'ERPX_DEV_WORKSPACE_KEY') || name === 'DATABASE_URL' || name === 'BETTER_AUTH_SECRET') throw new Error('EXTERNAL_RUNTIME_ENVIRONMENT_FORBIDDEN');
if (Bun.version !== '1.4.3' || (await Bun.$`/usr/local/bin/node --version`.text()).trim() !== 'v26.11.1') throw new Error('PINNED_TOOLCHAIN_REQUIRED');
const hasLease = existsSync(lease);
if (hasLease && (readFileSync(lease, 'utf8').trim() !== key || (statSync(lease).mode & 0o077) !== 0)) throw new Error('WORKSPACE_LEASE_MISMATCH');
if (!hasLease) throw new Error('WORKSPACE_LEASE_REQUIRED_USE_HOST_BOOTSTRAP');
if (!statSync(`${root}/.git`).isDirectory()) throw new Error('INDEPENDENT_SOURCE_CLONE_REQUIRED');
mkdirSync(`${root}/.runtime`, { recursive: true, mode: 0o700 });
const lock = `${root}/.runtime/dev-container-bootstrap.lock`;
try { mkdirSync(lock, { mode: 0o700 }); } catch (error) {
  if ((error as NodeJS.ErrnoException).code === 'EEXIST') throw new Error('BOOTSTRAP_LOCK_HELD_OR_STALE');
  throw error;
}
try {
const password = readFileSync('/run/erpx-secrets/postgres', 'utf8').trim();
if (!/^[a-f0-9]{64}$/.test(password)) throw new Error('ISOLATED_POSTGRES_SECRET_REQUIRED');
const admin = new URL('postgresql://postgres@127.0.0.1:5432/postgres'); admin.password = password;
const install = Bun.spawn(['bun', '--no-env-file', 'install', '--frozen-lockfile'], { cwd: root, env: { PATH: '/usr/local/bin:/usr/bin:/bin', HOME: '/root' }, stdout: 'inherit', stderr: 'inherit', stdin: 'ignore' });
if (await install.exited !== 0) throw new Error('DEPENDENCY_INSTALL_FAILED');
const { Pool } = await import('pg');
const pool = new Pool({ connectionString: admin.href, max: 1 });
let existing: boolean;
try {
  const identity = await pool.query<{ current_user: string; current_database: string }>('SELECT current_user, current_database()');
  if (identity.rows[0]?.current_user !== 'postgres' || identity.rows[0]?.current_database !== 'postgres') throw new Error('ISOLATED_POSTGRES_IDENTITY_REQUIRED');
  const result = await pool.query<{ present: boolean }>("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname LIKE 'erpx\\_dev\\_%' ESCAPE '\\') OR EXISTS (SELECT 1 FROM pg_database WHERE datname LIKE 'erpx\\_dev\\_%' ESCAPE '\\') AS present");
  existing = result.rows[0]?.present === true;
} finally { await pool.end(); }
const files = ['provisioning.json', 'databases.json', 'databases-jobs.json', 'databases-migrations.json'];
const present = files.map(name => existsSync(`${config}/${name}`));
if (existing !== present.every(Boolean) || (!existing && present.some(Boolean))) throw new Error('INCOMPLETE_PROVISIONING_STATE');
if (existing) {
  for (const name of files) if ((statSync(`${config}/${name}`).mode & 0o077) !== 0) throw new Error('PRIVATE_CONFIG_REQUIRED');
  const roles = JSON.parse(readFileSync(`${config}/provisioning.json`, 'utf8')) as Record<string, unknown>;
  const bindings = ['CONTROL_DB', 'CONTROL_READ_DB', ...['H001', 'H002', 'H003'].flatMap(id => [`${id}_DB`, `${id}_READ_DB`])];
  for (const [name, kind] of [['databases.json', 'app'], ['databases-jobs.json', 'jobs'], ['databases-migrations.json', 'migration']] as const) {
    const manifest = JSON.parse(readFileSync(`${config}/${name}`, 'utf8')) as { databases: Record<string, string> };
    if (!manifest.databases || Object.keys(manifest.databases).length !== bindings.length) throw new Error('INVALID_PERSISTED_MANIFEST');
    for (const binding of bindings) {
      const url = new URL(manifest.databases[binding]);
      const database = `erpx_dev_${binding.toLowerCase().replace(/_db$/, '').replace(/_read$/, '_read').replace(/^(h00[1-3])$/, '$1_write')}`;
      const role = `${database}_${kind}`;
      if (!['postgres:', 'postgresql:'].includes(url.protocol) || url.hostname !== '127.0.0.1' || url.port !== '5432' || url.search || url.hash || url.pathname !== `/${database}` || url.username !== role || typeof roles[role] !== 'string' || !/^[a-f0-9]{64}$/.test(roles[role] as string) || url.password !== roles[role]) throw new Error('INVALID_PERSISTED_MANIFEST');
    }
  }
}
const sourceIdPath = `${root}/.git/erpx-container-source-id`;
const phasePath = `${config}/bootstrap-phase.json`;
type Phase = { key: string; sourceId: string; phase: 'seed-pending' | 'complete' };
let phase: Phase;
function writePhase(value: Phase) {
  const temp = `${phasePath}.tmp`;
  writeFileSync(temp, JSON.stringify(value), { mode: 0o600, flag: 'wx' });
  renameSync(temp, phasePath);
}
if (existing) {
  if (!existsSync(sourceIdPath) || !existsSync(phasePath) || (statSync(sourceIdPath).mode & 0o077) !== 0 || (statSync(phasePath).mode & 0o077) !== 0) throw new Error('BOOTSTRAP_PHASE_REQUIRED');
  const sourceId = readFileSync(sourceIdPath, 'utf8').trim();
  phase = JSON.parse(readFileSync(phasePath, 'utf8')) as Phase;
  if (!/^[a-f0-9]{64}$/.test(sourceId) || phase.key !== key || phase.sourceId !== sourceId || !['seed-pending', 'complete'].includes(phase.phase)) throw new Error('BOOTSTRAP_PHASE_MISMATCH');
} else {
  if (existsSync(sourceIdPath) || existsSync(phasePath) || existsSync(`${config}/bootstrap-auth-secret`)) throw new Error('INCOMPLETE_PROVISIONING_STATE');
  mkdirSync(config, { recursive: true, mode: 0o700 });
  const sourceId = randomBytes(32).toString('hex');
  writeFileSync(sourceIdPath, sourceId, { mode: 0o600, flag: 'wx' });
  phase = { key, sourceId, phase: 'seed-pending' };
  writePhase(phase);
}
const env: Record<string, string> = { PATH: process.env.PATH ?? '/usr/local/bin:/usr/bin:/bin', HOME: '/root', ERPX_NODE_BIN: '/usr/local/bin/node', ERPX_BUN_BIN: '/usr/local/bin/bun', ERPX_ENVIRONMENT: 'local', ERPX_DATA_DIR: data, ERPX_DATABASE_MANIFEST: `${config}/databases.json`, ERPX_PUBLIC_ORIGIN: 'http://localhost:3100' };
const authSecretPath = `${config}/bootstrap-auth-secret`;
if (existing && !existsSync(authSecretPath)) throw new Error('EXISTING_AUTH_SECRET_REQUIRED');
if (!existing) {
  const { provision } = await import('./provision');
  await provision({ adminUrl: admin.href, dataDir: data, prefix: 'erpx_dev', beta: false });
  writeFileSync(authSecretPath, randomBytes(32).toString('hex'), { mode: 0o600, flag: 'wx' });
}
const authSecret = readFileSync(authSecretPath, 'utf8').trim();
if (!/^[a-f0-9]{64}$/.test(authSecret) || (statSync(authSecretPath).mode & 0o077) !== 0) throw new Error('INVALID_AUTH_SECRET');
env.BETTER_AUTH_SECRET = authSecret;
async function run(args: string[]) {
  const child = Bun.spawn(args, { cwd: root, env, stdout: 'inherit', stderr: 'inherit', stdin: 'ignore' });
  if (await child.exited !== 0) throw new Error('BOOTSTRAP_STEP_FAILED');
}
if (existing && phase.phase === 'complete') await run(['bun', '--no-env-file', 'scripts/local/migrate-development.ts']);
else await run(['bun', '--no-env-file', 'run', 'seed:local']);
await run(['bun', '--no-env-file', 'run', 'seed:verify:local']);
if (phase.phase === 'seed-pending') writePhase({ ...phase, phase: 'complete' });
console.log('BOOTSTRAP_COMPLETE: isolated transitional development data; application and HTTPS unverified');
} finally { rmdirSync(lock); }

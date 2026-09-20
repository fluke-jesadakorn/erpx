/** Configure the existing native Redis service; never install, restart or flush it. */
import { randomBytes } from 'node:crypto';
import { readFile, writeFile, chmod } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { createClient } from 'redis';

const envPath = join(process.cwd(), '.env.local');
let env = await readFile(envPath, 'utf8');
const credentials = JSON.parse(await readFile(process.env.ERPX_REDIS_ADMIN_CREDENTIALS ?? join(homedir(), '.config/dev-infra/credentials.json'), 'utf8'));
const existing = /^ERPX_REDIS_URL=(.+)$/m.exec(env)?.[1];
const password = existing ? decodeURIComponent(new URL(existing).password) : randomBytes(32).toString('hex');
const admin = createClient({ username: 'epsx_dev', password: credentials.redis_epsx_dev,
  socket: { host: '127.0.0.1', port: 6379, connectTimeout: 1000, reconnectStrategy: false } });
admin.on('error', () => {});
try {
  await admin.connect();
  await admin.sendCommand(['ACL', 'SETUSER', 'erpx_dev', 'reset', 'on', `>${password}`, '~erpx:dev:*',
    '+get', '+set', '+del', '+unlink', '+ping', '+hello', '+client|setinfo', '+client|setname', '+select']);
  await admin.sendCommand(['ACL', 'SAVE']);
  const url = `redis://erpx_dev:${encodeURIComponent(password)}@127.0.0.1:6379/0`;
  for (const [key, value] of Object.entries({ ERPX_REDIS_URL: url, ERPX_WORK_CACHE: 'redis', ERPX_DEV_PG_POOL_MAX: '6', ERPX_PERF_LOG: '1' })) {
    const line = `${key}=${value}`;
    env = new RegExp(`^${key}=.*$`, 'm').test(env) ? env.replace(new RegExp(`^${key}=.*$`, 'm'), line) : `${env.trimEnd()}\n${line}\n`;
  }
  await writeFile(envPath, env, { mode: 0o600 }); await chmod(envPath, 0o600);
  const client = createClient({ url, socket: { reconnectStrategy: false } });
  client.on('error', () => {});
  try {
    await client.connect();
    if (await client.ping() !== 'PONG') throw Error('REDIS_PING_FAILED');
    const probe = `erpx:dev:probe:${randomBytes(8).toString('hex')}`;
    await client.set(probe, 'ok', { PX: 1000 });
    if (await client.get(probe) !== 'ok') throw Error('REDIS_NAMESPACE_FAILED');
    await client.del(probe);
    let denied = false;
    try { await client.get('erpx:production:acl-probe'); } catch (error) { denied = String(error).includes('NOPERM'); }
    if (!denied) throw Error('REDIS_NAMESPACE_NOT_RESTRICTED');
  } finally { if (client.isOpen) client.destroy(); }
  console.log('Existing native Redis: ERPX dev ACL saved, namespace isolation and PING verified. Credentials stored only in .env.local.');
} finally { if (admin.isOpen) admin.destroy(); }

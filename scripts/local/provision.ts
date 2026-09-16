import { randomBytes } from 'node:crypto';
import { mkdir, readFile, writeFile, chmod } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { Pool } from 'pg';
import { PostgresDatabase } from '../../src/server/runtime/postgres';
import { migrateDatabase } from '../../src/server/runtime/migrations';
import type { DatabaseManifest } from '../../src/server/runtime/env';

const quote = (name: string) => { if (!/^[a-z][a-z0-9_]{2,55}$/.test(name)) throw new Error('INVALID_DATABASE_IDENTIFIER'); return `"${name}"`; };
export async function provision(options: { adminUrl: string; dataDir: string; prefix: string; beta: boolean }) {
  if (!/^erpx(?:_[a-z0-9]+)*$/.test(options.prefix)) throw new Error('ERPX_DATABASE_PREFIX_REQUIRED');
  const root = resolve(options.dataDir), config = join(root, 'config');
  await mkdir(config, { recursive: true, mode: 0o700 });
  const credentialsPath = join(config, 'provisioning.json');
  let credentials: Record<string,string> = {};
  try { credentials = JSON.parse(await readFile(credentialsPath, 'utf8')); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  const slots = Array.from({ length: options.beta ? 20 : 2 }, (_, index) => {
    const n = String(index + 1).padStart(3, '0'), id = options.beta ? `beta${n}` : `h${n}`;
    return { id, write: `${options.beta ? 'B' : 'H'}${n}_DB`, read: `${options.beta ? 'B' : 'H'}${n}_READ_DB`, files: `${options.beta ? 'B' : 'H'}${n}_FILES`, ...(options.beta ? { beta: true } : {}) };
  });
  const entries = [{ binding: 'CONTROL_DB', name: `${options.prefix}_control`, track: 'control' },
    { binding: 'CONTROL_READ_DB', name: `${options.prefix}_control_read`, track: 'control-read' },
    ...slots.flatMap(s => [{ binding:s.write,name:`${options.prefix}_${s.id}_write`,track:'holding-write' },{ binding:s.read,name:`${options.prefix}_${s.id}_read`,track:'holding-read' }])];
  const app: DatabaseManifest = { version:1, databases:{}, slots }, jobs: DatabaseManifest = { version:1, databases:{}, slots };
  const admin = new Pool({ connectionString: options.adminUrl, max:1 });
  try {
    for (const entry of entries) {
      const exists = await admin.query('SELECT 1 FROM pg_database WHERE datname=$1',[entry.name]);
      if (!exists.rowCount) await admin.query(`CREATE DATABASE ${quote(entry.name)} TEMPLATE template0 ENCODING 'UTF8'`);
      const ownerUrl = new URL(options.adminUrl); ownerUrl.pathname = entry.name;
      const db = new PostgresDatabase({ connectionString: ownerUrl.href });
      try {
        await migrateDatabase(db, entry.track);
        await db.pool.query('REVOKE ALL ON SCHEMA public FROM PUBLIC');
        await admin.query(`REVOKE ALL ON DATABASE ${quote(entry.name)} FROM PUBLIC`);
        for (const processRole of ['app','jobs'] as const) {
          const role = `${entry.name}_${processRole}`;
          if (!credentials[role]) {
            credentials[role] = randomBytes(32).toString('hex');
            await writeFile(credentialsPath, JSON.stringify(credentials), { mode:0o600 }); await chmod(credentialsPath, 0o600);
          }
          if (!(await admin.query('SELECT 1 FROM pg_roles WHERE rolname=$1',[role])).rowCount) await admin.query(`CREATE ROLE ${quote(role)} LOGIN`);
          // Password is generated hexadecimal, never supplied as SQL or logged.
          // The provisioning role is intentionally NOSUPERUSER. New roles already
          // default to NOSUPERUSER; repeating that clause requires superuser rights.
          await admin.query(`ALTER ROLE ${quote(role)} PASSWORD '${credentials[role]}' NOCREATEDB NOCREATEROLE`);
          await admin.query(`GRANT CONNECT ON DATABASE ${quote(entry.name)} TO ${quote(role)}`);
          await db.pool.query(`GRANT USAGE ON SCHEMA public TO ${quote(role)}`);
          const readOnly = entry.track.endsWith('-read') && processRole === 'app';
          await db.pool.query(`GRANT ${readOnly ? 'SELECT' : 'SELECT,INSERT,UPDATE,DELETE'} ON ALL TABLES IN SCHEMA public TO ${quote(role)}`);
          if (!readOnly) await db.pool.query(`GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO ${quote(role)}`);
          await db.pool.query(`REVOKE ALL ON erpx_migrations FROM ${quote(role)}`);
          const url = new URL(ownerUrl); url.username=role; url.password=credentials[role];
          (processRole === 'app' ? app : jobs).databases[entry.binding] = url.href;
        }
        if (entry.binding === 'CONTROL_DB') {
          if (options.beta) for (const slot of slots) await db.prepare('INSERT INTO beta_slots(holding_id,created_at,updated_at) VALUES(?,?,?) ON CONFLICT DO NOTHING').bind(slot.id,Date.now(),Date.now()).run();
          else for (const [index,slot] of slots.entries()) await db.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES(?,?,?,?,?,?,'active') ON CONFLICT DO NOTHING")
            .bind(slot.id,index===0?'acme':'northstar',index===0?'Acme Group':'Northstar Supply',slot.write,slot.read,slot.files).run();
        }
      } finally { await db.close(); }
    }
    for (const [name, manifest] of [['databases.json',app],['databases-jobs.json',jobs]] as const) {
      await writeFile(join(config,name), JSON.stringify(manifest,null,2), { mode:0o600 }); await chmod(join(config,name),0o600);
    }
    for (const slot of slots) await mkdir(join(root,'files',slot.id),{recursive:true,mode:0o700});
  } finally { await admin.end(); }
  return { databases:entries.length, slots:slots.length, dataDir:root };
}
if (import.meta.main) {
  const adminUrl=process.env.ERPX_ADMIN_DATABASE_URL;
  if (!adminUrl) throw new Error('ERPX_ADMIN_DATABASE_URL_REQUIRED');
  console.log(await provision({adminUrl,dataDir:process.env.ERPX_DATA_DIR??'.runtime/local',prefix:process.env.ERPX_DATABASE_PREFIX??'erpx',beta:!process.argv.includes('--development')}));
}

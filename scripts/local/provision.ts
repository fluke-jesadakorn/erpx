import { randomBytes } from 'node:crypto';
import { mkdir, readFile, writeFile, chmod } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { Pool } from 'pg';
import { PostgresDatabase } from '../../src/server/runtime/postgres';
import { migrateDatabase } from '../../src/server/runtime/migrations';
import type { DatabaseManifest } from '../../src/server/runtime/env';

const quote = (name: string) => {
  if (!/^[a-z][a-z0-9_]{2,55}$/.test(name)) throw new Error('INVALID_DATABASE_IDENTIFIER');
  return `"${name}"`;
};
const quoteCatalogIdentifier = (name: string) => {
  if (typeof name !== 'string' || name.length === 0 || name.includes('\0')) throw new Error('INVALID_CATALOG_IDENTIFIER');
  return `"${name.replaceAll('"', '""')}"`;
};

async function storedCredential(
  credentials: Record<string, string>,
  credentialsPath: string,
  role: string,
) {
  if (!credentials[role]) {
    credentials[role] = randomBytes(32).toString('hex');
    await writeFile(credentialsPath, JSON.stringify(credentials), { mode: 0o600 });
    await chmod(credentialsPath, 0o600);
  }
  // These values are later embedded in a SQL password literal. Only the
  // hexadecimal format generated here is accepted from the persisted file.
  if (!/^[a-f0-9]{64}$/.test(credentials[role])) throw new Error('INVALID_STORED_PROVISIONING_CREDENTIAL');
  return credentials[role];
}

async function ensureLoginRole(admin: Pool, role: string, password: string) {
  if (!(await admin.query('SELECT 1 FROM pg_roles WHERE rolname=$1', [role])).rowCount) {
    await admin.query(`CREATE ROLE ${quote(role)} LOGIN`);
  }
  await admin.query(`ALTER ROLE ${quote(role)} PASSWORD '${password}' NOCREATEDB NOCREATEROLE`);
}

/**
 * The earlier provisioner migrated as the cluster administrator and then used
 * REASSIGN OWNED. PostgreSQL 18 refuses to reassign cluster/system objects, so
 * resume only transfers application objects in public. Migrations then run as
 * the dedicated migration login, which owns the schema and its DDL objects.
 */
async function transferPublicObjects(db: PostgresDatabase, sourceRole: string, targetRole: string) {
  const target = quote(targetRole);
  const relations = await db.pool.query<{ relkind: string; schema_name: string; object_name: string }>(
    `SELECT c.relkind, n.nspname AS schema_name, c.relname AS object_name
       FROM pg_class c
       JOIN pg_namespace n ON n.oid = c.relnamespace
       JOIN pg_roles r ON r.oid = c.relowner
      WHERE n.nspname = 'public' AND r.rolname = $1
        AND c.relkind IN ('r', 'p', 'v', 'm', 'S', 'f')
      ORDER BY CASE c.relkind WHEN 'r' THEN 1 WHEN 'p' THEN 1 ELSE 2 END, c.relname`,
    [sourceRole],
  );
  for (const row of relations.rows) {
    const object = `${quoteCatalogIdentifier(row.schema_name)}.${quoteCatalogIdentifier(row.object_name)}`;
    const kind = row.relkind === 'v' ? 'VIEW'
      : row.relkind === 'm' ? 'MATERIALIZED VIEW'
        : row.relkind === 'S' ? 'SEQUENCE'
          : row.relkind === 'f' ? 'FOREIGN TABLE' : 'TABLE';
    await db.pool.query(`ALTER ${kind} ${object} OWNER TO ${target}`);
  }

  const routines = await db.pool.query<{
    prokind: string; schema_name: string; object_name: string; identity_arguments: string;
  }>(
    `SELECT p.prokind, n.nspname AS schema_name, p.proname AS object_name,
            pg_get_function_identity_arguments(p.oid) AS identity_arguments
       FROM pg_proc p
       JOIN pg_namespace n ON n.oid = p.pronamespace
       JOIN pg_roles r ON r.oid = p.proowner
      WHERE n.nspname = 'public' AND r.rolname = $1
        AND p.prokind IN ('f', 'p', 'a')
      ORDER BY p.proname, p.oid`,
    [sourceRole],
  );
  for (const row of routines.rows) {
    const kind = row.prokind === 'p' ? 'PROCEDURE' : row.prokind === 'a' ? 'AGGREGATE' : 'FUNCTION';
    const signature = `${quoteCatalogIdentifier(row.schema_name)}.${quoteCatalogIdentifier(row.object_name)}(${row.identity_arguments})`;
    await db.pool.query(`ALTER ${kind} ${signature} OWNER TO ${target}`);
  }

  const types = await db.pool.query<{ typtype: string; schema_name: string; object_name: string }>(
    `SELECT t.typtype, n.nspname AS schema_name, t.typname AS object_name
       FROM pg_type t
       JOIN pg_namespace n ON n.oid = t.typnamespace
       JOIN pg_roles r ON r.oid = t.typowner
      WHERE n.nspname = 'public' AND r.rolname = $1
        AND t.typrelid = 0 AND t.typelem = 0
        AND t.typtype IN ('b', 'd', 'e', 'm', 'r')
      ORDER BY t.typname`,
    [sourceRole],
  );
  for (const row of types.rows) {
    const object = `${quoteCatalogIdentifier(row.schema_name)}.${quoteCatalogIdentifier(row.object_name)}`;
    const kind = row.typtype === 'd' ? 'DOMAIN' : 'TYPE';
    await db.pool.query(`ALTER ${kind} ${object} OWNER TO ${target}`);
  }

  await db.pool.query(`ALTER SCHEMA public OWNER TO ${target}`);
}

async function setMigrationDefaults(db: PostgresDatabase, migrationRole: string, appRole: string, jobsRole: string, readOnly: boolean) {
  const migration = quote(migrationRole), app = quote(appRole), jobs = quote(jobsRole);
  await db.pool.query(`ALTER DEFAULT PRIVILEGES FOR ROLE ${migration} IN SCHEMA public GRANT ${readOnly ? 'SELECT' : 'SELECT,INSERT,UPDATE,DELETE'} ON TABLES TO ${app}`);
  await db.pool.query(`ALTER DEFAULT PRIVILEGES FOR ROLE ${migration} IN SCHEMA public GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO ${jobs}`);
  if (!readOnly) {
    await db.pool.query(`ALTER DEFAULT PRIVILEGES FOR ROLE ${migration} IN SCHEMA public GRANT USAGE,SELECT ON SEQUENCES TO ${app}`);
  }
  await db.pool.query(`ALTER DEFAULT PRIVILEGES FOR ROLE ${migration} IN SCHEMA public GRANT USAGE,SELECT ON SEQUENCES TO ${jobs}`);
}

export async function provision(options: { adminUrl: string; dataDir: string; prefix: string; beta: boolean }) {
  if (!/^erpx(?:_[a-z0-9]+)*$/.test(options.prefix)) throw new Error('ERPX_DATABASE_PREFIX_REQUIRED');
  const root = resolve(options.dataDir), config = join(root, 'config');
  await mkdir(config, { recursive: true, mode: 0o700 });
  const credentialsPath = join(config, 'provisioning.json');
  let credentials: Record<string, string> = {};
  try {
    credentials = JSON.parse(await readFile(credentialsPath, 'utf8'));
    if (!credentials || Array.isArray(credentials) || typeof credentials !== 'object') throw new Error('INVALID_STORED_PROVISIONING_CREDENTIALS');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }

  const customerSlots = Array.from({ length: options.beta ? 20 : 2 }, (_, index) => {
    const n = String(index + 1).padStart(3, '0'), id = options.beta ? `beta${n}` : `h${n}`;
    return { id, write: `${options.beta ? 'B' : 'H'}${n}_DB`, read: `${options.beta ? 'B' : 'H'}${n}_READ_DB`, files: `${options.beta ? 'B' : 'H'}${n}_FILES`, ...(options.beta ? { beta: true } : {}) };
  });
  const slots = [...customerSlots, { id: 'h003', write: 'H003_DB', read: 'H003_READ_DB', files: 'H003_FILES', internal: true }];
  const entries = [
    { binding: 'CONTROL_DB', name: `${options.prefix}_control`, track: 'control' },
    { binding: 'CONTROL_READ_DB', name: `${options.prefix}_control_read`, track: 'control-read' },
    ...slots.flatMap(slot => [
      { binding: slot.write, name: `${options.prefix}_${slot.id}_write`, track: 'holding-write' },
      { binding: slot.read, name: `${options.prefix}_${slot.id}_read`, track: 'holding-read' },
    ]),
  ];
  const app: DatabaseManifest = { version: 1, databases: {}, slots };
  const jobs: DatabaseManifest = { version: 1, databases: {}, slots };
  const migrations: DatabaseManifest = { version: 1, databases: {}, slots };
  const admin = new Pool({ connectionString: options.adminUrl, max: 1 });
  const adminRole = new URL(options.adminUrl).username;
  quote(adminRole);

  try {
    for (const entry of entries) {
      const appRole = `${entry.name}_app`;
      const jobsRole = `${entry.name}_jobs`;
      const migrationRole = `${entry.name}_migration`;
      const appPassword = await storedCredential(credentials, credentialsPath, appRole);
      const jobsPassword = await storedCredential(credentials, credentialsPath, jobsRole);
      const migrationPassword = await storedCredential(credentials, credentialsPath, migrationRole);
      await ensureLoginRole(admin, appRole, appPassword);
      await ensureLoginRole(admin, jobsRole, jobsPassword);
      await ensureLoginRole(admin, migrationRole, migrationPassword);

      const exists = await admin.query('SELECT 1 FROM pg_database WHERE datname=$1', [entry.name]);
      if (!exists.rowCount) await admin.query(`CREATE DATABASE ${quote(entry.name)} TEMPLATE template0 ENCODING 'UTF8'`);
      await admin.query(`REVOKE ALL ON DATABASE ${quote(entry.name)} FROM PUBLIC`);
      await admin.query(`ALTER DATABASE ${quote(entry.name)} OWNER TO ${quote(migrationRole)}`);
      await admin.query(`GRANT CONNECT ON DATABASE ${quote(entry.name)} TO ${quote(migrationRole)}`);
      await admin.query(`GRANT CONNECT ON DATABASE ${quote(entry.name)} TO ${quote(appRole)}, ${quote(jobsRole)}`);

      const adminUrl = new URL(options.adminUrl);
      adminUrl.pathname = entry.name;
      const adminDb = new PostgresDatabase({ connectionString: adminUrl.href });
      try {
        await adminDb.pool.query('REVOKE ALL ON SCHEMA public FROM PUBLIC');
        await transferPublicObjects(adminDb, adminRole, migrationRole);

        // A previous run granted this membership before hitting REASSIGN OWNED.
        // The cluster administrator already has the rights needed for recovery.
        await admin.query(`REVOKE ${quote(migrationRole)} FROM ${quote(adminRole)}`);

        const migrationUrl = new URL(adminUrl);
        migrationUrl.username = migrationRole;
        migrationUrl.password = migrationPassword;
        const migrationDb = new PostgresDatabase({ connectionString: migrationUrl.href });
        try {
          await setMigrationDefaults(migrationDb, migrationRole, appRole, jobsRole, entry.track.endsWith('-read'));
          await migrateDatabase(migrationDb, entry.track);
        } finally {
          await migrationDb.close();
        }

        for (const [processRole, password] of [['app', appPassword], ['jobs', jobsPassword]] as const) {
          const role = processRole === 'app' ? appRole : jobsRole;
          await admin.query(`GRANT CONNECT ON DATABASE ${quote(entry.name)} TO ${quote(role)}`);
          await adminDb.pool.query(`GRANT USAGE ON SCHEMA public TO ${quote(role)}`);
          const readOnly = entry.track.endsWith('-read') && processRole === 'app';
          await adminDb.pool.query(`GRANT ${readOnly ? 'SELECT' : 'SELECT,INSERT,UPDATE,DELETE'} ON ALL TABLES IN SCHEMA public TO ${quote(role)}`);
          if (!readOnly) await adminDb.pool.query(`GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO ${quote(role)}`);
          await adminDb.pool.query(`REVOKE ALL ON erpx_migrations FROM ${quote(role)}`);
          const url = new URL(adminUrl);
          url.username = role;
          url.password = password;
          (processRole === 'app' ? app : jobs).databases[entry.binding] = url.href;
        }

        const migrationManifestUrl = new URL(adminUrl);
        migrationManifestUrl.username = migrationRole;
        migrationManifestUrl.password = migrationPassword;
        migrations.databases[entry.binding] = migrationManifestUrl.href;

        if (entry.binding === 'CONTROL_DB') {
          if (options.beta) {
            for (const slot of customerSlots) {
              await adminDb.prepare('INSERT INTO beta_slots(holding_id,created_at,updated_at) VALUES(?,?,?) ON CONFLICT DO NOTHING')
                .bind(slot.id, Date.now(), Date.now()).run();
            }
          } else {
            for (const [index, slot] of customerSlots.entries()) {
              await adminDb.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES(?,?,?,?,?,?,'active') ON CONFLICT DO NOTHING")
                .bind(slot.id, index === 0 ? 'acme' : 'northstar', index === 0 ? 'Acme Group' : 'Northstar Supply', slot.write, slot.read, slot.files).run();
            }
          }
          const erpx = slots.find(slot => slot.id === 'h003')!;
          await adminDb.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES(?,?,?,?,?,?,'active') ON CONFLICT(holding_id) DO NOTHING")
            .bind(erpx.id, 'erpx', 'ERPX', erpx.write, erpx.read, erpx.files).run();
        }
      } finally {
        await adminDb.close();
      }
    }

    for (const [name, manifest] of [['databases.json', app], ['databases-jobs.json', jobs], ['databases-migrations.json', migrations]] as const) {
      const path = join(config, name);
      await writeFile(path, JSON.stringify(manifest, null, 2), { mode: 0o600 });
      await chmod(path, 0o600);
    }
    for (const slot of slots) await mkdir(join(root, 'files', slot.id), { recursive: true, mode: 0o700 });
  } finally {
    await admin.end();
  }
  return { databases: entries.length, slots: slots.length, dataDir: root };
}

if (import.meta.main) {
  const adminUrl = process.env.ERPX_ADMIN_DATABASE_URL;
  if (!adminUrl) throw new Error('ERPX_ADMIN_DATABASE_URL_REQUIRED');
  console.log(await provision({ adminUrl, dataDir: process.env.ERPX_DATA_DIR ?? '.runtime/local', prefix: process.env.ERPX_DATABASE_PREFIX ?? 'erpx', beta: !process.argv.includes('--development') }));
}

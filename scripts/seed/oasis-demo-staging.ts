import { randomBytes } from 'node:crypto';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { isAbsolute, join, resolve } from 'node:path';
import { Pool } from 'pg';
import { hashPassword } from 'better-auth/crypto';
import { businessRoles } from '@erpx/identity-access-ui/lib/business-roles';
import { migrateDatabase } from '@erpx/platform-runtime-server/adapters/runtime/migrations';
import { PostgresDatabase } from '@erpx/platform-runtime-server/adapters/runtime/postgres';
import { seedOasisInterview } from '../../fixtures/sales/builders/oasis-interview';

const scopeNames = ['control', 'h001', 'h002'] as const;
const holdingHost = 'oasisdreamhouse-demo.jesadakorn.com';
const accountHost = 'account-oasisdreamhouse-demo.jesadakorn.com';
const companyId = 'oasis-demo-company';
const branchId = 'oasis-demo-phuket';
const sourceRoot = resolve(import.meta.dir, '../..');
const namespace = 'erpx_stage_oasis_demo_';
const [command, state] = process.argv.slice(2);
let phase = 'arguments';

async function writePrivate(name: string, content: unknown) {
  await writeFile(join(state, name), JSON.stringify(content, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
}

function secret() { return randomBytes(32).toString('hex'); }
function urlFor(database: string, role: string, password: string) {
  const url = new URL(`postgresql://127.0.0.1:5432/${database}`);
  url.username = role;
  url.password = password;
  return url.href;
}

async function provision() {
  if (command !== 'provision' || !state || !isAbsolute(state) || process.argv.length !== 4)
    throw new Error('provision requires an absolute new state directory');
  if (Bun.version !== '1.4.3') throw new Error('Bun 1.4.3 is required');
  const credentialsPath = join(homedir(), '.config/dev-infra/credentials.json');
  const credentialsStat = await stat(credentialsPath);
  if (credentialsStat.mode & 0o077) throw new Error('Shared PostgreSQL administrator file must be private');
  const credentials = JSON.parse(await readFile(credentialsPath, 'utf8')) as { localdev_admin?: string };
  if (!credentials.localdev_admin) throw new Error('Shared PostgreSQL administrator unavailable');
  const admin = new Pool({ connectionString: urlFor('postgres', 'localdev_admin', credentials.localdev_admin), max: 1 });
  const scopes = scopeNames.map(scope => {
    const database = `${namespace}${scope}`;
    const roles = (['owner', 'app', 'read'] as const).map(access => ({ access, name: `${database}_${access}`, password: secret() }));
    return { scope, database, roles, urls: Object.fromEntries(roles.map(role => [role.access, urlFor(database, role.name, role.password)])) };
  });
  try {
    phase = 'postgres preflight';
    const existing = await admin.query('SELECT datname FROM pg_database WHERE datname=ANY($1) UNION SELECT rolname FROM pg_roles WHERE rolname=ANY($2)', [scopes.map(scope => scope.database), scopes.flatMap(scope => scope.roles.map(role => role.name))]);
    if (existing.rowCount) throw new Error('OASIS database names are already allocated');
    await mkdir(state, { mode: 0o700 });
    for (const { scope, database, roles, urls } of scopes) {
      phase = `${scope} provisioning`;
      for (const role of roles) await admin.query(`CREATE ROLE ${role.name} LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD '${role.password}'`);
      const owner = `${database}_owner`, app = `${database}_app`, read = `${database}_read`;
      await admin.query(`CREATE DATABASE ${database} OWNER ${owner} TEMPLATE template0`);
      await admin.query(`REVOKE ALL ON DATABASE ${database} FROM PUBLIC`);
      await admin.query(`GRANT CONNECT ON DATABASE ${database} TO ${app}, ${read}`);
      const migration = new PostgresDatabase({ connectionString: urls.owner, max: 1 });
      try {
        phase = `${scope} migrations`;
        await migrateDatabase(migration, scope === 'control' ? 'control' : 'holding-write', join(sourceRoot, 'migrations/postgres'));
        await migration.pool.query(`
          REVOKE CREATE ON SCHEMA public FROM PUBLIC;
          GRANT USAGE ON SCHEMA public TO ${app}, ${read};
          GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO ${app};
          GRANT SELECT ON ALL TABLES IN SCHEMA public TO ${read};
          GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO ${app};
          ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO ${app};
          ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO ${read};
          ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO ${app};
          REVOKE INSERT, UPDATE, DELETE ON erpx_migrations FROM ${app};
        `);
      } finally { await migration.close(); }
    }
  } finally { await admin.end(); }
  const [control, primary, secondary] = scopes;
  phase = 'seed sales Company';
  const holdingDb = new PostgresDatabase({ connectionString: primary.urls.app, max: 1 });
  const controlDb = new PostgresDatabase({ connectionString: control.urls.app, max: 1 });
  const now = Date.now();
  const actors = [
    { userId: 'oasis-demo-seller', name: 'Kitchen salesperson', email: 'seller@oasis-demo.example', role: 'sales_operator', password: `Oasis-${secret()}` },
    { userId: 'oasis-demo-manager', name: 'Kitchen sales manager', email: 'manager@oasis-demo.example', role: 'sales_manager', password: `Oasis-${secret()}` },
  ] as const;
  const [seller, manager] = actors;
  const dates = new Intl.DateTimeFormat('sv-SE', { timeZone: 'Asia/Bangkok' });
  const dueDate = dates.format(new Date(now + 86_400_000));
  const finance = { chart: 'oasis-demo-chart', receivable: 'oasis-demo-receivable', revenue: 'oasis-demo-revenue', tax: 'oasis-demo-tax', preparerId: 'oasis-demo-fixture-preparer', reviewerId: 'oasis-demo-fixture-reviewer' };
  let kitchen;
  try {
    await controlDb.batch([
      ...[[primary.scope, 'oasis-demo', 'OASIS Kitchen Demo'], [secondary.scope, 'oasis-internal', 'Inactive demo slot']].map(([id, slug, label]) => controlDb.prepare('INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES(?,?,?,?,?,?,?)').bind(id, slug, label, `${id.toUpperCase()}_DB`, `${id.toUpperCase()}_READ_DB`, `${id.toUpperCase()}_FILES`, 'active')),
      ...(await Promise.all(actors.map(async actor => [
        controlDb.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId, actor.name, actor.email, now, now),
        controlDb.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`, actor.userId, actor.userId, await hashPassword(actor.password), now, now),
        controlDb.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`, actor.userId, actor.name, now),
      ]))).flat(),
    ]);
    const grants = ['sales', 'finance', 'work', 'work-social'].map(packageId => ({ companyId, packageId, packageVersion: 1, state: 'active', startsAt: now, endsAt: null, version: 1, reason: 'Synthetic interview fixture', evidence: 'fixture:oasis-demo-staging' }));
    await holdingDb.batch([
      holdingDb.prepare("INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,'h001',?,?)").bind(manager.userId, now),
      holdingDb.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company','andaman-kitchen','Andaman Kitchen Demo',?)").bind(companyId, companyId),
      holdingDb.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch','phuket','Phuket Showroom',?,?,?)").bind(branchId, companyId, companyId, branchId),
      ...actors.flatMap(actor => [
        holdingDb.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,?,?,?,?,'holding')").bind(`grant-${actor.userId}`, actor.userId, actor.role, JSON.stringify(businessRoles[actor.role]), companyId, now - 1, now + 180 * 86_400_000),
        holdingDb.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'employee',?,?,?,?,'holding')").bind(`feed-${actor.userId}`, actor.userId, JSON.stringify(['collaboration']), companyId, now - 1, now + 180 * 86_400_000),
        holdingDb.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,preferred_name,email,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'TH',?,?,?)").bind(`person-${actor.userId}`, actor.userId, companyId, actor.name, actor.name, actor.email, manager.userId, now, now),
        holdingDb.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,'employee','salaried','active',?,?,'THB',?,?,?)").bind(`engagement-${actor.userId}`, `person-${actor.userId}`, companyId, branchId, dates.format(new Date(now)), actor.name, manager.userId, now, now),
      ]),
      ...['crm', 'sales', 'finance', 'collaboration'].map(moduleId => holdingDb.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,?,'pilot',1,?,?,?)").bind(companyId,moduleId,'Synthetic OASIS staging pilot',manager.userId,now)),
      holdingDb.prepare("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,'THB',?,'36',?,?,'Synthetic OASIS demo rate for accounting snapshots only')").bind('oasis-demo-thb-rate', companyId, dates.format(new Date(now)), finance.reviewerId, now),
      holdingDb.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'Andaman Demo Chart','THB',?,?,?)").bind(finance.chart, companyId, finance.preparerId, now, now),
      ...[[finance.receivable, '1100', 'Accounts receivable', 'asset'], [finance.revenue, '4100', 'Kitchen revenue', 'revenue'], [finance.tax, '2100', 'Output VAT', 'liability'], ['oasis-demo-expense', '5100', 'Cost of goods', 'expense']].map(([id, code, name, category]) => holdingDb.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?)").bind(id, finance.chart, companyId, code, name, category, now, now)),
      holdingDb.prepare('INSERT INTO commerce_settings VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(companyId, finance.chart, 'THB', finance.receivable, finance.tax, finance.revenue, finance.receivable, finance.tax, 'oasis-demo-expense', finance.tax, finance.tax, finance.receivable),
      ...grants.flatMap(grant => [
        holdingDb.prepare('INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(companyId, grant.packageId, grant.packageVersion, grant.state, grant.startsAt, grant.endsAt, grant.version, grant.reason, grant.evidence, now),
        holdingDb.prepare('INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES(?,?,?,?,?,NULL,?::jsonb,?)').bind(`oasis-demo-grant-${grant.packageId}`, companyId, grant.packageId, manager.userId, secret(), JSON.stringify(grant), now),
      ]),
    ]);
    kitchen = await seedOasisInterview(holdingDb, { target: 'oasis-demo-staging', run: randomBytes(6).toString('hex'), companyId, branchId, salespersonId: seller.userId, managerId: manager.userId, finance, dueDate });
  } finally { await Promise.all([holdingDb.close(), controlDb.close()]); }
  phase = 'write runtime state';
  const holdings = [primary, secondary].map(({ scope, urls }, index) => ({
    holdingId: scope, slug: index ? 'oasis-internal' : 'oasis-demo', host: index ? 'oasisdreamhouse-demo-internal.invalid' : holdingHost,
    writeBinding: `${scope.toUpperCase()}_DB`, readBinding: `${scope.toUpperCase()}_READ_DB`, filesBinding: `${scope.toUpperCase()}_FILES`,
    database: { url: urls.app }, readDatabase: { url: urls.read },
  }));
  const databaseBindings = Object.fromEntries(scopes.flatMap(({scope, urls}) => [[`${scope.toUpperCase()}_DB`, urls.app], [`${scope.toUpperCase()}_READ_DB`, urls.read]]));
  const companyScope = [{ holdingId: primary.scope, companyId }];
  await writePrivate('databases.json', { version: 1, databases: databaseBindings, slots: holdings.map(entry => ({ id: entry.holdingId, write: entry.writeBinding, read: entry.readBinding, files: entry.filesBinding })) });
  await writePrivate('next-runtime.json', { BETTER_AUTH_SECRET: secret() });
  await writePrivate('private-api.json', {
    version: 1, mode: 'oasis-demo-staging', environment: 'staging', transport: 'direct-loopback', listen: '127.0.0.1:3382',
    accountHost, accountBootstrap: true, requireEmailVerification: true, passwordWorkers: 2, maximumRequests: 24, requestTimeoutMs: 10000,
    accountSecurityKeys: { current: 'oasis-demo', keys: [{ id: 'oasis-demo', keyHex: secret() }] },
    control: { url: control.urls.app }, controlRead: { url: control.urls.read }, holdings,
    productOrigins: [{ key: 'sales', host: 'oasisdreamhouse-demo-sales.invalid', holdingId: primary.scope }],
    crmCompanies: companyScope, commerceCompanies: companyScope, financeJournalReads: companyScope,
    operationsCompanies: companyScope,
  });
  await writePrivate('rehearsal-accounts.json', actors);
  await writePrivate('demo-guide.json', { companyId, branchId, ...kitchen });
  console.log('Provisioned the isolated OASIS demo PostgreSQL bindings and private runtime state.');
}

try { await provision(); } catch (error) {
  const code = typeof error === 'object' && error !== null && 'code' in error && /^[A-Z0-9_]{5,60}$/.test(String(error.code)) ? ` (${error.code})` : '';
  console.error(`OASIS demo setup stopped at ${phase}${code}.`);
  process.exitCode = 1;
}

/** Synthetic purchase for the private keyed contributor workspace only.
 * This CLI fixture actor is not provider authority and is never used at runtime. */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { getLocalProxy } from '../local/fixtures';
import { packageGrantStore, readCompanyPackageGrants } from '@erpx/server-commercial/public/package-grants';
import { commercialCompanies } from '@erpx/server-organization/public/commercial-companies';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const root = '/workspace/erpx', config = `${root}/.runtime/local/config`;
const key = process.env.ERPX_DEV_WORKSPACE_KEY ?? '';
const id = 'contributor_finance_demo_v1';
const actor = 'contributor-synthetic-seed';
const grant = { companyId:'acme-co', packageId:'finance', packageVersion:1, state:'active' as const, startsAt:0, endsAt:null, version:1, reason:'Synthetic contributor Finance demonstration', evidence:'fixture:contributor-finance-v1' };
function matchesGrant(value: unknown): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>, keys = Object.keys(grant);
  return Object.keys(row).length === keys.length && keys.every(key => row[key] === grant[key as keyof typeof grant]);
}
function privateFile(path: string) { if (!existsSync(path) || (statSync(path).mode & 0o077) !== 0) throw Error('CONTRIBUTOR_PRIVATE_FILE_REQUIRED'); return readFileSync(path, 'utf8').trim(); }
function assertFixture() {
  if (process.platform !== 'linux' || !existsSync('/.dockerenv') || resolve(process.cwd()) !== root || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(key) || privateFile('/run/erpx-secrets/workspace-key') !== key || !statSync(`${root}/.git`).isDirectory() || !statSync(`${root}/.runtime/dev-container-bootstrap.lock`).isDirectory()) throw Error('CONTRIBUTOR_WORKSPACE_REQUIRED');
  if (readdirSync(root).some(name => (name === '.env' || name.startsWith('.env.')) && name !== '.env.example') || process.env.ERPX_ENVIRONMENT !== 'local' || process.env.NODE_ENV === 'production' || ![`${config}/databases.json`, `${config}/databases-jobs.json`].includes(process.env.ERPX_DATABASE_MANIFEST ?? '')) throw Error('CONTRIBUTOR_ENVIRONMENT_REQUIRED');
  const sourceId = privateFile(`${root}/.git/erpx-container-source-id`), phase = JSON.parse(privateFile(`${config}/bootstrap-phase.json`));
  if (!/^[a-f0-9]{64}$/.test(sourceId) || phase.key !== key || phase.sourceId !== sourceId || !['seed-pending','complete'].includes(phase.phase)) throw Error('CONTRIBUTOR_SOURCE_PHASE_REQUIRED');
  const roles = JSON.parse(privateFile(`${config}/provisioning.json`));
  for (const [file, role] of [['databases.json','app'], ['databases-jobs.json','jobs']] as const) {
    const manifest = JSON.parse(privateFile(`${config}/${file}`));
    const url = new URL(manifest.databases.H001_DB), username = `erpx_dev_h001_write_${role}`;
    if (!['postgres:','postgresql:'].includes(url.protocol) || url.hostname !== '127.0.0.1' || url.port !== '5432' || url.pathname !== '/erpx_dev_h001_write' || url.username !== username || url.search || url.hash || !/^[a-f0-9]{64}$/.test(roles[username]) || url.password !== roles[username]) throw Error('CONTRIBUTOR_DATABASE_REQUIRED');
  }
}
assertFixture();
const fixture = await getLocalProxy();
try {
  const db = fixture.env.H001_DB as PostgresDatabase;
  const authorize = async (transaction: SqlDatabase, companyId: string) => { assertFixture(); if (companyId !== grant.companyId || !(await commercialCompanies(transaction)).some(company => company.id === companyId && company.slug === 'acme-co')) throw Error('CONTRIBUTOR_COMPANY_REQUIRED'); return { userId: actor }; };
  const store = packageGrantStore(db, { authorize, obligations: async () => { throw Error('CONTRIBUTOR_WITHDRAWAL_FORBIDDEN'); } });
  const current = (await readCompanyPackageGrants(db, grant.companyId)).find(item => item.packageId === grant.packageId);
  if (current && !matchesGrant(current)) throw Error('CONTRIBUTOR_FINANCE_GRANT_CONFLICT');
  const beforeEvents = await db.prepare('SELECT id,actor_id FROM company_package_grant_events WHERE company_id=? AND package_id=?').bind(grant.companyId, grant.packageId).all<{id:string;actor_id:string}>();
  if (current ? beforeEvents.results.length !== 1 || beforeEvents.results[0]?.id !== id || beforeEvents.results[0]?.actor_id !== actor : beforeEvents.results.length !== 0) throw Error('CONTRIBUTOR_FINANCE_RECEIPT_CONFLICT');
  const result = await store.apply({ idempotencyKey:id, expectedVersion:0, grant });
  const after = (await readCompanyPackageGrants(db, grant.companyId)).find(item => item.packageId === grant.packageId);
  const events = await db.prepare('SELECT id,actor_id FROM company_package_grant_events WHERE company_id=? AND package_id=?').bind(grant.companyId, grant.packageId).all<{id:string;actor_id:string}>();
  if (!matchesGrant(result) || !matchesGrant(after) || events.results.length !== 1 || events.results[0]?.id !== id || events.results[0]?.actor_id !== actor) throw Error('CONTRIBUTOR_FINANCE_RECEIPT_CONFLICT');
  console.log('CONTRIBUTOR_FINANCE_DEMO_READY');
} finally { await fixture.dispose(); }

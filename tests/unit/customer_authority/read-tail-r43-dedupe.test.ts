import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { finalRootOwnerProjectionSources as financeProjectionSources } from '@erpx/finance-server/public/final-root-owner-projection-sources';
import { selectProjection } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r40-sources';
import { companyScopeSelect } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r33-sources';
import { allColumnsProjection } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r37-sources';
import { evidenceSource } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r41-sources';
import { supportRequestRead } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-tail-r42-sources';

const srcRoot = resolve(__dirname, '../../../modules/customer-authority/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe literals copied verbatim from the r33/r37/r41/r42 baselines
// (BEFORE=26). Only owner-local columns, closed tails and bound lists vary.
const SCOPE = 'SELECT ?::text company_id';
const WORKFORCE = 'SELECT ?::text company_id,?::text branch_id,?::text department_id';
const WORKFORCE_COLUMNS = ',?::text branch_id,?::text department_id';
const PROJECTION_BINDINGS = 'SELECT * FROM customer_access_bindings WHERE email=?';
const PROJECTION_SUPPORT = (table: string) => `SELECT * FROM customer_portal_support_${table}`;
const IDENTITY_ORIGINAL = `SELECT e.id,e.change_id,e.action,e.customer_version,e.created_at,e.version,c.status change_status,c.version change_version,p.name customer_name,p.code customer_code
  FROM customer_identity_change_events e
  JOIN customer_identity_changes c ON c.id=e.change_id
  JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id AND p.kind='customer'
  WHERE c.company_id=? AND e.created_at>=? AND e.created_at<?`;
const DUPLICATE_ORIGINAL = `SELECT r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version
    FROM customer_duplicate_reviews r
    JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'`;
const IDENTITY_COLS = 'e.id,e.change_id,e.action,e.customer_version,e.created_at,e.version,c.status change_status,c.version change_version,p.name customer_name,p.code customer_code';
const IDENTITY_TAIL = `
  FROM customer_identity_change_events e
  JOIN customer_identity_changes c ON c.id=e.change_id
  JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id AND p.kind='customer'
  WHERE c.company_id=? AND e.created_at>=? AND e.created_at<?`;
const DUPLICATE_COLS = 'r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version';
const DUPLICATE_TAIL = `
    FROM customer_duplicate_reviews r
    JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'`;
const SOURCE_COLS = 'request.*,binding.version binding_version,binding.created_by binding_prepared_by,binding.email,binding.status,binding.expires_at,original.branch_id,original.department_id,original.state,original.version order_version,jsonb_build_array(to_jsonb(request),to_jsonb(binding),to_jsonb(identity),to_jsonb(original))::text image';
const SOURCE_TAIL = `JOIN customer_access_bindings binding ON binding.id=request.binding_id AND binding.company_id=request.company_id AND binding.order_id=request.order_id AND binding.customer_id=request.customer_id JOIN customer_access_identities identity ON identity.binding_id=binding.id AND identity.user_id=request.user_id AND identity.email=binding.email JOIN commerce_orders original ON original.id=request.order_id AND original.company_id=request.company_id AND original.party_id=request.customer_id AND original.kind IN ('quote','sales') WHERE request.id=? AND request.action='request_support'`;
const THREADS_COLS = 'request.id,request.created_at,request.note,response.id response_id,response.response_version,response.message,response.created_at published_at,ack.created_at acknowledged_at,closure.created_at closed_at';
const THREADS_TAIL = 'LEFT JOIN LATERAL(${portalSupportResponsesSource} response WHERE response.request_id=request.id ORDER BY response.response_version DESC LIMIT 1) response ON true LEFT JOIN customer_portal_support_acknowledgements ack ON ack.request_id=request.id AND ack.response_id=response.id LEFT JOIN customer_portal_support_closures closure ON closure.request_id=request.id WHERE request.binding_id=? AND request.action=\'request_support\' AND (?::text IS NULL OR request.user_id=?) ORDER BY request.created_at,request.id';
const SOURCE_ORIGINAL = `SELECT ${SOURCE_COLS} FROM customer_portal_actions request ${SOURCE_TAIL}`;
const THREADS_ORIGINAL = `SELECT ${THREADS_COLS} FROM customer_portal_actions request ${THREADS_TAIL}`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r43 universal-head delegation family', () => {
  test('renders byte-identically to every pre-dedupe literal', () => {
    expect(selectProjection('id')).toBe('SELECT id');
    expect(companyScopeSelect()).toBe(SCOPE);
    expect(companyScopeSelect(WORKFORCE_COLUMNS)).toBe(WORKFORCE);
    expect(SCOPE).toBe(WORKFORCE.slice(0, SCOPE.length));
    expect(allColumnsProjection('customer_access_bindings', ' WHERE email=?')).toBe(PROJECTION_BINDINGS);
    for (const table of ['responses', 'acknowledgements', 'closures']) {
      expect(allColumnsProjection(`customer_portal_support_${table}`)).toBe(PROJECTION_SUPPORT(table));
    }
    const identity = evidenceSource(IDENTITY_COLS, IDENTITY_TAIL);
    const duplicate = evidenceSource(DUPLICATE_COLS, DUPLICATE_TAIL);
    expect(identity).toBe(IDENTITY_ORIGINAL);
    expect(duplicate).toBe(DUPLICATE_ORIGINAL);
    expect(supportRequestRead(SOURCE_COLS, SOURCE_TAIL)).toBe(SOURCE_ORIGINAL);
    expect(supportRequestRead(THREADS_COLS, THREADS_TAIL)).toBe(THREADS_ORIGINAL);
    for (const sql of [
      SCOPE, WORKFORCE, PROJECTION_BINDINGS,
      ...['responses', 'acknowledgements', 'closures'].map(PROJECTION_SUPPORT),
      identity, duplicate, SOURCE_ORIGINAL, THREADS_ORIGINAL,
    ]) readOnly(sql);
  });

  test('one universal head, four delegating named helpers', () => {
    const head = src('customer-authority-read-tail-r40-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(head.match(/export const selectProjection/g) ?? []).toHaveLength(1);

    for (const [name, symbol] of [
      ['customer-authority-read-tail-r33-sources.ts', 'companyScopeSelect'],
      ['customer-authority-read-tail-r37-sources.ts', 'allColumnsProjection'],
      ['customer-authority-read-tail-r41-sources.ts', 'evidenceSource'],
      ['customer-authority-read-tail-r42-sources.ts', 'supportRequestRead'],
    ] as Array<[string, string]>) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(0);
      expect(helper, name).toContain(`export const ${symbol}`);
      expect(helper, name).toContain('selectProjection(');
      expect(helper, name).toContain("from './customer-authority-read-tail-r40-sources'");
      expect(helper, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('pinned callers keep prepared statements, binds, locks, native kinds and eligibility', () => {
    const obligations = src('package-obligations.ts');
    expect(obligations.match(/companyScopeSelect\(/g) ?? []).toHaveLength(1);
    expect(obligations).toContain('WITH scope AS(${companyScopeSelect()})');
    expect(obligations).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");

    const gate = src('portal-release-gate.ts');
    expect(gate.match(/companyScopeSelect\(/g) ?? []).toHaveLength(1);
    expect(gate).toContain('const gateSource=`(${companyScopeSelect()}) ${gateAlias}`;');
    expect(gate.match(/releaseGateRead\(/g) ?? []).toHaveLength(2);

    const read = src('attachment-read.ts');
    expect(read.match(/companyScopeSelect\(/g) ?? []).toHaveLength(1);
    expect(read).toContain("`FROM(${companyScopeSelect(',?::text branch_id,?::text department_id')})workforce_actor_scope`");
    expect(read).toContain('existenceProbe(');

    const unit = src('portal-projection-unit.ts');
    expect(unit).toContain("allColumnsProjection('customer_access_bindings',' WHERE email=?')");
    const support = src('portal-support-sources.ts');
    expect(support).toContain('allColumnsProjection(`customer_portal_support_${table}`)');
    expect(support).toContain('LOCK TABLE customer_portal_support_responses,customer_portal_support_acknowledgements,customer_portal_support_closures IN SHARE ROW EXCLUSIVE MODE');
    expect(support).toContain('export function portalSupportSourceReadLocks');

    const exec = src('executive-main-evidence-sources.ts');
    expect(exec.match(/evidenceSource\(/g) ?? []).toHaveLength(2);
    expect(exec.match(/\.bind\(/g) ?? []).toHaveLength(4);
    expect(exec).toContain('.bind(...values, input.limit + 1)');
    expect(exec).toContain('.bind(actor.companyId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, id)');

    const portal = src('portal-support.ts');
    expect(portal.match(/supportRequestRead\(/g) ?? []).toHaveLength(2);
    expect(portal).toContain('.bind(requestId).first<Source>()');
    expect(portal).toContain('.bind(bindingId,userId??null,userId??null).all<');
    expect(portal).toContain("database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE').run()");
    expect(portal).toContain('customerPortalSourceReadLocks(database)');
    expect(portal).toContain('portalSupportSourceReadLocks(database)');
    expect(portal).toContain('pointRead(');
    expect(portal).toContain("'quote','sales'");

    expect(src('portal-access.ts')).toContain('CA_RD_PORTAL_BINDING_LOCK');
    expect(src('portal-access.ts')).toContain('nativePortalReadStatement');
    expect(src('portal-source-locks.ts')).toContain('LOCK TABLE');
    expect(src('portal-purchase-binding-source.ts')).toContain('o.kind IN (${kinds})');
    expect(src('assignment-eligibility-source.ts')).toContain('export const assignmentSource');
    expect(src('manager-eligibility.ts')).toContain('export const');
    expect(src('native-owner-read.ts')).toContain('export');
  });

  test('r21-r42 heads stay pinned; locks, native kinds and eligibility untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 1, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 1, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 0, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 0, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
      ['customer-authority-read-tail-r27-sources.ts', 0, 'export const accessEventEvidenceProbe'],
      ['customer-authority-read-tail-r28-sources.ts', 0, 'export const releaseGateRead'],
      ['customer-authority-read-tail-r29-sources.ts', 1, 'export const reviewEligibilityRows'],
      ['customer-authority-read-tail-r30-sources.ts', 0, 'export const existenceProbe'],
      ['customer-authority-read-tail-r31-sources.ts', 0, 'export const supportConclusionIdentity'],
      ['customer-authority-read-tail-r32-sources.ts', 0, 'export const factExistenceWitness'],
      ['customer-authority-read-tail-r33-sources.ts', 0, 'export const companyScopeSelect'],
      ['customer-authority-read-tail-r34-sources.ts', 0, 'export const bindingCommerceOrderSource'],
      ['customer-authority-read-tail-r35-sources.ts', 0, 'export const scalarProjection'],
      ['customer-authority-read-tail-r36-sources.ts', 0, 'export const pointRead'],
      ['customer-authority-read-tail-r37-sources.ts', 0, 'export const allColumnsProjection'],
      ['customer-authority-read-tail-r40-sources.ts', 1, 'export const selectProjection'],
      ['customer-authority-read-tail-r41-sources.ts', 0, 'export const evidenceSource'],
      ['customer-authority-read-tail-r42-sources.ts', 0, 'export const supportRequestRead'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
    }
    for (const name of [
      'portal-access.ts', 'portal-source-locks.ts', 'portal-purchase-binding-source.ts',
      'assignment-eligibility-source.ts', 'manager-eligibility.ts', 'package-obligations.ts',
    ]) {
      expect(src(name), name).not.toMatch(/selectProjection\(|supportRequestRead\(|evidenceSource\(/);
    }
  });

  test('retirement:ts-statements is strictly below the recorded BEFORE=26', () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(srcRoot);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT/g) ?? []).length, 0);
    expect(total).toBeLessThan(26);
    expect(total).toBe(14);
  });
});

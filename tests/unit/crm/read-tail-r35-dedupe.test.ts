import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmAssignmentModuleExistsSql, crmContactAccountReadSql } from '../../../packages/server-crm/src/adapters/read-tail-r35-sql';

const owner = resolve(__dirname, '../../../packages/server-crm/src');
const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
const totalSelects = () => {
  let total = 0;
  for (const rel of readdirSync(owner, { recursive: true })) {
    if (!String(rel).endsWith('.ts')) continue;
    total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
  }
  return total;
};

/** Retired inline SQL, exactly as it read before r35 (interpolations kept as
 * the concrete fragments the call sites pass). */
const retired: Array<[string, string, string]> = [
  ['identities contact list',
    crmContactAccountReadSql('c.id,c.account_id,c.name,c.detail,c.version', 'a',
      "WHERE c.company_id=? AND ${await crmNativeReadFilter('contact','c')} AND ${accountOrContactLeadScopeSql('contact_id','c.id','c.company_id')} ORDER BY c.updated_at DESC LIMIT 300"),
    "SELECT c.id,c.account_id,c.name,c.detail,c.version FROM crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.company_id=? AND ${await crmNativeReadFilter('contact','c')} AND ${accountOrContactLeadScopeSql('contact_id','c.id','c.company_id')} ORDER BY c.updated_at DESC LIMIT 300"],
  ['identities single contact',
    crmContactAccountReadSql('c.id,c.name,c.account_id', 'acc', 'WHERE c.id=? AND c.company_id=? AND ${contactScopeSql}'),
    'SELECT c.id,c.name,c.account_id FROM crm_contacts c JOIN crm_accounts acc ON acc.id=c.account_id WHERE c.id=? AND c.company_id=? AND ${contactScopeSql}'],
  ['metric bare assignment EXISTS',
    ' AND ' + crmAssignmentModuleExistsSql('', "'crm'::text)))"),
    " AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('crm'::text)))"],
  ['aliased assignment prefix',
    crmAssignmentModuleExistsSql('selected', ''),
    'EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('],
];

describe('crm read-tail r35 byte-identical read heads', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns each renderer once and composes it at every retired call site', () => {
    const helper = src('read-tail-r35-sql.ts');
    expect(helper.match(/crmContactAccountReadSql =/g)).toHaveLength(1);
    expect(helper.match(/crmAssignmentModuleExistsSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(2);

    const identities = src('identities.ts');
    expect(identities.match(/crmContactAccountReadSql\(/g)).toHaveLength(2);
    expect(identities).toContain("crmContactAccountReadSql('c.id,c.account_id,c.name,c.detail,c.version','a',");
    expect(identities).toContain("crmContactAccountReadSql('c.id,c.name,c.account_id','acc',");
    expect(identities).not.toContain('SELECT c.id,c.account_id,c.name,c.detail,c.version FROM crm_contacts');
    expect(identities).not.toContain('SELECT c.id,c.name,c.account_id FROM crm_contacts');

    const purchases = src('purchases.ts');
    expect(purchases).toContain("export const crmAssignmentModuleExists=crmAssignmentModuleExistsSql('selected','');");
    expect(src('metric-purchases.ts')).toContain("crmAssignmentModuleExistsSql('', \"'crm'::text)))\")");
    expect(src('metric-purchases.ts')).not.toContain('EXISTS(SELECT 1 FROM assignments WHERE id=?');
  });

  test('keeps bind order unchanged at every retired site', () => {
    const identities = src('identities.ts'), metric = src('metric-purchases.ts'), purchases = src('purchases.ts');
    expect(identities).toContain('.bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).all<Row>()');
    expect(identities).toContain('.bind(contactId,a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId).first<Row>()');
    expect(metric).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed")');
    expect(metric).toContain("${crmAssignmentModuleExists}'crm'::text)))");
    expect(src('analytics-purchases.ts')).toContain('${crmAssignmentModuleExists}?::text)))');
    expect(purchases).toContain("export const crmAssignmentModuleExists=crmAssignmentModuleExistsSql('selected','');");
  });

  test('leaves locks, command receipts, native kinds and r8-r34 helpers untouched', () => {
    const purchases = src('purchases.ts'), analytics = src('analytics-purchases.ts');
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(analytics).toContain('crmAssignmentModuleExists');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_16', 'crm_stmt_17']) expect(src('leads.ts')).toContain(kind);
    for (const [file, token] of [
      ['read-tail-r24-sql.ts', 'export const crmCreatorLeadScopeSql'],
      ['read-tail-r25-sql.ts', 'export const crmPurchaseRowProjection'],
      ['read-tail-r26-sql.ts', 'export const crmGuestLinkReservationSourceSql'],
      ['read-tail-r27-sql.ts', 'const crmOpportunityOwnerAnchor ='],
      ['read-tail-r28-sql.ts', 'export const crmCaseEventReadSql'],
      ['read-tail-r29-sql.ts', 'const crmAccountReadAnchor ='],
      ['read-tail-r30-sql.ts', 'export const crmCurrentScopeAllowedSql'],
      ['read-tail-r31-sql.ts', 'export const crmRevenueJournalHeadSql'],
      ['read-tail-r32-sql.ts', 'export const crmCommerceOrderReadSql'],
      ['read-tail-r33-sql.ts', 'export const crmClockedSourceScopeSql'],
      ['read-tail-r34-sql.ts', 'export const crmOwnedSourceRowSql'],
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements stays strictly below the r34 BEFORE=116 baseline', () => {
    expect(totalSelects()).toBe(49);
    expect(totalSelects()).toBeLessThan(114);
  });
});

import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmOwnedSourceRowSql } from '../../../packages/server-crm/src/adapters/read-tail-r34-sql';

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

/** Retired inline SQL, exactly as it read before r34 (interpolations replaced
 * by the concrete sample values passed to the renderer). */
const retired: Array<[string, string, string]> = [
  ['opportunity witness',
    crmOwnedSourceRowSql('witness::text witness', 'OPPORTUNITY_ROWS', 'opportunity'),
    'SELECT witness::text witness FROM (OPPORTUNITY_ROWS) opportunity WHERE id=? AND company_id=?'],
  ['parameterized source read',
    crmOwnedSourceRowSql('id,company_id,branch_id,department_id,witness::text witness', 'SOURCE_ROWS', 'actual', ' AND SCOPE'),
    'SELECT id,company_id,branch_id,department_id,witness::text witness FROM (SOURCE_ROWS) actual WHERE id=? AND company_id=? AND SCOPE'],
  ['linked document reference',
    crmOwnedSourceRowSql('id', 'ROWS', 'actual', ' AND (?::text IS NULL OR branch_id=?) AND kind=? AND (?::text IS NULL OR department_id=?)'),
    'SELECT id FROM (ROWS) actual WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND kind=? AND (?::text IS NULL OR department_id=?)'],
  ['workspace record lookup',
    crmOwnedSourceRowSql('kind,target_id', 'RECORD_ROWS', 'actual'),
    'SELECT kind,target_id FROM (RECORD_ROWS) actual WHERE id=? AND company_id=?'],
];

describe('crm read-tail r34 byte-identical owned-source row-head family', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns the head renderer once and composes it at all four retired call sites', () => {
    const helper = src('read-tail-r34-sql.ts');
    expect(helper.match(/crmOwnedSourceRowSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);

    const p = src('purchases.ts'), rp = src('record-purchases.ts');
    expect(p.match(/crmOwnedSourceRowSql\(/g)).toHaveLength(5);
    expect(rp.match(/crmOwnedSourceRowSql\(/g)).toHaveLength(1);
    expect(p).toContain("crmOwnedSourceRowSql('witness::text witness',crmPurchaseOpportunityRows,'opportunity')");
    expect(p).toContain("crmOwnedSourceRowSql('id,company_id,branch_id,department_id,witness::text witness',sourceRows[target.kind],'actual',` AND ${scoped}`)");
    expect(p).toContain("crmOwnedSourceRowSql('id',rows,'actual',` AND (?::text IS NULL OR branch_id=?) ${kind===\"reservation\"?\"\":\"AND kind=? AND (?::text IS NULL OR department_id=?)\"}`)");
    expect(p).toContain("crmOwnedSourceRowSql('1',source,alias,tail)");
    expect(p).toContain("crmOwnedSourceRowSql('*',crmMarketingCandidateSources,'candidate')");
    expect(rp).toContain("crmOwnedSourceRowSql('kind,target_id',recordRows,'actual')");

    expect(p).not.toContain('`SELECT witness::text witness FROM (${crmPurchaseOpportunityRows})');
    expect(p).not.toContain('`SELECT id,company_id,branch_id,department_id,witness::text witness FROM (');
    expect(p).not.toContain('`SELECT id FROM (${rows})');
    expect(p).not.toContain('`SELECT 1 FROM (${source})');
    expect(p).not.toContain('`SELECT * FROM (${crmMarketingCandidateSources})');
    expect(rp).not.toContain('`SELECT kind,target_id FROM (${recordRows})');
  });

  test('keeps bind order unchanged at every retired site', () => {
    const p = src('purchases.ts'), rp = src('record-purchases.ts');
    expect(p).toContain('.bind(target.id,actor.companyId,...args).first<Source>()');
    expect(p).toContain('.bind(id,actor.companyId,actor.branchId,actor.branchId,...(kind==="reservation"?[]:[kind,actor.departmentId,actor.departmentId])).first()');
    expect(rp).toContain(".bind(id,actor.companyId).first<{kind:CRMPurchaseTarget['kind'];target_id:string}>()");
    expect(p.match(/\.prepare\(opportunityWitnessSql\)/g)).toHaveLength(2);
    expect(p.match(/\.bind\(command\.target\.id,actor\.companyId\)\.first<\{witness:string\}>\(\)/g)).toHaveLength(2);
  });

  test('leaves locks, command receipts, native kinds and r8-r33 helpers untouched', () => {
    const purchases = src('purchases.ts'), analytics = src('analytics.ts'), rp = src('record-purchases.ts');
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain('crmCaseEventReceiptWitnessSql');
    expect(rp).toContain('crmWorkLeadOwnerProbeSql');
    expect(rp).toContain('crmAssignmentScopeExistsSql');
    expect(analytics).toContain('crmCaseEventLeadReadSql');
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
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=119', () => {
    expect(totalSelects()).toBe(95);
    expect(totalSelects()).toBeLessThan(114);
  });
});

import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmScalarSubquerySql } from '../../../packages/server-crm/src/adapters/read-tail-r43-sql';

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

/** Concrete stand-ins for the closed projection/body fragments each retired
 * site passes, including the cross-owner source and authority interpolations. */
const HR_PEOPLE = 'CRM_HR_PEOPLE';
const ORG_NODES = 'CRM_ORG_NODES';
const ALIAS = 'o';
const COLUMN = 'branch_id';
const ASSIGNMENT_READ = 'CRM_ASSIGNMENT_READ';
const ACTOR_SCOPE = 'CRM_ACTOR_SCOPE';

/** The six retired inline literals, exactly as they read at the r42 AFTER=78
 * baseline, paired with the helper rendering that replaces them. */
const retired: Array<[string, string, string]> = [
  ['lead owner display name',
    crmScalarSubquerySql('legal_name', `(${HR_PEOPLE}) hr_people WHERE company_id=l.company_id AND user_id=l.owner_id AND merged_into_id IS NULL LIMIT 1`),
    `(SELECT legal_name FROM (${HR_PEOPLE}) hr_people WHERE company_id=l.company_id AND user_id=l.owner_id AND merged_into_id IS NULL LIMIT 1)`],
  ['executive org label',
    crmScalarSubquerySql('n.display_name', `(${ORG_NODES}) n WHERE n.id=${ALIAS}.${COLUMN} AND n.company_id=${ALIAS}.company_id`),
    '(SELECT n.display_name FROM (CRM_ORG_NODES) n WHERE n.id=o.branch_id AND n.company_id=o.company_id)'],
  ['purchases guest-link contact id',
    crmScalarSubquerySql('contact_id', 'crm_guest_links WHERE id=actual.id'),
    '(SELECT contact_id FROM crm_guest_links WHERE id=actual.id)'],
  ['record-purchases activity lead id',
    crmScalarSubquerySql('lead_id', 'crm_lead_activities WHERE id=activity.id'),
    '(SELECT lead_id FROM crm_lead_activities WHERE id=activity.id)'],
  ['analytics cost journal id',
    crmScalarSubquerySql('journal_id', 'finance_journal_lines WHERE id=cost.journal_line_id'),
    '(SELECT journal_id FROM finance_journal_lines WHERE id=cost.journal_line_id)'],
  ['attachment file authority',
    crmScalarSubquerySql(ASSIGNMENT_READ, `${ACTOR_SCOPE} crm_file_scope`),
    `(SELECT ${ASSIGNMENT_READ} FROM ${ACTOR_SCOPE} crm_file_scope)`],
];

describe('crm read-tail r43 byte-identical scalar-subquery reuse', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect((rendered.match(/SELECT /g) ?? []).length).toBe(1);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns exactly one renderer with a single read token', () => {
    const helper = src('read-tail-r43-sql.ts');
    expect((helper.match(/crmScalarSubquerySql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('every retired site uses the renderer and keeps its wrapper', () => {
    for (const file of [
      'lead-owner-name.ts', 'executive-metric-projections.ts', 'purchases.ts',
      'record-purchases.ts', 'analytics.ts', 'attachment-purchases.ts',
    ]) {
      const text = src(file);
      expect(text).toContain("read-tail-r43-sql");
      expect((text.match(/crmScalarSubquerySql\(/g) ?? []).length).toBe(1);
    }
    expect(src('lead-owner-name.ts')).toContain('export const leadOwnerNameSql = crmScalarSubquerySql(');
    expect(src('executive-metric-projections.ts')).toContain('`COALESCE(${crmScalarSubquerySql(');
    expect(src('purchases.ts')).toContain('?crmScalarSubquerySql(');
    expect(src('record-purchases.ts')).toContain('lead.id=${crmScalarSubquerySql(');
    expect(src('analytics.ts')).toContain('journal.id=${crmScalarSubquerySql(');
    expect(src('attachment-purchases.ts')).toContain('"crm-file-authority",crmScalarSubquerySql(');
    /** Only the six retired literals are gone; the excluded families remain. */
    const retiredSites: Array<[string, string[]]> = [
      ['lead-owner-name.ts', ['`(SELECT legal_name FROM']],
      ['executive-metric-projections.ts', ['(SELECT n.display_name FROM']],
      ['purchases.ts', ['"(SELECT contact_id FROM']],
      ['record-purchases.ts', ['lead.id=(SELECT lead_id FROM']],
      ['analytics.ts', ['journal.id=(SELECT journal_id FROM']],
      ['attachment-purchases.ts', ['`(SELECT ${assignmentReadSql("crm_file_scope"']],
    ];
    for (const [file, needles] of retiredSites) {
      const text = src(file);
      for (const needle of needles) expect(text).not.toContain(needle);
    }
  });

  test('keeps every caller bind order unchanged', () => {
    expect(src('executive-metric-projections.ts')).toContain('values = [...purchased.args,...scopeArgs, f.from, f.to];');
    expect(src('purchases.ts')).toContain('.bind(target.id,actor.companyId,...args).first<Source>()');
    expect(src('record-purchases.ts')).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.userId,actor.userId,actor.companyId,...ids,...visible.args)');
    expect(src('analytics.ts')).toContain('.bind(...marketing.args,a.companyId,f.from,f.to,...(access?[a.branchId,a.departmentId,a.assignmentId,a.userId,a.role]:[]))');
    const a = src('attachment-purchases.ts');
    expect(a).toContain('[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]');
    expect(a).toContain('[...sourceArgs(actor,scope.target),scope.witness]');
  });

  test('keeps cross-owner source and authority interpolations intact', () => {
    expect(src('lead-owner-name.ts')).toContain('(${people_payrollProjectionSources.hr_people})');
    expect(src('executive-metric-projections.ts')).toContain('(${organizationProjectionSources.organization_nodes}) n WHERE n.id=${alias}.${column}');
    expect(src('analytics.ts')).toContain('JOIN (${crmAnalyticsPostedJournals}) journal ON journal.id=');
    expect(src('analytics.ts')).toContain('FROM (${marketing.sql}) cost');
    const a = src('attachment-purchases.ts');
    expect(a).toContain('crmScalarSubquerySql(assignmentReadSql("crm_file_scope","commerce.sales"),`${actorScopeSql} crm_file_scope`)');
    expect(a).toContain('import {actorScopeSql,');
  });

  test('leaves locks, receipt reads, native kinds, the stay aggregate and r8-r42 helpers untouched (r45 routes the three purchase probes)', () => {
    const p = src('purchases.ts');
    expect(p).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(p).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(p).toContain('EXISTS(${crmSelectOneSql(`crm_guest_links guest_link WHERE guest_link.contact_id=');
    expect(p).toContain('export const crmHotelReservationProbeSql=crmSelectOneSql(`(${crmHotelReservationSources}) reservation`);');
    expect(p).toContain('export const crmCaseEventReceiptWitnessSql=crmSelectOneSql(`crm_case_events event JOIN command_receipts receipt ON');
    expect(src('record-purchases.ts')).toContain('const nativeIds=[crmNativeIdSql(');
    expect(src('attachment-purchases.ts')).toContain('await nativeClearCommandAssertions(database, actor.companyId, key)');
    expect(src('analytics.ts')).toContain('(SELECT MIN(stay.checked_in_at) FROM (${crmHotelAnalyticsStays}) stay');
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
      ['read-tail-r35-sql.ts', 'export const crmContactAccountReadSql'],
      ['read-tail-r36-sql.ts', 'export const crmOpportunitySourceAnchorSql'],
      ['read-tail-r37-sql.ts', 'export const crmSalesOpportunitySql'],
      ['read-tail-r39-sql.ts', 'export const crmClockedReadSql'],
      ['read-tail-r40-sql.ts', 'export const crmScopedDescListSql'],
      ['read-tail-r42-sql.ts', 'export const crmSelectOneSql'],
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements drops from the r42 AFTER=61 to 57 (r45)', () => {
    expect(totalSelects()).toBe(49);
    expect(totalSelects()).toBeLessThan(61);
  });
});

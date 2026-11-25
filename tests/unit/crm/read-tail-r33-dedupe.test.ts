import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmClockedSourceScopeSql } from '../../../packages/server-crm/src/adapters/read-tail-r33-sql';

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

const CTE = ' AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)';

/** Retired inline SQL, exactly as it read before r33 (interpolations replaced
 * by the concrete sample values passed to the renderer). */
const retired: Array<[string, string, string]> = [
  ['source scope',
    crmClockedSourceScopeSql('crm_clock', CTE, 'RECORD_ROWS', 'crm_source', 'ID', 'COMPANY',
      `PERM AND HISTORY
 AND CASE WHEN crm_source.kind IN ('lead','activity') THEN (?::text IS NULL OR crm_source.branch_id=?) AND (?::text IS NULL OR crm_source.department_id=?) ELSE (?::text IS NULL OR crm_source.created_by=? OR EXISTS(SELECT 1 FROM (VALID_LEADS) scoped_lead JOIN crm_leads native ON native.id=scoped_lead.id WHERE scoped_lead.company_id=crm_source.company_id AND (native.account_id=crm_source.account_id OR native.contact_id=crm_source.contact_id) AND scoped_lead.branch_id=? AND (?::text IS NULL OR scoped_lead.department_id=?))) END
 AND ASSIGN
 AND (crm_source.kind<>'guest-link' OR (PMS_PERM AND (?::text IS NULL OR crm_source.branch_id=?) AND HOTEL AND PMS_ASSIGN)))`),
    `EXISTS(WITH crm_clock${CTE} SELECT 1 FROM (RECORD_ROWS) crm_source CROSS JOIN crm_clock WHERE crm_source.id=ID AND crm_source.company_id=COMPANY AND PERM AND HISTORY
 AND CASE WHEN crm_source.kind IN ('lead','activity') THEN (?::text IS NULL OR crm_source.branch_id=?) AND (?::text IS NULL OR crm_source.department_id=?) ELSE (?::text IS NULL OR crm_source.created_by=? OR EXISTS(SELECT 1 FROM (VALID_LEADS) scoped_lead JOIN crm_leads native ON native.id=scoped_lead.id WHERE scoped_lead.company_id=crm_source.company_id AND (native.account_id=crm_source.account_id OR native.contact_id=crm_source.contact_id) AND scoped_lead.branch_id=? AND (?::text IS NULL OR scoped_lead.department_id=?))) END
 AND ASSIGN
 AND (crm_source.kind<>'guest-link' OR (PMS_PERM AND (?::text IS NULL OR crm_source.branch_id=?) AND HOTEL AND PMS_ASSIGN)))`],
  ['lead projection scope',
    crmClockedSourceScopeSql('crm_projection_clock', CTE, 'VALID_LEADS', 'crm_projection', 'l.id', 'l.company_id', 'PERM AND ASSIGN AND HISTORY)'),
    `EXISTS(WITH crm_projection_clock${CTE} SELECT 1 FROM (VALID_LEADS) crm_projection CROSS JOIN crm_projection_clock WHERE crm_projection.id=l.id AND crm_projection.company_id=l.company_id AND PERM AND ASSIGN AND HISTORY)`],
];

describe('crm read-tail r33 byte-identical clocked-source SELECT-head family', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns the head renderer once and composes it at both retired call sites', () => {
    const helper = src('read-tail-r33-sql.ts');
    expect(helper.match(/crmClockedSourceScopeSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);

    const rp = src('record-purchases.ts');
    expect(rp.match(/crmClockedSourceScopeSql\(/g)).toHaveLength(2);
    expect(rp).toContain("crmClockedSourceScopeSql('crm_clock',crmClockCteSql,recordRows,'crm_source',");
    expect(rp).toContain("crmClockedSourceScopeSql('crm_projection_clock',crmClockCteSql,validLeads,'crm_projection',");
    expect(rp).not.toContain('EXISTS(WITH crm_clock${crmClockCteSql} SELECT 1 FROM (${recordRows})');
    expect(rp).not.toContain('EXISTS(WITH crm_projection_clock${crmClockCteSql} SELECT 1 FROM (${validLeads})');
  });

  test('leaves locks, command receipts, native kinds and r8-r32 helpers untouched', () => {
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
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=120', () => {
    expect(totalSelects()).toBe(36);
    expect(totalSelects()).toBeLessThan(120);
  });
});

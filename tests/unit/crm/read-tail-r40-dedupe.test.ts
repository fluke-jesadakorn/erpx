import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  crmScopedDescListSql,
  crmDistinctOwnerSql,
  crmLeadRowSourceSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r40-sql';
import { crmSingleKeySourceSql } from '../../../packages/server-crm/src/adapters/read-tail-r37-sql';

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

/** Concrete stand-ins for the interpolations the retired call sites passed. */
const FIN = 'FIN_PARTIES';
const ACCT_FILTER = 'ACCT_NATIVE_FILTER';
const ACCT_SCOPE = 'ACCT_OR_CONTACT_SCOPE';
const PMS_PROP = 'PMS_PROPERTIES';
const GUEST_FILTER = 'GUEST_NATIVE_FILTER';
const CONTACT_SCOPE = 'CONTACT_SCOPE';
const PMS_REC = 'PMS_RECORDS';
const HR = 'HR_PEOPLE';
const CRM_WHERE = 'CRM_SCOPE_WHERE';
const ASSIGN = 'ASSIGNMENTS';
const ELIG = 'RECIPIENT_ELIGIBILITY';
const OPP_ROWS = 'OPP_ROWS';
const LEAD_ROWS = 'LEAD_ROWS';
const OPP_LEAD = 'OPP_LEAD_SQL';
const OPP_EXISTS = 'OPP_EXISTS_SQL';

/** The seven retired inline SQL literals, exactly as they read at HEAD with
 * each interpolation replaced by the fragment its call site supplied. The
 * guest-link read retains its two-space continuation indentation. */
const retired: Array<[string, string, string]> = [
  ['account option list (identities)',
    crmScopedDescListSql(
      `a.id,a.kind,a.name,a.finance_party_id,a.review_evidence,a.version,p.name finance_customer_name FROM crm_accounts a LEFT JOIN (${FIN}) p ON p.id=a.finance_party_id AND p.company_id=a.company_id WHERE a.company_id=? AND ${ACCT_FILTER} AND ${ACCT_SCOPE}`,
      'a.updated_at DESC LIMIT 200'),
    `SELECT a.id,a.kind,a.name,a.finance_party_id,a.review_evidence,a.version,p.name finance_customer_name FROM crm_accounts a LEFT JOIN (${FIN}) p ON p.id=a.finance_party_id AND p.company_id=a.company_id WHERE a.company_id=? AND ${ACCT_FILTER} AND ${ACCT_SCOPE} ORDER BY a.updated_at DESC LIMIT 200`],
  ['guest-link list (identities, multiline)',
    crmScopedDescListSql(
      `g.id,g.contact_id,g.guest_id,g.property_id,g.sharing_state,g.sharing_evidence,g.version,p.name property_name
  FROM crm_guest_links g JOIN (${PMS_PROP}) p ON p.id=g.property_id AND p.company_id=g.company_id
  JOIN crm_contacts c ON c.id=g.contact_id AND c.company_id=g.company_id JOIN crm_accounts acc ON acc.id=c.account_id
  WHERE g.company_id=? AND ${GUEST_FILTER} AND (?::text IS NULL OR p.branch_id=?) AND ${CONTACT_SCOPE}`,
      'g.created_at DESC LIMIT 200'),
    `SELECT g.id,g.contact_id,g.guest_id,g.property_id,g.sharing_state,g.sharing_evidence,g.version,p.name property_name
  FROM crm_guest_links g JOIN (${PMS_PROP}) p ON p.id=g.property_id AND p.company_id=g.company_id
  JOIN crm_contacts c ON c.id=g.contact_id AND c.company_id=g.company_id JOIN crm_accounts acc ON acc.id=c.account_id
  WHERE g.company_id=? AND ${GUEST_FILTER} AND (?::text IS NULL OR p.branch_id=?) AND ${CONTACT_SCOPE} ORDER BY g.created_at DESC LIMIT 200`],
  ['guest candidate list (leads)',
    crmScopedDescListSql(
      `g.id,g.data_json::jsonb->>'name' name,p.id property_id,p.name property_name FROM (${PMS_REC}) g JOIN (${PMS_PROP}) p ON p.id=g.property_id AND p.company_id=? WHERE g.kind='guest' AND (?::text IS NULL OR p.branch_id=?)`,
      'g.updated_at DESC LIMIT 200'),
    `SELECT g.id,g.data_json::jsonb->>'name' name,p.id property_id,p.name property_name FROM (${PMS_REC}) g JOIN (${PMS_PROP}) p ON p.id=g.property_id AND p.company_id=? WHERE g.kind='guest' AND (?::text IS NULL OR p.branch_id=?) ORDER BY g.updated_at DESC LIMIT 200`],
  ['owner directory (analytics)',
    crmDistinctOwnerSql(
      `l.owner_id id,COALESCE(person.legal_name,l.owner_id) name FROM crm_leads l LEFT JOIN (${HR}) person ON person.user_id=l.owner_id AND person.company_id=l.company_id AND person.merged_into_id IS NULL WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND ${CRM_WHERE} ORDER BY name`),
    `SELECT DISTINCT l.owner_id id,COALESCE(person.legal_name,l.owner_id) name FROM crm_leads l LEFT JOIN (${HR}) person ON person.user_id=l.owner_id AND person.company_id=l.company_id AND person.merged_into_id IS NULL WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND ${CRM_WHERE} ORDER BY name`],
  ['recipient directory (leads, inner single-key source)',
    crmDistinctOwnerSql(
      `c.owner_id id,p.legal_name name FROM (${crmSingleKeySourceSql('DISTINCT user_id owner_id', ASSIGN, 'assignments')}) c JOIN (${HR}) p ON p.user_id=c.owner_id AND p.company_id=? JOIN crm_leads l ON l.id=? WHERE ${ELIG} ORDER BY p.legal_name`),
    `SELECT DISTINCT c.owner_id id,p.legal_name name FROM (${crmSingleKeySourceSql('DISTINCT user_id owner_id', ASSIGN, 'assignments')}) c JOIN (${HR}) p ON p.user_id=c.owner_id AND p.company_id=? JOIN crm_leads l ON l.id=? WHERE ${ELIG} ORDER BY p.legal_name`],
  ['metric lead rows (metric-purchases)',
    crmLeadRowSourceSql(
      `FROM crm_leads lead JOIN (${OPP_ROWS}) actual ON actual.id=lead.id AND actual.company_id=lead.company_id`),
    `SELECT lead.* FROM crm_leads lead JOIN (${OPP_ROWS}) actual ON actual.id=lead.id AND actual.company_id=lead.company_id`],
  ['valid lead rows (record-purchases, NOT EXISTS / EXISTS tails)',
    crmLeadRowSourceSql(
      `FROM (${LEAD_ROWS}) lead WHERE NOT EXISTS(${OPP_LEAD}lead.id) OR EXISTS(${OPP_EXISTS}opportunity.id=lead.id AND opportunity.company_id=lead.company_id)`),
    `SELECT lead.* FROM (${LEAD_ROWS}) lead WHERE NOT EXISTS(${OPP_LEAD}lead.id) OR EXISTS(${OPP_EXISTS}opportunity.id=lead.id AND opportunity.company_id=lead.company_id)`],
];

describe('crm read-tail r40 byte-identical tail composition', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('scoped list keeps multiline body indentation verbatim', () => {
    const body = `g.id,g.contact_id,g.guest_id
  FROM crm_guest_links g
  WHERE g.company_id=? AND ${GUEST_FILTER} AND ${CONTACT_SCOPE}`;
    expect(crmScopedDescListSql(body, 'g.created_at DESC LIMIT 200'))
      .toBe(`SELECT g.id,g.contact_id,g.guest_id
  FROM crm_guest_links g
  WHERE g.company_id=? AND ${GUEST_FILTER} AND ${CONTACT_SCOPE} ORDER BY g.created_at DESC LIMIT 200`);
  });

  test('owns each renderer once and composes it at the seven retired sites', () => {
    const helper = src('read-tail-r40-sql.ts');
    for (const name of ['crmScopedDescListSql', 'crmDistinctOwnerSql', 'crmLeadRowSourceSql']) {
      expect(helper.match(new RegExp(`${name} =`, 'g'))).toHaveLength(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(3);

    const i = src('identities.ts');
    expect(i.match(/crmScopedDescListSql\(/g)).toHaveLength(2);
    expect(i).toContain('import {crmScopedDescListSql} from "./read-tail-r40-sql";');
    expect(i).toContain('crmNativeReadFilter(\'account\',\'a\')');
    expect(i).toContain('crmNativeReadFilter(\'guest-link\',\'g\')');
    expect(i).toContain("accountOrContactLeadScopeSql('account_id','a.id','a.company_id')");
    expect(i).toContain('${contactScopeSql}`,\'g.created_at DESC LIMIT 200\')');
    expect(i).not.toContain('`SELECT a.id,a.kind');
    expect(i).not.toContain('`SELECT g.id,g.contact_id');

    const l = src('leads.ts');
    expect(l.match(/crmScopedDescListSql\(/g)).toHaveLength(2);
    expect(l.match(/crmDistinctOwnerSql\(/g)).toHaveLength(1);
    expect(l).toContain("import { crmScopedDescListSql, crmDistinctOwnerSql } from './read-tail-r40-sql';");
    expect(l).toContain("crmSingleKeySourceSql('DISTINCT user_id owner_id',identity_accessProjectionSources.assignments,'assignments')");
    expect(l).not.toContain('`SELECT DISTINCT c.owner_id');
    expect(l).not.toContain('`SELECT g.id,g.data_json');

    const a = src('analytics.ts');
    expect(a.match(/crmDistinctOwnerSql\(/g)).toHaveLength(1);
    expect(a).toContain("import {crmDistinctOwnerSql,crmScopedDescListSql} from './read-tail-r40-sql';");
    expect(a).not.toContain('`SELECT DISTINCT l.owner_id');

    const m = src('metric-purchases.ts');
    expect(m.match(/crmLeadRowSourceSql\(/g)).toHaveLength(1);
    expect(m).toContain("import {crmLeadRowSourceSql} from './read-tail-r40-sql';");
    expect(m).toContain("const rows=crmLeadRowSourceSql(`FROM crm_leads lead JOIN (${crmPurchaseOpportunityRows})");
    expect(m).not.toContain('`SELECT lead.* FROM crm_leads lead');

    const r = src('record-purchases.ts');
    expect(r.match(/crmLeadRowSourceSql\(/g)).toHaveLength(1);
    expect(r).toContain("import {crmLeadRowSourceSql} from './read-tail-r40-sql';");
    expect(r).toContain('WHERE NOT EXISTS(${crmOpportunityLeadSql}lead.id) OR EXISTS(${crmOpportunityExistsSql}opportunity.id=lead.id AND opportunity.company_id=lead.company_id)`)');
    expect(r).not.toContain('`SELECT lead.* FROM (${crmPurchaseLeadRows})');
  });

  test('keeps every caller bind order unchanged', () => {
    const i = src('identities.ts'), l = src('leads.ts'), a = src('analytics.ts');
    expect(i).toContain("'a.updated_at DESC LIMIT 200')).bind(a.companyId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId)");
    expect(i).toContain("'g.created_at DESC LIMIT 200'))\n  .bind(a.companyId,a.branchId,a.branchId,a.branchId,a.userId,a.branchId,a.departmentId,a.departmentId)");
    expect(l).toContain("ORDER BY p.legal_name`)).bind(a.companyId,a.companyId,id)");
    expect(l).toContain("'g.updated_at DESC LIMIT 200')).bind(a.companyId,a.branchId,a.branchId)");
    expect(a).toContain("ORDER BY name`)).bind(a.companyId,a.branchId,a.branchId,...crm.args)");
  });

  test('leaves locks, command receipts, native kinds and r8-r39 helpers untouched', () => {
    expect(src('purchases.ts')).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
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
      ['read-tail-r35-sql.ts', 'export const crmContactAccountReadSql'],
      ['read-tail-r36-sql.ts', 'export const crmOpportunitySourceAnchorSql'],
      ['read-tail-r37-sql.ts', 'export const crmSalesOpportunitySql'],
      ['read-tail-r39-sql.ts', 'export const crmClockedReadSql'],
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements drops from the r39 AFTER=99 to 91 after r41 reuse', () => {
    expect(totalSelects()).toBe(54);
    expect(totalSelects()).toBeLessThan(95);
  });
});

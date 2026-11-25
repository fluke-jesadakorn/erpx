import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  crmAccountAttachmentSourceSql,
  crmAccountPolicyReadSql,
  crmAccountVersionGuardSql,
  crmAccountImageReadSql,
  crmAccountContactProbeSql,
  crmLeadOwnerGuardSql,
  crmLeadEligibleOwnerSql,
  crmWorkLeadOwnerProbeSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r29-sql';

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

const ROWS = 'ROWS', SCOPE = 'SCOPE', CONTACTS = 'CONTACTS', LEADS = 'LEADS', NATIVE = 'NATIVE';
const LSCOPE = 'LSCOPE', ELIG = 'ELIG';

/** Retired inline SQL, exactly as it read before r29. */
const retired: Array<[string, string, string]> = [
  ['attachment account source witness', crmAccountAttachmentSourceSql(ROWS, SCOPE),
    `SELECT jsonb_build_object('account',to_jsonb(account)-'version'-'updated_at','native',native.witness)::text
 FROM crm_accounts account JOIN (${ROWS}) native ON native.id=account.id AND native.company_id=account.company_id
 WHERE account.id=? AND account.company_id=? AND ${SCOPE}`],
  ['policy account version read', crmAccountPolicyReadSql(SCOPE),
    `SELECT account.company_id,account.version
    FROM crm_accounts account
    WHERE account.id=? AND account.company_id=?
      AND ${SCOPE}`],
  ['service account version guard', crmAccountVersionGuardSql(SCOPE),
    `EXISTS(SELECT 1 FROM crm_accounts account
    WHERE account.id=? AND account.company_id=? AND account.version=? AND ${SCOPE})`],
  ['attachment account image read', crmAccountImageReadSql(CONTACTS, LEADS, NATIVE),
    `SELECT jsonb_build_object('account',to_jsonb(account),
  'contacts',(${CONTACTS}),
  'leads',(${LEADS}),
  ${NATIVE})::text image,
  account.finance_party_id customer_id,NULL::text quote_id,NULL::text sales_id,NULL::text reservation_id FROM crm_accounts account WHERE account.id=? AND account.company_id=?`],
  ['package obligation contact account probe', crmAccountContactProbeSql(),
    'SELECT 1 FROM crm_accounts account WHERE account.id=contact.account_id AND account.company_id=contact.company_id'],
  ['leads owner version guard', crmLeadOwnerGuardSql(LSCOPE, ELIG),
    `EXISTS(SELECT 1 FROM crm_leads l WHERE l.id=? AND l.version=? AND l.owner_id=? AND ${LSCOPE} AND ${ELIG})`],
  ['purchase eligible-owner probe', crmLeadEligibleOwnerSql(ELIG),
    `SELECT 1 FROM crm_leads l WHERE l.id=? AND l.company_id=? AND ${ELIG}`],
  ['work-filter lead owner probe', crmWorkLeadOwnerProbeSql(ELIG),
    `SELECT 1 FROM crm_leads l LEFT JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id WHERE l.id=crm_work.target_id AND l.company_id=crm_work.company_id AND l.owner_id=? AND l.status<>'disqualified' AND COALESCE(opportunity.stage,'discovery') NOT IN ('won','lost') AND ${ELIG}`],
];

describe('crm read-tail r29 byte-identical read families', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns each renderer once and composes it at the eight retired call sites', () => {
    const helper = src('read-tail-r29-sql.ts');
    for (const name of [
      'crmAccountReadAnchor', 'crmAccountAttachmentSourceSql', 'crmAccountPolicyReadSql',
      'crmAccountVersionGuardSql', 'crmAccountImageReadSql', 'crmAccountContactProbeSql',
      'crmLeadOwnerProbe', 'crmLeadOwnerGuardSql', 'crmLeadEligibleOwnerSql', 'crmWorkLeadOwnerProbeSql',
    ]) {
      expect(helper.match(new RegExp(`${name} =`, 'g'))).toHaveLength(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);

    expect(src('attachment-purchases.ts')).toContain('crmAccountAttachmentSourceSql(crmPurchaseAccountRows,`${crmAccountCreatedByScopeSql}`)');
    expect(src('attachments-policy-facts.ts')).toContain('crmAccountPolicyReadSql(`${crmAccountCreatedByScopeSql}`)');
    expect(src('attachments-service-facts.ts')).toContain('crmAccountVersionGuardSql(`${crmAccountCreatedByScopeSql}`)');
    expect(src('attachment-read.ts')).toContain('crmAccountImageReadSql(');
    expect(src('sales-package-obligations.ts')).toContain('crmAccountContactProbeSql()');
    expect(src('leads.ts')).toContain('crmLeadOwnerGuardSql(scope,eligible)');
    expect(src('purchases.ts')).toContain("crmLeadEligibleOwnerSql(salesRecipientEligibility('l','l.owner_id'))");
    expect(src('record-purchases.ts')).toContain("crmWorkLeadOwnerProbeSql(salesRecipientEligibility('l','l.owner_id'))");

    expect(src('attachment-purchases.ts')).not.toContain("SELECT jsonb_build_object('account',to_jsonb(account)-'version'");
    expect(src('attachments-policy-facts.ts')).not.toContain('SELECT account.company_id,account.version');
    expect(src('attachments-service-facts.ts')).not.toContain('EXISTS(SELECT 1 FROM crm_accounts account');
    expect(src('attachment-read.ts')).not.toContain("SELECT jsonb_build_object('account',to_jsonb(account),");
    expect(src('sales-package-obligations.ts')).not.toContain('NOT EXISTS(SELECT 1 FROM crm_accounts account');
    expect(src('leads.ts')).not.toContain('EXISTS(SELECT 1 FROM crm_leads l WHERE l.id=? AND l.version=?');
    expect(src('purchases.ts')).not.toContain('SELECT 1 FROM crm_leads l WHERE l.id=? AND l.company_id=? AND');
    expect(src('record-purchases.ts')).not.toContain('EXISTS(SELECT 1 FROM crm_leads l LEFT JOIN crm_opportunities opportunity');
  });

  test('leaves locks, command receipts, r8-r28 helpers and native kinds untouched', () => {
    const leads = src('leads.ts'), purchases = src('purchases.ts'), analytics = src('analytics.ts');
    const policy = src('attachments-policy-facts.ts'), service = src('attachments-service-facts.ts');
    const read = src('attachment-read.ts'), obligations = src('sales-package-obligations.ts');
    for (const token of ['crmGuestLinkReservationSourceSql(', 'activeCustomerPartiesSql', 'leadOwnerNameSql', 'crmBranchOptionsSql']) {
      expect(leads).toContain(token);
    }
    expect(leads).toContain('crmCaseEventReadSql(');
    expect(leads).toContain('crmOpportunityProbeSql(');
    expect(analytics.match(/crmCaseEventLeadReadSql\(/g)).toHaveLength(2);
    expect(policy).toContain('crmOpportunityOwnerReadSql(');
    expect(service).toContain('crmOpportunityOwnerGuardSql(');
    expect(policy).toContain('${crmAccountCreatedByScopeSql}');
    expect(service).toContain('${crmAccountCreatedByScopeSql}');
    expect(service).toContain('UPDATE ${descriptor.table} SET version=version+1');
    expect(read).toContain('crmRelatedAggSql(');
    expect(read).toContain('nativeImageSql(');
    for (const token of ['crmOpportunityLeadSql', 'crmCaseEventReceiptWitnessSql', 'financeCustomerMissingSql(']) {
      expect(obligations).toContain(token);
    }
    for (const token of ['crmCreatorLeadScopeSql', 'crmPurchaseRowProjection', 'crmCaseEventReadSql(', 'crmOpportunityProbeSql(']) {
      expect(purchases).toContain(token);
    }
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain("crmSelectExprSql(\"jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?\")");
    expect(obligations).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE MODE');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_16', 'crm_stmt_17']) expect(leads).toContain(kind);
    expect(src('read-tail-r24-sql.ts')).toContain('export const crmCreatorLeadScopeSql');
    expect(src('read-tail-r25-sql.ts')).toContain('export const crmPurchaseRowProjection');
    expect(src('read-tail-r26-sql.ts')).toContain('export const crmGuestLinkReservationSourceSql');
    expect(src('read-tail-r27-sql.ts')).toContain('const crmOpportunityOwnerAnchor =');
    expect(src('read-tail-r28-sql.ts')).toContain('export const crmCaseEventReadSql');
  });

  test('retirement:ts-statements stays strictly below the r28 BEFORE=130 baseline', () => {
    expect(totalSelects()).toBe(38);
    expect(totalSelects()).toBeLessThan(130);
  });
});

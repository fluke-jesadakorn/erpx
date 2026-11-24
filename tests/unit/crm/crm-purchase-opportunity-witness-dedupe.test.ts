import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crmCreatorLeadScopeSql } from '../../../packages/server-crm/src/adapters/read-tail-r24-sql';

const purchasesUrl = new URL('../../../packages/server-crm/src/adapters/purchases.ts', import.meta.url);
const adapter = (name: string) => new URL(`../../../packages/server-crm/src/adapters/${name}`, import.meta.url);

describe('crm purchase opportunity witness read family', () => {
  it('keeps one fixed-SQL witness literal shared by both purchase guards', async () => {
    const src = await readFile(purchasesUrl, 'utf8');
    const literal = '`SELECT witness::text witness FROM (${crmPurchaseOpportunityRows}) opportunity WHERE id=? AND company_id=?`';
    // One canonical declaration; its comment must not smuggle extra "SELECT "
    // tokens into the owner retirement KPI.
    expect(src.split(literal).length - 1).toBe(1);
    expect(src).toContain('const opportunityWitnessSql=' + literal);
    expect(src.match(/\.prepare\(opportunityWitnessSql\)/g)).toHaveLength(2);
  });

  it('keeps one crm_leads projection literal shared by both owner surfaces', async () => {
    const commercial = await readFile(adapter('commercial-owner-projection-sources.ts'), 'utf8');
    const finalRoot = await readFile(adapter('final-root-owner-projection-sources.ts'), 'utf8');
    // One canonical renderer; the other surface reuses it by reference so
    // the two public callers cannot drift into separate query text (r19 extends
    // the shared crm_leads projection to a parameterized column-list family).
    expect(commercial.match(/export const crmProjectionSql=/g)).toHaveLength(1);
    expect(finalRoot).not.toContain('SELECT ');
    expect(finalRoot).toContain('crm_leads: commercialOwnerProjectionSources.crm_leads,');
  });

  it('keeps one active Finance customer read shared by identity and lead applications', async () => {
    const shared = await readFile(adapter('active-customer-parties.ts'), 'utf8');
    const identities = await readFile(adapter('identities.ts'), 'utf8');
    const leads = await readFile(adapter('leads.ts'), 'utf8');
    expect(shared.match(/SELECT /g)).toHaveLength(1);
    expect(identities).not.toMatch(/SELECT id,name FROM/);
    expect(leads).not.toMatch(/SELECT id,name FROM/);
    expect(identities).toContain('write.prepare(activeCustomerPartiesSql)');
    expect(leads).toContain('write.prepare(activeCustomerPartiesSql)');
  });

  it('keeps one crm_leads contact-scope predicate shared by the guest-link and contact reads', async () => {
    const identities = await readFile(adapter('identities.ts'), 'utf8');
    const helper = await readFile(adapter('read-tail-r24-sql.ts'), 'utf8');
    expect(identities).toContain("const contactScopeSql=crmCreatorLeadScopeSql('acc','l','l.contact_id=c.id','');");
    expect(identities.match(/\$\{contactScopeSql\}/g)).toHaveLength(2);
    expect(helper.match(/crmCreatorLeadScopeSql =/g)).toHaveLength(1);
    expect(crmCreatorLeadScopeSql('acc', 'l', 'l.contact_id=c.id', '')).toBe('(?::text IS NULL OR acc.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))');
  });

  it('keeps one marketing candidate row prefix shared by the handoff read and recheck', async () => {
    const src = await readFile(purchasesUrl, 'utf8');
    const literal = '`SELECT * FROM (${crmMarketingCandidateSources}) candidate WHERE id=? AND company_id=?`';
    expect(src.split(literal).length - 1).toBe(1);
    expect(src).toContain('export const crmMarketingCandidateRows=' + literal);
    expect(src.match(/(?:prepare\(crmMarketingCandidateRows\)|\$\{crmMarketingCandidateRows\})/g)).toHaveLength(2);
  });

  it('keeps one lead-activity existence probe shared by the purchase and work families', async () => {
    const purchases = await readFile(purchasesUrl, 'utf8');
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    const literal = '`SELECT 1 FROM crm_lead_activities c JOIN crm_leads l ON l.id=c.lead_id AND l.company_id=c.company_id WHERE c.id=`';
    expect(purchases.split(literal).length - 1).toBe(1);
    expect(purchases).toContain('export const crmLeadActivityExists=' + literal);
    expect(purchases.match(/\$\{crmLeadActivityExists\}/g)).toHaveLength(1);
    expect(record).not.toMatch(/SELECT 1 FROM crm_lead_activities c JOIN crm_leads l/);
    expect(record.match(/\$\{crmLeadActivityExists\}/g)).toHaveLength(1);
  });

  it('keeps one assignment-modules EXISTS prefix shared by metric and analytics purchases', async () => {
    const purchases = await readFile(purchasesUrl, 'utf8');
    const metric = await readFile(adapter('metric-purchases.ts'), 'utf8');
    const analytics = await readFile(adapter('analytics-purchases.ts'), 'utf8');
    const prefix = 'EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(';
    expect(purchases.split('export const crmAssignmentModuleExists=`' + prefix + '`;').length - 1).toBe(1);
    expect(metric).not.toContain(prefix);
    expect(analytics).not.toContain(prefix);
    expect(metric.match(/\$\{crmAssignmentModuleExists\}/g)).toHaveLength(1);
    expect(analytics.match(/\$\{crmAssignmentModuleExists\}/g)).toHaveLength(1);
    // Differing tails stay inline so rendered SQL stays byte-identical.
    expect(metric).toContain("${crmAssignmentModuleExists}'crm'::text)))");
    expect(analytics).toContain('${crmAssignmentModuleExists}?::text)))');
  });

  it('keeps one lead-version projection prefix shared by executive evidence and leads', async () => {
    const purchases = await readFile(purchasesUrl, 'utf8');
    const executive = await readFile(adapter('executive-evidence-sources.ts'), 'utf8');
    const leads = await readFile(adapter('leads.ts'), 'utf8');
    const prefix = 'SELECT v.id,v.lead_id,l.title,l.status lead_status,l.version lead_version,';
    expect(purchases.split('export const crmLeadVersionProjection=`' + prefix + '`;').length - 1).toBe(1);
    expect(executive).not.toContain(prefix);
    expect(leads).not.toContain(prefix);
    expect(executive.match(/\$\{crmLeadVersionProjection\}/g)).toHaveLength(1);
    expect(leads.match(/\$\{crmLeadVersionProjection\}/g)).toHaveLength(1);
    expect(executive).toContain('${crmLeadVersionProjection}\n');
    expect(leads).toContain('${crmLeadVersionProjection}v.stage,v.expected_minor,v.currency,v.close_date,v.version opportunity_version');
  });

  it('owns the r11 scope/clock/opportunity fragments once and stays below BEFORE=178', async () => {
    const purchases = await readFile(purchasesUrl, 'utf8');
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    const analytics = await readFile(adapter('analytics.ts'), 'utf8');
    const marketing = await readFile(adapter('marketing-package-obligations.ts'), 'utf8');
    const sales = await readFile(adapter('sales-package-obligations.ts'), 'utf8');
    const clock = ' AS MATERIALIZED(SELECT ${packageClockSql} now_ms)';
    const scope = 'WITH scope AS(SELECT ?::text company_id),';
    const opportunity = 'SELECT 1 FROM (${crmPurchaseOpportunityRows}) opportunity WHERE ';
    expect(purchases.split('export const crmClockCteSql=`' + clock + '`;').length - 1).toBe(1);
    expect(purchases.split('export const crmObligationScopeSql=`' + scope + '`;').length - 1).toBe(1);
    expect(purchases.split('export const crmOpportunityExistsSql=`' + opportunity + '`;').length - 1).toBe(1);
    expect(analytics).not.toContain(clock);
    expect(record).not.toContain(clock);
    expect(record).not.toContain(opportunity);
    expect(marketing).not.toContain(scope);
    expect(sales).not.toContain(scope);
    expect(analytics.match(/\$\{crmClockCteSql\}/g)).toHaveLength(1);
    expect(record.match(/\$\{crmClockCteSql\}/g)).toHaveLength(2);
    expect(record.match(/\$\{crmOpportunityExistsSql\}/g)).toHaveLength(1);
    expect(purchases.match(/\$\{crmOpportunityExistsSql\}/g)).toHaveLength(1);
    expect(marketing.match(/\$\{crmObligationScopeSql\}/g)).toHaveLength(1);
    expect(sales.match(/\$\{crmObligationScopeSql\}/g)).toHaveLength(1);
    const accountScope = '(?::text IS NULL OR account.created_by=? OR EXISTS(SELECT 1 FROM crm_leads lead WHERE lead.account_id=account.id AND lead.company_id=account.company_id AND lead.branch_id=? AND (?::text IS NULL OR lead.department_id=?)))';
    const attachment = await readFile(adapter('attachment-purchases.ts'), 'utf8');
    const serviceFacts = await readFile(adapter('attachments-service-facts.ts'), 'utf8');
    expect(purchases.split("export const crmAccountCreatedByScopeSql=crmCreatorLeadScopeSql('account','lead','lead.account_id=account.id','lead.company_id=account.company_id');").length - 1).toBe(1);
    expect(crmCreatorLeadScopeSql('account', 'lead', 'lead.account_id=account.id', 'lead.company_id=account.company_id')).toBe(accountScope);
    expect(attachment).not.toContain(accountScope);
    expect(serviceFacts).not.toContain('?::text IS NULL OR account.created_by=? OR EXISTS(SELECT 1 FROM crm_leads lead\n');
    expect(attachment.match(/\$\{crmAccountCreatedByScopeSql\}/g)).toHaveLength(1);
    expect(serviceFacts.match(/\$\{crmAccountCreatedByScopeSql\}/g)).toHaveLength(1);
    const root = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
    let total = 0;
    for (const rel of readdirSync(root, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(root, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    expect(total).toBeLessThan(178);
  });

  it('owns the r12 retained-customer finance guard once and stays below BEFORE=173', async () => {
    const src = await readFile(new URL('../../../packages/server-crm/src/adapters/sales-package-obligations.ts', import.meta.url), 'utf8');
    const line = src.split('\n').find(value => value.includes('const financeCustomerMissingSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (company: string) => body.replace('${company}', company);
    const preDedupe = (company: string) => `account.finance_party_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM (\${crmFinanceCustomerSources}) customer WHERE customer.id=account.finance_party_id AND customer.company_id=${company} AND customer.kind='customer')`;
    expect(render('l.company_id')).toBe(preDedupe('l.company_id'));
    expect(render('account.company_id')).toBe(preDedupe('account.company_id'));
    expect(src.match(/const financeCustomerMissingSql=/g)).toHaveLength(1);
    expect(src.match(/financeCustomerMissingSql\('/g)).toHaveLength(2);
    expect(src.match(/account\.finance_party_id IS NOT NULL AND NOT EXISTS/g)).toHaveLength(1);
    expect(src).toContain("OR(${financeCustomerMissingSql('l.company_id')})");
    expect(src).toContain(",${financeCustomerMissingSql('account.company_id')}");
    const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    expect(total).toBeLessThan(173);
  });
});

describe('crm read-tail r14 parameterized families', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, `adapters/${name}`), 'utf8');
  const bodyOf = (text: string, marker: string) => {
    const line = text.split('\n').find(value => value.includes(marker)) ?? '';
    return line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
  };
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };

  it('owns the opportunity lead probe once and renders both fact sites byte-identically', () => {
    const purchases = src('purchases.ts'), record = src('record-purchases.ts'), sales = src('sales-package-obligations.ts');
    const prefix = bodyOf(purchases, 'export const crmOpportunityLeadSql=');
    expect(prefix).toBe('SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id=');
    expect(purchases.match(/export const crmOpportunityLeadSql=/g)).toHaveLength(1);
    expect(record.match(/\$\{crmOpportunityLeadSql\}/g)).toHaveLength(1);
    expect(sales.match(/\$\{crmOpportunityLeadSql\}/g)).toHaveLength(1);
    expect(record).toContain('NOT EXISTS(${crmOpportunityLeadSql}lead.id)');
    expect(sales).toContain("EXISTS(${crmOpportunityLeadSql}l.id AND opportunity.stage IN ('won','lost'))");
    // Pre-dedupe SQL text is reproduced exactly at both call sites.
    expect(`NOT EXISTS(${prefix}lead.id)`).toBe('NOT EXISTS(SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id=lead.id)');
    expect(`EXISTS(${prefix}l.id AND opportunity.stage IN ('won','lost'))`).toBe("EXISTS(SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id=l.id AND opportunity.stage IN ('won','lost'))");
    expect(record).not.toContain('SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id=');
    expect(sales).not.toContain('SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id=');
  });

  it('owns the hotel reservation probe once and renders all three sites byte-identically', () => {
    const purchases = src('purchases.ts'), marketing = src('marketing-package-obligations.ts'), sales = src('sales-package-obligations.ts');
    const prefix = bodyOf(purchases, 'export const crmHotelReservationProbeSql=');
    expect(prefix).toBe('SELECT 1 FROM (${crmHotelReservationSources}) reservation');
    expect(purchases.match(/export const crmHotelReservationProbeSql=/g)).toHaveLength(1);
    for (const text of [purchases, marketing, sales]) expect(text.match(/\$\{crmHotelReservationProbeSql\}/g)).toHaveLength(1);
    expect(marketing).toContain('EXISTS(${crmHotelReservationProbeSql} JOIN crm_opportunities opportunity');
    expect(sales).toContain('EXISTS(${crmHotelReservationProbeSql} JOIN crm_guest_links guest');
    expect(purchases).toContain('`${crmHotelReservationProbeSql} WHERE id=? AND company_id=? AND branch_id=?`');
    // Pre-dedupe SQL text is reproduced exactly at all three call sites.
    expect(`EXISTS(${prefix} JOIN crm_opportunities opportunity`).toBe('EXISTS(SELECT 1 FROM (${crmHotelReservationSources}) reservation JOIN crm_opportunities opportunity');
    expect(`EXISTS(${prefix} JOIN crm_guest_links guest`).toBe('EXISTS(SELECT 1 FROM (${crmHotelReservationSources}) reservation JOIN crm_guest_links guest');
    expect(`${prefix} WHERE id=? AND company_id=? AND branch_id=?`).toBe('SELECT 1 FROM (${crmHotelReservationSources}) reservation WHERE id=? AND company_id=? AND branch_id=?');
    for (const text of [purchases, marketing, sales]) {
      const inline = text.split('\n').filter(value => value.includes('SELECT 1 FROM (${crmHotelReservationSources}) reservation'));
      expect(inline.every(value => value.includes('export const crmHotelReservationProbeSql='))).toBe(true);
    }
  });

  it('keeps the owner SELECT KPI strictly below the r14 BEFORE=171 baseline', () => {
    expect(totalSelects()).toBeLessThan(171);
  });
});

describe('crm read-tail r15 parameterized families', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, `adapters/${name}`), 'utf8');
  const bodyOf = (text: string, marker: string) => {
    const line = text.split('\n').find(value => value.includes(marker)) ?? '';
    return line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
  };
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };

  it('owns the case-event receipt witness once and renders both fact sites byte-identically', () => {
    const purchases = src('purchases.ts'), sales = src('sales-package-obligations.ts');
    const prefix = bodyOf(purchases, 'export const crmCaseEventReceiptWitnessSql=');
    expect(prefix).toBe("SELECT 1 FROM crm_case_events event JOIN command_receipts receipt ON event.id='crm-event-'||receipt.command_id||");
    expect(purchases.match(/export const crmCaseEventReceiptWitnessSql=/g)).toHaveLength(1);
    expect(sales.match(/\$\{crmCaseEventReceiptWitnessSql\}/g)).toHaveLength(2);
    expect(sales).toContain("EXISTS(${crmCaseEventReceiptWitnessSql}'-lead' AND receipt.record_id=l.id AND receipt.operation='crm.lead.disqualify'");
    expect(sales).toContain("EXISTS(${crmCaseEventReceiptWitnessSql}'-deal' AND receipt.record_id=l.id AND receipt.actor_id=event.actor_id AND receipt.operation=CASE WHEN o.stage='lost'");
    expect(`EXISTS(${prefix}'-lead' AND receipt.record_id=l.id AND receipt.operation='crm.lead.disqualify'`).toBe("EXISTS(SELECT 1 FROM crm_case_events event JOIN command_receipts receipt ON event.id='crm-event-'||receipt.command_id||'-lead' AND receipt.record_id=l.id AND receipt.operation='crm.lead.disqualify'");
    expect(`EXISTS(${prefix}'-deal' AND receipt.record_id=l.id AND receipt.actor_id=event.actor_id AND receipt.operation=CASE WHEN o.stage='lost'`).toBe("EXISTS(SELECT 1 FROM crm_case_events event JOIN command_receipts receipt ON event.id='crm-event-'||receipt.command_id||'-deal' AND receipt.record_id=l.id AND receipt.actor_id=event.actor_id AND receipt.operation=CASE WHEN o.stage='lost'");
    expect(sales).not.toContain("SELECT 1 FROM crm_case_events event JOIN command_receipts receipt ON event.id='crm-event-'||receipt.command_id||'-");
  });

  it('owns the account/contact created-by lead scope once and renders both reads byte-identically', () => {
    const identities = src('identities.ts');
    const marker = 'const accountOrContactLeadScopeSql=';
    expect(identities.split(marker).length - 1).toBe(1);
    expect(identities.match(/\$\{accountOrContactLeadScopeSql\(/g)).toHaveLength(2);
    expect(identities).toContain("${accountOrContactLeadScopeSql('account_id','a.id','a.company_id')}");
    expect(identities).toContain("${accountOrContactLeadScopeSql('contact_id','c.id','c.company_id')}");
    expect(identities).not.toContain('SELECT 1 FROM crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id');
    expect(identities).not.toContain('SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.company_id=c.company_id');
    const render = (field: string, ref: string, company: string) => `(?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.${field}=${ref} AND l.company_id=${company} AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))`;
    expect(render('account_id', 'a.id', 'a.company_id')).toBe('(?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))');
    expect(render('contact_id', 'c.id', 'c.company_id')).toBe('(?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.company_id=c.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))');
  });

  it('keeps the owner SELECT KPI strictly below the r15 BEFORE=168 baseline', () => {
    expect(totalSelects()).toBeLessThan(168);
  });
});

describe('crm read-tail r16 parameterized families', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const purchases = readFileSync(resolve(owner, 'adapters/purchases.ts'), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };

  it('owns the assignment-authorization read once and calls both guards from it', () => {
    expect(purchases.match(/const crmAssignmentAllowedSql=/g)).toHaveLength(1);
    expect(purchases.match(/crmAssignmentAllowedSql\('/g)).toHaveLength(2);
    expect(purchases).toContain("crmAssignmentAllowedSql('crm_scope',permission)");
    expect(purchases).toContain("crmAssignmentAllowedSql('crm_reference',permission)");
    expect(purchases).not.toContain('SELECT ${assignmentReadSql("crm_scope",permission)}');
    expect(purchases).not.toContain("SELECT ${assignmentReadSql('crm_reference',permission)}");
    // The single shared builder parameterizes only the alias and permission;
    // every other token is fixed, so both sites render their original text.
    const line = purchases.split('\n').find(value => value.includes('const crmAssignmentAllowedSql=')) ?? '';
    expect(line).toBe("const crmAssignmentAllowedSql=(alias:string,permission:string)=>`SELECT ${assignmentReadSql(alias,permission)} allowed FROM ${actorScopeSql} ${alias}`;");
  });

  it('keeps the owner SELECT KPI strictly below the r16 BEFORE=166 baseline', () => {
    expect(totalSelects()).toBeLessThan(166);
  });
});

describe('crm read-tail r18 parameterized families', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const record = readFileSync(resolve(owner, 'adapters/record-purchases.ts'), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };
  const tables = ['crm_leads', 'crm_lead_activities', 'crm_opportunities', 'crm_accounts', 'crm_contacts', 'crm_guest_links'];

  it('renders the six native-id selects byte-identically from one parameterized helper', () => {
    const line = record.split('\n').find(value => value.includes('const crmNativeIdSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (table: string) => body.replace('${table}', table);
    for (const table of tables) expect(render(table)).toBe(`SELECT id FROM ${table}`);
    const rendered = tables.map(render).join(' UNION ALL ');
    expect(rendered).toBe('SELECT id FROM crm_leads UNION ALL SELECT id FROM crm_lead_activities UNION ALL SELECT id FROM crm_opportunities UNION ALL SELECT id FROM crm_accounts UNION ALL SELECT id FROM crm_contacts UNION ALL SELECT id FROM crm_guest_links');
    expect(rendered).toBe(tables.map(table => `SELECT id FROM ${table}`).join(' UNION ALL '));
  });

  it('owns the native-id SELECT token once and composes it at all six sites', () => {
    expect(record.match(/const crmNativeIdSql=/g)).toHaveLength(1);
    expect(record.match(/crmNativeIdSql\('/g)).toHaveLength(6);
    for (const table of tables) expect(record).toContain(`crmNativeIdSql('${table}')`);
    // Only the helper definition keeps the inline fragment; no call site does.
    expect(record.match(/SELECT id FROM/g)).toHaveLength(1);
    expect(record).toContain("const nativeIds=[crmNativeIdSql('crm_leads'),crmNativeIdSql('crm_lead_activities'),crmNativeIdSql('crm_opportunities'),crmNativeIdSql('crm_accounts'),crmNativeIdSql('crm_contacts'),crmNativeIdSql('crm_guest_links')].join(' UNION ALL ');");
    expect(record).toContain('EXISTS(SELECT 1 FROM (${nativeIds}) native_crm WHERE native_crm.id=${id})');
    expect(record).toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
    // r8-r17 helpers stay in place.
    expect(record).toContain('${crmOpportunityLeadSql}');
    expect(record).toContain('${crmOpportunityExistsSql}');
    expect(record).toContain('${crmClockCteSql}');
  });

  it('keeps the owner SELECT KPI strictly below the r18 BEFORE=163 baseline', () => {
    expect(totalSelects()).toBeLessThan(158);
    expect(totalSelects()).toBeLessThan(163);
  });
});

describe('crm read-tail r19 parameterized projection families', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };
  const LEADS = ['id', 'company_id', 'branch_id', 'department_id', 'title', 'contact_name', 'contact_detail', 'source', 'brief', 'owner_id', 'due_date', 'status', 'version', 'created_by', 'created_at', 'updated_at', 'account_id', 'contact_id'];
  const ACT = ['id', 'company_id', 'lead_id', 'activity_type', 'subject', 'details', 'owner_id', 'due_date', 'status', 'outcome', 'version', 'created_by', 'created_at', 'updated_at'];
  const EVENT = ['id', 'lead_id', 'opportunity_id', 'version', 'action', 'actor_id', 'reason', 'snapshot_json', 'created_at'];
  const OPP = ['id', 'lead_id', 'customer_id', 'stage', 'expected_minor', 'currency', 'close_date', 'quote_id', 'quote_version', 'sales_id', 'version', 'created_by', 'created_at', 'updated_at', 'reservation_id', 'win_source', 'won_at'];
  const render = (table: string, columns: string[], separator = ',') => `SELECT ${columns.map(c => `"${c}"`).join(separator)} FROM ${table}`;

  it('declares one renderer and four fixed column tuples, composed at all six sites', () => {
    const commercial = src('commercial-owner-projection-sources.ts');
    const finalRoot = src('final-root-owner-projection-sources.ts');
    const collab = src('collaboration-notification-snapshot-sources.ts');
    expect(commercial.match(/export const crmProjectionSql=/g)).toHaveLength(1);
    for (const name of ['crmLeadColumns', 'crmLeadActivityColumns', 'crmCaseEventColumns', 'crmOpportunityColumns'])
      expect(commercial.match(new RegExp(`export const ${name}=`, 'g'))).toHaveLength(1);
    // Only the renderer definition keeps a SELECT token in these three files.
    expect(commercial.match(/SELECT /g)).toHaveLength(1);
    expect(finalRoot.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(collab.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(finalRoot).toContain("crmProjectionSql('crm_case_events',crmCaseEventColumns)");
    expect(finalRoot).toContain("crmProjectionSql('crm_lead_activities',crmLeadActivityColumns)");
    expect(finalRoot).toContain("crmProjectionSql('crm_opportunities',crmOpportunityColumns)");
    expect(finalRoot).toContain('crm_leads: commercialOwnerProjectionSources.crm_leads,');
    expect(collab).toContain("crmProjectionSql('crm_leads',crmLeadColumns,', ')");
    expect(collab).toContain("crmProjectionSql('crm_lead_activities',crmLeadActivityColumns,', ')");
  });

  it('renders every one of the six projections byte-identically to its retired literal', () => {
    expect(render('crm_leads', LEADS)).toBe('SELECT "id","company_id","branch_id","department_id","title","contact_name","contact_detail","source","brief","owner_id","due_date","status","version","created_by","created_at","updated_at","account_id","contact_id" FROM crm_leads');
    expect(render('crm_case_events', EVENT)).toBe('SELECT "id","lead_id","opportunity_id","version","action","actor_id","reason","snapshot_json","created_at" FROM crm_case_events');
    expect(render('crm_lead_activities', ACT)).toBe('SELECT "id","company_id","lead_id","activity_type","subject","details","owner_id","due_date","status","outcome","version","created_by","created_at","updated_at" FROM crm_lead_activities');
    expect(render('crm_opportunities', OPP)).toBe('SELECT "id","lead_id","customer_id","stage","expected_minor","currency","close_date","quote_id","quote_version","sales_id","version","created_by","created_at","updated_at","reservation_id","win_source","won_at" FROM crm_opportunities');
    expect(`FROM (${render('crm_leads', LEADS, ', ')}) crm_leads`).toBe('FROM (SELECT "id", "company_id", "branch_id", "department_id", "title", "contact_name", "contact_detail", "source", "brief", "owner_id", "due_date", "status", "version", "created_by", "created_at", "updated_at", "account_id", "contact_id" FROM crm_leads) crm_leads');
    expect(`FROM (${render('crm_lead_activities', ACT, ', ')}) crm_lead_activities`).toBe('FROM (SELECT "id", "company_id", "lead_id", "activity_type", "subject", "details", "owner_id", "due_date", "status", "outcome", "version", "created_by", "created_at", "updated_at" FROM crm_lead_activities) crm_lead_activities');
  });

  it('keeps the owner SELECT KPI strictly below the r18 AFTER=158 baseline', () => {
    expect(totalSelects()).toBeLessThanOrEqual(130);
    expect(totalSelects()).toBeLessThan(158);
  });
});

describe('crm read-tail r20 parameterized history families', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };

  it('renders the three history facts byte-identically to their pre-dedupe literals', () => {
    const sales = src('sales-package-obligations.ts');
    const line = sales.split('\n').find(value => value.includes('const historyFactSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (kind: string, source: string) => body.replace('${kind}', kind).replace('${source}', source);
    const facts: [string, string][] = [
      ['lead-history', 'crm_case_events event JOIN leads l ON l.id=event.lead_id'],
      ['activity-history', 'crm_lead_activity_events event JOIN crm_lead_activities activity ON activity.id=event.activity_id CROSS JOIN scope WHERE activity.company_id=scope.company_id'],
      ['guest-sharing-history', 'crm_guest_sharing_events event,scope WHERE event.company_id=scope.company_id'],
    ];
    for (const [kind, source] of facts)
      expect(render(kind, source)).toBe(`UNION ALL SELECT '${kind}',event.id,to_jsonb(event),false FROM ${source}`);
    expect(sales.match(/const historyFactSql=/g)).toHaveLength(1);
    expect(sales.match(/\$\{historyFactSql\('/g)).toHaveLength(3);
    expect(sales).not.toContain("UNION ALL SELECT 'lead-history'");
    expect(sales).not.toContain("UNION ALL SELECT 'activity-history'");
    expect(sales).not.toContain("UNION ALL SELECT 'guest-sharing-history'");
  });

  it('keeps the history family read-only and leaves r8-r19 helpers and locks untouched', () => {
    const sales = src('sales-package-obligations.ts');
    const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
    for (const line of sales.split('\n').filter(value => value.includes('historyFactSql(') && !value.includes('const historyFactSql=')))
      expect(line).not.toMatch(lockRe);
    expect(sales).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE MODE');
    expect(sales).toContain('const financeCustomerMissingSql=');
  });

  it('keeps the owner SELECT KPI strictly below the r19 AFTER=153 baseline', () => {
    expect(totalSelects()).toBeLessThanOrEqual(130);
    expect(totalSelects()).toBeLessThan(153);
  });
});

describe('crm read-tail r21 parameterized branch option family', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };
  const ORG = '${organizationProjectionSources.organization_nodes}';

  it('renders both branch option lists byte-identically from one parameterized helper', () => {
    const purchases = src('purchases.ts');
    const line = purchases.split('\n').find(value => value.includes('export const crmBranchOptionsSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (nameAlias: string, activeOnly: boolean) => body
      .replace('${nameAlias}', nameAlias)
      .replace('${activeOnly?" AND status=\'active\'":\'\'}', activeOnly ? " AND status='active'" : '');
    expect(render(' name', false)).toBe(`SELECT id,display_name name FROM (${ORG}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND (?::text IS NULL OR id=?) ORDER BY display_name`);
    expect(render('', true)).toBe(`SELECT id,display_name FROM (${ORG}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name`);
  });

  it('owns the branch option SELECT token once and composes it at both readers', () => {
    const purchases = src('purchases.ts');
    const analytics = src('analytics.ts');
    const leads = src('leads.ts');
    expect(purchases.match(/export const crmBranchOptionsSql=/g)).toHaveLength(1);
    expect(purchases.match(/SELECT id,display_name/g)).toHaveLength(1);
    expect(analytics.match(/SELECT id,display_name/g) ?? []).toHaveLength(0);
    expect(leads.match(/SELECT id,display_name/g) ?? []).toHaveLength(0);
    expect(analytics).toContain("crmBranchOptionsSql(' name',false)");
    expect(leads).toContain("crmBranchOptionsSql('',true)");
    // r8-r20 helpers and the owned CRM source lock stay in place.
    for (const token of ['crmAssignmentAllowedSql', 'crmAccountCreatedByScopeSql', 'opportunityWitnessSql', 'crmClockCteSql', 'crmOpportunityLeadSql', 'crmHotelReservationProbeSql', 'crmCaseEventReceiptWitnessSql'])
      expect(purchases).toContain(token);
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
  });

  it('keeps the owner SELECT KPI strictly below the r20 BEFORE=151 baseline', () => {
    expect(totalSelects()).toBeLessThanOrEqual(130);
    expect(totalSelects()).toBeLessThan(151);
  });
});

describe('crm read-tail r22 assignment-scope EXISTS family', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };

  it('renders all three assignment-scope guards byte-identically from one parameterized helper', () => {
    const purchases = src('purchases.ts');
    const line = purchases.split('\n').find(value => value.includes('export const crmAssignmentScopeExistsSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (columns: string, alias: string, permission: string) => body
      .replace('${columns}', columns)
      .replaceAll('${alias}', alias)
      .replace('${assignmentReadSql(alias,permission)}', `\${assignmentReadSql('${alias}','${permission}')}`);
    const members: [string, string, string][] = [
      ['property.company_id,property.branch_id,?::text department_id', 'hotel_scope', 'pms.read'],
      ['cost.company_id,?::text branch_id,?::text department_id', 'cost_scope', 'finance.read'],
      ['crm_source.company_id,COALESCE(crm_source.branch_id,?::text) branch_id,COALESCE(crm_source.department_id,?::text) department_id', 'crm_authority', 'commerce.read'],
    ];
    const expected = [
      `EXISTS(SELECT 1 FROM (SELECT property.company_id,property.branch_id,?::text department_id) hotel_scope WHERE \${assignmentReadSql('hotel_scope','pms.read')})`,
      `EXISTS(SELECT 1 FROM (SELECT cost.company_id,?::text branch_id,?::text department_id) cost_scope WHERE \${assignmentReadSql('cost_scope','finance.read')})`,
      `EXISTS(SELECT 1 FROM (SELECT crm_source.company_id,COALESCE(crm_source.branch_id,?::text) branch_id,COALESCE(crm_source.department_id,?::text) department_id) crm_authority WHERE \${assignmentReadSql('crm_authority','commerce.read')})`,
    ];
    members.forEach(([columns, alias, permission], i) => expect(render(columns, alias, permission)).toBe(expected[i]));
    expect(purchases.match(/export const crmAssignmentScopeExistsSql=/g)).toHaveLength(1);
  });

  it('owns the scope-guard SELECT tokens once and composes them at all three readers', () => {
    const purchases = src('purchases.ts'), analytics = src('analytics.ts'), record = src('record-purchases.ts');
    expect(analytics.match(/crmAssignmentScopeExistsSql\(/g)).toHaveLength(2);
    expect(record.match(/crmAssignmentScopeExistsSql\(/g)).toHaveLength(1);
    expect(analytics).toContain("crmAssignmentScopeExistsSql('property.company_id,property.branch_id,?::text department_id','hotel_scope','pms.read')");
    expect(analytics).toContain("crmAssignmentScopeExistsSql('cost.company_id,?::text branch_id,?::text department_id','cost_scope','finance.read')");
    expect(record).toContain("crmAssignmentScopeExistsSql('crm_source.company_id,COALESCE(crm_source.branch_id,?::text) branch_id,COALESCE(crm_source.department_id,?::text) department_id','crm_authority','commerce.read')");
    for (const text of [analytics, record]) {
      expect(text).not.toMatch(/EXISTS\(SELECT 1 FROM \(SELECT property\.company_id/);
      expect(text).not.toMatch(/EXISTS\(SELECT 1 FROM \(SELECT cost\.company_id/);
      expect(text).not.toMatch(/EXISTS\(SELECT 1 FROM \(SELECT crm_source\.company_id/);
    }
    // r8-r21 helpers and the owned CRM source lock stay in place.
    for (const token of ['crmAssignmentAllowedSql', 'crmAccountCreatedByScopeSql', 'opportunityWitnessSql', 'crmClockCteSql', 'crmOpportunityLeadSql', 'crmHotelReservationProbeSql', 'crmCaseEventReceiptWitnessSql', 'crmBranchOptionsSql'])
      expect(purchases).toContain(token);
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
  });

  it('keeps the owner SELECT KPI strictly below the r21 BEFORE=150 baseline', () => {
    expect(totalSelects()).toBeLessThanOrEqual(130);
    expect(totalSelects()).toBeLessThan(150);
  });
});

describe('crm read-tail r23 reference-existence and account-scope families', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };

  it('renders both reference-existence probes byte-identically from one parameterized helper', () => {
    const purchases = src('purchases.ts');
    const line = purchases.split('\n').find(value => value.includes('const crmSourceExistsSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (source: string, alias: string, tail: string) => body
      .replace('${source}', source)
      .replace('${alias}', alias)
      .replace('${tail}', tail);
    expect(render('crmMarketingCampaignSources', 'campaign', '')).toBe('SELECT 1 FROM (crmMarketingCampaignSources) campaign WHERE id=? AND company_id=?');
    expect(render('crmMarketingCandidateSources', 'candidate', ' AND branch_id=? AND (?::text IS NULL OR department_id=?)')).toBe('SELECT 1 FROM (crmMarketingCandidateSources) candidate WHERE id=? AND company_id=? AND branch_id=? AND (?::text IS NULL OR department_id=?)');
    expect(purchases.match(/const crmSourceExistsSql=/g)).toHaveLength(1);
  });

  it('composes the existence probe at both reference checks and the account scope at the policy fact', () => {
    const purchases = src('purchases.ts'), policy = src('attachments-policy-facts.ts');
    expect(purchases.match(/crmSourceExistsSql\(/g)).toHaveLength(2);
    expect(purchases).toContain("crmSourceExistsSql(crmMarketingCampaignSources,'campaign')");
    expect(purchases).toContain("crmSourceExistsSql(crmMarketingCandidateSources,'candidate',' AND branch_id=? AND (?::text IS NULL OR department_id=?)')");
    expect(purchases).not.toContain('`SELECT 1 FROM (${crmMarketingCampaignSources})');
    expect(purchases).not.toContain('`SELECT 1 FROM (${crmMarketingCandidateSources})');
    expect(policy).toContain('${crmAccountCreatedByScopeSql}');
    expect(policy).not.toMatch(/EXISTS\(\s*SELECT 1 FROM crm_leads lead/);
    for (const token of ['crmAssignmentScopeExistsSql', 'crmAccountCreatedByScopeSql', 'crmAssignmentAllowedSql', 'opportunityWitnessSql', 'crmClockCteSql', 'crmOpportunityLeadSql', 'crmHotelReservationProbeSql', 'crmCaseEventReceiptWitnessSql', 'crmBranchOptionsSql'])
      expect(purchases).toContain(token);
  });

  it('keeps the owner SELECT KPI strictly below the r22 BEFORE=146 baseline', () => {
    expect(totalSelects()).toBeLessThanOrEqual(130);
    expect(totalSelects()).toBeLessThan(146);
  });
});

describe('crm read-tail r24 creator EXISTS lead-scope family', () => {
  const owner = fileURLToPath(new URL('../../../packages/server-crm/src/', import.meta.url));
  const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
    }
    return total;
  };

  /** Retired inline literals, exactly as they read before r24. */
  const retired: Record<string, string> = {
    contactScope: "(?::text IS NULL OR acc.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))",
    accountScope: "(?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))",
    contactScopeA: "(?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.company_id=c.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))",
    accountCreatedBy: "(?::text IS NULL OR account.created_by=? OR EXISTS(SELECT 1 FROM crm_leads lead WHERE lead.account_id=account.id AND lead.company_id=account.company_id AND lead.branch_id=? AND (?::text IS NULL OR lead.department_id=?)))",
    sourceAccount: "(?::text IS NULL OR actual.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.account_id=actual.id AND l.company_id=actual.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))",
    sourceGuestLink: "(?::text IS NULL OR actual.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=(SELECT contact_id FROM crm_guest_links WHERE id=actual.id) AND l.company_id=actual.company_id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))",
  };

  it('renders byte-identical read-only SQL for every retired inline site', () => {
    const cases: Array<[string, string]> = [
      [retired.contactScope, crmCreatorLeadScopeSql('acc', 'l', 'l.contact_id=c.id', '')],
      [retired.accountScope, crmCreatorLeadScopeSql('a', 'l', 'l.account_id=a.id', 'l.company_id=a.company_id')],
      [retired.contactScopeA, crmCreatorLeadScopeSql('a', 'l', 'l.contact_id=c.id', 'l.company_id=c.company_id')],
      [retired.accountCreatedBy, crmCreatorLeadScopeSql('account', 'lead', 'lead.account_id=account.id', 'lead.company_id=account.company_id')],
      [retired.sourceAccount, crmCreatorLeadScopeSql('actual', 'l', 'l.account_id=actual.id', 'l.company_id=actual.company_id')],
      [retired.sourceGuestLink, crmCreatorLeadScopeSql('actual', 'l', 'l.contact_id=(SELECT contact_id FROM crm_guest_links WHERE id=actual.id)', 'l.company_id=actual.company_id')],
    ];
    for (const [expected, rendered] of cases) {
      expect(rendered).toBe(expected);
      expect(rendered).toContain('SELECT ');
      expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  it('owns the renderer once and composes it at the four retired sites', () => {
    expect(src('read-tail-r24-sql.ts').match(/crmCreatorLeadScopeSql =/g)).toHaveLength(1);
    const identities = src('identities.ts');
    expect(identities).toContain("const contactScopeSql=crmCreatorLeadScopeSql('acc','l','l.contact_id=c.id','');");
    expect(identities).toContain("const accountOrContactLeadScopeSql=(field:string,ref:string,company:string)=>crmCreatorLeadScopeSql('a','l',`l.${field}=${ref}`,`l.company_id=${company}`);");
    expect(identities).not.toContain('created_by=? OR EXISTS(SELECT 1 FROM crm_leads ');
    const purchases = src('purchases.ts');
    expect(purchases).toContain("export const crmAccountCreatedByScopeSql=crmCreatorLeadScopeSql('account','lead','lead.account_id=account.id','lead.company_id=account.company_id');");
    expect(purchases).toContain(":crmCreatorLeadScopeSql('actual','l',");
    expect(purchases).not.toContain('created_by=? OR EXISTS(SELECT 1 FROM crm_leads ');
    for (const token of ['crmAssignmentScopeExistsSql', 'crmAccountCreatedByScopeSql', 'crmAssignmentAllowedSql', 'opportunityWitnessSql', 'crmClockCteSql', 'crmOpportunityLeadSql', 'crmHotelReservationProbeSql', 'crmCaseEventReceiptWitnessSql', 'crmBranchOptionsSql'])
      expect(purchases).toContain(token);
  });

  it('keeps the owner SELECT KPI strictly below the r24 BEFORE=144 baseline', () => {
    expect(totalSelects()).toBeLessThanOrEqual(130);
    expect(totalSelects()).toBeLessThan(144);
  });
});

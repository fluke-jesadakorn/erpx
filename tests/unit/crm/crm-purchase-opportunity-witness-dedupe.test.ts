import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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
    // One canonical declaration; the other surface reuses it by reference so
    // the two public callers cannot drift into separate query text.
    expect(commercial.match(/FROM crm_leads"/g)).toHaveLength(1);
    expect(finalRoot).not.toContain('FROM crm_leads"');
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
    const predicate = '(?::text IS NULL OR acc.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))';
    expect(identities.split(predicate).length - 1).toBe(1);
    expect(identities).toContain('const contactScopeSql=`' + predicate + '`;');
    expect(identities.match(/\$\{contactScopeSql\}/g)).toHaveLength(2);
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
    expect(purchases.split('export const crmAccountCreatedByScopeSql=`' + accountScope + '`;').length - 1).toBe(1);
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
    expect(totalSelects()).toBe(158);
    expect(totalSelects()).toBeLessThan(163);
  });
});

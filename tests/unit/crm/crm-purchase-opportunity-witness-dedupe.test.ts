import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

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
});

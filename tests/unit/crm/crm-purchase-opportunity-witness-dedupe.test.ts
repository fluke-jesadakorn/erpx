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
});

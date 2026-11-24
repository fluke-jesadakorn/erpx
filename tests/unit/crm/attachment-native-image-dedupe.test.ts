import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const owner = resolve(__dirname, '../../../packages/server-crm/src');
const attachment = () => readFileSync(resolve(owner, 'adapters/attachment-read.ts'), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

describe('crm attachment native-image read family (r13)', () => {
  it('renders byte-identical SQL for both the account and opportunity witnesses', () => {
    const src = attachment();
    const line = src.split('\n').find(value => value.includes('const nativeImageSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (rows: string, predicate: string) => body.replace('${rows}', rows).replace('${predicate}', predicate);

    const accountInline = "'native',(SELECT jsonb_agg(to_jsonb(native)) FROM (${crmPurchaseAccountRows}) native WHERE native.id=account.id AND native.company_id=account.company_id)";
    const opportunityInline = "'native',(SELECT jsonb_agg(to_jsonb(native)) FROM (${crmPurchaseOpportunityRows}) native WHERE native.id=lead.id AND native.company_id=lead.company_id)";

    expect(render('${crmPurchaseAccountRows}', 'native.id=account.id AND native.company_id=account.company_id')).toBe(accountInline);
    expect(render('${crmPurchaseOpportunityRows}', 'native.id=lead.id AND native.company_id=lead.company_id')).toBe(opportunityInline);
  });

  it('owns the fragment once and composes it at both call sites', () => {
    const src = attachment();
    expect(src.match(/const nativeImageSql=/g)).toHaveLength(1);
    expect(src.match(/nativeImageSql\(/g)).toHaveLength(2);
    // Only the helper definition keeps the inline fragment; no call site does.
    expect(src.match(/'native',\(SELECT jsonb_agg/g)).toHaveLength(1);
    expect(src).toContain("nativeImageSql(crmPurchaseAccountRows,'native.id=account.id AND native.company_id=account.company_id')");
    expect(src).toContain("${nativeImageSql(crmPurchaseOpportunityRows,'native.id=lead.id AND native.company_id=lead.company_id')})");
  });

  it('keeps the owner SELECT KPI strictly below the r13 BEFORE=172 baseline', () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += selectCount(readFileSync(resolve(owner, String(rel)), 'utf8'));
    }
    expect(total).toBeLessThan(172);
  });
});

describe('crm attachment related-image aggregate family (r17)', () => {
  const src = () => attachment();
  const totalSelects = () => {
    let total = 0;
    for (const rel of readdirSync(owner, { recursive: true })) {
      if (!String(rel).endsWith('.ts')) continue;
      total += selectCount(readFileSync(resolve(owner, String(rel)), 'utf8'));
    }
    return total;
  };

  it('renders byte-identical SQL for all three retired inline subqueries', () => {
    const line = src().split('\n').find(value => value.includes('const crmRelatedAggSql=')) ?? '';
    const body = line.slice(line.indexOf('`') + 1, line.lastIndexOf('`'));
    const render = (alias: string, from: string) => body.replaceAll('${alias}', alias).replace('${from}', from);

    expect(render('contact', 'crm_contacts contact WHERE contact.account_id=account.id AND contact.company_id=account.company_id'))
      .toBe("SELECT jsonb_agg(to_jsonb(contact) ORDER BY contact.id) FROM crm_contacts contact WHERE contact.account_id=account.id AND contact.company_id=account.company_id");
    expect(render('lead', 'crm_leads lead WHERE lead.account_id=account.id AND lead.company_id=account.company_id'))
      .toBe("SELECT jsonb_agg(to_jsonb(lead) ORDER BY lead.id) FROM crm_leads lead WHERE lead.account_id=account.id AND lead.company_id=account.company_id");
    expect(render('link', 'crm_guest_links link WHERE link.company_id=lead.company_id AND link.contact_id=lead.contact_id'))
      .toBe("SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM crm_guest_links link WHERE link.company_id=lead.company_id AND link.contact_id=lead.contact_id");
  });

  it('owns the fragment once and composes it at all three call sites', () => {
    const text = src();
    expect(text.match(/const crmRelatedAggSql=/g)).toHaveLength(1);
    expect(text.match(/crmRelatedAggSql\(/g)).toHaveLength(3);
    expect(text).toContain("crmRelatedAggSql('contact','crm_contacts contact WHERE contact.account_id=account.id AND contact.company_id=account.company_id')");
    expect(text).toContain("crmRelatedAggSql('lead','crm_leads lead WHERE lead.account_id=account.id AND lead.company_id=account.company_id')");
    expect(text).toContain("${crmRelatedAggSql('link','crm_guest_links link WHERE link.company_id=lead.company_id AND link.contact_id=lead.contact_id')}");
    // Only the helper definition keeps the inline aggregate token; no call site does.
    expect(text.match(/SELECT jsonb_agg\(to_jsonb\(\$\{alias\}\) ORDER BY/g)).toHaveLength(1);
    expect(text.match(/SELECT jsonb_agg\(to_jsonb\((contact|lead|link)\) ORDER BY/g)).toBeNull();
    expect(src().match(/SELECT jsonb_agg\(to_jsonb\(native\)\)/g)).toHaveLength(1); // r13 family untouched
  });

  it('keeps the owner SELECT KPI strictly below the r17 BEFORE=165 baseline', () => {
    expect(totalSelects()).toBeLessThan(165);
  });
});

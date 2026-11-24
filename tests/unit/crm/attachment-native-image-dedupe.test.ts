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
    expect(src.match(/\$\{nativeImageSql\(/g)).toHaveLength(2);
    // Only the helper definition keeps the inline fragment; no call site does.
    expect(src.match(/'native',\(SELECT jsonb_agg/g)).toHaveLength(1);
    expect(src).toContain("${nativeImageSql(crmPurchaseAccountRows,'native.id=account.id AND native.company_id=account.company_id')})");
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

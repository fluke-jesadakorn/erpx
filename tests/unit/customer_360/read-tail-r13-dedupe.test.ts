import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  companyScopeSource,
  customerVisiblePredicate,
  scopedProjection,
} from '../../../packages/server-customer_360/src/adapters/customer-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_360/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Pre-dedupe scopedRow renderings copied verbatim from the r5/r12 baseline. The
// r13 head family unifies scopedRow's `SELECT <column> FROM <relation> <alias>
// WHERE <alias>.<key>=p.id` with scopedProjection's `SELECT <columns> FROM
// <source> WHERE <predicate>`; whole-string byte identity is the oracle.
const rows = [
  { args: ['1', 'scoped_orders', 'o', 'party_id'] as const, sql: 'SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id', sha: '3a0a384fb5eae21eaa289477b35ebeb2e9d3f7631a1bbb0c7aad6a7bce562b2b' },
  { args: ['1', 'scoped_leads', 'l', 'customer_id'] as const, sql: 'SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id', sha: '63ce9c8586e226448cb689df291aa5d404926487b3fdd42d71b154ad11089e22' },
  { args: ['1', 'scoped_service', 'g', 'customer_id'] as const, sql: 'SELECT 1 FROM scoped_service g WHERE g.customer_id=p.id', sha: '279be332a7fbf4a11f6160ef8219c1cad4f25589151b7e456216c6b84893642b' },
  { args: ['1', 'scoped_invoices', 'i', 'party_id'] as const, sql: 'SELECT 1 FROM scoped_invoices i WHERE i.party_id=p.id', sha: '22fc63a3d5a5c5ed2bf5f30cd74a762441ab5fe0a250cc2204580a3fe8f6278d' },
  { args: ['id', 'scoped_orders', 'so', 'party_id'] as const, sql: 'SELECT id FROM scoped_orders so WHERE so.party_id=p.id', sha: '0bbc690e7d082b156c10dbd0933635cc844806411eafbf332a880b9497694313' },
];

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Customer 360 read-tail r13 scoped-projection head family', () => {
  it('the single head reproduces every pre-dedupe scoped row byte-for-byte', () => {
    for (const row of rows) {
      const [column, relation, alias, key] = row.args;
      const emitted = scopedProjection(column, `${relation} ${alias}`, `${alias}.${key}=p.id`);
      expect(emitted).toBe(row.sql);
      expect(sha(emitted)).toBe(row.sha);
      readOnly(emitted);
    }
    // The composed visibility predicate keeps its exact emitted SQL.
    expect(customerVisiblePredicate(true, true, true)).toBe(
      '(EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id) OR EXISTS(SELECT 1 FROM scoped_service g WHERE g.customer_id=p.id) OR EXISTS(SELECT 1 FROM scoped_invoices i WHERE i.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_legal lg WHERE lg.order_id IN (SELECT id FROM scoped_orders so WHERE so.party_id=p.id)))',
    );
    expect(companyScopeSource('portal_release')).toBe('(SELECT ?::text company_id) portal_release');
  });

  it('r14 composes the legal visibility EXISTS through the same head byte-for-byte', () => {
    // Pre-cut r13 literal; the source literal is deleted, the emitted SQL is not changed.
    const legal = 'EXISTS(SELECT 1 FROM scoped_legal lg WHERE lg.order_id IN (SELECT id FROM scoped_orders so WHERE so.party_id=p.id))';
    expect(sha(legal)).toBe('8b11ff115e8924df8deb6fa609ad836bd73c4b6f1501f0e4ffcff0fc3568b7bd');
    const inner = scopedProjection('id', 'scoped_orders so', 'so.party_id=p.id');
    const composed = `EXISTS(${scopedProjection('1', 'scoped_legal lg', `lg.order_id IN (${inner})`)})`;
    expect(composed).toBe(legal);
    expect(composed.match(/SELECT /g)).toHaveLength(2);
    readOnly(composed);
    const reads = src('customer-read-sources.ts');
    expect(reads).not.toContain('EXISTS(SELECT 1 FROM scoped_legal');
    expect(reads).toContain("scopedProjection('1', 'scoped_legal lg'");
  });

  it('one shared head definition; r5 helper composes it and timeline imports it', () => {
    const reads = src('customer-read-sources.ts');
    expect(reads.match(/export const scopedProjection = \(/g)).toHaveLength(1);
    expect(reads.match(/SELECT /g)).toHaveLength(2); // head + company scope source; legal EXISTS composes the head (r14)
    const wrapper = reads.match(/const scopedRow = [\s\S]*?;\n/)?.[0] ?? '';
    expect(wrapper).toContain('scopedProjection(column, `${relation} ${alias}`, `${alias}.${key}=p.id`)');
    expect(wrapper).not.toContain('SELECT '); // the duplicate head literal is gone

    const timeline = src('timeline.ts');
    expect(timeline).not.toContain('const scopedProjection = (');
    expect(timeline).toContain('scopedProjection } from "./customer-read-sources"');
    expect(timeline.match(/\$\{scopedProjection\(/g)).toHaveLength(8); // r9 six + r10 two, unchanged
  });

  it('r3-r12 helpers, locks, native reads and cross-owner pins stay untouched', () => {
    const reads = src('customer-read-sources.ts');
    expect(reads).toContain('export function customerVisiblePredicate('); // r5
    expect(reads).toContain('export const companyScopeSource ='); // r8
    const obligations = src('package-obligations.ts');
    expect(obligations).toContain('const supportConclusionExists='); // r3
    expect(obligations).toContain('LOCK TABLE customer_portal_actions IN SHARE ROW EXCLUSIVE MODE');
    expect(obligations).toContain('assertPortalAuthorityPackageObligationSources');
    expect(obligations).toContain('${obligationFactHead('); // r9
    const portal = src('portal.ts');
    expect(portal).not.toContain('SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE'); // r15: planned natively
    expect(portal).toContain('nativeC360ReadStatement(');
    const timeline = src('timeline.ts');
    expect(timeline).toContain('const eventSelect='); // r6
    expect(timeline).toContain('const eventRow='); // r7
    for (const owner of ['commerce', 'crm', 'finance', 'legal', 'sales', 'support'])
      expect(timeline).toContain(`@erpx/server-${owner}/public/final-root-owner-projection-sources`);
    const native: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.rs')) native.push(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(native.some((source) => source.includes('scopedProjection'))).toBe(false);
  });

  it('retirement:ts-statements is strictly lower than BEFORE=11 (expected 10)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(10);
    expect(total).toBeLessThan(11);
  });
});

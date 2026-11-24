import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
const { captured } = vi.hoisted(() => ({ captured: [] as string[] }));
vi.mock('@erpx/server-workflow/public/command-assertions', () => ({
  assertion: (_db: unknown, _id: string, _step: string, query: string) => {
    captured.push(query);
    return { sql: query };
  },
}));
import { ecommerceExistsGuard } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';
import { OperationalAttachmentEcommerceFacts } from '../../../packages/server-ecommerce/src/adapters/attachments-operational-facts';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const ao = read('adapters/attachments-operational-facts.ts');
const shared = read('adapters/read-tail-sql.ts');

// Pre-dedupe guard SQL copied verbatim from HEAD (r9 local frame), kept only as byte-identity oracles.
const head = {
  listing: 'EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=? AND l.version=?)',
  generic: (table: string) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
};

describe('Ecommerce read-tail r18 EXISTS-head family', () => {
  test('the single shared frame rebuilds both guards byte-identically', () => {
    expect(ecommerceExistsGuard('ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id', 'l.id=? AND c.company_id=? AND l.version=?')).toBe(head.listing);
    expect(ecommerceExistsGuard('ecommerce_listings', 'id=? AND company_id=? AND version=?')).toBe(head.generic('ecommerce_listings'));
  });

  test('one canonical frame and one alias; the duplicate literal is gone', () => {
    expect(shared.match(/export const ecommerceExistsGuard = /g) ?? []).toHaveLength(1);
    expect(ao.match(/const attachmentVersionGuardSql = ecommerceExistsGuard;/g) ?? []).toHaveLength(1);
    expect(ao).toContain('ecommerceExistsGuard } from "./read-tail-sql"');
    expect(ao.match(/attachmentVersionGuardSql\(/g) ?? []).toHaveLength(2);
    expect(ao).not.toContain('`EXISTS(SELECT 1 FROM ${source} WHERE ${where})`');
    expect(shared.match(/EXISTS\(SELECT 1 FROM \$\{(table|source)\} WHERE \$\{where\}\)/g) ?? []).toHaveLength(1);
  });

  test('composed guards stay read-only with unchanged binds and no added locks', () => {
    for (const sql of [head.listing, head.generic('ecommerce_listings')]) {
      expect(sql).toMatch(/^EXISTS\(SELECT 1 FROM /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
      expect((sql.match(/\?/g) ?? []).length).toBe(3);
    }
    expect(ao).not.toMatch(/LOCK\s+TABLE/i);
    expect(shared).not.toMatch(/LOCK\s+TABLE/i);
    expect(read('adapters/attachment-purchases.ts')).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
    expect(read('adapters/package-obligations.ts')).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
  });

  test('reachable versionGuard still emits the byte-identical listing guard', () => {
    captured.length = 0;
    const facts = OperationalAttachmentEcommerceFacts({} as any);
    facts.versionGuard('cmd', 'ecommerce_listing', 'l1', 'c1', 3, 0);
    expect(captured).toEqual([head.listing]);
    expect(facts.versionGuard('cmd', 'unsupported', 'x', 'c1', 1, 0)).toBeNull();
  });
});

describe('Ecommerce read-tail r18 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r18 BEFORE=48 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(48);
    expect(total).toBe(42);
  });
});

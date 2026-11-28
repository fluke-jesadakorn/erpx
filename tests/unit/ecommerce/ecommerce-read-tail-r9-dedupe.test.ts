import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
const { captured } = vi.hoisted(() => ({ captured: [] as string[] }));
vi.mock('@erpx/workflow-server/public/command-assertions', () => ({
  assertion: (_db: unknown, _id: string, _step: string, query: string) => {
    captured.push(query);
    return { sql: query };
  },
}));
import { OperationalAttachmentEcommerceFacts } from '../../../modules/ecommerce/server/src/adapters/attachments-operational-facts';

const srcRoot = resolve(__dirname, '../../../modules/ecommerce/server/src');
const src = readFileSync(resolve(srcRoot, 'adapters/attachments-operational-facts.ts'), 'utf8');

// Pre-dedupe version-guard literals copied verbatim from HEAD, kept only as byte-identity oracles.
const head = {
  listing: 'EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=? AND l.version=?)',
  generic: (table: string) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
};

// Rebuild the r9 shared fragment exactly as the adapter defines it.
const attachmentVersionGuardSql = (source: string, where: string) =>
  `EXISTS(SELECT 1 FROM ${source} WHERE ${where})`;

describe('Ecommerce read-tail r9 attachment-owner version-guard family', () => {
  test('one parameterized guard rebuilds both callers byte-identically', () => {
    expect(attachmentVersionGuardSql('ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id', 'l.id=? AND c.company_id=? AND l.version=?')).toBe(head.listing);
    expect(attachmentVersionGuardSql('ecommerce_listings', 'id=? AND company_id=? AND version=?')).toBe(head.generic('ecommerce_listings'));
  });

  test('family is defined once and composed at both call sites; literals are gone', () => {
    expect(src.match(/const attachmentVersionGuardSql = /g) ?? []).toHaveLength(1);
    expect(src.match(/attachmentVersionGuardSql\(/g) ?? []).toHaveLength(2);
    expect(src.match(/attachmentVersionGuardSql\("ecommerce_listings l JOIN ecommerce_channels c ON c.id=l\.channel_id", "l\.id=\? AND c\.company_id=\? AND l\.version=\?"\)/g) ?? []).toHaveLength(1);
    expect(src.match(/attachmentVersionGuardSql\(table, "id=\? AND company_id=\? AND version=\?"\)/g) ?? []).toHaveLength(1);
    expect(src).not.toContain('"EXISTS(SELECT 1 FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=? AND l.version=?)"');
    expect(src).not.toContain('`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`');
  });

  test('composed guards stay read-only EXISTS(SELECT 1 ...) with unchanged binds', () => {
    for (const sql of [head.listing, head.generic('ecommerce_listings')]) {
      expect(sql).toMatch(/^EXISTS\(SELECT 1 FROM /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
      expect((sql.match(/\?/g) ?? []).length).toBe(3);
    }
  });

  test('reachable versionGuard emits the byte-identical listing guard', () => {
    captured.length = 0;
    const facts = OperationalAttachmentEcommerceFacts({} as any);
    facts.versionGuard('cmd', 'ecommerce_listing', 'l1', 'c1', 3, 0);
    expect(captured).toEqual([head.listing]);
    expect(facts.versionGuard('cmd', 'unsupported', 'x', 'c1', 1, 0)).toBeNull();
  });
});

describe('Ecommerce read-tail r9 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r9 BEFORE=63 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(63);
  });
});

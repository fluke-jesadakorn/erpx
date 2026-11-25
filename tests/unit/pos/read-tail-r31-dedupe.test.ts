import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');

// Verbatim pre-r31 literals recorded from the AST of `pos.ts` at BEFORE=195
// (HEAD 440db47894478f9b53262ac1b26029f8c7dbcef2). These are byte-identity
// oracles only, never live query sources.
const SQL = [
  "SELECT s.*,r.name register_name FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?) AND (?=0 OR s.state IN ('open','held','payment_pending')) ORDER BY s.created_at DESC LIMIT 150",
  'SELECT f.*,s.number sale_number,s.currency FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND (?::text IS NULL OR f.sale_id=?) ORDER BY f.created_at DESC LIMIT 100',
  "SELECT * FROM pos_connections WHERE company_id=? AND state='ready'",
  'SELECT * FROM pos_settlements WHERE company_id=? ORDER BY created_at DESC LIMIT 100',
] as const;
const BINDS = [6, 5, 1, 1] as const;

describe('POS read-tail r31 list-read native cutover', () => {
  test('native registry holds every retired literal byte-identically with binds 6/5/1/1', () => {
    const registry = read('list_reads.rs');
    for (let i = 0; i < SQL.length; i++) {
      expect(registry).toContain(`"${SQL[i]}"`);
      expect((SQL[i].match(/\?/g) ?? []).length).toBe(BINDS[i]);
    }
    expect((registry.match(/pub const KINDS: &\[&str\] = &\[/g) ?? []).length).toBe(1);
    expect((registry.match(/pos_list_/g) ?? []).length).toBeGreaterThanOrEqual(4);
  });

  test('former inline literals are gone and all four call sites delegate', () => {
    const pos = read('pos.ts');
    for (const sql of SQL) expect(pos).not.toContain(sql);
    expect(pos.match(/nativePos(SalesList|RefundsList|ConnectionsRow|SettlementsList)Statement\(db/g)).toHaveLength(4);
  });

  test('row locks, PMS fragments and day-report/Stripe registries stay inline', () => {
    const core = read('pos-core.ts');
    const pos = read('pos.ts');
    expect(core).toContain('SELECT * FROM pos_sales WHERE id=? FOR UPDATE');
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(pos).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
  });

  test('retirement:ts-statements stays strictly lower than the r31 BEFORE=195 baseline (182 after r36)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(182);
    expect(total).toBeLessThan(191);
  });
});

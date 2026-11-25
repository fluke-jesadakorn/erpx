import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');

// Verbatim pre-r30 literals recorded from the AST of `pos-core.ts` at
// BEFORE=199 (blob 4215e2d44bfe598a0b58379fe9e3693ba576d099). These are
// byte-identity oracles only, never live query sources.
const SQL = [
  'SELECT COALESCE(SUM(quantity_milli),0) n FROM pos_lines WHERE fulfillment_id=? AND id<?',
  'SELECT s.created_by FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.created_by=? LIMIT 1',
  "SELECT l.id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=? AND s.state='completed' AND l.returned_milli<l.quantity_milli",
  "SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE p.sale_id=? AND rp.state<>'succeeded'",
] as const;
const BINDS = [2, 2, 1, 1] as const;

describe('POS read-tail r30 child-tail native cutover', () => {
  test('native registry holds every retired literal byte-identically with binds 2/2/1/1', () => {
    const registry = read('child_tail_reads.rs');
    for (let i = 0; i < SQL.length; i++) {
      expect(registry).toContain(`"${SQL[i]}"`);
      expect((SQL[i].match(/\?/g) ?? []).length).toBe(BINDS[i]);
    }
    expect((registry.match(/pub const KINDS: &\[&str\] = &\[/g) ?? []).length).toBe(1);
    expect((registry.match(/pos_child_tail_/g) ?? []).length).toBeGreaterThanOrEqual(4);
  });

  test('former inline literals are gone and all four call sites delegate', () => {
    const core = read('pos-core.ts');
    for (const sql of SQL) expect(core).not.toContain(sql);
    expect(core.match(/nativePosChildTail(LinesSum|CreatedBy|CompletedLine|RefundPayment)Statement\(db/g)).toHaveLength(4);
  });

  test('row locks, PMS fragments and day-report/Stripe registries stay inline', () => {
    const core = read('pos-core.ts');
    const pos = read('pos.ts');
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(pos).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
  });

  test('retirement:ts-statements stays strictly lower than the r30 BEFORE=199 baseline (173 after r42)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(170);
    expect(total).toBeLessThan(199);
  });
});

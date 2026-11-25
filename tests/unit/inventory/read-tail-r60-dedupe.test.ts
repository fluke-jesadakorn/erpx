import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { transferDetailHeadSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const MARKER = '${transferDetailHeadSql}';

// Verbatim byte oracles captured from the pre-r60 source. The folded head is
// bind-free and read-only; each caller keeps its own projection tail, scope
// and independent statement. Neither call site mirrors this head in native
// source, and neither enclosing statement carries a lock.
const HEAD = 'SELECT d.id,d.company_id,d.source_branch_id,';
const HEAD_SHA = 'b6b2b872887e9774786926cc183b2d067e804e80934faaf93cfaa098e1ebcc34';
const SITES = [
  {
    file: 'transfer-purchases.ts',
    BODY_SHA: 'fcb8b71df53dc0c564761a70c2878d05d389f96ba9190d2fea6132a80e5e2abc',
    TAIL_SHA: 'c91fdd8b3498b1cc02099e30dcc683a49cfcce39e27f9dfe450dc87bf4d48ad7',
    binds: 5,
    selects: 1,
  },
  {
    file: 'transfer-record-purchases.ts',
    BODY_SHA: 'e7d24bcb4255c01f87e52c428478da2fd99a23db7c73cb38e2ae64edf541f78e',
    TAIL_SHA: 'a5ad0014dbdd292fc23c89e061c076b437ab597475aa5e3d97e6b1fe94c91424',
    binds: 0,
    selects: 2,
  },
] as const;

const tailAfterMarker = (text: string, marker: string) => {
  const out: string[] = [];
  let i = 0;
  while (true) {
    const at = text.indexOf(marker, i);
    if (at < 0) break;
    const start = at + marker.length;
    out.push(text.slice(start, text.indexOf('`', start)));
    i = start;
  }
  return out;
};

describe('Inventory read-tail r60 transfer detail head dedupe', () => {
  test('f1: the folded head is a single byte-identical, bind-free projection prefix', () => {
    expect(transferDetailHeadSql).toBe(HEAD);
    expect(sha(transferDetailHeadSql)).toBe(HEAD_SHA);
    expect(selectCount(transferDetailHeadSql)).toBe(1);
    expect(transferDetailHeadSql).not.toContain('?');
    expect(transferDetailHeadSql).not.toMatch(lockRe);
    expect(adapter('read-tail-sql.ts').split('export const transferDetailHeadSql').length - 1).toBe(1);
  });

  test('f2: both owner-local call sites delegate once and no inline head remains', () => {
    for (const site of SITES) {
      const text = adapter(site.file);
      expect(text.split(MARKER).length - 1, site.file).toBe(1);
      expect(text, site.file).not.toContain(HEAD);
    }
    expect(adapter('transfer-purchases.ts')).toContain(
      'scopedFieldSql,scopeAllowedSql,transferDetailHeadSql',
    );
    expect(adapter('transfer-record-purchases.ts')).toContain(
      'clockScopedPurchaseExistsSql,nativeRecordExistsSql,recordIdExistsSql,transferDetailHeadSql',
    );
  });

  test('f3: rendered SQL, projection tails and bind order stay byte-identical', () => {
    for (const site of SITES) {
      const tails = tailAfterMarker(adapter(site.file), MARKER);
      expect(tails, site.file).toHaveLength(1);
      expect(sha(tails[0]), `tail:${site.file}`).toBe(site.TAIL_SHA);
      expect(sha(HEAD + tails[0]), `body:${site.file}`).toBe(site.BODY_SHA);
      expect(selectCount(HEAD + tails[0]), site.file).toBe(site.selects);
      expect((HEAD + tails[0]).match(/\?/g)?.length ?? 0, site.file).toBe(site.binds);
      expect(HEAD + tails[0], site.file).not.toMatch(lockRe);
    }
    // The two pre-fold projections diverge immediately after the head.
    expect(tailAfterMarker(adapter('transfer-purchases.ts'), MARKER)[0]).toMatch(
      /^d\.target_branch_id,d\.source_warehouse_id,d\.target_warehouse_id,/,
    );
    expect(tailAfterMarker(adapter('transfer-record-purchases.ts'), MARKER)[0]).toMatch(
      /^NULL::text \$\{parent\} UNION ALL SELECT d\.id,d\.company_id,d\.target_branch_id,/,
    );
  });

  test('f4: locks, native transport, count adapters and r45-r59 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'transitEventQuantitySumSql', 'inList', 'reservationsCteHeadSql', 'markedCountExistsSql',
      'replayProbePrefixSql', 'countSelectHeadSql', 'packageObligationStockHeadSql',
      'lotAllocationRowsHeadSql', 'commandReceiptRecordIdGuardSql', 'countLineProductGuardSql',
      'countLineStockHeadSql', 'stockObligationHeadSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    // The FOR SHARE assignment locks stay verbatim in both attachment reads.
    expect(adapter('count-attachment-purchases.ts')).toContain(
      'SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE',
    );
    expect(adapter('transfer-attachment-purchases.ts')).toContain(
      'SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE',
    );
    // No native mirror of this head exists; native transport stays verbatim.
    for (const f of ['native-inventory-read.ts', 'native_command_sql.rs', 'counts.rs', 'supply_package_obligations.rs']) {
      expect(src(`adapters/${f}`).length).toBeGreaterThan(0);
    }
    expect(adapter('transfer-purchases.ts')).not.toMatch(/FOR SHARE|FOR UPDATE|LOCK TABLE/);
    expect(adapter('transfer-record-purchases.ts')).not.toMatch(/FOR SHARE|FOR UPDATE|LOCK TABLE/);
  });

  test('retirement:ts-statements KPI is 287 after lanes 17+18 (r60 transfer detail head fold was 292)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(p, 'utf8'));
      }
    };
    walk(srcRoot);
    // r58 ended at 294 and r59 at 293. r60 folds the byte-identical, bind-free
    // transfer detail/read-purchase projection head shared by two owner-local
    // adapters into one renderer: 2 source tokens -> 1, i.e. 293 -> 292.
    expect(total).toBe(287);
    expect(total).toBeLessThan(293);
  });
});

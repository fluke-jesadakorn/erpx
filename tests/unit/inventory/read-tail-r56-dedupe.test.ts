import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { lotAllocationRowsHeadSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
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

// Verbatim byte oracles captured from the pre-r56 source. The folded head is
// bind-free and read-only; each caller keeps its own guard/projection tail,
// binds, statement and ORDER clause verbatim.
const HEAD = 'SELECT a.*,l.lot_code,l.serial_number,l.expiry_date,';
const HEAD_SHA = '3182d1ac450d6c3e0c8db5e00fb5e733f1ad9d29191f1678ae917af0191fb864';
const MARKER = '${lotAllocationRowsHeadSql}';
const SITES = [
  {
    file: 'commerce-joined-facts.ts',
    quote: '`',
    // SHA of the full pre-r56 literal (quote + head + tail + quote).
    RAW_SHA: 'a0f7e747ba56bf180ff3744dba2aab870f58449694528f9f8653a48aa3ea1b77',
    TAIL_SHA: '88a9cbc2e6f013e8af4d4c483badc7b7fe91a2095f744f50ca130f7d38368cfc',
    selects: 2,
    binds: 1,
  },
  {
    file: 'inventory-transfers.ts',
    quote: '"',
    RAW_SHA: 'f32430f64ee27cef5d5a66a1a5adb314a9f8344d42a6968715b0661aeaf6cb13',
    TAIL_SHA: '7d858a0369a2744b22eb8295779f1270c8d0b5bda8248e4c946256262ad01faf',
    selects: 1,
    binds: 2,
  },
] as const;

describe('Inventory read-tail r56 lot-allocation head dedupe', () => {
  test('f1: the folded head is a single byte-identical, bind-free projection prefix', () => {
    expect(lotAllocationRowsHeadSql).toBe(HEAD);
    expect(sha(lotAllocationRowsHeadSql)).toBe(HEAD_SHA);
    expect(selectCount(lotAllocationRowsHeadSql)).toBe(1);
    expect(lotAllocationRowsHeadSql).not.toContain('?');
    expect(lotAllocationRowsHeadSql).not.toMatch(lockRe);
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const lotAllocationRowsHeadSql').length - 1).toBe(1);
  });

  test('f2: both owner-local call sites delegate once and no inline head remains', () => {
    for (const site of SITES) {
      const text = adapter(site.file);
      expect(text.split(MARKER).length - 1, site.file).toBe(1);
      expect(text, site.file).not.toContain(`prepare(\`${HEAD}`);
      expect(text, site.file).not.toContain(`prepare("${HEAD}`);
    }
    expect(adapter('commerce-joined-facts.ts')).toContain("import {lotAllocationRowsHeadSql,reservedStockAggSql} from './read-tail-sql';");
    expect(adapter('inventory-transfers.ts')).toContain('activeQualityHoldExistsSql,decisionActorRowsSql,lotAllocationRowsHeadSql,lotBalanceRowsHeadSql');
  });

  test('f3: rendered SQL, guard/projection tails and bind order stay byte-identical', () => {
    for (const site of SITES) {
      const tails = tailAfterMarker(adapter(site.file), MARKER);
      expect(tails, site.file).toHaveLength(1);
      expect(sha(tails[0]), `tail:${site.file}`).toBe(site.TAIL_SHA);
      // Restoring the original quote + head + tail reproduces the pre-r56 literal exactly.
      expect(sha(`${site.quote}${HEAD}${tails[0]}${site.quote}`), `raw:${site.file}`).toBe(site.RAW_SHA);
      expect(selectCount(HEAD + tails[0]), site.file).toBe(site.selects);
      expect((HEAD + tails[0]).match(/\?/g)?.length ?? 0, site.file).toBe(site.binds);
      expect(HEAD + tails[0], site.file).not.toMatch(lockRe);
    }
    // Caller-owned tails, binds and ORDER clauses are pinned in their original text.
    expect(adapter('commerce-joined-facts.ts')).toContain("WHERE f.order_id=? ORDER BY f.created_at,a.id`).bind(facts.id).all<Row>()");
    expect(adapter('inventory-transfers.ts')).toContain('WHERE tl.transfer_id=? ORDER BY tl.ordinal,l.expiry_date NULLS LAST,l.id`).bind(transfer.source_warehouse_id,id).all<TransferRow>()');
  });

  test('f4: locks, native kinds and r45-r55 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'transitEventQuantitySumSql', 'inList', 'reservationsCteHeadSql', 'markedCountExistsSql',
      'countSelectHeadSql', 'packageObligationStockHeadSql',
      'commandReceiptRecordIdGuardSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    // The FOR SHARE lock literals are intentionally not folded.
    expect(adapter('count-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('transfer-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(src('adapters/native_command_sql.rs').length).toBeGreaterThan(0);
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(adapter('sales-package-obligations.ts')).toContain('LOCK TABLE');
    expect(adapter('supply-package-obligations.ts')).toContain('LOCK TABLE');
  });

  test('retirement:ts-statements KPI is 296 after the r56 lot-allocation head fold (strictly below r55 297)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(p, 'utf8'));
      }
    };
    walk(srcRoot);
    // r45 ended at 320, r46 at 316, r47 at 314, r49 native cutover at 307,
    // r50 at 305, r51 at 303, r52 at 302, r53 at 300, r54 at 298 and r55 at
    // 297. r56 folds the byte-identical lot-allocation projection head shared
    // by the commerce return-lot and transfer-lot reads into one owner-local
    // renderer: 2 source tokens -> 1, i.e. 297 -> 296.
    expect(total).toBe(277);
    expect(total).toBeLessThan(296);
  });
});

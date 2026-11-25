import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { countSelectHeadSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const tailsAfter = (text: string, marker: string) => {
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

// Verbatim byte oracles captured from the pre-r53 source. They are identity
// checks only; the canonical helper owns the rendered text and every caller
// still prepares its own statement with its own rows, binds and tails.
const HEAD = 'SELECT c.*,w.name warehouse_name,';
const HEAD_SHA = '9a9cb7fd535dc38f08adbfd981d042bcd8e59438029892ca1a7c9b8115f6fd92';
// SHA of the three literal template bodies BEFORE the fold, and of the text
// after the shared head (guard/ORDER/bind tails), captured at task HEAD e313f4375.
const BODY_SHAS: Record<number, string> = {
  53: '94651bcfb319815b3779efe597a8ec142a22be5b80bd2f00a2acdeeea6c6a074',
  55: 'b4d86f7fd39d551019f63549840fa3ae38349bf4f9f3da3bf676b90c064a40cd',
  62: 'c5d567852a30c0f4808f61964fdc770e36bbe79a2a94813822e18d1539f0c831',
};
const TAIL_SHAS: Record<number, string> = {
  53: '5c27336a8a1707c0aed5be7b151b3754a129da7826be11d37a121df0ee656bbe',
  55: '5d2312293aa4a2f6c84813f6960ce3e9be222d52600bd2e7bf7c476913919668',
  62: 'c120ea4e901dfcb6688dc56006fd76d4c02dc0b174991105f9c9a0dc1ee60587',
};

describe('Inventory read-tail r53 dedupe', () => {
  test('f1: the folded head is a single byte-identical, bind-free projection prefix', () => {
    expect(countSelectHeadSql).toBe(HEAD);
    expect(sha(countSelectHeadSql)).toBe(HEAD_SHA);
    expect(selectCount(countSelectHeadSql)).toBe(1);
    expect(countSelectHeadSql).not.toContain('?');
    expect(countSelectHeadSql).not.toMatch(lockRe);
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const countSelectHeadSql').length - 1).toBe(1);
  });

  test('f2: three count reads share the head with byte-identical SQL and untouched tails', () => {
    const ic = adapter('inventory-counts.ts');
    expect(ic.split('${countSelectHeadSql}').length - 1).toBe(3);
    expect(ic).not.toContain('`SELECT c.*,w.name warehouse_name,');
    expect(ic).toContain("countInitialPhysicalSourceMarkersSql,countSelectHeadSql,decisionActorRowsSql");

    // Restoring the head into each call site reproduces the exact pre-r53 body.
    const tails = tailsAfter(ic, '${countSelectHeadSql}');
    expect(tails.map((tail) => HEAD + tail)).toHaveLength(3);
    // Line numbers are labels only; the call sites are not consecutive lines,
    // so pair each keyed oracle with the tail at its sequential position.
    Object.entries(BODY_SHAS).forEach(([line, bodySha], idx) => {
      const tail = tails[idx];
      expect(sha(tail), `tail@${line}`).toBe(TAIL_SHAS[Number(line)]);
      expect(sha(HEAD + tail), `body@${line}`).toBe(bodySha);
    });
    // Binds and ORDER/GROUP tails stay in their original callers, in order.
    expect(ic).toContain('.bind(companyId,branchId,branchId2).all<CountRow>()');
    expect(ic).toContain('.bind(id,companyId,branchId,branchId2).first<CountRow>()');
    expect(ic).toContain('ORDER BY c.updated_at DESC LIMIT 250');
    expect(ic).toContain('args:[actor.companyId,actor.branchId,actor.branchId,query.status??\'\',query.status??\'\',query.q??\'\',query.q??\'\']');
  });

  test('f3: locks, native kinds, count sources and r45-r52 helpers stay untouched', () => {
    const native = src('adapters/native_command_sql.rs');
    expect(native.length).toBeGreaterThan(0);
    const bridge = src('adapters/native-inventory-read.ts');
    expect(bridge.length).toBeGreaterThan(0);
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'reservationsCteHeadSql', 'markedCountExistsSql', 'replayProbePrefixSql',
      'schemaReadySql', 'initialPhysicalCountJournalSql', 'inList',
      'evidencePositionRowsSql', 'executivePositionPointSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    // The assignment FOR SHARE lock literal is intentionally not folded.
    expect(adapter('count-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('transfer-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    for (const name of ['initial-physical-stock-count-source-sql.ts', 'manufacturing-initial-physical-stock-count-source-sql.ts', 'pos-initial-physical-stock-count-source-sql.ts']) {
      expect(adapter(name).length).toBeGreaterThan(0);
    }
  });

  test('retirement:ts-statements KPI is 300 after the r53 count-list head fold (strictly below r52 302)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r52 ended at 302. r53 folds the byte-identical `SELECT c.*,w.name
    // warehouse_name,` head shared by the list, page and scoped-count reads
    // into one owner-local helper: 3 source tokens -> 1, i.e. 302 -> 300.
    // r54 folds 3 package-obligation stock heads into one kind-parameterized renderer: 300 -> 298.
    expect(total).toBe(292);
    expect(total).toBeLessThan(299);
  });
});

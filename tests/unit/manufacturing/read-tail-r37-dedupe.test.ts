import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingSettlementFactSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r17-sql';
import { manufacturingPurchaseScopedRows } from '../../../packages/server-manufacturing/src/adapters/purchases';
import { manufacturingPurchaseScopedSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r24-sql';
import { manufacturingProjectionFromSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r37-sql';

const adapter = (name: string) => new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url);
const publicSrc = (name: string) => new URL(`../../../packages/server-manufacturing/src/public/${name}`, import.meta.url);
const sha256 = (text: string) => createHash('sha256').update(text).digest('hex');
const S = (alias: string) => manufacturingPurchaseScopedRows(alias);

/** Byte oracles captured from the pre-fold HEAD renderings of the r17
 * settlement-fact and r24 purchased-scope heads. Both opened the identical
 * projection-from-source head, differing only in the trailing source text; the
 * r37 emitter renders each one verbatim on the caller's own statement. The
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const FAMILY: readonly {
  name: string;
  render: () => string;
  oracle: string;
  sha256: string;
}[] = [
  {
    name: 'settlement/order',
    render: () => manufacturingSettlementFactSelect("'order' kind,o.id,to_jsonb(o)::text fact", "orders o WHERE o.status NOT IN ('completed','cancelled') OR NOT o.source_valid"),
    oracle: `SELECT 'order' kind,o.id,to_jsonb(o)::text fact FROM orders o WHERE o.status NOT IN ('completed','cancelled') OR NOT o.source_valid`,
    sha256: '27a3080fb83f7509db9b1dfcd86f986ff25602ac9c73aff13346d5eef1d40fcb',
  },
  {
    name: 'settlement/specification',
    render: () => manufacturingSettlementFactSelect("'specification',s.id,to_jsonb(s)::text", "manufacturing_specifications s,scope WHERE scope.full AND s.company_id=scope.company_id AND s.status NOT IN ('approved','rejected','retired')"),
    oracle: `SELECT 'specification',s.id,to_jsonb(s)::text FROM manufacturing_specifications s,scope WHERE scope.full AND s.company_id=scope.company_id AND s.status NOT IN ('approved','rejected','retired')`,
    sha256: '6cdec7f05e35eb613c1129daca219a6f5c89c3229cf293122fc529494f327c53',
  },
  {
    name: 'settlement/completed-cost',
    render: () => manufacturingSettlementFactSelect("'completed-cost',o.id,to_jsonb(o)::text", "orders o LEFT JOIN manufacturing_cost_snapshots cost ON cost.order_id=o.id WHERE o.status='completed' AND (cost.id IS NULL OR cost.total_minor<>o.material_cost_minor+o.labor_cost_minor+o.overhead_cost_minor OR cost.output_quantity_milli IS DISTINCT FROM o.reported_output_milli)"),
    oracle: `SELECT 'completed-cost',o.id,to_jsonb(o)::text FROM orders o LEFT JOIN manufacturing_cost_snapshots cost ON cost.order_id=o.id WHERE o.status='completed' AND (cost.id IS NULL OR cost.total_minor<>o.material_cost_minor+o.labor_cost_minor+o.overhead_cost_minor OR cost.output_quantity_milli IS DISTINCT FROM o.reported_output_milli)`,
    sha256: '3920563089791ceb64b57fafd3888aee153fdb51d0faff08bf192432e26029c9',
  },
  {
    name: 'scope/metric',
    render: () => manufacturingPurchaseScopedSelect('production_metric.id', 'production_metric', "CROSS JOIN production_metric_clock WHERE production_metric.kind='order' AND M AND P"),
    oracle: `SELECT production_metric.id FROM ${S('production_metric')} CROSS JOIN production_metric_clock WHERE production_metric.kind='order' AND M AND P`,
    sha256: 'd20321d5f7da94a3ca1fc79f0e2a96589e8e67895c4bfc1b20ca07c5999495d7',
  },
  {
    name: 'scope/record',
    render: () => manufacturingPurchaseScopedSelect('1', 'production_source', 'CROSS JOIN production_source_clock WHERE production_source.id=? AND S'),
    oracle: `SELECT 1 FROM ${S('production_source')} CROSS JOIN production_source_clock WHERE production_source.id=? AND S`,
    sha256: '947a612e895ad097d4dd6be85dcf9b834256bdc58f33f55788747145d499daae',
  },
  {
    name: 'scope/file',
    render: () => manufacturingPurchaseScopedSelect('1', 'production_file_scope', 'WHERE id=? AND company_id=? AND A'),
    oracle: `SELECT 1 FROM ${S('production_file_scope')} WHERE id=? AND company_id=? AND A`,
    sha256: 'd252d28889eaf1d6ea9bd591380f80c903838c86f28612baac00df483ce25a5f',
  },
];

/** Post-fold whole-file pins proving each delegating helper keeps its own
 * arguments, callers and surrounding text byte-unchanged beyond the head. */
const SETTLEMENT_HELPER_SHA256 = '6148599099b4f976ed67ce0b00159ace964e446d550ad4bc2d4ee3c2a8b7a846';
const SCOPED_HELPER_SHA256 = 'adff1bbe57ae0038ac67e0fef8cd0bfcf3d43fd00f256828c0e01e57318b2baa';
/** Lock-bearing, native and cross-owner witnesses are untouched by r37. */
const NATIVE_COMMAND_SHA256 = '51cda9ad07a4ecbdd6c098f1b2e859a947b8309cea74dfec26fe975b842b25a7';
const FINANCE_RECORD_SHA256 = '1940f9f739ebdd8d82cd3b1d9c7efff88a227a5aeef132d79680ffaa1f1d331b';
const COMMAND_CALLER_SHA256 = '3bbca63d700786746c0fa5c038c5e09d84c4e96b0416d8c86a17e039724024cf';
const OBLIGATIONS_CALLER_SHA256 = '4dfaa4aace1489173320dbb81787971bc780c7dcaad99cb7a730993599b79163';

async function tsFiles(directory: URL): Promise<URL[]> {
  const found: URL[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory));
  }
  return found;
}

describe('manufacturing read-tail r37 projection-from-source head-family fold', () => {
  it('renders all six pre-fold head renderings byte-identically to their oracles', () => {
    for (const { name, render, oracle, sha256: expected } of FAMILY) {
      expect(sha256(oracle), name).toBe(expected);
      expect(render(), name).toBe(oracle);
      expect(oracle, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps one byte-identical projection head across both helper families', () => {
    for (const { name, oracle, render } of FAMILY) {
      const split = oracle.indexOf(' FROM ');
      expect(split, name).toBeGreaterThan(0);
      const projection = oracle.slice('SELECT '.length, split);
      const source = oracle.slice(split + ' FROM '.length);
      expect(projection, name).not.toContain('SELECT');
      expect(source.length, name).toBeGreaterThan(0);
      expect(manufacturingProjectionFromSelect(projection, source), name).toBe(oracle);
      expect(render(), name).toBe(oracle);
      expect(oracle, name).toMatch(/^SELECT .+ FROM /);
    }
  });

  it('defines one text-only emitter and routes both helpers through it', async () => {
    const helper = await readFile(adapter('manufacturing-read-tail-r37-sql.ts'), 'utf8');
    expect(helper.match(/export const manufacturingProjectionFromSelect/g) ?? []).toHaveLength(1);
    expect(helper.split('SELECT').length - 1).toBe(1);
    expect(helper).toContain('`SELECT ${projection} FROM ${source}`');
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    expect(helper).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);

    const settlement = await readFile(adapter('manufacturing-read-tail-r17-sql.ts'), 'utf8');
    expect(settlement).toContain("from \"./manufacturing-read-tail-r37-sql\"");
    expect(settlement.split('manufacturingProjectionFromSelect(').length - 1).toBe(1);
    expect(settlement).toContain('manufacturingProjectionFromSelect(projection, source)');
    expect(settlement).not.toContain('`SELECT ${projection} FROM ${source}`');

    const scoped = await readFile(adapter('manufacturing-read-tail-r24-sql.ts'), 'utf8');
    expect(scoped).toContain("from \"./manufacturing-read-tail-r37-sql\"");
    expect(scoped.split('manufacturingProjectionFromSelect(').length - 1).toBe(1);
    expect(scoped).toContain('manufacturingProjectionFromSelect(projection, `${manufacturingPurchaseScopedRows(alias)} ${tail}`)');
    expect(scoped).not.toContain('`SELECT ${projection} FROM ${manufacturingPurchaseScopedRows(alias)} ${tail}`');
  });

  it('leaves both delegating helpers byte-stable beyond the delegated head', async () => {
    expect(sha256(await readFile(adapter('manufacturing-read-tail-r17-sql.ts'), 'utf8'))).toBe(SETTLEMENT_HELPER_SHA256);
    expect(sha256(await readFile(adapter('manufacturing-read-tail-r24-sql.ts'), 'utf8'))).toBe(SCOPED_HELPER_SHA256);
    // Every caller keeps its own prepared statement and binds; r37 never prepares.
    expect((await readFile(adapter('package-obligations.ts'), 'utf8')).split('manufacturingSettlementFactSelect(').length - 1).toBe(6);
    expect((await readFile(adapter('metric-purchases.ts'), 'utf8')).split('manufacturingPurchaseScopedSelect(').length - 1).toBe(1);
    expect((await readFile(adapter('record-purchases.ts'), 'utf8')).split('manufacturingPurchaseScopedSelect(').length - 1).toBe(1);
    expect((await readFile(adapter('attachment-purchases.ts'), 'utf8')).split('manufacturingPurchaseScopedSelect(').length - 1).toBe(1);
  });

  it('leaves locks/native/cross-owner witnesses untouched', async () => {
    expect(sha256(await readFile(publicSrc('native_command.rs'), 'utf8'))).toBe(NATIVE_COMMAND_SHA256);
    expect(sha256(await readFile(adapter('finance-record-sources.ts'), 'utf8'))).toBe(FINANCE_RECORD_SHA256);
    expect(sha256(await readFile(adapter('production-command-sources.ts'), 'utf8'))).toBe(COMMAND_CALLER_SHA256);
    expect(sha256(await readFile(adapter('package-obligations.ts'), 'utf8'))).toBe(OBLIGATIONS_CALLER_SHA256);
    const finance = await readFile(adapter('finance-record-sources.ts'), 'utf8');
    expect(finance).toContain('LOCK TABLE planning_scenarios,planning_recommendations');
    expect(finance).toContain('LOCK TABLE manufacturing_specifications,manufacturing_specification_lines');
    const command = await readFile(adapter('production-command-sources.ts'), 'utf8');
    expect(command).toContain('${manufacturingIdSelect("manufacturing_orders")} WHERE id=? FOR UPDATE');
    const obligations = await readFile(adapter('package-obligations.ts'), 'utf8');
    expect(obligations).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
  });

  it('pins the manufacturing owner SELECT KPI at 45 after the r22 native detail-read cut (46->45)', async () => {
    let total = 0;
    for (const file of await tsFiles(new URL('../../../packages/server-manufacturing/src/', import.meta.url))) {
      total += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    }
    expect(total).toBe(42); // package-obligation owner-table heads 45->42.
    expect(total).toBeLessThan(49);
  });
});

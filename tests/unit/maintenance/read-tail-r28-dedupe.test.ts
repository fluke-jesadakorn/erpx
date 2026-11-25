import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

const adapters = new URL('../../../packages/server-maintenance/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, adapters), 'utf8');
const srcRoot = resolve(__dirname, '../../../packages/server-maintenance/src');

/** Pre-r28 byte oracles: the three facts UNION rows rendered by the retired
 * `SELECT ${kind},${columns} FROM ${from}` inline head. */
const workKind = "'work' kind";
const workColumns = "work.id,work.version,work.status state,NOT work.source_valid OR work.status NOT IN ('completed','cancelled') OR work.downtime_started_at IS NOT NULL AND work.downtime_ended_at IS NULL pending";
const workRow = `SELECT ${workKind},${workColumns} FROM work`;
const planKind = "'preventive-plan'";
const planColumns = "plan.id,plan.version,plan.status,NOT plan.source_valid OR plan.status<>'cancelled'";
const planRow = `SELECT ${planKind},${planColumns} FROM plans plan`;
const occurrenceKind = "'occurrence'";
const occurrenceColumns = "occurrence.id,occurrence.plan_version,'ambiguous-source',true";

describe('maintenance read-tail r28 fact-row delegation', () => {
  it('renders all three facts UNION rows byte-identically through settlementRefSource', async () => {
    const { factRow, maintenancePackageSettlementSources } = await import('../../../packages/server-maintenance/src/adapters/package-obligations');
    const { settlementRefSource } = await import('../../../packages/server-maintenance/src/adapters/asset-settlement-refs');
    const { projectionLookup } = await import('../../../packages/server-maintenance/src/adapters/projection-lookup');
    const { assetPackageSettlementSources: as } = await import('../../../packages/server-assets/src/public/package-obligations');
    const { work } = maintenancePackageSettlementSources;
    const occurrenceFrom = `maintenance_preventive_occurrences occurrence JOIN plans plan ON plan.id=occurrence.plan_id LEFT JOIN (${work}) generated ON generated.id=occurrence.work_order_id AND generated.company_id=plan.company_id WHERE generated.id IS NULL OR generated.asset_id<>plan.asset_id OR occurrence.plan_version>plan.version OR NOT generated.source_valid OR occurrence.source_asset_version>${projectionLookup("version", as.assets, "actual", "actual.id=plan.asset_id AND actual.company_id=plan.company_id")}`;
    const occurrenceRow = `SELECT ${occurrenceKind},${occurrenceColumns} FROM ${occurrenceFrom}`;
    for (const [kind, columns, from, oracle] of [
      [workKind, workColumns, 'work', workRow],
      [planKind, planColumns, 'plans plan', planRow],
      [occurrenceKind, occurrenceColumns, occurrenceFrom, occurrenceRow],
    ] as const) {
      expect(factRow(kind, columns, from)).toBe(oracle);
      expect(settlementRefSource(`${kind},${columns}`, from)).toBe(oracle);
    }
    expect(factRow('x', 'y', 't')).toBe(settlementRefSource('x,y', 't'));
    for (const sql of [workRow, planRow, occurrenceRow]) {
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps one delegate definition, three callers and no inline SELECT head', async () => {
    const source = await src('package-obligations.ts');
    expect(source.match(/export const factRow=\(kind:string,columns:string,from:string\)=>settlementRefSource\(`\$\{kind\},\$\{columns\}`,from\);/g) ?? []).toHaveLength(1);
    expect(source).toContain('import {settlementRefSource} from "./asset-settlement-refs"');
    expect(source.split('factRow(').length - 1).toBe(3);
    expect(source).not.toContain('`SELECT ${kind},${columns} FROM ${from}`');
    // ownerSourceSql, scopedSourceCte and the WITH scope point read remain.
    expect(source.match(/SELECT/g)).toHaveLength(3);
    expect(source).toContain('${factRow("\'work\' kind"');
    expect(source).toContain('${factRow("\'preventive-plan\'"');
    expect(source).toContain('${factRow("\'occurrence\'"');
  });

  it('leaves locks, native reads and every r3-r27 family untouched', async () => {
    const names = [
      'package-obligations.ts', 'asset-settlement-refs.ts', 'work.ts',
      'purchase-sources.ts', 'finance-record-sources.ts', 'projection-lookup.ts',
    ];
    const texts = await Promise.all(names.map(src));
    expect(texts[0]).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
    expect(texts[1]).toContain('export const settlementRefSource');
    expect(texts[1]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
    expect(texts[2]).toContain('export const workAssetSource');
    expect(texts[2]).toContain('settlementRefSource("*","maintenance_work_events"," WHERE work_order_id=? ORDER BY version")');
    expect(texts[3]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
    expect(texts[4]).toContain('LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE');
    for (const symbol of ['export const projectionHead', 'export const projectionLookup', 'export const existsGuardPrefix', 'export const notExistsGuardPrefix']) {
      expect(texts[5]).toContain(symbol);
    }
    for (const text of texts) {
      expect(text).not.toMatch(/INSERT\s+INTO|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
    }
  });

  it('retirement:ts-statements stays strictly below the r28 BEFORE=21 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(19);
    expect(total).toBeLessThan(20);
  });
});

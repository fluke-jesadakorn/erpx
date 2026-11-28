import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingAccountingProfileSelect } from '../../../modules/manufacturing/server/src/adapters/manufacturing-read-tail-r19-sql';
import { manufacturingStockFootprintSelect } from '../../../modules/manufacturing/server/src/adapters/manufacturing-read-tail-r23-sql';
import { manufacturingOwnerTableSelect } from '../../../modules/manufacturing/server/src/adapters/manufacturing-read-tail-r36-sql';
import {
  manufacturingCostAllocationProbe,
  manufacturingPackageSettlementSources,
} from '../../../modules/manufacturing/server/src/adapters/package-obligations';
import { manufacturingPurchaseOrderRows } from '../../../modules/manufacturing/server/src/adapters/purchases';

const adapter = (name: string) => new URL(`../../../modules/manufacturing/server/src/adapters/${name}`, import.meta.url);
const publicSrc = (name: string) => new URL(`../../../modules/manufacturing/server/src/public/${name}`, import.meta.url);
const sha256 = (text: string) => createHash('sha256').update(text).digest('hex');

/** Byte oracles captured from the pre-fold HEAD renderings of the accounting-
 * profile, stock-footprint and package-settlement source-head families. All of
 * them opened the same byte-identical projection head up to
 * `FROM manufacturing_`, differing only in the trailing
 * owner-table/alias/join/predicate text; the r36 emitter renders each one
 * verbatim on the caller's own prepared statement. The SHA-256 values are
 * byte-identity oracles only, never live query sources. */
const FAMILY: readonly {
  name: string;
  render: () => string;
  oracle: string;
  sha256: string;
}[] = [
  {
    name: 'profiles/all',
    render: () => manufacturingAccountingProfileSelect('*', '', "company_id=? AND status='approved'"),
    oracle: `SELECT * FROM manufacturing_accounting_profiles WHERE company_id=? AND status='approved'`,
    sha256: 'dfde2af7af81bb03a1ee65585b17f0e00717e51b5d5c51a048b1555af485bc86',
  },
  {
    name: 'profiles/columns',
    render: () => manufacturingAccountingProfileSelect('id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id', '', 'company_id=? ORDER BY updated_at DESC'),
    oracle: `SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=? ORDER BY updated_at DESC`,
    sha256: '7cc001f4620e4bff5713c9c1fb73b702de73328cea191f96e83b207ee2b2eac4',
  },
  {
    name: 'profiles/scoped',
    render: () => manufacturingAccountingProfileSelect('p.*,to_jsonb(p)::text source', 'p', 'company_id=? ORDER BY id'),
    oracle: `SELECT p.*,to_jsonb(p)::text source FROM manufacturing_accounting_profiles p WHERE company_id=? ORDER BY id`,
    sha256: '62100577e82f53d4e64c4213dd6394220cb5257a61a6efcf8e4b6c6261a93b34',
  },
  {
    name: 'profiles/witness',
    render: () => manufacturingAccountingProfileSelect("id,company_id,NULL::text branch_id,false advanced,to_jsonb(profile)-'status'-'version'-'updated_at'-'reviewed_by' witness", 'profile', ''),
    oracle: `SELECT id,company_id,NULL::text branch_id,false advanced,to_jsonb(profile)-'status'-'version'-'updated_at'-'reviewed_by' witness FROM manufacturing_accounting_profiles profile`,
    sha256: '423f752c40eff02cb0801b18c6fe348cd70dc782aae5a3e562edf648a9fe33bc',
  },
  {
    name: 'footprint/specifications',
    render: () => manufacturingStockFootprintSelect('o.id order_id,o.company_id,o.warehouse_id,s.output_product_id product_id', 'manufacturing_specifications', 's', 's.id=o.specification_id AND s.company_id=o.company_id'),
    oracle: `SELECT o.id order_id,o.company_id,o.warehouse_id,s.output_product_id product_id FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id AND s.company_id=o.company_id`,
    sha256: '064a9f5bafd6d1eda07fb0553cf47e9a5af0d363e97a2d3bf139c78e6e093874',
  },
  {
    name: 'footprint/lines',
    render: () => manufacturingStockFootprintSelect('o.id,o.company_id,o.warehouse_id,line.product_id', 'manufacturing_specification_lines', 'line', 'line.specification_id=o.specification_id'),
    oracle: `SELECT o.id,o.company_id,o.warehouse_id,line.product_id FROM manufacturing_orders o JOIN manufacturing_specification_lines line ON line.specification_id=o.specification_id`,
    sha256: 'b683b8bd63ba8de0f41b4d0e9e25595e4bdf95ac8a3c71130967ff42f8aae6a1',
  },
];

/** Byte oracles for the package-settlement source heads that joined the same
 * owner-table head family in the next read round. They render on the callers'
 * own statements exactly as the former inline `SELECT ... FROM manufacturing_`
 * strings did. */
const SETTLEMENT: readonly {
  name: string;
  render: () => string;
  oracle: string;
}[] = [
  {
    name: 'settlement/orders',
    render: () => manufacturingPackageSettlementSources.orders,
    oracle: `SELECT o.*,COALESCE(valid.advanced,false) advanced,valid.id IS NOT NULL source_valid FROM manufacturing_orders o LEFT JOIN (${manufacturingPurchaseOrderRows}) valid ON valid.id=o.id AND valid.company_id=o.company_id`,
  },
  {
    name: 'settlement/postings',
    render: () => manufacturingPackageSettlementSources.postings,
    oracle: 'SELECT p.*,profile.chart_id,profile.currency profile_currency,profile.wip_account_id FROM manufacturing_order_postings p LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=p.profile_id AND profile.company_id=p.company_id',
  },
  {
    name: 'probe/joined',
    render: () => manufacturingCostAllocationProbe(true),
    oracle: 'SELECT 1 FROM manufacturing_cost_allocations allocation JOIN orders o ON o.id=allocation.order_id WHERE allocation.pool_id=pool.id',
  },
  {
    name: 'probe/plain',
    render: () => manufacturingCostAllocationProbe(false),
    oracle: 'SELECT 1 FROM manufacturing_cost_allocations allocation WHERE allocation.pool_id=pool.id',
  },
];

/** Post-fold whole-file pins proving each helper delegates only its head and
 * keeps its own arguments, tail and callers byte-unchanged. */
const PROFILE_HELPER_SHA256 = '2510a8a0c83112502c07211a7069b7451095438fa3e4de26d333997a968836e7';
const FOOTPRINT_HELPER_SHA256 = '6050531a7afd11b1a4b53e4c6a724821800da447a198548763f74ca8b73098da';
/** Lock-bearing, native and cross-owner witnesses are untouched by r36. */
const NATIVE_COMMAND_SHA256 = '51cda9ad07a4ecbdd6c098f1b2e859a947b8309cea74dfec26fe975b842b25a7';

async function tsFiles(directory: URL): Promise<URL[]> {
  const found: URL[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory));
  }
  return found;
}

describe('manufacturing read-tail r36 owner-table head-family fold', () => {
  it('renders all six pre-fold helper renderings byte-identically to their oracles', () => {
    for (const { name, render, oracle, sha256: expected } of FAMILY) {
      expect(sha256(oracle), name).toBe(expected);
      expect(render(), name).toBe(oracle);
      expect(oracle, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('renders all four package-settlement source heads byte-identically', () => {
    for (const { name, render, oracle } of SETTLEMENT) {
      expect(render(), name).toBe(oracle);
      expect(oracle, name).toMatch(/^SELECT .+ FROM manufacturing_/);
      expect(oracle, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps one byte-identical projection head across every caller family', () => {
    for (const { name, oracle } of [...FAMILY, ...SETTLEMENT]) {
      const split = oracle.indexOf(' FROM manufacturing_');
      expect(split, name).toBeGreaterThan(0);
      const columns = oracle.slice('SELECT '.length, split);
      const tail = oracle.slice('SELECT '.length + columns.length + ' FROM manufacturing_'.length);
      expect(columns, name).not.toContain('SELECT');
      if (name.startsWith('profiles/') || name.startsWith('footprint/')) expect(tail, name).not.toContain('SELECT');
      expect(manufacturingOwnerTableSelect(columns, tail), name).toBe(oracle);
      expect(oracle, name).toMatch(/^SELECT .+ FROM manufacturing_/);
    }
  });

  it('defines one text-only emitter and routes both helpers through it', async () => {
    const helper = await readFile(adapter('manufacturing-read-tail-r36-sql.ts'), 'utf8');
    expect(helper.match(/export const manufacturingOwnerTableSelect/g) ?? []).toHaveLength(1);
    expect(helper.split('`SELECT').length - 1).toBe(1);
    expect(helper).toContain('`SELECT ${columns} FROM manufacturing_${tail}`');
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    expect(helper).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);

    const profiles = await readFile(adapter('manufacturing-read-tail-r19-sql.ts'), 'utf8');
    expect(profiles).toContain("from './manufacturing-read-tail-r36-sql'");
    expect(profiles.split('manufacturingOwnerTableSelect(').length - 1).toBe(1);
    expect(profiles).toContain('manufacturingOwnerTableSelect(columns, `accounting_profiles${alias ? ` ${alias}` : ""}${where ? ` WHERE ${where}` : ""}`)');
    expect(profiles).not.toContain('`SELECT ${columns} FROM manufacturing_accounting_profiles');

    const footprint = await readFile(adapter('manufacturing-read-tail-r23-sql.ts'), 'utf8');
    expect(footprint).toContain("from './manufacturing-read-tail-r36-sql'");
    expect(footprint.split('manufacturingOwnerTableSelect(').length - 1).toBe(1);
    expect(footprint).toContain('manufacturingOwnerTableSelect(columns, `orders o JOIN ${table} ${alias} ON ${on}`)');
    expect(footprint).not.toContain('`SELECT ${columns} FROM manufacturing_orders');

    const settlement = await readFile(adapter('package-obligations.ts'), 'utf8');
    expect(settlement).toContain('from "./manufacturing-read-tail-r36-sql"');
    expect(settlement.split('manufacturingOwnerTableSelect(').length - 1).toBe(3);
    for (const inline of [
      '`SELECT o.*,COALESCE(valid.advanced,false)',
      '"SELECT p.*,profile.chart_id',
      '`SELECT 1 FROM manufacturing_cost_allocations allocation',
    ]) expect(settlement).not.toContain(inline);
    // The plain row sources and stock-footprint union keep their own heads.
    expect(settlement.split('manufacturingRowsSelect(').length - 1).toBe(4);
    expect(settlement.split('manufacturingStockFootprintSelect(').length - 1).toBe(2);
    expect(settlement).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
  });

  it('leaves both delegating helpers byte-stable beyond the delegated head', async () => {
    expect(sha256(await readFile(adapter('manufacturing-read-tail-r19-sql.ts'), 'utf8'))).toBe(PROFILE_HELPER_SHA256);
    expect(sha256(await readFile(adapter('manufacturing-read-tail-r23-sql.ts'), 'utf8'))).toBe(FOOTPRINT_HELPER_SHA256);
    // Every caller keeps its own prepared statement and binds; r36 never prepares.
    expect((await readFile(adapter('purchases.ts'), 'utf8')).split('manufacturingAccountingProfileSelect(').length - 1).toBe(1);
    expect((await readFile(adapter('production-command-sources.ts'), 'utf8')).split('manufacturingAccountingProfileSelect(').length - 1).toBe(0); // accountingProfiles cut to native owner
    expect((await readFile(adapter('package-obligations.ts'), 'utf8')).split('manufacturingStockFootprintSelect(').length - 1).toBe(2);
  });

  it('leaves locks/native/cross-owner witnesses untouched', async () => {
    expect(sha256(await readFile(publicSrc('native_command.rs'), 'utf8'))).toBe(NATIVE_COMMAND_SHA256);
    const finance = await readFile(adapter('finance-record-sources.ts'), 'utf8');
    expect(finance).toContain('LOCK TABLE planning_scenarios,planning_recommendations');
    expect(finance).toContain('LOCK TABLE manufacturing_specifications,manufacturing_specification_lines');
    const command = await readFile(adapter('production-command-sources.ts'), 'utf8');
    expect(command).toContain('${manufacturingIdSelect("manufacturing_orders")} WHERE id=? FOR UPDATE');
  });

  it('pins the manufacturing owner SELECT KPI at 42 after the package-settlement head fold (45->42)', async () => {
    let total = 0;
    for (const file of await tsFiles(new URL('../../../modules/manufacturing/server/src/', import.meta.url))) {
      total += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    }
    expect(total).toBe(42); // package-obligation owner-table heads 45->42.
    expect(total).toBeLessThan(50);
  });
});

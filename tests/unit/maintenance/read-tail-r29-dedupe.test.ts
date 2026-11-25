import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

const adapters = new URL('../../../packages/server-maintenance/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, adapters), 'utf8');
const srcRoot = resolve(__dirname, '../../../packages/server-maintenance/src');

/** Pre-r29 byte oracles: the descriptor head shared by the operational
 * (module-scoped) and policy (permission-checked) attachment fact sources was
 * rendered by the inline `SELECT company_id,${branch},${department},${version}
 * FROM ${table} WHERE id=? AND company_id=?${extra}` literal. r29 keeps every
 * composed string byte-identical while delegating that head to the owner-local
 * settlementRefSource renderer. Binds stay in the two callers. */
const noExtra = 'SELECT company_id,branch_id branch_id,department_id department_id,version FROM maintenance_work_orders WHERE id=? AND company_id=?';
const withAliases = 'SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM maintenance_work_orders WHERE id=? AND company_id=?';
const withExtra = "SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM maintenance_preventive_plans WHERE id=? AND company_id=? AND plan.status='active'";

describe('maintenance read-tail r29 descriptor-head delegation', () => {
  it('renders all descriptor callers byte-identically through settlementRefSource', async () => {
    const { attachmentDescriptorReadSql } = await import('../../../packages/server-maintenance/src/adapters/attachment-descriptor-read');
    const { settlementRefSource } = await import('../../../packages/server-maintenance/src/adapters/asset-settlement-refs');
    for (const [got, oracle] of [
      [attachmentDescriptorReadSql('maintenance_work_orders', 'branch_id branch_id', 'department_id department_id', 'version'), noExtra],
      [attachmentDescriptorReadSql('maintenance_work_orders', 'branch_id AS branch_id', 'department_id AS department_id', 'version AS version'), withAliases],
      [attachmentDescriptorReadSql('maintenance_preventive_plans', 'branch_id AS branch_id', 'department_id AS department_id', 'version AS version', "plan.status='active'"), withExtra],
    ] as const) {
      expect(got).toBe(oracle);
    }
    // The family's head is exactly the canonical settlementRefSource head; the
    // empty-tail case stays byte-identical to the r19/r26/r27/r28 callers.
    expect(settlementRefSource('company_id,branch_id branch_id,department_id department_id,version', 'maintenance_work_orders', ' WHERE id=? AND company_id=?')).toBe(noExtra);
    expect(settlementRefSource('x', 't')).toBe('SELECT x FROM t');
    for (const sql of [noExtra, withAliases, withExtra]) {
      expect(sql.startsWith('SELECT company_id,')).toBe(true);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps one delegate definition, both callers and no inline SELECT head', async () => {
    const helper = await src('attachment-descriptor-read.ts');
    const operational = await src('attachments-operational-facts.ts');
    const policy = await src('attachments-policy-facts.ts');
    expect(helper.match(/SELECT/g)).toBeNull();
    expect(helper).toContain('import {settlementRefSource} from "./asset-settlement-refs"');
    expect(helper).toContain('settlementRefSource(`company_id,${branch},${department},${version}`,table,');
    expect(helper).not.toContain('`SELECT company_id,${branch},${department},${version} FROM ${table}');
    expect(operational).toContain('attachmentDescriptorReadSql(moduleTarget.table');
    expect(policy).toContain('attachmentDescriptorReadSql(descriptor.table');
    // Binds are unchanged: both callers still supply targetId and companyId.
    expect(operational).toContain('.bind(facts.targetId,facts.companyId)');
    expect(policy).toContain('.bind(facts.targetId,facts.companyId)');
  });

  it('leaves the canonical head, locks, native reads and r3-r28 families untouched', async () => {
    const names = [
      'asset-settlement-refs.ts', 'projection-lookup.ts', 'work.ts',
      'package-obligations.ts', 'finance-record-sources.ts', 'purchase-sources.ts',
      'calendar-maintenance-facts.ts', 'executive-metric-projections.ts',
    ];
    const texts = await Promise.all(names.map(src));
    expect(texts[0].match(/SELECT/g)).toHaveLength(1);
    expect(texts[0]).toContain('export const settlementRefSource');
    expect(texts[0]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
    for (const symbol of ['export const projectionHead', 'export const projectionLookup', 'export const existsGuardPrefix', 'export const notExistsGuardPrefix']) {
      expect(texts[1]).toContain(symbol);
    }
    expect(texts[2]).toContain('export const workAssetSource');
    expect(texts[3]).toContain('export const factRow=');
    expect(texts[3]).toContain('export const scopedSourceCte=');
    expect(texts[4]).toContain('const journalLineSum=');
    expect(texts[5]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
    expect(texts[6]).toContain('export const scheduledSourceSql');
    expect(texts[7]).toContain('export const windowedIntervalCte');
    for (const text of texts) {
      expect(text).not.toMatch(/INSERT\s+INTO|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
    }
  });

  it('retirement:ts-statements stays strictly below the r30 BEFORE=19 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(18);
    expect(total).toBeLessThan(19);
  });
});

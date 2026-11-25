import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

const adapters = new URL('../../../packages/server-maintenance/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, adapters), 'utf8');
const srcRoot = resolve(__dirname, '../../../packages/server-maintenance/src');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

/** Pre-r31 byte oracles: the Calendar scheduled outer read carried the inline
 * alias-qualified row head `SELECT s.*`, the same byte-identical head family
 * r30 delegated for the Work/Plan source image and scoped source CTE. r31 keeps
 * the whole composed statement byte-identical while routing this third caller
 * through the owner-local aliasRowsSql renderer. Binds stay in the caller. */
const head = 'SELECT s.*';
const stubFragment = 'SELECT s.*,<OWNER> owner_name';

describe('maintenance read-tail r31 alias-row head reuse', () => {
  it('pins the byte-identical head oracles', () => {
    expect(sha(head)).toBe('32080fa23a93aaa14f929763ee0e5e7be3eb3a2de0748febf96fdaf1e226a9a4');
    expect(sha(stubFragment)).toBe('2a47be030e953fdaa1fe031cee5e9505c6a91265ffc4eb790cf62d9547b8d77f');
  });

  it('renders the calendar head byte-identically through aliasRowsSql', async () => {
    const { aliasRowsSql } = await import('../../../packages/server-maintenance/src/adapters/read-tail-r30-alias-rows');
    const { maintenanceOwnerName } = await import('../../../packages/server-maintenance/src/adapters/maintenance-owner-name');
    const { calendarMaintenanceMaintenanceFacts } =
      await import('../../../packages/server-maintenance/src/adapters/calendar-maintenance-facts');

    expect(aliasRowsSql('s', '')).toBe(head);
    const owner = maintenanceOwnerName('s');
    const fragment = `SELECT s.*,${owner} owner_name`;
    expect(aliasRowsSql('s', `,${owner} owner_name`)).toBe(fragment);
    expect(sha(fragment)).toBe('10200a37ade3b3dcf34ba6c8ca397cf4928cd0e05566be1d7b4449bb63d7d3c1');

    // Full shipped statement, captured at the prepare boundary with a stub DB.
    let sql = '';
    const db = {
      prepare(statement: string) {
        sql = statement;
        return { bind() { return this; }, all() { return Promise.resolve({ results: [] }); } };
      },
    };
    await calendarMaintenanceMaintenanceFacts(db as never).read1({
      value1: 'c', value2: null, value3: null, value4: '', value5: 0, value6: 0,
      value7: 'mine', value8: '', value9: null, value10: '', value11: 1,
    });
    expect(sql).toContain(fragment);
    expect(sha(sql)).toBe('58dc60282dbee2a712a70cea5f229d74009318f79e8edcdd3458c04f9af48b0f');
    for (const text of [head, fragment, sql]) {
      expect(text).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps one head definition, the calendar caller and no inline alias-qualified head', async () => {
    const helper = await src('read-tail-r30-alias-rows.ts');
    const calendar = await src('calendar-maintenance-facts.ts');
    expect(helper.match(/SELECT/g)).toHaveLength(1);
    expect(helper).toContain('export const aliasRowsSql');
    expect(calendar).toContain('import {aliasRowsSql} from "./read-tail-r30-alias-rows"');
    expect(calendar).toContain('${aliasRowsSql("s", `,${maintenanceOwnerName(\'s\')} owner_name`)}');
    expect(calendar.match(/SELECT/g)).toHaveLength(1);
    expect(calendar).not.toContain('  SELECT s.*,');
    // The scheduled projection family, in-list read and owner-image read stay.
    expect(calendar).toContain('export const scheduledSourceSql');
    expect(calendar).toContain('IN ${subqueryHead("jsonb_array_elements_text(?::jsonb)")}');
    expect(calendar).toContain('projectionLookup("user_id",people_payrollProjectionSources.hr_people');
    expect(calendar).not.toContain('LOCK TABLE');
  });

  it('leaves locks, native reads, r3-r30 families and cross-owner consumers untouched', async () => {
    const names = [
      'package-obligations.ts', 'asset-settlement-refs.ts', 'projection-lookup.ts',
      'purchase-sources.ts', 'finance-record-sources.ts',
      'executive-metric-projections.ts', 'work.ts',
    ];
    const texts = await Promise.all(names.map(src));
    expect(texts[0]).toContain('export const factRow=');
    expect(texts[0]).toContain('export const scopedSourceCte=');
    expect(texts[0]).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
    expect(texts[1]).toContain('export const settlementRefSource');
    expect(texts[1]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
    expect(texts[2]).toContain('export const projectionHead');
    expect(texts[2]).toContain('export const existsGuardPrefix');
    expect(texts[3]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
    expect(texts[4]).toContain('const journalLineSum=');
    expect(texts[4]).toContain('LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE');
    expect(texts[5]).toContain('export const windowedIntervalCte');
    expect(texts[6]).toContain('export const workAssetSource');
    for (const text of texts) {
      expect(text).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
    }
  });

  it('retirement:ts-statements stays strictly below the r31 BEFORE=18 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(16);
    expect(total).toBeLessThan(18);
  });
});

import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { manufacturingPurchaseRecordSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r16-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url), 'utf8');

/** Stand-in for the imported purchased production-record rows. The helper only
 * wraps text, so byte identity does not depend on the rows themselves. */
const S = '<purchased production record rows>';
const original = (columns: string, alias: string, where: string) =>
  `SELECT ${columns} FROM (${S}) ${alias} WHERE ${where}`;

const ATTACHMENT = original('capability,advanced,witness::text witness', 'actual_production_file', 'id=? AND company_id=? AND kind=?');
const WORKSPACE = original('id,capability,advanced', 'production_workspace_source', 'company_id=? AND (branch_id IS NULL OR ?::text IS NULL OR branch_id=?)');
const RECORD = original('kind,module_id', 'production_record', 'id=? AND company_id=?');

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('manufacturing read-tail r16 parameterized family dedupe', () => {
  it('renders the shared purchased-record select byte-identically to all three originals', () => {
    expect(manufacturingPurchaseRecordSelect('capability,advanced,witness::text witness', S, 'actual_production_file', 'id=? AND company_id=? AND kind=?')).toBe(ATTACHMENT);
    expect(manufacturingPurchaseRecordSelect('id,capability,advanced', S, 'production_workspace_source', 'company_id=? AND (branch_id IS NULL OR ?::text IS NULL OR branch_id=?)')).toBe(WORKSPACE);
    expect(manufacturingPurchaseRecordSelect('kind,module_id', S, 'production_record', 'id=? AND company_id=?')).toBe(RECORD);
  });

  it('routes every family call site through the shared helper with no residual inline SQL', async () => {
    const targets: readonly (readonly [string, string])[] = [
      ['attachment-purchases.ts', '`SELECT capability,advanced,witness::text witness FROM (${manufacturingPurchaseRecordRows}) actual_production_file WHERE'],
      ['purchases.ts', '`SELECT id,capability,advanced FROM (${manufacturingPurchaseRecordRows}) production_workspace_source WHERE'],
      ['record-purchases.ts', '`SELECT kind,module_id FROM (${manufacturingPurchaseRecordRows}) production_record WHERE'],
    ];
    for (const [name, inline] of targets) {
      const text = await src(name);
      expect(text.split('from "./manufacturing-read-tail-r16-sql"').length - 1).toBe(1);
      expect(text.split('manufacturingPurchaseRecordSelect(').length - 1).toBe(1);
      expect(text).not.toContain(inline);
    }
  });

  it('keeps the helper read-only and leaves r11/r4/r15 helpers and locks untouched', async () => {
    for (const rendered of [ATTACHMENT, WORKSPACE, RECORD]) {
      expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const helper = await src('manufacturing-read-tail-r16-sql.ts');
    expect(helper).toContain('export const manufacturingPurchaseRecordSelect');
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    expect(await src('attachment-read-sources.ts')).toContain('manufacturingOrderLineageAgg');
    expect(await src('finance-record-sources.ts')).toContain('manufacturingJournalLineSum');
    const command = await src('production-command-sources.ts');
    expect(command).toContain('approvedSpecGuardSql');
    expect(command).toContain('operationEventDoneGuardSql');
    expect(command).toContain('manufacturingOrderJoinSelect(');
    expect(await src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(await src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(command).toContain('WHERE id=? FOR UPDATE');
  });

  it('keeps the manufacturing owner SELECT KPI strictly below the recorded BEFORE=94 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-manufacturing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(94);
    expect(count).toBeLessThanOrEqual(86); // r17 lowered 94->89; r18 89->86; r16 originally lowered 96->94.
  });
});

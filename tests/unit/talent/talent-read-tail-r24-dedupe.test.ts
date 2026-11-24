import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/package-obligations.ts', import.meta.url);
const prefixFile = new URL('../../../packages/server-talent/src/adapters/talent-read-tail-r27-sources.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted EXISTS fragments (r23 baseline). Only the FROM
// source and WHERE predicate are parameterized; the emitted SQL is unchanged.
const EXPECTED = {
  branch: `EXISTS(SELECT 1 FROM (SRC) branch WHERE branch.id=r.branch_id AND branch.company_id=r.company_id AND branch.kind IN ('branch','headquarters'))`,
  department: `EXISTS(SELECT 1 FROM (SRC) department WHERE department.id=r.department_id AND department.company_id=r.company_id AND department.branch_id=r.branch_id AND department.kind='department')`,
  offered: `EXISTS(SELECT 1 FROM offers o WHERE o.application_id=a.id AND o.source_valid AND o.status IN ('declined','withdrawn') AND o.terminal_evidence)`,
  accepted: `EXISTS(SELECT 1 FROM offers o JOIN talent_hire_handoffs h ON h.offer_id=o.id AND h.company_id=o.company_id WHERE o.application_id=a.id AND o.source_valid AND o.status='accepted' AND o.terminal_evidence)`,
  handoff: `EXISTS(SELECT 1 FROM talent_hire_handoffs h WHERE h.offer_id=o.id AND h.company_id=o.company_id)`,
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

function evaluate(text: string, prefixText: string): (source: string, where: string) => string {
  const prefix = prefixText.match(/export const existsGuardPrefix = [\s\S]*?;\n/)![0].replace('export const', 'const').replace(/:\s*string/g, '');
  const helper = text.match(/const existsRow=[\s\S]*?;\n/)![0].replace(/:\s*string/g, '');
  return new Function(`${prefix}\n${helper}\nreturn existsRow;`)();
}

describe('talent read-tail r24 EXISTS-row family dedupe', () => {
  it('declares the shared prefix once and routes all five lineages through it', async () => {
    const [text, prefixText] = await Promise.all([readFile(file, 'utf8'), readFile(prefixFile, 'utf8')]);
    expect(prefixText.split('export const existsGuardPrefix =').length - 1).toBe(1);
    expect(prefixText.split('EXISTS(SELECT 1 FROM').length - 1).toBe(1);
    expect(text.split('const existsRow=').length - 1).toBe(1);
    expect(text.split('${existsRow(').length - 1).toBe(5);
    expect(text).toContain('${existsGuardPrefix(source)}${where})');
    // The remaining literal EXISTS(SELECT 1 FROM string is the r19 offer-event
    // prefix; no inline NOT EXISTS(SELECT remains.
    expect(text.split('EXISTS(SELECT 1 FROM').length - 1).toBe(1);
    expect(text.split('NOT EXISTS(SELECT 1 FROM').length - 1).toBe(0);
  });

  it('emits byte-identical EXISTS SQL for all five pinned lineages', async () => {
    const [text, prefixText] = await Promise.all([readFile(file, 'utf8'), readFile(prefixFile, 'utf8')]);
    const f = evaluate(text, prefixText);
    expect(f('(SRC) branch', `branch.id=r.branch_id AND branch.company_id=r.company_id AND branch.kind IN ('branch','headquarters')`)).toBe(EXPECTED.branch);
    expect(f('(SRC) department', `department.id=r.department_id AND department.company_id=r.company_id AND department.branch_id=r.branch_id AND department.kind='department'`)).toBe(EXPECTED.department);
    expect(f('offers o', `o.application_id=a.id AND o.source_valid AND o.status IN ('declined','withdrawn') AND o.terminal_evidence`)).toBe(EXPECTED.offered);
    expect(f('offers o JOIN talent_hire_handoffs h ON h.offer_id=o.id AND h.company_id=o.company_id', `o.application_id=a.id AND o.source_valid AND o.status='accepted' AND o.terminal_evidence`)).toBe(EXPECTED.accepted);
    expect(f('talent_hire_handoffs h', `h.offer_id=o.id AND h.company_id=o.company_id`)).toBe(EXPECTED.handoff);
  });

  it('keeps source locks and consumers unchanged', async () => {
    const text = await readFile(file, 'utf8');
    for (const ref of [
      'LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts',
      'talentSettlementPlacementReadLocks(database)',
      'export const talentPackageSettlementSources',
      'export async function lockTalentPackageObligationSources',
      'export async function talentPackageObligations',
      'kind,r.id,r.version,r.status state,NOT r.source_valid OR r.status NOT IN',
    ]) expect(text, ref).toContain(ref);
    expect(text).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below BEFORE=47', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(44);
    expect(count).toBeLessThan(45);
  });
});

import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/metric-purchases.ts', import.meta.url);
const util = new URL('../../../packages/server-talent/src/adapters/talent-read-tail-r32-sources.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r27 baseline, BEFORE=51). Both metric source
// rows are owner-local literals, so the pinned strings are stable.
const REQUISITIONS = `SELECT * FROM talent_requisitions`;
const APPLICATIONS = `SELECT a.*,r.branch_id,r.department_id FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text: string): (cols: string, from: string) => string {
  const helper = text.match(/const talentSourceRows = [\s\S]*?;\n/)![0]
    .replace(/:\s*string/g, '');
  return new Function(`${helper}\nreturn talentSourceRows;`)();
}

describe('talent read-tail r28 metric source-row family dedupe', () => {
  it('declares the shared family once and routes both source rows through it', async () => {
    const [u, t] = await Promise.all([util, file].map(f => readFile(f, 'utf8')));
    expect(u.split('const talentSourceRows =').length - 1).toBe(1);
    // the only literal "SELECT <cols> FROM <from>" prefix is the shared helper body
    expect(u.split('SELECT ${cols} FROM ${from}').length - 1).toBe(1);
    expect(t.split('import {talentSourceRows}').length - 1).toBe(1);
    expect(t.split('talentSourceRows(').length - 1).toBe(2); // two call sites
    expect(t.split('SELECT ${cols} FROM ${from}').length - 1).toBe(0);
  });

  it('emits byte-identical SQL for both former inline source rows', async () => {
    const rows = evaluate(await readFile(util, 'utf8'));
    expect(rows('*', 'talent_requisitions')).toBe(REQUISITIONS);
    expect(rows('a.*,r.branch_id,r.department_id', 'talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id')).toBe(APPLICATIONS);
  });

  it('keeps source consumers, purchase gates and locks unchanged', async () => {
    const text = await readFile(file, 'utf8');
    for (const ref of [
      'export async function talentMetricHistoryAccess',
      'export async function talentMetricSource',
      'if(await talentPackageEnforcementMode()!=="enforced")return {sql:rows,args:[]',
      'purchasedCapabilityPredicate("talent_metric_owned.company_id"',
      'assignmentReadSql("talent_metric_owned","records.read")',
      'assignmentReadSql("talent_metric_owned","records.review")',
    ]) expect(text, ref).toContain(ref);
    expect(text).not.toMatch(/LOCK TABLE|FOR\s+UPDATE|FOR\s+SHARE|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|pg_advisory/i);
  });

  it('re-pins the talent owner SELECT KPI strictly below BEFORE=48 to 47', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(44);
    expect(count).toBeLessThan(45);
  });
});

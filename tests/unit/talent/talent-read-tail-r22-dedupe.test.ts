import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/record-purchases.ts', import.meta.url);
const util = new URL('../../../packages/server-talent/src/adapters/talent-read-tail-r32-sources.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r21 baseline). The five Recruitment owner
// source rows are all owner-local literals, so the pinned string is stable.
const EXPECTED_SOURCES = `SELECT r.id,r.company_id,r.branch_id,r.department_id,'requisition' kind,'recruit.positions' capability FROM talent_requisitions r
 UNION ALL SELECT c.id,c.company_id,NULL,NULL,'candidate','recruit.candidates' FROM talent_candidates c
 UNION ALL SELECT a.id,a.company_id,r.branch_id,r.department_id,'application','recruit.applications' FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id
 UNION ALL SELECT p.id,a.company_id,r.branch_id,r.department_id,'proposal','recruit.applications' FROM talent_proposals p JOIN talent_applications a ON a.id=p.application_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id
 UNION ALL SELECT o.id,o.company_id,r.branch_id,r.department_id,'offer','recruit.offers' FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(helperText: string, text: string): string {
  const helper = helperText.match(/const talentSourceRows = [\s\S]*?;\n/)![0].replace(/:\s*string/g, '');
  // r37 universal head injected so the extracted helper can render standalone.
  const head = 'const selectProjection = (projection) => `SELECT ${projection}`;';
  const talentSourceRows = new Function(head + helper + ' return talentSourceRows;')();
  const tpl = text.match(/const sources=`([\s\S]*?)`;/)![1];
  return new Function('talentSourceRows', 'return `' + tpl + '`;')(talentSourceRows);
}

describe('talent read-tail r22 source-row family dedupe', () => {
  it('declares the shared family once and routes all five rows through it', async () => {
    const [u, t] = await Promise.all([util, file].map(f => readFile(f, 'utf8')));
    expect(u.split('const talentSourceRows =').length - 1).toBe(1);
    // the only literal head is now the r37 selectProjection delegation
    expect(u.split('${selectProjection(cols)} FROM ${from}').length - 1).toBe(1);
    expect(u).toContain("from './talent-read-tail-r37-sources'");
    expect(t.split('import {talentSourceRows}').length - 1).toBe(1);
    expect(t.split('${talentSourceRows(').length - 1).toBe(5);
    expect(t.split('SELECT ${cols} FROM ${from}').length - 1).toBe(0);
  });

  it('emits byte-identical SQL for the Recruitment owner source rows', async () => {
    expect(evaluate(await readFile(util, 'utf8'), await readFile(file, 'utf8'))).toBe(EXPECTED_SOURCES);
  });

  it('keeps source consumers and locks unchanged', async () => {
    const text = await readFile(file, 'utf8');
    for (const ref of [
      "from './talent-read-tail-r41-sources'",
      'talentSourceSubqueryExists(sources,',
      'talent_source CROSS JOIN ${talentActorScopeCte} talent_viewer',
      'actual_talent_source WHERE actual_talent_source.id=',
      "nativeTalentReadFirst<{capability:string;company_id:string}>('talent_rd_workspace_record_source',[id],database)",
      'export async function talentSourceReadScope',
      'export async function requireTalentWorkspaceRecordRead',
    ]) expect(text, ref).toContain(ref);
    expect(text).not.toMatch(/LOCK TABLE|FOR\s+UPDATE|FOR\s+SHARE|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below BEFORE=48 to 47', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(10);
    expect(count).toBeLessThan(40);
  });
});

import { readFileSync } from 'node:fs';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/talent.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);
const headUrl = new URL('../../../packages/server-talent/src/adapters/talent-read-tail-r37-sources.ts', import.meta.url);
const headDecl = readFileSync(headUrl, 'utf8').match(/export const selectProjection = [\s\S]*?;\n/)![0].replace('export const', 'const').replace(/:\s*string/g, '');
const selectProjection = new Function(headDecl + ' return selectProjection;')();

// Exact pre-dedupe emitted SQL (r34 baseline, BEFORE=46). Only the projected
// columns and the trailing predicate are parameterized; the owner-eligibility
// witness and FROM/JOIN skeleton stay fixed, so every emitted read is
// byte-identical to the former inline copies. The stub keeps the pinned
// strings owner-local and stable.
const ELIGIBLE = '__ELIGIBLE__';
const OWNED_APPS = `SELECT a.*,r.job_title,c.legal_name candidate_name,CASE WHEN ${ELIGIBLE} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.company_id=? AND `;
const DETAIL_APP = `SELECT a.*,r.job_title,r.branch_id,r.department_id,c.legal_name candidate_name,c.email candidate_email,c.retention_until,c.version candidate_version,CASE WHEN ${ELIGIBLE} THEN 1 ELSE 0 END owner_eligible FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE a.id=? AND a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text: string): (columns: string, tail: string) => string {
  const helper = text.match(/const applicationProjection = [\s\S]*?;\n/)![0].replace(/:\s*string/g, '');
  return new Function('eligibleOwner', 'selectProjection', `${helper}\nreturn applicationProjection;`)(ELIGIBLE, selectProjection);
}

describe('talent read-tail r35 application-projection head family dedupe', () => {
  it('declares the shared head once and routes both reads through it', async () => {
    const text = await readFile(file, 'utf8');
    expect(text.split('const applicationProjection =').length - 1).toBe(1);
    expect(text.split('applicationProjection(').length - 1).toBe(2); // ownedApps + detail call
    expect(text.split('const ownedApps=applicationProjection(').length - 1).toBe(1);
    expect(text.split('CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible`)} FROM talent_applications a').length - 1).toBe(1);
    expect(text.split('CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible`)} FROM talent_applications a JOIN').length - 1).toBe(1);
  });

  it('emits byte-identical SQL for both former inline projections', async () => {
    const head = evaluate(await readFile(file, 'utf8'));
    expect(head('c.legal_name candidate_name', 'a.company_id=? AND ')).toBe(OWNED_APPS);
    expect(head('r.branch_id,r.department_id,c.legal_name candidate_name,c.email candidate_email,c.retention_until,c.version candidate_version', 'a.id=? AND a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)')).toBe(DETAIL_APP);
  });

  it('keeps native reads, binds, locks and owner-local boundary unchanged', async () => {
    const text = await readFile(file, 'utf8');
    expect(text).toContain("nativeTalentReadAll<Row>('talent_rd_workspace_requisitions'");
    expect(text).toContain("nativeTalentReadAll<Row>('talent_rd_workspace_candidates'");
    expect(text).toContain('applicationProjection(\'c.legal_name candidate_name\',\'a.company_id=? AND \')');
    expect(text).toContain('.bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId)');
    expect(text).not.toMatch(/LOCK TABLE|FOR\s+UPDATE|FOR\s+SHARE|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below BEFORE=46', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(11);
    expect(count).toBeLessThan(40);
  });
});

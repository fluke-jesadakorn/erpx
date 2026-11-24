import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const file = new URL('../../../packages/server-talent/src/adapters/executive-evidence-sources.ts', import.meta.url);
const srcDir = new URL('../../../packages/server-talent/src/', import.meta.url);

// Exact pre-dedupe emitted SQL (r25 baseline, BEFORE=53). Only the projection,
// FROM tail and order are parameterized; the purchased metric source and the
// bind order stay in the caller, so every emitted statement is byte-identical.
const REQUISITION = `SELECT r.id,r.job_title,r.target_start_date,r.status,r.version,r.updated_at,r.branch_id,r.department_id
      FROM (SRC) r WHERE W
      ORDER BY r.job_title,r.id LIMIT ?`;
const APPLICATION = `SELECT a.id,a.requisition_id,a.status,a.version,a.updated_at,r.job_title,r.branch_id,r.department_id
      FROM (SRC) a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE W
      ORDER BY a.updated_at DESC,a.id DESC LIMIT ?`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
function evaluate(text: string): (projection: string, fromTail: string, order: string) => string {
  const helper = text.match(/const evidenceSelect = [\s\S]*?LIMIT \?`;\n/)![0].replace(/:\s*string/g, '');
  // r37 universal head injected so the extracted helper can render standalone.
  const head = 'const selectProjection = (projection) => `SELECT ${projection}`;';
  return new Function('source', `${head}\n${helper}\nreturn evidenceSelect;`)({ sql: 'SRC' });
}

describe('talent read-tail r26 evidence page family dedupe', () => {
  it('declares the shared family once and routes both pages through it', async () => {
    const text = await readFile(file, 'utf8');
    expect(text.split('const evidenceSelect =').length - 1).toBe(1);
    expect(text.split('evidenceSelect(').length - 1).toBe(2);
    // the page head now renders through the r37 selectProjection head
    expect(text.split('${selectProjection(projection)}').length - 1).toBe(1);
    expect(text).toContain("from './talent-read-tail-r37-sources'");
    expect(text).not.toMatch(/SELECT r\.id,r\.job_title,r\.target_start_date/);
    expect(text).not.toMatch(/SELECT a\.id,a\.requisition_id,a\.status/);
  });

  it('emits byte-identical SQL for the requisition and application pages', async () => {
    const select = evaluate(await readFile(file, 'utf8'));
    expect(select(
      'r.id,r.job_title,r.target_start_date,r.status,r.version,r.updated_at,r.branch_id,r.department_id',
      'r WHERE W', 'r.job_title,r.id',
    )).toBe(REQUISITION);
    expect(select(
      'a.id,a.requisition_id,a.status,a.version,a.updated_at,r.job_title,r.branch_id,r.department_id',
      `a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE W`, 'a.updated_at DESC,a.id DESC',
    )).toBe(APPLICATION);
  });

  it('keeps source consumers, binds and locks unchanged', async () => {
    const text = await readFile(file, 'utf8');
    for (const ref of [
      'export function executiveEvidenceSources',
      'const source=await talentMetricSource(db,actor,input.metric)',
      '.bind(...values, input.limit + 1).all<TalentEvidenceRow>()',
      'where.join(" AND ")',
    ]) expect(text, ref).toContain(ref);
    expect(text).not.toMatch(/LOCK TABLE|FOR\s+UPDATE|FOR\s+SHARE|INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|pg_advisory/i);
  });

  it('lowers the talent owner SELECT KPI strictly below the recorded 53 r25 baseline', async () => {
    let count = 0;
    for (const f of await tsFiles(srcDir)) count += (await readFile(f, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(39);
    expect(count).toBeLessThan(40);
  });
});
